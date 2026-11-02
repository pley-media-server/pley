using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;

namespace Pley.Tv.Core.Local;

/// <summary>Where a program chain plays: before any menu, the disc's own menus, a title set's menus, or a title.</summary>
public enum DvdDomain
{
    FirstPlay,
    VmgMenu,
    VtsMenu,
    Title,
}

/// <summary>A cell of a program chain: its sectors (in its domain's VOBs), its still time and its command.</summary>
/// <param name="Still">Seconds it stays after it plays; 255: until a button moves on.</param>
/// <param name="Command">Its cell command, 1-based in the chain's cell commands; 0: none.</param>
/// <param name="AngleBlock">Of an angle block (1 first, 2 middle, 3 last); 0 not.</param>
public sealed record DvdPgcCell(long FirstSector, long LastSector, long PlaybackMs, int Still, int Command, int AngleBlock);

/// <summary>
/// A program chain (PGC): what plays and what runs. Its pre commands run as it starts, its cells
/// play in order (each perhaps with its own command), its post commands run as it ends; its
/// programs start at the cells its program map names. Next, previous and "go up" name chains in
/// the same table. Its palette colours its subpictures (menus' buttons, subtitles).
/// </summary>
public sealed record DvdPgc(
    IReadOnlyList<byte[]> Pre,
    IReadOnlyList<byte[]> Post,
    IReadOnlyList<byte[]> CellCommands,
    IReadOnlyList<int> ProgramCells,
    IReadOnlyList<DvdPgcCell> Cells,
    int Next,
    int Previous,
    int GoUp,
    int Still,
    IReadOnlyList<uint> Palette)
{
    /// <summary>The program a cell (1-based) is in, 1-based.</summary>
    public int ProgramOf(int cell)
    {
        var program = 1;
        for (var p = 0; p < ProgramCells.Count; p++)
            if (ProgramCells[p] <= cell) program = p + 1;
        return program;
    }
}

/// <summary>A menu chain: its menu type (2 title, 3 root, 4 subpicture, 5 audio, 6 angle, 7 chapter; 0 none) and whether it is the entry for it.</summary>
public sealed record DvdMenuPgc(int Type, bool Entry, DvdPgc Pgc);

/// <summary>A title of the disc: its title set, its number there, and how many chapters (PTTs) it has.</summary>
public sealed record DvdTitleEntry(int Vts, int VtsTitle, int Chapters, int Angles);

/// <summary>A title set: its title chains, each title's chapters (chain and program), its menus, and its VOBs.</summary>
public sealed record DvdTitleSet(
    int Number,
    IReadOnlyList<DvdPgc> TitlePgcs,
    IReadOnlyList<IReadOnlyList<(int Pgcn, int Pgn)>> Chapters,
    IReadOnlyList<DvdMenuPgc> Menus,
    DiscFile? MenuVob,
    IReadOnlyList<DiscFile> TitleVobs)
{
    /// <summary>Its menus are 16:9 (anamorphic), else 4:3.</summary>
    public bool MenuWide { get; init; }
}

/// <summary>
/// A DVD-Video disc's navigation, read from its IFOs: the first-play chain, the disc's menus, its
/// titles, and each title set's chains and menus. What <see cref="DvdVm"/> runs. The first menu
/// language unit is taken (a disc's menus in other languages aren't offered).
/// </summary>
public sealed class DvdDisc
{
    const int Sector = DiscImage.Sector;
    const int IfoMost = 4 * 1024 * 1024;

    public DvdPgc? FirstPlay { get; init; }
    public IReadOnlyList<DvdMenuPgc> VmgMenus { get; init; } = new DvdMenuPgc[0];
    public IReadOnlyList<DvdTitleEntry> Titles { get; init; } = new DvdTitleEntry[0];
    public IReadOnlyDictionary<int, DvdTitleSet> Sets { get; init; } = new Dictionary<int, DvdTitleSet>();
    public DiscFile? VmgMenuVob { get; init; }
    /// <summary>The disc's menus are 16:9 (anamorphic), else 4:3.</summary>
    public bool VmgMenuWide { get; init; }
    /// <summary>A PAL disc (720×576), else NTSC (720×480).</summary>
    public bool Pal { get; init; }

    /// <summary>The disc in the image <paramref name="read"/> gives; null when it isn't DVD-Video.</summary>
    public static DvdDisc? Read(Func<long, int, byte[]> read)
    {
        var files = DiscImage.VideoTs(read);
        if (files == null || !files.TryGetValue("VIDEO_TS.IFO", out var vmgFile)) return null;
        var vmg = read(vmgFile.Offset, (int)Math.Min(vmgFile.Length, IfoMost));
        if (vmg.Length < 0x100 || Encoding.ASCII.GetString(vmg, 0, 12) != "DVDVIDEO-VMG") return null;

        var fp = (int)U32(vmg, 0x84);
        var firstPlay = fp > 0 && fp < vmg.Length ? Pgc(vmg, fp) : null;
        var vmgMenus = Menus(vmg, (int)U32(vmg, 0xc8) * Sector);

        var titles = new List<DvdTitleEntry>();
        var tt = (int)U32(vmg, 0xc4) * Sector;
        if (tt > 0 && tt + 8 <= vmg.Length)
            for (var i = 0; i < U16(vmg, tt); i++)
            {
                var at = tt + 8 + i * 12;
                if (at + 12 > vmg.Length) break;
                titles.Add(new DvdTitleEntry(vmg[at + 6], vmg[at + 7], U16(vmg, at + 2), vmg[at + 1]));
            }

        var sets = new Dictionary<int, DvdTitleSet>();
        foreach (var n in titles.Select(t => t.Vts).Distinct())
        {
            if (!files.TryGetValue($"VTS_{n:00}_0.IFO", out var ifo)) continue;
            var v = read(ifo.Offset, (int)Math.Min(ifo.Length, IfoMost));
            if (v.Length < 0x100 || Encoding.ASCII.GetString(v, 0, 12) != "DVDVIDEO-VTS") continue;
            var pgcs = new List<DvdPgc>();
            var pgcit = (int)U32(v, 0xcc) * Sector;
            if (pgcit > 0 && pgcit + 8 <= v.Length)
                for (var i = 0; i < U16(v, pgcit); i++)
                {
                    var at = pgcit + (int)U32(v, pgcit + 8 + i * 8 + 4);
                    pgcs.Add(at < v.Length ? Pgc(v, at) ?? Empty : Empty);
                }
            var chapters = new List<IReadOnlyList<(int, int)>>();
            var ptt = (int)U32(v, 0xc8) * Sector;
            if (ptt > 0 && ptt + 8 <= v.Length)
            {
                var count = U16(v, ptt);
                var last = ptt + (int)U32(v, ptt + 4) + 1;
                for (var t = 0; t < count; t++)
                {
                    var from = ptt + (int)U32(v, ptt + 8 + t * 4);
                    var to = t + 1 < count ? ptt + (int)U32(v, ptt + 8 + (t + 1) * 4) : last;
                    var list = new List<(int, int)>();
                    for (var at = from; at + 4 <= Math.Min(to, v.Length); at += 4) list.Add((U16(v, at), U16(v, at + 2)));
                    chapters.Add(list);
                }
            }
            files.TryGetValue($"VTS_{n:00}_0.VOB", out var menuVob);
            var vobs = new List<DiscFile>();
            for (var k = 1; k <= 9 && files.TryGetValue($"VTS_{n:00}_{k}.VOB", out var vob); k++) vobs.Add(vob);
            sets[n] = new DvdTitleSet(n, pgcs, chapters, Menus(v, (int)U32(v, 0xd0) * Sector), menuVob, vobs) { MenuWide = Wide(v[0x100]) };
        }
        files.TryGetValue("VIDEO_TS.VOB", out var vmgVob);
        return new DvdDisc
        {
            FirstPlay = firstPlay,
            VmgMenus = vmgMenus,
            Titles = titles,
            Sets = sets,
            VmgMenuVob = vmgVob,
            VmgMenuWide = Wide(vmg[0x100]),
            Pal = ((vmg[0x100] >> 4) & 3) == 1,
        };
    }

    /// <summary>Whether video attributes starting with <paramref name="attributes"/> say 16:9.</summary>
    static bool Wide(byte attributes) => ((attributes >> 2) & 3) == 3;

    static readonly DvdPgc Empty = new DvdPgc(new byte[0][], new byte[0][], new byte[0][], new int[0], new DvdPgcCell[0], 0, 0, 0, 0, new uint[16]);

    /// <summary>A menu table (PGCI_UT): its first language unit's chains with their menu types.</summary>
    static IReadOnlyList<DvdMenuPgc> Menus(byte[] b, int ut)
    {
        var menus = new List<DvdMenuPgc>();
        if (ut <= 0 || ut + 16 > b.Length || U16(b, ut) == 0) return menus;
        var lu = ut + (int)U32(b, ut + 8 + 4);
        if (lu + 8 > b.Length) return menus;
        for (var i = 0; i < U16(b, lu); i++)
        {
            var entry = lu + 8 + i * 8;
            if (entry + 8 > b.Length) break;
            var category = b[entry];
            var at = lu + (int)U32(b, entry + 4);
            if (at >= b.Length) continue;
            menus.Add(new DvdMenuPgc(category & 0x0f, (category & 0x80) != 0, Pgc(b, at) ?? Empty));
        }
        return menus;
    }

    /// <summary>A program chain at <paramref name="at"/>.</summary>
    static DvdPgc? Pgc(byte[] b, int at)
    {
        if (at + 0xec > b.Length) return null;
        int programs = b[at + 2], cellCount = b[at + 3];
        var palette = new uint[16];
        for (var i = 0; i < 16; i++) palette[i] = (uint)(U32(b, at + 0xa4 + i * 4) & 0xffffff);

        var pre = new List<byte[]>();
        var post = new List<byte[]>();
        var cellCommands = new List<byte[]>();
        var commands = U16(b, at + 0xe4);
        if (commands > 0 && at + commands + 8 <= b.Length)
        {
            var c = at + commands;
            int nPre = U16(b, c), nPost = U16(b, c + 2), nCell = U16(b, c + 4);
            byte[] Command(int k) => b.Skip(c + 8 + k * 8).Take(8).ToArray();
            for (var k = 0; k < nPre; k++) pre.Add(Command(k));
            for (var k = 0; k < nPost; k++) post.Add(Command(nPre + k));
            for (var k = 0; k < nCell; k++) cellCommands.Add(Command(nPre + nPost + k));
        }

        var map = U16(b, at + 0xe6);
        var programCells = new List<int>();
        for (var p = 0; p < programs && map > 0 && at + map + p < b.Length; p++) programCells.Add(b[at + map + p]);

        var cells = new List<DvdPgcCell>();
        var playback = U16(b, at + 0xe8);
        for (var k = 0; k < cellCount && playback > 0; k++)
        {
            var c = at + playback + k * 24;
            if (c + 24 > b.Length) break;
            var angle = ((b[c] >> 4) & 3) == 1 ? b[c] >> 6 : 0;
            cells.Add(new DvdPgcCell(U32(b, c + 8), U32(b, c + 20), Bcd(b, c + 4), b[c + 2], b[c + 3], angle));
        }
        return new DvdPgc(pre, post, cellCommands, programCells, cells, U16(b, at + 0x9c), U16(b, at + 0x9e), U16(b, at + 0xa0), b[at + 0xa2], palette);
    }

    static long Bcd(byte[] b, int at)
    {
        int D(byte x) => (x >> 4) * 10 + (x & 15);
        var fps = (b[at + 3] >> 6) == 1 ? 25.0 : 29.97;
        return (D(b[at]) * 3600L + D(b[at + 1]) * 60L + D(b[at + 2])) * 1000 + (long)(D((byte)(b[at + 3] & 0x3f)) * 1000 / fps);
    }

    static int U16(byte[] b, int at) => at + 2 <= b.Length ? (b[at] << 8) | b[at + 1] : 0;

    static long U32(byte[] b, int at) => at + 4 <= b.Length ? ((long)b[at] << 24) | ((long)b[at + 1] << 16) | ((long)b[at + 2] << 8) | b[at + 3] : 0;
}
