using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Subtitles.Pictures;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Local;

/// <summary>
/// A menu cell's buttons: the first highlight its navigation packs carry, and the subpicture unit
/// the buttons are drawn with (its pixels and its control sequences); and its video's first
/// picture (<see cref="MpegStill"/>), the whole of a still menu's. Any may be missing.
/// </summary>
/// <param name="Frame">The video from its sequence header through its first picture, as an elementary stream.</param>
public sealed record DvdMenuPicture(DvdHighlight? Highlight, byte[]? Unit, Spu? Control, byte[]? Frame = null);

/// <summary>
/// A DVD menu's buttons as Pley draws them over the menu's video: read out of the menu cell's
/// first megabytes, then drawn with the selected button in its colour scheme's colours, as a
/// player's highlight does.
/// </summary>
public static class DvdMenu
{
    const int Sector = DiscImage.Sector;
    /// <summary>Sectors read at once: 32 KB, under the large-object heap, whose arrays only full collections free.</summary>
    const int Chunk = 16;
    /// <summary>Sectors looked through for the buttons: 4 MB, past the first seconds of an animated menu.</summary>
    const int LookMost = 2048;
    /// <summary>A first picture's most: a DVD's I-picture is a few hundred kilobytes.</summary>
    const int FrameMost = 1024 * 1024;

    /// <summary>
    /// The buttons of <paramref name="cell"/> in <paramref name="vob"/>, read through
    /// <paramref name="read"/>: the subpicture is stream <paramref name="stream"/> (0x20 + it).
    /// </summary>
    public static DvdMenuPicture Read(Func<long, int, byte[]> read, DiscFile vob, DvdPgcCell cell, int stream = 0)
    {
        DvdHighlight? highlight = null;
        byte[]? unit = null;
        byte[]? whole = null;
        var filled = 0;
        var frame = new System.IO.MemoryStream();
        var frameDone = false;
        var pictures = 0;
        var last = Math.Min(cell.LastSector, cell.FirstSector + LookMost - 1);
        for (var s = cell.FirstSector; s <= last && (highlight == null || whole == null || !frameDone); s += Chunk)
        {
            var count = (int)Math.Min(Chunk, last - s + 1);
            var offset = vob.Offset + s * Sector;
            if (offset >= vob.Offset + vob.Length) break;
            var bytes = read(offset, count * Sector);
            for (var at = 0; at + Sector <= bytes.Length; at += Sector)
            {
                highlight ??= DvdButtons.Read(bytes, at);
                if (whole != null && frameDone) continue;
                foreach (var (id, _, from, to) in DvdSubtitles.Packets(bytes, at))
                {
                    if (id is >= 0xe0 and <= 0xef && !frameDone)
                    {
                        // The video from its sequence header, until a second picture starts.
                        var begin = from;
                        if (frame.Length == 0)
                        {
                            begin = StartCode(bytes, from, to, 0xb3);
                            if (begin < 0) continue;
                        }
                        var before = (int)frame.Length;
                        frame.Write(bytes, begin, to - begin);
                        var buffer = frame.GetBuffer();
                        for (var i = Math.Max(0, before - 3); i + 3 < frame.Length; i++)
                            if (buffer[i] == 0 && buffer[i + 1] == 0 && buffer[i + 2] == 1 && (buffer[i + 3] == 0 || buffer[i + 3] == 0xb7) && (buffer[i + 3] == 0xb7 || ++pictures == 2))
                            {
                                frameDone = true;
                                break;
                            }
                        if (frame.Length > FrameMost) frameDone = true;
                        continue;
                    }
                    if (whole != null) continue;
                    if (id != 0xbd || from >= to || bytes[from] != 0x20 + stream) continue;
                    var start = from + 1;
                    if (unit == null)
                    {
                        // A unit starts with its size.
                        if (to - start < 4) continue;
                        var size = (bytes[start] << 8) | bytes[start + 1];
                        if (size < 4) continue;
                        unit = new byte[size];
                        filled = 0;
                    }
                    var n = Math.Min(to - start, unit.Length - filled);
                    Array.Copy(bytes, start, unit, filled, n);
                    filled += n;
                    if (filled == unit.Length)
                    {
                        whole = unit;
                        break;
                    }
                }
            }
        }
        return new DvdMenuPicture(highlight, whole, whole != null ? VobSubDecoder.ParseSpu(whole) : null, frame.Length > 0 ? frame.ToArray() : null);
    }

    /// <summary>Where start code <paramref name="code"/> is in <paramref name="b"/> between the bounds; -1 when it isn't.</summary>
    static int StartCode(byte[] b, int from, int to, byte code)
    {
        for (var i = from; i + 3 < to; i++)
            if (b[i] == 0 && b[i + 1] == 0 && b[i + 2] == 1 && b[i + 3] == code) return i;
        return -1;
    }

    /// <summary>
    /// The menu's subpicture with button <paramref name="selected"/> highlighted: the pixels in
    /// its rectangle take its colour scheme's selected colours and contrast (its action ones while
    /// <paramref name="pressed"/>), the rest those the unit gives; null when there's nothing to draw.
    /// </summary>
    /// <param name="palette">The menu chain's 16 colours, as Y, Cr, Cb.</param>
    public static SubtitleBitmap? Draw(DvdMenuPicture menu, IReadOnlyList<uint> palette, int selected, bool pressed = false)
    {
        if (menu.Unit == null || menu.Control is not { } spu) return null;
        var codes = VobSubDecoder.Codes(menu.Unit, spu);
        if (codes == null) return null;
        var width = spu.X2 - spu.X1 + 1;
        var height = spu.Y2 - spu.Y1 + 1;
        var colours = new (byte R, byte G, byte B)[16];
        for (var i = 0; i < 16 && i < palette.Count; i++) colours[i] = DvdVideo.Rgb((int)(palette[i] >> 16) & 0xff, (int)(palette[i] >> 8) & 0xff, (int)palette[i] & 0xff);
        var button = menu.Highlight?.Buttons.FirstOrDefault(b => b.Number == selected);
        var schemes = menu.Highlight?.Schemes;
        // Scheme 0: the button isn't highlighted.
        var scheme = button != null && schemes != null && button.Scheme >= 1 && button.Scheme <= schemes.Count ? schemes[button.Scheme - 1] : null;
        var lit = scheme == null ? null : pressed ? scheme.ActionColors : scheme.SelectedColors;
        var litAlpha = scheme == null ? null : pressed ? scheme.ActionContrast : scheme.SelectedContrast;
        var rgba = new byte[width * height * 4];
        for (var y = 0; y < height; y++)
        {
            var py = spu.Y1 + y;
            var row = button != null && py >= button.Y1 && py <= button.Y2;
            for (var x = 0; x < width; x++)
            {
                var i = y * width + x;
                var c = codes[i];
                var px = spu.X1 + x;
                var on = lit != null && row && px >= button!.X1 && px <= button.X2;
                var colour = colours[(on ? lit! : spu.Colors)[c] & 15];
                var o = i * 4;
                rgba[o] = colour.R;
                rgba[o + 1] = colour.G;
                rgba[o + 2] = colour.B;
                rgba[o + 3] = (byte)((on ? litAlpha! : spu.Alpha)[c] * 17);
            }
        }
        return new SubtitleBitmap(spu.X1, spu.Y1, width, height, rgba);
    }

    /// <summary>The button the remote moves to from <paramref name="selected"/>: as the button says, else where it is.</summary>
    public static int Move(DvdHighlight highlight, int selected, Direction direction)
    {
        var button = highlight.Buttons.FirstOrDefault(b => b.Number == selected);
        if (button == null) return highlight.Buttons.Count > 0 ? highlight.Buttons[0].Number : 0;
        var to = direction switch
        {
            Direction.Up => button.Up,
            Direction.Down => button.Down,
            Direction.Left => button.Left,
            _ => button.Right,
        };
        return to >= 1 && highlight.Buttons.Any(b => b.Number == to) ? to : selected;
    }
}
