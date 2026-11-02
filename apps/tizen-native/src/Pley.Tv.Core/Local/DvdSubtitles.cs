using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;

namespace Pley.Tv.Core.Local;

/// <summary>
/// A DVD title's subtitles read out of its VOBs: the subpicture units of one subpicture stream,
/// carried in private stream 1 (0xBD) packets with the substream 0x20 + its number, each timed by
/// its packet's PTS from its cell's first picture. The title's packs are read in order, every
/// sector, as the subtitles are interleaved with the film; units come as they're read.
/// </summary>
public static class DvdSubtitles
{
    const int Sector = DiscImage.Sector;
    /// <summary>Sectors read at once: 1 MB.</summary>
    const int Chunk = 512;
    /// <summary>The cell the film is in is read from this long before where it is.</summary>
    const long LeadMs = 2_000;
    /// <summary>Tries at finding that point by its pictures' times, each from where the last landed.</summary>
    const int Aims = 8;
    /// <summary>Chunks looked through at a cell's start for its first picture's time.</summary>
    const int FirstPictureChunks = 4;

    /// <summary>
    /// Stream <paramref name="id"/>'s units of <paramref name="title"/>, read from
    /// <paramref name="image"/>, each passed to <paramref name="found"/> as it comes. One buffer
    /// for the whole title: a film's gigabytes in fresh arrays had the TV's collector busy.
    /// </summary>
    /// <param name="fromMs">
    /// Where the film is: its cell is read first, from a little before that point (its bytes taken
    /// as its time), then that cell's start, then the cells after it, then those before.
    /// </param>
    /// <param name="progress">Where the reading has got to in the film, after each chunk: through stretches without subtitles too.</param>
    public static void Read(System.IO.Stream image, DvdTitle title, int id, Action<SubtitleBlock> found, CancellationToken cancel = default, long fromMs = 0,
        Action<long>? progress = null)
    {
        var substream = 0x20 + id;
        var buffer = new byte[Chunk * Sector];
        var cells = title.Cells;
        var first = 0;
        for (var i = 0; i < cells.Count; i++)
            if (cells[i].StartMs <= fromMs) first = i;
        for (var k = 0; k < cells.Count; k++)
        {
            var c = (first + k) % cells.Count;
            var cell = cells[c];
            var bytes = cell.Spans.Sum(x => x.Length);
            // The cell's clock: its first picture's time, at its start.
            var firstPts = FirstPicture(cell, bytes);
            long start = 0;
            if (k == 0 && fromMs > cell.StartMs + LeadMs && firstPts is long origin)
            {
                var length = (c + 1 < cells.Count ? cells[c + 1].StartMs : title.DurationMs) - cell.StartMs;
                if (length > 0) start = Aim(cell, bytes, origin, fromMs - LeadMs - cell.StartMs, length);
            }
            Units(cell, start, bytes, firstPts);
            if (start > 0) Units(cell, 0, start, firstPts);
        }

        // The cell's units in its bytes from … to, each passed on as its last packet comes.
        void Units(DvdCell cell, long from, long to, long? firstPts)
        {
            byte[]? unit = null;
            var filled = 0;
            long unitPts = 0;
            var ci = cells.ToList().IndexOf(cell);
            var cellLength = (ci + 1 < cells.Count ? cells[ci + 1].StartMs : title.DurationMs) - cell.StartMs;
            var cellBytes = cell.Spans.Sum(x => x.Length);
            var reached = from;
            foreach (var count in Chunks(cell, from, to))
            {
                reached += count;
                if (progress != null && cellBytes > 0) progress(cell.StartMs + (long)(reached / (double)cellBytes * Math.Max(0, cellLength)));
                for (var s = 0; s + Sector <= count; s += Sector)
                {
                    foreach (var (streamId, pts, packetFrom, end) in Packets(buffer, s))
                    {
                        if (streamId != 0xbd || packetFrom >= end || buffer[packetFrom] != substream) continue;
                        var at = packetFrom + 1;
                        if (pts is long begins)
                        {
                            // A unit begins: its size is its first two bytes.
                            if (end - at < 2) continue;
                            var size = (buffer[at] << 8) | buffer[at + 1];
                            if (size < 4) continue;
                            unit = new byte[size];
                            filled = 0;
                            unitPts = begins;
                        }
                        if (unit == null) continue;
                        var n = Math.Min(end - at, unit.Length - filled);
                        Array.Copy(buffer, at, unit, filled, n);
                        filled += n;
                        if (filled < unit.Length) continue;
                        found(new SubtitleBlock(cell.StartMs + Math.Max(0, unitPts - (firstPts ?? unitPts)) / 90, null, unit));
                        unit = null;
                    }
                }
            }
        }

        // Where in the cell's bytes its pictures reach <paramref name="target"/> ms (from the cell's
        // start), a little before: first as its bytes go with its time, then corrected by the time
        // of the picture found there, as the player seeks; never past it.
        long Aim(DvdCell cell, long bytes, long origin, long target, long length)
        {
            // A bracket: lo's pictures are at or before the target, hi's after; each try goes
            // between them as the bytes go with the time, and narrows the side it lands on.
            long lo = 0, hi = bytes;
            long loMs = 0, hiMs = length;
            for (var i = 0; i < Aims && hi - lo > Chunk * Sector; i++)
            {
                var guess = lo + (long)((target - loMs) / (double)Math.Max(1, hiMs - loMs) * (hi - lo));
                var at = Math.Max(lo + Sector, Math.Min(hi - Sector, guess)) / Sector * Sector;
                if (PictureAt(cell, at, bytes) is not long pts) break;
                var ms = (pts - origin) / 90;
                if (ms <= target)
                {
                    lo = at;
                    loMs = ms;
                    if (target - ms < 1_000) break;
                }
                else
                {
                    hi = at;
                    hiMs = ms;
                }
            }
            return lo;
        }

        // The time of the first picture from <paramref name="at"/> in the cell, within a chunk.
        long? PictureAt(DvdCell cell, long at, long bytes)
        {
            foreach (var count in Chunks(cell, at, Math.Min(bytes, at + (long)Chunk * Sector)))
                for (var s = 0; s + Sector <= count; s += Sector)
                    foreach (var (streamId, pts, _, _) in Packets(buffer, s))
                        if (pts is long p && streamId is >= 0xe0 and <= 0xef) return p;
            return null;
        }

        // The cell's first video PTS, within its first few chunks.
        long? FirstPicture(DvdCell cell, long bytes)
        {
            var looked = 0;
            foreach (var count in Chunks(cell, 0, bytes))
            {
                for (var s = 0; s + Sector <= count; s += Sector)
                    foreach (var (streamId, pts, _, _) in Packets(buffer, s))
                        if (pts is long p && streamId is >= 0xe0 and <= 0xef) return p;
                if (++looked >= FirstPictureChunks) break;
            }
            return null;
        }

        // The cell's bytes from … to (counted across its spans), into the buffer a chunk at a time: each chunk's length.
        IEnumerable<int> Chunks(DvdCell cell, long from, long to)
        {
            long spanStart = 0;
            foreach (var span in cell.Spans)
            {
                var spanEnd = spanStart + span.Length;
                for (var at = Math.Max(from, spanStart); at < Math.Min(to, spanEnd); at += (long)Chunk * Sector)
                {
                    cancel.ThrowIfCancellationRequested();
                    var count = (int)Math.Min((long)Chunk * Sector, Math.Min(to, spanEnd) - at);
                    image.Seek(span.Offset + (at - spanStart), System.IO.SeekOrigin.Begin);
                    var got = 0;
                    while (got < count)
                    {
                        var n = image.Read(buffer, got, count - got);
                        if (n == 0) break;
                        got += n;
                    }
                    // Whole sectors only: a short read's last part sector is left.
                    Array.Clear(buffer, got, buffer.Length - got);
                    yield return count;
                }
                spanStart = spanEnd;
            }
        }
    }

    /// <summary>
    /// The PES packets of the pack sector at <paramref name="at"/>: each one's stream, PTS (90 kHz)
    /// when it has one, and its payload's bounds in <paramref name="b"/>.
    /// </summary>
    internal static IEnumerable<(int StreamId, long? Pts, int From, int To)> Packets(byte[] b, int at)
    {
        var end = at + Sector;
        // The MPEG-2 pack header: 14 bytes and its stuffing.
        if (b[at] != 0 || b[at + 1] != 0 || b[at + 2] != 1 || b[at + 3] != 0xba) yield break;
        var p = at + 14 + (b[at + 13] & 7);
        while (p + 6 <= end)
        {
            if (b[p] != 0 || b[p + 1] != 0 || b[p + 2] != 1) yield break;
            var streamId = b[p + 3];
            var length = (b[p + 4] << 8) | b[p + 5];
            var next = Math.Min(end, p + 6 + length);
            if (streamId == 0xbd || streamId is >= 0xe0 and <= 0xef)
            {
                if (p + 9 > end) yield break;
                var headerLength = b[p + 8];
                long? pts = null;
                if ((b[p + 7] & 0x80) != 0 && p + 14 <= end)
                    pts = ((long)(b[p + 9] >> 1 & 7) << 30) | ((long)b[p + 10] << 22) | ((long)(b[p + 11] >> 1) << 15) | ((long)b[p + 12] << 7) | (long)(b[p + 13] >> 1);
                yield return (streamId, pts, p + 9 + headerLength, next);
            }
            p = next;
        }
    }
}
