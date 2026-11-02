using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;

namespace Pley.Tv.Core.Local;

/// <summary>A folder, a video or a disc image (<paramref name="Disc"/>: a DVD's .iso) in a folder of a drive.</summary>
public sealed record LocalEntry(string Path, string Name, bool Folder, long Size = 0, bool Disc = false);

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

    /// <summary>A disc image: a DVD's titles play from it (<see cref="DvdVideo"/>).</summary>
    public static bool IsDisc(string name) => string.Equals(Path.GetExtension(name), ".iso", StringComparison.OrdinalIgnoreCase);

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
            if (!Shown(name) || !IsVideo(name) && !IsDisc(name)) continue;
            long size = 0;
            try
            {
                size = new FileInfo(path).Length;
            }
            catch (Exception)
            {
                // Its size is only shown.
            }
            videos.Add(new LocalEntry(path, name, false, size, IsDisc(name)));
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

    /// <summary>A subtitle file's text: UTF-8 when it is, else Latin-1, close to the Western code page older SubRip files are in.</summary>
    public static string Decode(byte[] bytes)
    {
        try
        {
            return new UTF8Encoding(false, true).GetString(bytes);
        }
        catch (DecoderFallbackException)
        {
            var chars = new char[bytes.Length];
            for (var i = 0; i < bytes.Length; i++) chars[i] = (char)bytes[i];
            return new string(chars);
        }
    }
}
