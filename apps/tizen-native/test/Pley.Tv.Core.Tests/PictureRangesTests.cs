using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Subtitles.Pictures;
using Xunit;
using static Pley.Tv.Core.Tests.PictureKit;
using MksReader = Pley.Tv.Core.Subtitles.Pictures.Mks;

namespace Pley.Tv.Core.Tests;

/// <summary>A .mks as FFmpeg makes the server's: a SeekHead naming its Cues, its clusters, then the Cues listing them.</summary>
static class CuedMks
{
    static int[] U32(long v) => new[] { (int)((v >> 24) & 0xff), (int)((v >> 16) & 0xff), (int)((v >> 8) & 0xff), (int)(v & 0xff) };

    /// <summary>A PGS track with a cluster at each of <paramref name="clusterMs"/>, each holding one picture at its start.</summary>
    public static byte[] Pgs(params int[] clusterMs)
    {
        var shown = PgsDisplaySet(0x80, true, rle: new[] { 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0 });
        var data = shown.Select(v => (int)v).ToArray();
        var clusters = clusterMs.Select(ms => El(0x1f43b675, Cat(El(0xe7, U32(ms)), El(0xa3, Cat(new[] { 0x81 }, U16(0), new[] { 0x80 }, data))))).ToList();
        var info = El(0x1549a966, El(0x2ad7b1, new[] { 0x0f, 0x42, 0x40 }));
        var tracks = El(0x1654ae6b, TrackEntry("S_HDMV/PGS"));
        // The SeekHead's size doesn't depend on where the Cues are: a 4-byte position.
        int[] SeekHead(long cues) => El(0x114d9b74, El(0x4dbb, Cat(El(0x53ab, new[] { 0x1c, 0x53, 0xbb, 0x6b }), El(0x53ac, U32(cues)))));
        var before = SeekHead(0).Length + info.Length + tracks.Length;
        var positions = new List<long>();
        long at = before;
        foreach (var c in clusters)
        {
            positions.Add(at);
            at += c.Length;
        }
        var cues = El(0x1c53bb6b, Cat(clusterMs.Select((ms, i) =>
            El(0xbb, Cat(El(0xb3, U32(ms)), El(0xb7, Cat(El(0xf7, new[] { 1 }), El(0xf1, U32(positions[i]))))))).ToArray()));
        var segment = El(0x18538067, Cat(SeekHead(at), info, tracks, Cat(clusters.ToArray()), cues));
        return Bytes(Cat(El(0x1a45dfa3, El(0x4282, Str("matroska"))), segment));
    }

    /// <summary>A server serving <paramref name="file"/> with ranges, keeping what was asked.</summary>
    public static Func<long, long, Task<SubtitlePart>> Serving(byte[] file, List<(long, long)> asked, bool complete = true) => (from, to) =>
    {
        asked.Add((from, to));
        var end = Math.Min(to, file.Length - 1);
        var part = from > end ? new byte[0] : file.Skip((int)from).Take((int)(end - from + 1)).ToArray();
        return Task.FromResult(new SubtitlePart(part, from, complete ? file.Length : null, complete));
    };
}

public class PictureRangesTests
{
    static readonly int[] EveryTwoMinutes = Enumerable.Range(0, 10).Select(i => i * 120_000).ToArray();

    [Fact]
    public void The_index_names_the_first_cluster_and_the_cues_list_every_cluster()
    {
        var file = CuedMks.Pgs(EveryTwoMinutes);
        var index = MksReader.ReadIndex(file.Take(PictureRanges.HeadBytes).ToArray())!;
        Assert.NotNull(index.CuesAt);
        Assert.Equal(1_000_000, index.ScaleNs);
        var clusters = MksReader.ReadCues(file.Skip((int)index.CuesAt!.Value).ToArray(), index)!;
        Assert.Equal(EveryTwoMinutes.Select(ms => (long)ms), clusters.Select(c => c.TimeMs));
        Assert.Equal(index.FirstCluster, clusters[0].At);
        // The header before the first cluster and any cluster read as one file.
        var cut = file.Take((int)index.FirstCluster).Concat(file.Skip((int)clusters[4].At).Take((int)(clusters[5].At - clusters[4].At))).ToArray();
        Assert.Equal(new[] { 480_000L }, MksReader.ReadPictureTrack(cut).Samples.Select(s => s.TimeMs));
    }

    /// <summary>The pictures around the film are read alone, in one request, and never twice; the rest as they are wanted.</summary>
    [Fact]
    public async Task Only_the_clusters_around_the_film_are_read()
    {
        var file = CuedMks.Pgs(EveryTwoMinutes);
        var asked = new List<(long, long)>();
        var ranges = (await PictureRanges.Open(CuedMks.Serving(file, asked)))!;
        Assert.Equal(10, ranges.Clusters);
        Assert.Equal(2, asked.Count);
        var track = await ranges.Read(470_000, 800_000);
        Assert.Equal("S_HDMV/PGS", track.CodecId);
        Assert.Equal(new long[] { 360_000, 480_000, 600_000, 720_000 }, track.Samples.Select(s => s.TimeMs));
        Assert.Equal(3, asked.Count);
        Assert.True(asked[2].Item2 - asked[2].Item1 < file.Length / 2);
        Assert.True(ranges.Has(500_000, 750_000));
        Assert.False(ranges.Has(900_000, 1_000_000));
        await ranges.Read(500_000, 700_000);
        Assert.Equal(3, asked.Count);
        track = await ranges.Read(0, 100_000);
        Assert.Equal(new long[] { 0, 360_000, 480_000, 600_000, 720_000 }, track.Samples.Select(s => s.TimeMs));
        // The last clusters: to where the Cues start.
        track = await ranges.Read(1_000_000, 2_000_000);
        Assert.Equal(1_080_000, track.Samples.Last().TimeMs);
        Assert.Equal(PictureSubtitles.Of(track)!.FrameAt(1_080_500)?.Index, track.Samples.Count - 1);
    }

    /// <summary>
    /// A picture read before a part not read yet shows no further than where that part begins: what
    /// clears it may be there, and a seek into it showed the old picture until that part came.
    /// </summary>
    [Fact]
    public async Task A_picture_stops_where_the_part_not_read_begins()
    {
        var file = CuedMks.Pgs(EveryTwoMinutes);
        var ranges = (await PictureRanges.Open(CuedMks.Serving(file, new List<(long, long)>())))!;
        var subs = PictureSubtitles.Of(await ranges.Read(470_000, 800_000))!;
        Assert.NotNull(subs.FrameAt(800_000));
        Assert.Null(subs.FrameAt(850_000));
        Assert.Equal(840_000, subs.NextChange(800_000));
        subs = PictureSubtitles.Of(await ranges.Read(850_000, 900_000))!;
        Assert.Equal(840_000, subs.TimeOf(subs.FrameAt(850_000)!.Value.Index));
    }

    /// <summary>A server or proxy that answers a range with the whole file: read whole once, not in parts that are each the whole.</summary>
    [Fact]
    public async Task A_server_without_ranges_has_it_read_whole()
    {
        var file = CuedMks.Pgs(EveryTwoMinutes.Concat(Enumerable.Range(10, 2_000).Select(i => i * 120_000)).ToArray());
        Assert.True(file.Length > PictureRanges.HeadBytes);
        var asked = 0;
        Func<long, long, Task<SubtitlePart>> whole = (_, _) =>
        {
            asked++;
            return Task.FromResult(new SubtitlePart(file, 0, file.Length, true));
        };
        Assert.Null(await PictureRanges.Open(whole));
        Assert.Equal(1, asked);
    }

    /// <summary>The header is asked for where the player is (<c>?at=</c>), as the whole track is; the rest at the track's own address.</summary>
    [Fact]
    public async Task The_header_can_be_asked_for_elsewhere()
    {
        var file = CuedMks.Pgs(EveryTwoMinutes);
        var asked = new List<(long, long)>();
        var headAsked = new List<(long, long)>();
        var ranges = await PictureRanges.Open(CuedMks.Serving(file, asked), CuedMks.Serving(file, headAsked));
        Assert.NotNull(ranges);
        Assert.Equal(new[] { (0L, (long)PictureRanges.HeadBytes - 1) }, headAsked);
        Assert.Single(asked);
    }

    [Fact]
    public async Task A_file_without_cues_or_still_being_made_is_read_whole()
    {
        var asked = new List<(long, long)>();
        var plain = PictureKit.Mks("S_HDMV/PGS", new[] { new PictureKit.MksBlock(1000, PgsDisplaySet(0x80, true)) });
        Assert.Null(await PictureRanges.Open(CuedMks.Serving(plain, asked)));
        Assert.Null(await PictureRanges.Open(CuedMks.Serving(CuedMks.Pgs(EveryTwoMinutes), asked, complete: false)));
    }
}
