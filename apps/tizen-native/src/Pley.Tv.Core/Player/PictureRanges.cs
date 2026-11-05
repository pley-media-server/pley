using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Subtitles.Pictures;

namespace Pley.Tv.Core.Player;

/// <summary>
/// A picture track's .mks read a part at a time, by the clusters its Cues list: the pictures around
/// where the film is first, then those ahead as it plays, those around a seek as it jumps. Read whole
/// (20 MB for a film's PGS), a resumed film's subtitles came seconds after its picture, the download
/// took the network from the video's first seconds, and it was read again every time the film
/// opened. A file without Cues, or one the server is still making, isn't read this way
/// (<see cref="Open"/> is null): it's read whole, as before.
/// </summary>
public sealed class PictureRanges
{
    /// <summary>The bytes asked for first: the file's header, up to its first cluster, in a few KB.</summary>
    public const int HeadBytes = 64 << 10;

    /// <summary>A file's Cues at most: a film's are tens of KB.</summary>
    public const int MostCuesBytes = 2 << 20;

    readonly Func<long, long, Task<SubtitlePart>> fetch;
    readonly byte[] head;
    readonly IReadOnlyList<MksCluster> clusters;
    /// <summary>Where the clusters end: the Cues after them, or the file's end.</summary>
    readonly long clustersEnd;
    readonly bool[] loaded;
    readonly SemaphoreSlim one = new SemaphoreSlim(1, 1);
    /// <summary>Every sample read so far, by time.</summary>
    readonly SortedDictionary<long, PictureSample> samples = new SortedDictionary<long, PictureSample>();
    readonly string codecId;
    readonly byte[]? codecPrivate;

    PictureRanges(Func<long, long, Task<SubtitlePart>> fetch, byte[] head, IReadOnlyList<MksCluster> clusters, long clustersEnd)
    {
        this.fetch = fetch;
        this.head = head;
        // The track's codec, from the header alone: a part without pictures says it too.
        var track = Mks.ReadPictureTrack(head);
        codecId = track.CodecId;
        codecPrivate = track.CodecPrivate;
        this.clusters = clusters;
        this.clustersEnd = clustersEnd;
        loaded = new bool[clusters.Count];
    }

    /// <summary>How many clusters the Cues list.</summary>
    public int Clusters => clusters.Count;

    /// <summary>
    /// The file's header and Cues, read through <paramref name="fetch"/> (bytes from, to inclusive),
    /// the header through <paramref name="fetchHead"/> when given; null when it can't be read in parts:
    /// being made still, without Cues, not a Matroska file, or served whole whatever was asked (a
    /// server or proxy without ranges: each part would be the whole file again).
    /// </summary>
    public static async Task<PictureRanges?> Open(Func<long, long, Task<SubtitlePart>> fetch, Func<long, long, Task<SubtitlePart>>? fetchHead = null)
    {
        var first = await (fetchHead ?? fetch)(0, HeadBytes - 1).ConfigureAwait(false);
        if (!first.Complete || first.From != 0 || first.Total is not long total || first.Bytes.Length > HeadBytes) return null;
        var index = Mks.ReadIndex(first.Bytes);
        if (index?.CuesAt is not long cuesAt || cuesAt >= total) return null;
        var cueBytes = await fetch(cuesAt, Math.Min(total, cuesAt + MostCuesBytes) - 1).ConfigureAwait(false);
        if (cueBytes.From != cuesAt) return null;
        var clusters = Mks.ReadCues(cueBytes.Bytes, index);
        if (clusters == null || clusters.Count == 0 || clusters[0].At != index.FirstCluster) return null;
        var head = new byte[index.FirstCluster];
        Array.Copy(first.Bytes, head, head.Length);
        return new PictureRanges(fetch, head, clusters, cuesAt > clusters[clusters.Count - 1].At ? cuesAt : total);
    }

    /// <summary>Whether the pictures from <paramref name="fromMs"/> to <paramref name="toMs"/> are read.</summary>
    public bool Has(long fromMs, long toMs)
    {
        var (first, end) = Span(fromMs, toMs);
        for (var i = first; i < end; i++)
            if (!loaded[i]) return false;
        return true;
    }

    /// <summary>
    /// Reads the clusters holding <paramref name="fromMs"/> to <paramref name="toMs"/> not read yet, one
    /// request a run of them, and gives the track as read so far: every sample read, by time.
    /// </summary>
    public async Task<PictureTrack> Read(long fromMs, long toMs)
    {
        await one.WaitAsync().ConfigureAwait(false);
        try
        {
            var (first, end) = Span(fromMs, toMs);
            var i = first;
            while (i < end)
            {
                if (loaded[i])
                {
                    i++;
                    continue;
                }
                var run = i;
                while (i < end && !loaded[i]) i++;
                var from = clusters[run].At;
                var to = (i < clusters.Count ? clusters[i].At : clustersEnd) - 1;
                var part = await fetch(from, to).ConfigureAwait(false);
                if (part.From != from) throw new MatroskaException("the server didn't answer the range asked for");
                // The header and the run of clusters read as one file, cut where the run ends.
                var file = new byte[head.Length + part.Bytes.Length];
                Buffer.BlockCopy(head, 0, file, 0, head.Length);
                Buffer.BlockCopy(part.Bytes, 0, file, head.Length, part.Bytes.Length);
                var track = Mks.ReadPictureTrack(file);
                foreach (var sample in track.Samples) samples[sample.TimeMs] = sample;
                for (var k = run; k < i; k++) loaded[k] = true;
            }
            return new PictureTrack(codecId, codecPrivate, samples.Values.ToList(), UnreadFrom());
        }
        finally
        {
            one.Release();
        }
    }

    /// <summary>Where each run of clusters not read yet begins: a sample read before one shows no further.</summary>
    List<long> UnreadFrom()
    {
        var from = new List<long>();
        for (var k = 0; k < clusters.Count; k++)
            if (!loaded[k] && (k == 0 || loaded[k - 1])) from.Add(clusters[k].TimeMs);
        return from;
    }

    /// <summary>The clusters holding <paramref name="fromMs"/> to <paramref name="toMs"/>: from the one it starts in to the first after it.</summary>
    (int First, int End) Span(long fromMs, long toMs)
    {
        var first = 0;
        while (first + 1 < clusters.Count && clusters[first + 1].TimeMs <= fromMs) first++;
        var end = first;
        while (end < clusters.Count && clusters[end].TimeMs <= toMs) end++;
        return (first, Math.Max(end, first + 1));
    }
}
