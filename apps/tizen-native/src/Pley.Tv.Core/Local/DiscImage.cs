using System;
using System.Collections.Generic;
using System.Text;

namespace Pley.Tv.Core.Local;

/// <summary>A file inside a disc image: where its bytes start in the image, and how many.</summary>
public sealed record DiscFile(string Name, long Offset, long Length);

/// <summary>
/// A DVD's disc image (.iso) read without mounting it, which the TV doesn't let an app do: its
/// ISO 9660 directory, which a DVD-Video image carries beside UDF (the "UDF bridge"), gives the
/// files of VIDEO_TS and where each lies in the image.
/// </summary>
public static class DiscImage
{
    public const int Sector = 2048;

    /// <summary>The most of a directory read: VIDEO_TS has a few dozen entries.</summary>
    const int DirectoryMost = 256 * 1024;

    /// <summary>
    /// VIDEO_TS's files, by name ("VTS_01_1.VOB"), in the image <paramref name="read"/> gives
    /// (offset, count); null when it isn't an ISO 9660 image or has no VIDEO_TS.
    /// </summary>
    public static IReadOnlyDictionary<string, DiscFile>? VideoTs(Func<long, int, byte[]> read)
    {
        // Volume descriptors from sector 16 until the terminator; the primary one has the root.
        for (var sector = 16; sector < 32; sector++)
        {
            var d = read((long)sector * Sector, Sector);
            if (d.Length < 190 || Encoding.ASCII.GetString(d, 1, 5) != "CD001") return null;
            if (d[0] == 255) return null;
            if (d[0] != 1) continue;
            var root = Record(d, 156);
            if (root == null) return null;
            var top = Directory(read, root);
            if (!top.TryGetValue("VIDEO_TS", out var videoTs) || !videoTs.Folder) return null;
            var files = new Dictionary<string, DiscFile>(StringComparer.OrdinalIgnoreCase);
            foreach (var entry in Directory(read, videoTs).Values)
                if (!entry.Folder) files[entry.Name] = new DiscFile(entry.Name, entry.Extent * Sector, entry.Length);
            return files;
        }
        return null;
    }

    sealed record Entry(string Name, long Extent, long Length, bool Folder);

    static Dictionary<string, Entry> Directory(Func<long, int, byte[]> read, Entry dir)
    {
        var entries = new Dictionary<string, Entry>(StringComparer.OrdinalIgnoreCase);
        var bytes = read(dir.Extent * Sector, (int)Math.Min(dir.Length, DirectoryMost));
        var at = 0;
        while (at < bytes.Length)
        {
            var length = bytes[at];
            // Records don't cross sectors: a zero pads to the next one.
            if (length == 0)
            {
                at = (at / Sector + 1) * Sector;
                continue;
            }
            if (Record(bytes, at) is { } e && e.Name != "." && e.Name != "..") entries[e.Name] = e;
            at += length;
        }
        return entries;
    }

    /// <summary>A directory record: its extent and size (both-endian; the little-endian half read), flags and name.</summary>
    static Entry? Record(byte[] b, int at)
    {
        if (at + 33 > b.Length) return null;
        var length = b[at];
        if (length < 34 || at + length > b.Length) return null;
        var extent = Le32(b, at + 2);
        var size = Le32(b, at + 10);
        var folder = (b[at + 25] & 2) != 0;
        int nameLength = b[at + 32];
        string name;
        if (nameLength == 1 && b[at + 33] == 0) name = ".";
        else if (nameLength == 1 && b[at + 33] == 1) name = "..";
        else
        {
            name = Encoding.ASCII.GetString(b, at + 33, Math.Min(nameLength, length - 33));
            var version = name.IndexOf(';');
            if (version >= 0) name = name.Substring(0, version);
            name = name.TrimEnd('.');
        }
        return new Entry(name, extent, size, folder);
    }

    static long Le32(byte[] b, int at) => b[at] | ((long)b[at + 1] << 8) | ((long)b[at + 2] << 16) | ((long)b[at + 3] << 24);
}
