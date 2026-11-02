using System;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Text;
using System.Threading.Tasks;
using Pley.Tv.Core.Local;
using Pley.Tv.Core.Net;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>A DVD's disc image on a drive: its files, its titles, and a title served as one file to the player.</summary>
public class DiscTests
{
    const int Sector = DiscImage.Sector;

    /// <summary>An ISO 9660 directory record: its extent and size both ways round, its flags and name.</summary>
    static byte[] Record(string name, int extent, int size, bool folder)
    {
        var id = name == "." ? new byte[] { 0 } : name == ".." ? new byte[] { 1 } : Encoding.ASCII.GetBytes(name + (folder ? "" : ";1"));
        var length = 33 + id.Length + (id.Length % 2 == 0 ? 1 : 0);
        var r = new byte[length];
        r[0] = (byte)length;
        BitConverter.GetBytes(extent).CopyTo(r, 2);
        BitConverter.GetBytes(extent).Reverse().ToArray().CopyTo(r, 6);
        BitConverter.GetBytes(size).CopyTo(r, 10);
        BitConverter.GetBytes(size).Reverse().ToArray().CopyTo(r, 14);
        r[25] = (byte)(folder ? 2 : 0);
        r[32] = (byte)id.Length;
        id.CopyTo(r, 33);
        return r;
    }

    static void Be16(byte[] b, int at, int v)
    {
        b[at] = (byte)(v >> 8);
        b[at + 1] = (byte)v;
    }

    static void Be32(byte[] b, int at, long v)
    {
        for (var i = 0; i < 4; i++) b[at + i] = (byte)(v >> (24 - 8 * i));
    }

    /// <summary>
    /// A disc image of one title set with one title of two chapters: two cells, 15 s and 25 s, over
    /// the ten sectors of VTS_01_1.VOB, each sector filled with its own number.
    /// </summary>
    static byte[] Dvd()
    {
        var iso = new byte[35 * Sector];
        // The primary volume descriptor, its root at sector 18; then the terminator.
        iso[16 * Sector] = 1;
        Encoding.ASCII.GetBytes("CD001").CopyTo(iso, 16 * Sector + 1);
        Record(".", 18, Sector, true).CopyTo(iso, 16 * Sector + 156);
        iso[17 * Sector] = 255;
        Encoding.ASCII.GetBytes("CD001").CopyTo(iso, 17 * Sector + 1);
        var root = Record(".", 18, Sector, true).Concat(Record("..", 18, Sector, true)).Concat(Record("VIDEO_TS", 19, Sector, true)).ToArray();
        root.CopyTo(iso, 18 * Sector);
        var files = Record(".", 19, Sector, true).Concat(Record("..", 18, Sector, true))
            .Concat(Record("VIDEO_TS.IFO", 20, 2 * Sector, false))
            .Concat(Record("VTS_01_0.IFO", 22, 3 * Sector, false))
            .Concat(Record("VTS_01_1.VOB", 25, 10 * Sector, false)).ToArray();
        files.CopyTo(iso, 19 * Sector);

        // VIDEO_TS.IFO: its title search table in its second sector, one title in title set 1.
        var vmg = 20 * Sector;
        Encoding.ASCII.GetBytes("DVDVIDEO-VMG").CopyTo(iso, vmg);
        Be32(iso, vmg + 0xc4, 1);
        var tt = vmg + Sector;
        Be16(iso, tt, 1);
        Be32(iso, tt + 4, 8 + 12 - 1);
        iso[tt + 8 + 1] = 1;
        Be16(iso, tt + 8 + 2, 2);
        iso[tt + 8 + 6] = 1;
        iso[tt + 8 + 7] = 1;

        // VTS_01_0.IFO: MPEG-2 NTSC 720x480, English AC-3 5.1; its chapters in sector 1, its chains in sector 2.
        var vts = 22 * Sector;
        Encoding.ASCII.GetBytes("DVDVIDEO-VTS").CopyTo(iso, vts);
        Be32(iso, vts + 0xc8, 1);
        Be32(iso, vts + 0xcc, 2);
        iso[vts + 0x200] = 0x40;
        Be16(iso, vts + 0x202, 1);
        iso[vts + 0x204] = 0x04;
        iso[vts + 0x205] = 0x05;
        Encoding.ASCII.GetBytes("en").CopyTo(iso, vts + 0x206);
        var ptt = vts + Sector;
        Be16(iso, ptt, 1);
        Be32(iso, ptt + 4, 12 + 8 - 1);
        Be32(iso, ptt + 8, 12);
        Be16(iso, ptt + 12, 1);
        Be16(iso, ptt + 14, 1);
        Be16(iso, ptt + 16, 1);
        Be16(iso, ptt + 18, 2);
        var pgcit = vts + 2 * Sector;
        Be16(iso, pgcit, 1);
        Be32(iso, pgcit + 8, 0x81000000);
        Be32(iso, pgcit + 12, 16);
        var pgc = pgcit + 16;
        iso[pgc + 2] = 2;
        iso[pgc + 3] = 2;
        // 00:00:40, 0 frames, 30 fps.
        iso[pgc + 6] = 0x40;
        iso[pgc + 7] = 0xc0;
        iso[pgc + 0x0c] = 0x80;
        Be16(iso, pgc + 0xe6, 0xec);
        Be16(iso, pgc + 0xe8, 0xee);
        iso[pgc + 0xec] = 1;
        iso[pgc + 0xed] = 2;
        var cells = pgc + 0xee;
        iso[cells + 6] = 0x15;
        iso[cells + 7] = 0xc0;
        Be32(iso, cells + 20, 3);
        iso[cells + 24 + 6] = 0x25;
        iso[cells + 24 + 7] = 0xc0;
        Be32(iso, cells + 24 + 8, 4);
        Be32(iso, cells + 24 + 20, 9);
        for (var s = 0; s < 10; s++) Enumerable.Repeat((byte)(s + 1), Sector).ToArray().CopyTo(iso, (25 + s) * Sector);
        return iso;
    }

    static Func<long, int, byte[]> Reader(byte[] file) => (offset, count) => file.Skip((int)offset).Take(count).ToArray();

    [Fact]
    public void A_disc_images_VIDEO_TS_files_are_found_in_its_ISO_9660_directory()
    {
        var files = DiscImage.VideoTs(Reader(Dvd()))!;
        Assert.Equal(new[] { "VIDEO_TS.IFO", "VTS_01_0.IFO", "VTS_01_1.VOB" }, files.Keys.OrderBy(k => k));
        Assert.Equal((25L * Sector, 10L * Sector), (files["VTS_01_1.VOB"].Offset, files["VTS_01_1.VOB"].Length));
    }

    [Fact]
    public void A_DVDs_title_has_its_length_chapters_audio_and_cells()
    {
        var title = DvdVideo.Titles(Reader(Dvd())).Single();
        Assert.Equal((1, 40_000L), (title.Number, title.DurationMs));
        Assert.Equal(new[] { (0L, 15_000L), (15_000L, 40_000L) }, title.Chapters.Select(c => (c.StartMs, c.EndMs)));
        var video = title.Streams[0];
        Assert.Equal(("mpeg2video", 720, 480), (video.Codec, video.Width ?? 0, video.Height ?? 0));
        var audio = title.Streams.Single(s => s.Kind == "audio");
        Assert.Equal(("ac3", "en", 6), (audio.Codec, audio.Language, audio.Channels ?? 0));
        // Both cells, back to back in the VOB: one stretch of the image.
        Assert.Equal(new[] { new DiscSpan(25L * Sector, 10L * Sector) }, title.Spans);
    }

    [Fact]
    public void An_image_that_isnt_a_DVD_has_no_titles()
    {
        Assert.Empty(DvdVideo.Titles(Reader(new byte[40 * Sector])));
    }

    /// <summary>A pack sector: the MPEG-2 pack header, then PES packets (stream, PTS in 90 kHz or none, payload), padded to 2048.</summary>
    static byte[] Pack(params (int Stream, long? Pts, byte[] Payload)[] packets)
    {
        var b = new byte[Sector];
        new byte[] { 0, 0, 1, 0xba, 0x44, 0, 4, 0, 4, 1, 1, 0x89, 0xc3, 0xf8 }.CopyTo(b, 0);
        var at = 14;
        foreach (var (stream, pts, payload) in packets)
        {
            var header = pts == null ? new byte[] { 0x81, 0x00, 0 } : new byte[] { 0x81, 0x80, 5, (byte)(0x21 | ((pts.Value >> 29) & 0x0e)), (byte)(pts.Value >> 22), (byte)(((pts.Value >> 14) & 0xfe) | 1), (byte)(pts.Value >> 7), (byte)(((pts.Value << 1) & 0xfe) | 1) };
            var length = header.Length + payload.Length;
            new byte[] { 0, 0, 1, (byte)stream, (byte)(length >> 8), (byte)length }.CopyTo(b, at);
            header.CopyTo(b, at + 6);
            payload.CopyTo(b, at + 6 + header.Length);
            at += 6 + length;
        }
        return b;
    }

    [Fact]
    public void A_DVD_titles_subtitles_are_read_out_of_its_packs_timed_from_their_cell()
    {
        // Cell 2 starts 15 s into the title; its first picture's PTS is 900000 (10 s).
        // A subtitle unit of 8 bytes, its size first, in two packets: the first with the PTS 1080000 (12 s).
        var unit = new byte[] { 0, 8, 0, 4, 0xaa, 0xbb, 0xcc, 0xdd };
        var image = Pack((0xe0, 900_000, new byte[] { 0, 0, 1, 0xb3 }))
            .Concat(Pack((0xbd, 1_080_000, new byte[] { 0x21 }.Concat(unit.Take(5)).ToArray()), (0xbd, 900_000, new byte[] { 0x20, 9, 9 })))
            .Concat(Pack((0xbd, null, new byte[] { 0x21 }.Concat(unit.Skip(5)).ToArray()))).ToArray();
        var title = new DvdTitle(1, 60_000, new Pley.Tv.Core.Api.Chapter[0], new Pley.Tv.Core.Api.MediaStream[0], new[] { new DiscSpan(0, image.Length) })
        {
            Cells = new[] { new DvdCell(0, new DiscSpan[0]), new DvdCell(15_000, new[] { new DiscSpan(0, image.Length) }) },
        };
        var found = new System.Collections.Generic.List<SubtitleBlock>();
        DvdSubtitles.Read(new MemoryStream(image), title, 1, found.Add);
        // Stream 1 (0x21) only: stream 0's packet isn't its.
        var block = Assert.Single(found);
        Assert.Equal(15_000 + 2_000, block.StartMs);
        Assert.Equal(unit, block.Data);
        // Read from the cell the film is in: the same unit, whichever cell comes first.
        found.Clear();
        DvdSubtitles.Read(new MemoryStream(image), title, 1, found.Add, fromMs: 20_000);
        Assert.Equal(17_000, Assert.Single(found).StartMs);
    }

    [Fact]
    public async Task Stretches_of_a_file_are_served_as_one_with_ranges()
    {
        var path = Path.Combine(Path.GetTempPath(), "pley-spans-" + Guid.NewGuid().ToString("N"));
        File.WriteAllBytes(path, Enumerable.Range(0, 100).Select(i => (byte)i).ToArray());
        using var server = new SpanServer();
        try
        {
            // Bytes 10-19, then 50-59: the second span's first byte is the served file's 10th.
            var url = server.Serve(path, new[] { new DiscSpan(10, 10), new DiscSpan(50, 10) });
            using var http = new HttpClient();
            Assert.Equal(Enumerable.Range(10, 10).Concat(Enumerable.Range(50, 10)).Select(i => (byte)i), await http.GetByteArrayAsync(url));
            var ask = new HttpRequestMessage(HttpMethod.Get, url);
            ask.Headers.Range = new System.Net.Http.Headers.RangeHeaderValue(8, 12);
            var answer = await http.SendAsync(ask);
            Assert.Equal(System.Net.HttpStatusCode.PartialContent, answer.StatusCode);
            Assert.Equal("bytes 8-12/20", answer.Content.Headers.ContentRange!.ToString());
            Assert.Equal(new byte[] { 18, 19, 50, 51, 52 }, await answer.Content.ReadAsByteArrayAsync());
            // Another path, or after it stops: nothing.
            Assert.Equal(System.Net.HttpStatusCode.NotFound, (await http.GetAsync(url.Replace("title.mpg", "other.mpg"))).StatusCode);
        }
        finally
        {
            File.Delete(path);
        }
    }
}
