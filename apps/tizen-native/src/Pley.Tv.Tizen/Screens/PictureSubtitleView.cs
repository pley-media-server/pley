using System;
using System.Collections.Generic;
using Pley.Tv.Core.Subtitles.Pictures;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Picture subtitles (PGS, VobSub, DVB) over the video, as the web app's PictureSubtitles draws
/// them: each bitmap of the frame where its plane puts it, the plane (a Blu-ray's 1920×1080, a
/// DVD's 720×576) stretched over the whole screen. Each bitmap's RGBA bytes become a texture shown
/// by an image; those of the frame before leave the screen as the next one comes, and their
/// textures go a second later, once the renderer is done with them: freed at once, as a track
/// being read out came again every few seconds (its frames new objects, the same pictures), the
/// app closed natively. The same picture again is kept as it is. Unlike text, pictures stay
/// where they are while the controls show, under them. The pictures leaving hold
/// <see cref="LeavingMostBytes"/> at most, their copies and textures together: frames changing
/// every tick, as a crafted file's may, would pile them up, so past that the next pictures wait for
/// the sweep.
/// </summary>
sealed class PictureSubtitleView : View
{
    readonly List<(ImageView Image, ImageUrl Url, PixelData Pixels, long Bytes)> shown = new List<(ImageView, ImageUrl, PixelData, long)>();
    /// <summary>
    /// Pictures off the screen, every view's, kept until the renderer is surely done with their
    /// textures: past the view itself, which may go with the player before then.
    /// </summary>
    static readonly List<(ImageView Image, ImageUrl Url, PixelData Pixels, long Bytes, long At)> Leaving = new List<(ImageView, ImageUrl, PixelData, long, long)>();
    static long leavingBytes;
    const long LeavingMostBytes = 128L << 20;
    /// <summary>Views whose next pictures wait for <see cref="Leaving"/> to make room.</summary>
    static readonly List<PictureSubtitleView> Waiting = new List<PictureSubtitleView>();
    /// <summary>Lets go of what left over a second ago; held here, as a timer nothing holds is collected while it runs.</summary>
    static Timer? sweep;
    const uint LetGoMs = 1000;
    PictureFrame? frame;
    /// <summary>The frame showing now, while its pictures wait for room.</summary>
    PictureFrame? held;

    public PictureSubtitleView()
    {
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
    }

    /// <summary>The frame showing now; null clears the screen.</summary>
    public void Show(PictureFrame? next)
    {
        if (Disposed || next == frame) return;
        // The same pictures again (a track read again): as they are.
        if (Same(frame, next))
        {
            frame = next;
            if (held != null) held = next;
            return;
        }
        frame = next;
        held = null;
        Clear();
        if (next == null || next.PlaneWidth <= 0 || next.PlaneHeight <= 0) return;
        if (leavingBytes + Bytes(next) > LeavingMostBytes)
        {
            held = next;
            if (!Waiting.Contains(this)) Waiting.Add(this);
            return;
        }
        var sx = SizeWidth / next.PlaneWidth;
        var sy = SizeHeight / next.PlaneHeight;
        foreach (var b in next.Bitmaps)
        {
            if (b.Width <= 0 || b.Height <= 0) continue;
            // PixelData copies the bytes: the decoder keeps its own for the frames around the playhead.
            var pixels = new PixelData(b.Rgba, (uint)b.Rgba.Length, (uint)b.Width, (uint)b.Height, PixelFormat.RGBA8888);
            var url = pixels.GenerateUrl();
            var image = new ImageView
            {
                ResourceUrl = url.ToString(),
                Position = new Position((float)Math.Round(b.X * sx), (float)Math.Round(b.Y * sy)),
                Size = new Size((float)Math.Round(b.Width * sx), (float)Math.Round(b.Height * sy)),
            };
            Add(image);
            shown.Add((image, url, pixels, Bytes(b)));
        }
    }

    /// <summary>The frame held for room, now that the sweep let some pictures go: shown, or held again.</summary>
    void ShowHeld()
    {
        var next = held;
        if (Disposed || next == null) return;
        held = null;
        frame = null;
        Show(next);
    }

    /// <summary>The pictures leave the screen now; their textures in a second (<see cref="sweep"/>).</summary>
    void Clear()
    {
        var now = Environment.TickCount64;
        foreach (var (image, url, pixels, bytes) in shown)
        {
            Remove(image);
            Leaving.Add((image, url, pixels, bytes, now));
            leavingBytes += bytes;
        }
        shown.Clear();
        if (Leaving.Count == 0) return;
        if (sweep == null)
        {
            sweep = new Timer(LetGoMs);
            sweep.Tick += (s, e) =>
            {
                Sweep();
                var waiting = Waiting.ToArray();
                Waiting.Clear();
                foreach (var view in waiting) view.ShowHeld();
                return Leaving.Count > 0 || Waiting.Count > 0;
            };
        }
        if (!sweep.IsRunning()) sweep.Start();
    }

    static void Sweep()
    {
        var now = Environment.TickCount64;
        for (var i = Leaving.Count - 1; i >= 0; i--)
        {
            var (image, url, pixels, bytes, at) = Leaving[i];
            if (now - at < LetGoMs) continue;
            image.Dispose();
            url.Dispose();
            pixels.Dispose();
            Leaving.RemoveAt(i);
            leavingBytes -= bytes;
        }
    }

    /// <summary>What a picture holds once shown: its PixelData copy, and its texture as much again.</summary>
    static long Bytes(SubtitleBitmap b) => 2L * b.Rgba.Length;

    static long Bytes(PictureFrame frame)
    {
        long bytes = 0;
        foreach (var b in frame.Bitmaps) bytes += Bytes(b);
        return bytes;
    }

    /// <summary>Whether two frames show the same pictures at the same places.</summary>
    static bool Same(PictureFrame? a, PictureFrame? b)
    {
        if (a == null || b == null || a.PlaneWidth != b.PlaneWidth || a.PlaneHeight != b.PlaneHeight || a.Bitmaps.Count != b.Bitmaps.Count) return false;
        for (var i = 0; i < a.Bitmaps.Count; i++)
        {
            var x = a.Bitmaps[i];
            var y = b.Bitmaps[i];
            if (x.X != y.X || x.Y != y.Y || x.Width != y.Width || x.Height != y.Height || !x.Rgba.AsSpan().SequenceEqual(y.Rgba)) return false;
        }
        return true;
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        // Off the screen with the view; their textures a second later, as any. Only when disposed
        // by the app: collected, its images may have gone first (Kit/Held.cs).
        // A view waiting for room is held by Waiting, so is never collected while it waits.
        if (type == DisposeTypes.Explicit)
        {
            Clear();
            held = null;
            Waiting.Remove(this);
        }
        base.Dispose(type);
    }
}
