using System;
using System.Collections.Generic;
using System.Linq;

namespace Pley.Tv.Core.Local;

/// <summary>What the disc's navigation wants played next.</summary>
public abstract record DvdStep
{
    /// <summary>
    /// Cell <paramref name="Cell"/> (1-based) of <paramref name="Pgc"/> in its domain: played, it
    /// is followed by <see cref="DvdVm.CellEnded"/> (after its still time, unless that is endless:
    /// then a button moves on). <paramref name="Button"/> is the button to highlight first.
    /// </summary>
    public sealed record Play(DvdDomain Domain, int Vts, int PgcNumber, DvdPgc Pgc, int Cell, int Button) : DvdStep;

    /// <summary>The disc's navigation ended (Exit, or nothing left to play).</summary>
    public sealed record Stop : DvdStep;
}

/// <summary>
/// A DVD's virtual machine, as the DVD-Video specification lays it out (written from its published
/// instruction tables, not from any player's code): 16 general registers (GPRM) and 24 system ones
/// (SPRM), program chains' pre, cell and post commands, buttons' commands, and the links, jumps and
/// calls between the disc's domains. It says what to play (<see cref="DvdStep"/>); the player plays
/// it and tells it when a cell ends or a button is pressed.
/// </summary>
public sealed class DvdVm
{
    readonly DvdDisc disc;
    readonly Random random = new Random();

    /// <summary>The general registers, GPRM 0-15.</summary>
    public ushort[] Gprm { get; } = new ushort[16];

    /// <summary>The system registers, SPRM 0-23, with the player's defaults.</summary>
    public ushort[] Sprm { get; } = new ushort[24];

    /// <summary>Told each chain the machine enters and each stream it sets: for following a disc's logic.</summary>
    public Action<string>? Trace { get; set; }

    public DvdDomain Domain { get; private set; }
    public int Vts { get; private set; }
    public int PgcNumber { get; private set; }
    public DvdPgc? Pgc { get; private set; }
    public int Cell { get; private set; }

    /// <summary>Where a CallSS left the title: RSM goes back there.</summary>
    (int Vts, int PgcNumber, int Cell, ushort[] Sprm)? resume;

    /// <summary>Commands run in one go before giving up: a disc's loop that never links.</summary>
    const int MostSteps = 10_000;

    public DvdVm(DvdDisc disc, string language = "en")
    {
        this.disc = disc;
        var lang = (ushort)((language[0] << 8) | language[1]);
        Sprm[0] = lang;
        Sprm[1] = 15;
        Sprm[2] = 62;
        Sprm[3] = 1;
        Sprm[4] = 1;
        Sprm[5] = 1;
        Sprm[7] = 1;
        Sprm[8] = 1 << 10;
        Sprm[12] = (ushort)('U' << 8 | 'S');
        Sprm[13] = 15;
        // 16:9 display, normal mode; plays Dolby, MPEG and PCM audio.
        Sprm[14] = 3 << 10;
        Sprm[15] = 0x5000;
        Sprm[16] = lang;
        Sprm[18] = lang;
        Sprm[20] = 0xff;
    }

    /// <summary>The disc starts: its first-play chain, else its title menu, else its first title.</summary>
    public DvdStep Start()
    {
        if (disc.FirstPlay is { } fp) return Enter(DvdDomain.FirstPlay, 0, 0, fp, 1, pre: true);
        return Do(new Jump(JumpKind.Ss, Space: 1, Menu: 2)) is DvdStep.Play play ? play : Do(new Jump(JumpKind.Tt, Ttn: 1));
    }

    /// <summary>The cell playing ended (or its still ran out): its command, then the next cell, or the chain's end.</summary>
    public DvdStep CellEnded()
    {
        if (Pgc == null) return new DvdStep.Stop();
        var cell = Pgc.Cells[Cell - 1];
        if (cell.Command > 0 && cell.Command <= Pgc.CellCommands.Count && Run(new[] { Pgc.CellCommands[cell.Command - 1] }) is { } act) return Do(act);
        return PlayCell(NextCell(Cell));
    }

    /// <summary>The chain playing ended as a whole (a title played through in Pley's player): its post commands, then its next chain.</summary>
    public DvdStep ChainEnded() => EndPgc();

    /// <summary>A menu's button was pressed: its command runs.</summary>
    public DvdStep Button(byte[] command, int number)
    {
        Sprm[8] = (ushort)(number << 10);
        return Run(new[] { command }) is { } act ? Do(act) : new DvdStep.Play(Domain, Vts, PgcNumber, Pgc!, Cell, Sprm[8] >> 10);
    }

    /// <summary>The remote's Menu key: the title set's root menu (in a title), else the disc's title menu.</summary>
    public DvdStep Menu()
    {
        if (Domain == DvdDomain.Title)
        {
            resume = (Vts, PgcNumber, Cell, (ushort[])Sprm.Clone());
            if (Do(new Jump(JumpKind.Ss, Space: 2, Menu: 3, Vts: Vts)) is DvdStep.Play root) return root;
        }
        return Do(new Jump(JumpKind.Ss, Space: 1, Menu: 2));
    }

    // ---- chains and cells

    DvdStep Enter(DvdDomain domain, int vts, int number, DvdPgc pgc, int cell, bool pre)
    {
        Domain = domain;
        Vts = vts;
        PgcNumber = number;
        Pgc = pgc;
        Cell = cell;
        Trace?.Invoke($"enters {domain} vts {vts} chain {number}: pre [{string.Join(" ", pgc.Pre.Select(c => BitConverter.ToString(c).Replace("-", "")))}], GPRM {string.Join(",", Gprm.Take(8))}");
        if (domain == DvdDomain.Title) Sprm[6] = (ushort)number;
        if (pre && Run(pgc.Pre) is { } act) return Do(act);
        return PlayCell(cell);
    }

    int NextCell(int cell)
    {
        var next = cell + 1;
        // The first angle of a block only: its others are skipped.
        while (Pgc != null && next <= Pgc.Cells.Count && Pgc.Cells[next - 1].AngleBlock > 1) next++;
        return next;
    }

    DvdStep PlayCell(int cell)
    {
        if (Pgc == null) return new DvdStep.Stop();
        if (cell < 1 || cell > Pgc.Cells.Count) return EndPgc();
        Cell = cell;
        if (Domain == DvdDomain.Title) Sprm[7] = (ushort)ChapterOf(cell);
        return new DvdStep.Play(Domain, Vts, PgcNumber, Pgc, cell, Math.Max(1, Sprm[8] >> 10));
    }

    /// <summary>The chain ended: its post commands, then its next chain, else nothing.</summary>
    DvdStep EndPgc()
    {
        if (Pgc == null) return new DvdStep.Stop();
        if (Run(Pgc.Post) is { } act) return Do(act);
        if (Pgc.Next > 0 && Chain(Domain, Vts, Pgc.Next) is { } next) return Enter(Domain, Vts, Pgc.Next, next, 1, pre: true);
        return new DvdStep.Stop();
    }

    /// <summary>The chapter of the title playing a cell is in.</summary>
    int ChapterOf(int cell)
    {
        if (!disc.Sets.TryGetValue(Vts, out var set) || Sprm[5] < 1 || Sprm[5] > set.Chapters.Count || Pgc == null) return 1;
        var program = Pgc.ProgramOf(cell);
        var chapters = set.Chapters[Sprm[5] - 1];
        var found = 1;
        for (var i = 0; i < chapters.Count; i++)
            if (chapters[i].Pgcn == PgcNumber && chapters[i].Pgn <= program) found = i + 1;
        return found;
    }

    /// <summary>Chain <paramref name="number"/> (1-based) of a domain's table: a title set's titles' chains, or its or the disc's menus.</summary>
    DvdPgc? Chain(DvdDomain domain, int vts, int number)
    {
        IReadOnlyList<DvdPgc> table = domain switch
        {
            DvdDomain.Title => disc.Sets.TryGetValue(vts, out var s) ? s.TitlePgcs : new DvdPgc[0],
            DvdDomain.VtsMenu => disc.Sets.TryGetValue(vts, out var m) ? m.Menus.Select(x => x.Pgc).ToList() : new DvdPgc[0],
            DvdDomain.VmgMenu => disc.VmgMenus.Select(x => x.Pgc).ToList(),
            _ => new DvdPgc[0],
        };
        return number >= 1 && number <= table.Count ? table[number - 1] : null;
    }

    // ---- links and jumps

    abstract record Act;

    /// <summary>A link within the domain: a sub-instruction (<paramref name="Sub"/>), or a PGC, PTT, program or cell.</summary>
    sealed record Link(int Sub = 0, int Pgcn = 0, int Pttn = 0, int Pgn = 0, int Cn = 0, int Button = 0) : Act;

    enum JumpKind { Exit, Tt, VtsTt, VtsPtt, Ss, CallSs }

    /// <summary>
    /// A jump: <paramref name="Space"/> for JumpSS and CallSS is 0 first play, 1 the disc's menu
    /// (<paramref name="Menu"/>), 2 a title set's menu, 3 the disc's chain <paramref name="Pgcn"/>.
    /// </summary>
    sealed record Jump(JumpKind Kind, int Ttn = 0, int Pttn = 0, int Space = 0, int Menu = 0, int Vts = 0, int Pgcn = 0, int ResumeCell = 0) : Act;

    DvdStep Do(Act act)
    {
        switch (act)
        {
            case Link l:
                if (l.Button > 0) Sprm[8] = (ushort)(l.Button << 10);
                return DoLink(l);
            case Jump j:
                return DoJump(j);
        }
        return new DvdStep.Stop();
    }

    DvdStep DoLink(Link l)
    {
        if (Pgc == null) return new DvdStep.Stop();
        if (l.Pgcn > 0) return Chain(Domain, Vts, l.Pgcn) is { } p ? Enter(Domain, Vts, l.Pgcn, p, 1, pre: true) : new DvdStep.Stop();
        if (l.Pttn > 0) return Chapter(Sprm[5], l.Pttn);
        if (l.Pgn > 0) return PlayCell(l.Pgn <= Pgc.ProgramCells.Count ? Pgc.ProgramCells[l.Pgn - 1] : int.MaxValue);
        if (l.Cn > 0) return PlayCell(l.Cn);
        var program = Pgc.ProgramOf(Cell);
        int ProgramCell(int pg) => pg >= 1 && pg <= Pgc.ProgramCells.Count ? Pgc.ProgramCells[pg - 1] : int.MaxValue;
        switch (l.Sub)
        {
            case 0x01: return PlayCell(Cell);
            case 0x02: return PlayCell(NextCell(Cell));
            case 0x03: return PlayCell(Math.Max(1, Cell - 1));
            case 0x05: return PlayCell(ProgramCell(program));
            case 0x06: return PlayCell(ProgramCell(program + 1));
            case 0x07: return PlayCell(ProgramCell(Math.Max(1, program - 1)));
            case 0x09: return PlayCell(1);
            case 0x0a: return Pgc.Next > 0 && Chain(Domain, Vts, Pgc.Next) is { } n ? Enter(Domain, Vts, Pgc.Next, n, 1, pre: true) : new DvdStep.Stop();
            case 0x0b: return Pgc.Previous > 0 && Chain(Domain, Vts, Pgc.Previous) is { } p ? Enter(Domain, Vts, Pgc.Previous, p, 1, pre: true) : new DvdStep.Stop();
            case 0x0c: return Pgc.GoUp > 0 && Chain(Domain, Vts, Pgc.GoUp) is { } u ? Enter(Domain, Vts, Pgc.GoUp, u, 1, pre: true) : new DvdStep.Stop();
            case 0x0d: return Run(Pgc.Post) is { } act ? Do(act) : new DvdStep.Stop();
            case 0x10: return Resume();
            default: return PlayCell(Cell);
        }
    }

    DvdStep DoJump(Jump j)
    {
        switch (j.Kind)
        {
            case JumpKind.Exit:
                return new DvdStep.Stop();
            case JumpKind.Tt:
                if (j.Ttn < 1 || j.Ttn > disc.Titles.Count) return new DvdStep.Stop();
                var title = disc.Titles[j.Ttn - 1];
                Sprm[4] = (ushort)j.Ttn;
                Vts = title.Vts;
                return Chapter(title.VtsTitle, 1);
            case JumpKind.VtsTt:
                return Chapter(j.Ttn, 1);
            case JumpKind.VtsPtt:
                return Chapter(j.Ttn, j.Pttn);
            case JumpKind.CallSs:
                if (Domain == DvdDomain.Title) resume = (Vts, PgcNumber, j.ResumeCell > 0 ? j.ResumeCell : Cell, (ushort[])Sprm.Clone());
                return Space(j);
            default:
                return Space(j);
        }
    }

    /// <summary>JumpSS and CallSS: first play, a menu of the disc's or of a title set's, or a chain of the disc's.</summary>
    DvdStep Space(Jump j)
    {
        switch (j.Space)
        {
            case 0:
                return disc.FirstPlay is { } fp ? Enter(DvdDomain.FirstPlay, 0, 0, fp, 1, pre: true) : new DvdStep.Stop();
            case 1:
            {
                var at = IndexOf(disc.VmgMenus, j.Menu);
                return at > 0 ? Enter(DvdDomain.VmgMenu, 0, at, disc.VmgMenus[at - 1].Pgc, 1, pre: true) : new DvdStep.Stop();
            }
            case 2:
            {
                var vts = j.Vts > 0 ? j.Vts : Vts;
                if (j.Ttn > 0) Sprm[5] = (ushort)j.Ttn;
                if (!disc.Sets.TryGetValue(vts, out var set)) return new DvdStep.Stop();
                var at = IndexOf(set.Menus, j.Menu);
                return at > 0 ? Enter(DvdDomain.VtsMenu, vts, at, set.Menus[at - 1].Pgc, 1, pre: true) : new DvdStep.Stop();
            }
            case 3:
                return Chain(DvdDomain.VmgMenu, 0, j.Pgcn) is { } pgc ? Enter(DvdDomain.VmgMenu, 0, j.Pgcn, pgc, 1, pre: true) : new DvdStep.Stop();
        }
        return new DvdStep.Stop();
    }

    /// <summary>The entry chain for a menu type, 1-based; else its first chain of that type; 0 none.</summary>
    static int IndexOf(IReadOnlyList<DvdMenuPgc> menus, int type)
    {
        for (var i = 0; i < menus.Count; i++)
            if (menus[i].Entry && menus[i].Type == type) return i + 1;
        for (var i = 0; i < menus.Count; i++)
            if (menus[i].Type == type) return i + 1;
        return 0;
    }

    /// <summary>Chapter <paramref name="pttn"/> of the title set's title <paramref name="vtsTitle"/>: its chain, from its program.</summary>
    DvdStep Chapter(int vtsTitle, int pttn)
    {
        if (!disc.Sets.TryGetValue(Vts, out var set) || vtsTitle < 1 || vtsTitle > set.Chapters.Count) return new DvdStep.Stop();
        var chapters = set.Chapters[vtsTitle - 1];
        if (pttn < 1 || pttn > chapters.Count) return new DvdStep.Stop();
        var (pgcn, pgn) = chapters[pttn - 1];
        var pgc = Chain(DvdDomain.Title, Vts, pgcn);
        if (pgc == null) return new DvdStep.Stop();
        Sprm[5] = (ushort)vtsTitle;
        Sprm[7] = (ushort)pttn;
        var ttn = disc.Titles.ToList().FindIndex(t => t.Vts == Vts && t.VtsTitle == vtsTitle);
        if (ttn >= 0) Sprm[4] = (ushort)(ttn + 1);
        var cell = pgn >= 1 && pgn <= pgc.ProgramCells.Count ? pgc.ProgramCells[pgn - 1] : 1;
        // The chain's pre commands run when it's entered at its start.
        return Enter(DvdDomain.Title, Vts, pgcn, pgc, cell, pre: pgn <= 1);
    }

    /// <summary>RSM: back to where CallSS left the title, its system registers as they were.</summary>
    DvdStep Resume()
    {
        if (resume is not { } r || Chain(DvdDomain.Title, r.Vts, r.PgcNumber) is not { } pgc) return new DvdStep.Stop();
        resume = null;
        for (var i = 4; i <= 7; i++) Sprm[i] = r.Sprm[i];
        Domain = DvdDomain.Title;
        Vts = r.Vts;
        PgcNumber = r.PgcNumber;
        Pgc = pgc;
        return PlayCell(r.Cell);
    }

    // ---- commands

    /// <summary>Runs <paramref name="commands"/> in order (Goto moves among them, Break ends them): the link or jump one asks for, else null.</summary>
    Act? Run(IReadOnlyList<byte[]> commands)
    {
        var line = 0;
        for (var steps = 0; line < commands.Count && steps < MostSteps; steps++)
        {
            var c = commands[line];
            var (act, go, stop) = Execute(c);
            if (act != null) return act;
            if (stop) return null;
            line = go > 0 ? go - 1 : line + 1;
        }
        return null;
    }

    /// <summary>One command: the link or jump it asks for, the line it goes to (1-based; 0 the next), or that it breaks.</summary>
    (Act? Act, int Goto, bool Break) Execute(byte[] b)
    {
        if (b.Length < 8) return (null, 0, false);
        var type = b[0] >> 5;
        switch (type)
        {
            case 0:
            {
                if (!Compare(b[1], b[3], b[4], b[5], (b[1] & 0x80) != 0)) return (null, 0, false);
                return (b[1] & 0x0f) switch
                {
                    1 => (null, b[7], false),
                    2 => (null, 0, true),
                    3 => (null, b[7], false),
                    _ => (null, 0, false),
                };
            }
            case 1:
            {
                var jump = (b[0] & 0x10) != 0;
                if (!jump)
                {
                    if (!Compare(b[1], b[3], b[4], b[5], (b[1] & 0x80) != 0)) return (null, 0, false);
                    return (LinkOf(b[1] & 0x0f, b[6], b[7]), 0, false);
                }
                // Jumps compare two registers, in bytes 6 and 7.
                if (((b[1] >> 4) & 7) != 0 && !Test((b[1] >> 4) & 7, Get(b[6]), Get(b[7]))) return (null, 0, false);
                return ((b[1] & 0x0f) switch
                {
                    1 => new Jump(JumpKind.Exit),
                    2 => new Jump(JumpKind.Tt, Ttn: b[5]),
                    3 => new Jump(JumpKind.VtsTt, Ttn: b[5]),
                    5 => new Jump(JumpKind.VtsPtt, Ttn: b[5], Pttn: ((b[2] & 3) << 8) | b[3]),
                    6 => new Jump(JumpKind.Ss, Space: b[5] >> 6, Menu: b[5] & 0x0f, Vts: b[4], Ttn: b[3], Pgcn: ((b[2] << 8) | b[3]) & 0x7fff),
                    8 => new Jump(JumpKind.CallSs, Space: b[5] >> 6, Menu: b[5] & 0x0f, Pgcn: ((b[2] << 8) | b[3]) & 0x7fff, ResumeCell: b[4]),
                    _ => (Act?)null,
                }, 0, false);
            }
            case 2:
            {
                var cmp = (b[1] >> 4) & 7;
                if (cmp != 0 && !Test(cmp, Get(b[6]), Get(b[7]))) return (null, 0, false);
                SetSystem(b);
                return (cmp == 0 ? LinkOf(b[1] & 0x0f, b[6], b[7]) : null, 0, false);
            }
            case 3:
            {
                var cmp = (b[1] >> 4) & 7;
                if (cmp != 0 && !Test(cmp, Get(b[2]), (b[1] & 0x80) != 0 ? (ushort)((b[6] << 8) | b[7]) : Get(b[7]))) return (null, 0, false);
                var direct = (b[0] & 0x10) != 0;
                Set(b[0] & 0x0f, b[3] & 0x0f, direct ? (ushort)((b[4] << 8) | b[5]) : Get(b[5]), direct ? -1 : b[5]);
                return (cmp == 0 ? LinkOf(b[1] & 0x0f, b[6], b[7]) : null, 0, false);
            }
            case 4:
            {
                // Set, then compare the register set with cr2, then link.
                var scr = b[1] & 0x0f;
                var setDirect = (b[0] & 0x10) != 0;
                Set(b[0] & 0x0f, scr, setDirect ? (ushort)((b[2] << 8) | b[3]) : Get(b[3]), setDirect ? -1 : b[3]);
                var cmpDirect = (b[1] & 0x80) != 0;
                if (!Test((b[1] >> 4) & 7, Gprm[scr], cmpDirect ? (ushort)((b[4] << 8) | b[5]) : Get(b[5]))) return (null, 0, false);
                return (new Link(Sub: b[7] & 0x1f, Button: b[6] >> 2), 0, false);
            }
            case 5:
            case 6:
            {
                // Compare cr1 with cr2; if so, set; then link (type 6 always, type 5 only if so).
                var setDirect = (b[0] & 0x10) != 0;
                var cmpDirect = (b[1] & 0x80) != 0;
                var cr1 = setDirect ? b[4] : b[3];
                var holds = Test((b[1] >> 4) & 7, Get(cr1), cmpDirect ? (ushort)((b[4] << 8) | b[5]) : Get(b[5]));
                if (holds) Set(b[0] & 0x0f, b[1] & 0x0f, setDirect ? (ushort)((b[2] << 8) | b[3]) : Get(b[2]), setDirect ? -1 : b[2]);
                if (!holds && type == 5) return (null, 0, false);
                return (new Link(Sub: b[7] & 0x1f, Button: b[6] >> 2), 0, false);
            }
        }
        return (null, 0, false);
    }

    /// <summary>The link a link command names (type 1, or a set's link), in its last two bytes.</summary>
    static Act? LinkOf(int code, byte b6, byte b7) => code switch
    {
        1 => (b7 & 0x1f) == 0 ? null : new Link(Sub: b7 & 0x1f, Button: b6 >> 2),
        4 => new Link(Pgcn: ((b6 << 8) | b7) & 0x7fff),
        5 => new Link(Pttn: ((b6 & 3) << 8) | b7, Button: b6 >> 2),
        6 => new Link(Pgn: b7 & 0x7f, Button: b6 >> 2),
        7 => new Link(Cn: b7, Button: b6 >> 2),
        _ => null,
    };

    /// <summary>A type 0 or link command's compare: register cr1 (byte 3) with register cr2 (byte 5) or the value in bytes 4-5.</summary>
    bool Compare(byte b1, byte cr1, byte b4, byte b5, bool direct)
    {
        var op = (b1 >> 4) & 7;
        if (op == 0) return true;
        return Test(op, Get(cr1), direct ? (ushort)((b4 << 8) | b5) : Get(b5));
    }

    static bool Test(int op, ushort a, ushort b) => op switch
    {
        1 => (a & b) != 0,
        2 => a == b,
        3 => a != b,
        4 => a >= b,
        5 => a > b,
        6 => a <= b,
        7 => a < b,
        _ => true,
    };

    /// <summary>A register's value: 0-15 the general ones, 0x80-0x97 the system ones.</summary>
    ushort Get(byte r) => r < 16 ? Gprm[r] : r >= 0x80 && r <= 0x97 ? Sprm[r - 0x80] : (ushort)0;

    /// <summary>A set operation on general register <paramref name="d"/>; <paramref name="from"/> is the register the value came from (swap), -1 a value.</summary>
    void Set(int op, int d, ushort v, int from)
    {
        var a = Gprm[d];
        switch (op)
        {
            case 1: Gprm[d] = v; break;
            case 2:
                Gprm[d] = v;
                if (from >= 0 && from < 16) Gprm[from] = a;
                break;
            case 3: Gprm[d] = (ushort)Math.Min(0xffff, a + v); break;
            case 4: Gprm[d] = (ushort)Math.Max(0, a - v); break;
            case 5: Gprm[d] = (ushort)Math.Min(0xffff, a * v); break;
            case 6: Gprm[d] = v == 0 ? (ushort)0xffff : (ushort)(a / v); break;
            case 7: Gprm[d] = v == 0 ? (ushort)0xffff : (ushort)(a % v); break;
            case 8: Gprm[d] = v == 0 ? (ushort)0 : (ushort)random.Next(1, v + 1); break;
            case 9: Gprm[d] = (ushort)(a & v); break;
            case 10: Gprm[d] = (ushort)(a | v); break;
            case 11: Gprm[d] = (ushort)(a ^ v); break;
        }
    }

    /// <summary>Type 2: the system registers set (streams, navigation timer, counter mode, karaoke, highlighted button).</summary>
    void SetSystem(byte[] b)
    {
        var direct = (b[0] & 0x10) != 0;
        switch (b[0] & 0x0f)
        {
            case 1:
                // Audio, subpicture, angle: each when its flag is set, from a register or a value.
                if ((b[3] & 0x80) != 0) Sprm[1] = direct ? (ushort)(b[3] & 0x7f) : Gprm[b[3] & 0x0f];
                if ((b[4] & 0x80) != 0) Sprm[2] = direct ? (ushort)(b[4] & 0x7f) : Gprm[b[4] & 0x0f];
                if ((b[5] & 0x80) != 0) Sprm[3] = direct ? (ushort)(b[5] & 0x7f) : Gprm[b[5] & 0x0f];
                Trace?.Invoke($"SetSTN {BitConverter.ToString(b).Replace("-", "")}: audio {Sprm[1]}, subpicture {Sprm[2]:x}");
                break;
            case 2:
                Sprm[9] = direct ? (ushort)((b[2] << 8) | b[3]) : Get(b[3]);
                Sprm[10] = (ushort)((b[4] << 8) | b[5]);
                break;
            case 3:
                Gprm[b[5] & 0x0f] = direct ? (ushort)((b[2] << 8) | b[3]) : Get(b[3]);
                break;
            case 4:
                Sprm[11] = direct ? (ushort)((b[4] << 8) | b[5]) : Gprm[b[5] & 0x0f];
                break;
            case 6:
                Sprm[8] = direct ? (ushort)((b[4] << 8) | b[5]) : Gprm[b[5] & 0x0f];
                break;
        }
    }
}
