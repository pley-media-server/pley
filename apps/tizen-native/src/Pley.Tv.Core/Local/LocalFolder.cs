using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;

namespace Pley.Tv.Core.Local;

/// <summary>A folder or a video in a folder of a drive.</summary>
public sealed record LocalEntry(string Path, string Name, bool Folder, long Size = 0);

/// <summary>A subtitle file beside a video: "Film.en.forced.srt" for "Film.mkv".</summary>
public sealed record Sidecar(string Path, string Codec, string? Language, string? Title, bool Forced);

/// <summary>What a drive's folders show, and the subtitle files beside a video.</summary>
public static class LocalFolder
{
    /// <summary>The containers the TV's player opens (<see cref="Player.TizenCaps"/>'s, less sound alone).</summary>
    static readonly HashSet<string> Videos = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
    {
        ".mkv", ".mp4", ".m4v", ".mov", ".ts", ".m2ts", ".mts", ".webm", ".avi", ".mpg", ".mpeg",
    };

    /// <summary>What Windows and macOS leave on a drive, never shown.</summary>
    static readonly HashSet<string> System = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
    {
        "System Volume Information", "$RECYCLE.BIN", "RECYCLER", "lost+found", "Thumbs.db", "desktop.ini",
    };

    public static bool IsVideo(string name) => Videos.Contains(Path.GetExtension(name));

    /// <summary>Hidden ones (a dot, a dollar) and the systems' own aren't shown.</summary>
    public static bool Shown(string name) => name.Length > 0 && name[0] != '.' && name[0] != '$' && !System.Contains(name);

    /// <summary>
    /// <paramref name="dir"/>'s folders, then its videos, each by name as a person sorts them
    /// ("Part 2" before "Part 10"); other files aren't shown. Throws as <see cref="Directory"/> does
    /// for a folder that can't be read.
    /// </summary>
    public static IReadOnlyList<LocalEntry> List(string dir)
    {
        var folders = new List<LocalEntry>();
        var videos = new List<LocalEntry>();
        foreach (var path in Directory.GetDirectories(dir))
        {
            var name = Path.GetFileName(path);
            if (Shown(name)) folders.Add(new LocalEntry(path, name, true));
        }
        foreach (var path in Directory.GetFiles(dir))
        {
            var name = Path.GetFileName(path);
            if (!Shown(name) || !IsVideo(name)) continue;
            long size = 0;
            try
            {
                size = new FileInfo(path).Length;
            }
            catch (Exception)
            {
                // Its size is only shown.
            }
            videos.Add(new LocalEntry(path, name, false, size));
        }
        folders.Sort((a, b) => Natural(a.Name, b.Name));
        videos.Sort((a, b) => Natural(a.Name, b.Name));
        return folders.Concat(videos).ToList();
    }

    /// <summary>Compares names by their runs of digits as numbers, the rest ignoring case.</summary>
    public static int Natural(string a, string b)
    {
        int i = 0, j = 0;
        while (i < a.Length && j < b.Length)
        {
            if (char.IsDigit(a[i]) && char.IsDigit(b[j]))
            {
                int si = i, sj = j;
                while (i < a.Length && char.IsDigit(a[i])) i++;
                while (j < b.Length && char.IsDigit(b[j])) j++;
                var x = a.Substring(si, i - si).TrimStart('0');
                var y = b.Substring(sj, j - sj).TrimStart('0');
                if (x.Length != y.Length) return x.Length.CompareTo(y.Length);
                var c = string.CompareOrdinal(x, y);
                if (c != 0) return c;
                continue;
            }
            var d = char.ToUpperInvariant(a[i]).CompareTo(char.ToUpperInvariant(b[j]));
            if (d != 0) return d;
            i++;
            j++;
        }
        return (a.Length - i).CompareTo(b.Length - j);
    }

    /// <summary>The title a video shows: its file name without the extension.</summary>
    public static string Title(string path) => Path.GetFileNameWithoutExtension(path);

    /// <summary>
    /// The subtitle files beside <paramref name="video"/> among <paramref name="names"/> (its
    /// folder's files): those named as it is, then a language ("en", "eng", "English"), "forced",
    /// "sdh", in any order. SubRip and WebVTT, which the app draws.
    /// </summary>
    public static IReadOnlyList<Sidecar> Sidecars(string video, IEnumerable<string> names)
    {
        var dir = Path.GetDirectoryName(video) ?? "";
        var stem = Path.GetFileNameWithoutExtension(video);
        var found = new List<Sidecar>();
        foreach (var name in names)
        {
            var ext = Path.GetExtension(name).ToLowerInvariant();
            var codec = ext == ".srt" ? "subrip" : ext == ".vtt" ? "webvtt" : null;
            if (codec == null || !name.StartsWith(stem, StringComparison.OrdinalIgnoreCase)) continue;
            var middle = name.Substring(stem.Length, name.Length - stem.Length - ext.Length);
            // "Film 2.srt" is Film 2's, not Film's.
            if (middle.Length > 0 && middle[0] != '.' && middle[0] != '_') continue;
            string? language = null;
            var forced = false;
            var titles = new List<string>();
            foreach (var part in middle.Split(new[] { '.', '_', ' ' }, StringSplitOptions.RemoveEmptyEntries))
            {
                var p = part.ToLowerInvariant();
                if (p == "forced") forced = true;
                else if (p is "sdh" or "cc" or "hi") titles.Add("SDH");
                else if (language == null && LanguageOf(p) is string l) language = l;
                else titles.Add(part);
            }
            found.Add(new Sidecar(Path.Combine(dir, name), codec, language, titles.Count > 0 ? string.Join(" ", titles) : null, forced));
        }
        found.Sort((a, b) => Natural(a.Path, b.Path));
        return found;
    }

    static readonly Dictionary<string, string> Names = new Dictionary<string, string>
    {
        ["english"] = "eng", ["french"] = "fre", ["spanish"] = "spa", ["german"] = "ger", ["italian"] = "ita",
        ["portuguese"] = "por", ["japanese"] = "jpn", ["korean"] = "kor", ["chinese"] = "chi", ["arabic"] = "ara",
        ["russian"] = "rus", ["dutch"] = "dut", ["swedish"] = "swe", ["hindi"] = "hin", ["turkish"] = "tur",
    };

    /// <summary>A language code ("en", "eng", "pt-br") or English name, as the code the player's labels read; null for anything else.</summary>
    static string? LanguageOf(string p)
    {
        if (Names.TryGetValue(p, out var code)) return code;
        return Regex.IsMatch(p, "^[a-z]{2,3}(-[a-z]{2})?$") ? p : null;
    }

    /// <summary>
    /// A SubRip file as WebVTT, as the server would serve it: the header, and its times with a full
    /// stop. A WebVTT file is kept as it is.
    /// </summary>
    public static string ToWebVtt(string text)
    {
        if (text.Length > 0 && text[0] == '﻿') text = text.Substring(1);
        text = text.Replace("\r\n", "\n").Replace('\r', '\n');
        if (text.TrimStart().StartsWith("WEBVTT", StringComparison.Ordinal)) return text;
        var times = Regex.Replace(text, @"(\d{1,2}:\d{2}:\d{2}),(\d{1,3})", "$1.$2");
        return "WEBVTT\n\n" + times;
    }

    /// <summary>
    /// A subtitle file's text, as in the Android app's DriveFolder.decode. A byte-order mark says
    /// its encoding: UTF-16 or UTF-32, as Windows tools save "Unicode" text (UTF-8's goes in
    /// <see cref="ToWebVtt"/>). Without one, UTF-16 shows by its NUL bytes, which text in UTF-8 or a
    /// code page never has; then UTF-8 when it is, else Windows-1252, the Western code page older
    /// SubRip files are in.
    /// </summary>
    public static string Decode(byte[] bytes)
    {
        bool Starts(params byte[] mark) => bytes.Length >= mark.Length && bytes.Take(mark.Length).SequenceEqual(mark);
        if (Starts(0xFF, 0xFE, 0, 0)) return Utf32(bytes, 4, littleEndian: true);
        if (Starts(0, 0, 0xFE, 0xFF)) return Utf32(bytes, 4, littleEndian: false);
        if (Starts(0xFF, 0xFE)) return Utf16(bytes, 2, littleEndian: true);
        if (Starts(0xFE, 0xFF)) return Utf16(bytes, 2, littleEndian: false);
        if (Utf16Order(bytes) is bool little) return Utf16(bytes, 0, little);
        try
        {
            return new UTF8Encoding(false, true).GetString(bytes);
        }
        catch (DecoderFallbackException)
        {
            var chars = new char[bytes.Length];
            for (var i = 0; i < bytes.Length; i++) chars[i] = bytes[i] is >= 0x80 and <= 0x9F ? Windows1252High[bytes[i] - 0x80] : (char)bytes[i];
            return new string(chars);
        }
    }

    /// <summary>Windows-1252's 0x80–0x9F: the euro, curly quotes, dashes and the like (the five it leaves out stay as Latin-1 has them).</summary>
    const string Windows1252High =
        "€\u0081‚ƒ„…†‡ˆ‰Š‹Œ\u008DŽ\u008F" +
        "\u0090‘’“”•–—˜™š›œ\u009DžŸ";

    /// <summary>
    /// Whether UTF-16 without its mark is little-endian, from its first 4 KB: where it is Latin
    /// text, times or numbers, every other byte is NUL, the high one. Null when it isn't UTF-16.
    /// </summary>
    static bool? Utf16Order(byte[] bytes)
    {
        var pairs = Math.Min(bytes.Length, 4096) / 2;
        int first = 0, second = 0;
        for (var i = 0; i < pairs; i++)
        {
            if (bytes[2 * i] == 0) first++;
            if (bytes[2 * i + 1] == 0) second++;
        }
        bool Many(int n) => pairs >= 8 && n * 4 >= pairs;
        if (Many(second) && first * 8 <= second) return true;
        if (Many(first) && second * 8 <= first) return false;
        return null;
    }

    /// <summary>UTF-16 text from <paramref name="from"/>; a lone surrogate reads as U+FFFD and an odd last byte is dropped.</summary>
    static string Utf16(byte[] bytes, int from, bool littleEndian)
    {
        var units = new char[(bytes.Length - from) / 2];
        for (var i = 0; i < units.Length; i++)
        {
            int a = bytes[from + 2 * i], b = bytes[from + 1 + 2 * i];
            units[i] = (char)(littleEndian ? a | (b << 8) : (a << 8) | b);
        }
        var text = new StringBuilder(units.Length);
        for (var i = 0; i < units.Length; i++)
        {
            if (char.IsHighSurrogate(units[i]) && i + 1 < units.Length && char.IsLowSurrogate(units[i + 1]))
            {
                text.Append(units[i]).Append(units[i + 1]);
                i++;
            }
            else
            {
                text.Append(char.IsSurrogate(units[i]) ? '�' : units[i]);
            }
        }
        return text.ToString();
    }

    /// <summary>UTF-32 text from <paramref name="from"/>; what isn't a character reads as U+FFFD, and a last word cut short is dropped.</summary>
    static string Utf32(byte[] bytes, int from, bool littleEndian)
    {
        var text = new StringBuilder();
        for (var at = from; at + 4 <= bytes.Length; at += 4)
        {
            uint point = littleEndian
                ? bytes[at] | ((uint)bytes[at + 1] << 8) | ((uint)bytes[at + 2] << 16) | ((uint)bytes[at + 3] << 24)
                : ((uint)bytes[at] << 24) | ((uint)bytes[at + 1] << 16) | ((uint)bytes[at + 2] << 8) | bytes[at + 3];
            text.Append(point is <= 0x10FFFF and not (>= 0xD800 and <= 0xDFFF) ? char.ConvertFromUtf32((int)point) : "�");
        }
        return text.ToString();
    }
}
