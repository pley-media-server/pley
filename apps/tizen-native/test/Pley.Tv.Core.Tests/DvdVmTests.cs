using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Local;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// A DVD's virtual machine on small discs made of chains: its commands written byte by byte from
/// the DVD-Video instruction tables, so each test also checks how the machine reads them.
/// </summary>
public class DvdVmTests
{
    // ---- commands, as the instruction tables lay them out

    /// <summary>JumpSS to the disc's menu <paramref name="menu"/> (2: the title menu).</summary>
    static byte[] JumpVmgMenu(int menu = 2) => new byte[] { 0x30, 0x06, 0, 0, 0, (byte)(0x40 | menu), 0, 0 };

    /// <summary>JumpSS to a title set's menu (3: root).</summary>
    static byte[] JumpVtsMenu(int vts, int menu = 3) => new byte[] { 0x30, 0x06, 0, 0, (byte)vts, (byte)(0x80 | menu), 0, 0 };

    /// <summary>CallSS to the title set's root menu, resuming at <paramref name="cell"/>.</summary>
    static byte[] CallVtsMenu(int cell, int menu = 3) => new byte[] { 0x30, 0x08, 0, 0, (byte)cell, (byte)(0x80 | menu), 0, 0 };

    static byte[] JumpTt(int ttn) => new byte[] { 0x30, 0x02, 0, 0, 0, (byte)ttn, 0, 0 };

    static byte[] LinkPgcn(int pgcn) => new byte[] { 0x20, 0x04, 0, 0, 0, 0, (byte)(pgcn >> 8), (byte)pgcn };

    static byte[] LinkCn(int cn) => new byte[] { 0x20, 0x07, 0, 0, 0, 0, 0, (byte)cn };

    static byte[] LinkSub(int sub, int button = 0) => new byte[] { 0x20, 0x01, 0, 0, 0, 0, (byte)(button << 2), (byte)sub };

    /// <summary>GPRM <paramref name="r"/> = <paramref name="value"/> (type 3, direct, mov).</summary>
    static byte[] Mov(int r, int value) => new byte[] { 0x71, 0x00, 0, (byte)r, (byte)(value >> 8), (byte)value, 0, 0 };

    /// <summary>if (GPRM <paramref name="r"/> == <paramref name="value"/>) goto <paramref name="line"/> (type 0, direct compare).</summary>
    static byte[] IfEqualGoto(int r, int value, int line) => new byte[] { 0x00, 0xa1, 0, (byte)r, (byte)(value >> 8), (byte)value, 0, (byte)line };

    static DvdPgcCell Cell(long first, long last, int still = 0, int command = 0) => new DvdPgcCell(first, last, 1_000, still, command, 0);

    static DvdPgc Chain(IReadOnlyList<DvdPgcCell> cells, byte[][]? pre = null, byte[][]? post = null, byte[][]? cellCommands = null, int[]? programs = null, int next = 0) =>
        new DvdPgc(pre ?? new byte[0][], post ?? new byte[0][], cellCommands ?? new byte[0][], programs ?? new[] { 1 }, cells, next, 0, 0, 0, new uint[16]);

    /// <summary>
    /// A disc that starts at its title menu (a still cell); title 1 has two chapters, a cell each,
    /// and goes back to the menu as it ends; title set 1 has a root menu.
    /// </summary>
    static DvdDisc Disc(DvdPgc? firstPlay = null, DvdPgc? titleMenu = null, DvdPgc? title = null)
    {
        var menu = titleMenu ?? Chain(new[] { Cell(0, 9, still: 255) });
        var film = title ?? Chain(new[] { Cell(0, 99), Cell(100, 199) }, post: new[] { JumpVmgMenu() }, programs: new[] { 1, 2 });
        var root = Chain(new[] { Cell(0, 4, still: 255) });
        return new DvdDisc
        {
            FirstPlay = firstPlay ?? Chain(new DvdPgcCell[0], pre: new[] { JumpVmgMenu() }),
            VmgMenus = new[] { new DvdMenuPgc(2, true, menu) },
            Titles = new[] { new DvdTitleEntry(1, 1, 2, 1) },
            Sets = new Dictionary<int, DvdTitleSet>
            {
                [1] = new DvdTitleSet(1, new[] { film }, new[] { new[] { (1, 1), (1, 2) } }, new[] { new DvdMenuPgc(3, true, root) }, null, new DiscFile[0]),
            },
        };
    }

    [Fact]
    public void The_disc_starts_at_its_first_play_which_goes_to_the_title_menu()
    {
        var vm = new DvdVm(Disc());
        var play = Assert.IsType<DvdStep.Play>(vm.Start());
        Assert.Equal((DvdDomain.VmgMenu, 1, 1), (play.Domain, play.PgcNumber, play.Cell));
        Assert.Equal(255, play.Pgc.Cells[0].Still);
    }

    [Fact]
    public void A_buttons_jump_plays_the_title_and_the_title_ends_back_at_the_menu()
    {
        var vm = new DvdVm(Disc());
        vm.Start();
        var title = Assert.IsType<DvdStep.Play>(vm.Button(JumpTt(1), 2));
        Assert.Equal((DvdDomain.Title, 1, 1, 1), (title.Domain, title.Vts, title.PgcNumber, title.Cell));
        Assert.Equal(1, vm.Sprm[4]);
        Assert.Equal(2, vm.Sprm[8] >> 10);
        // The second chapter, then the chain's end: its post command goes to the title menu.
        var second = Assert.IsType<DvdStep.Play>(vm.CellEnded());
        Assert.Equal((2, 2), (second.Cell, (int)vm.Sprm[7]));
        Assert.Equal(DvdDomain.VmgMenu, Assert.IsType<DvdStep.Play>(vm.CellEnded()).Domain);
    }

    [Fact]
    public void Registers_compares_and_gotos_run_as_the_commands_say()
    {
        // GPRM0 = 5; if GPRM0 == 5 goto 4; (3: JumpTT 1, skipped); 4: link to chain 2.
        var second = Chain(new[] { Cell(50, 59, still: 255) });
        var menu = Chain(new[] { Cell(0, 9, still: 255) }, pre: new[] { Mov(0, 5), IfEqualGoto(0, 5, 4), JumpTt(1), LinkPgcn(2) });
        var disc = Disc(titleMenu: menu);
        disc = new DvdDisc { FirstPlay = disc.FirstPlay, VmgMenus = new[] { new DvdMenuPgc(2, true, menu), new DvdMenuPgc(0, false, second) }, Titles = disc.Titles, Sets = disc.Sets };
        var vm = new DvdVm(disc);
        var play = Assert.IsType<DvdStep.Play>(vm.Start());
        Assert.Equal((DvdDomain.VmgMenu, 2), (play.Domain, play.PgcNumber));
        Assert.Equal(5, vm.Gprm[0]);
    }

    [Fact]
    public void A_cells_command_runs_as_it_ends()
    {
        // The menu's cell loops on itself.
        var menu = Chain(new[] { Cell(0, 9, command: 1) }, cellCommands: new[] { LinkCn(1) });
        var vm = new DvdVm(Disc(titleMenu: menu));
        vm.Start();
        var again = Assert.IsType<DvdStep.Play>(vm.CellEnded());
        Assert.Equal((DvdDomain.VmgMenu, 1), (again.Domain, again.Cell));
    }

    [Fact]
    public void Menu_in_a_title_opens_its_root_menu_and_resume_goes_back()
    {
        var vm = new DvdVm(Disc());
        vm.Start();
        vm.Button(JumpTt(1), 1);
        vm.CellEnded();
        var root = Assert.IsType<DvdStep.Play>(vm.Menu());
        Assert.Equal((DvdDomain.VtsMenu, 1), (root.Domain, root.Vts));
        var back = Assert.IsType<DvdStep.Play>(vm.Button(LinkSub(0x10), 1));
        Assert.Equal((DvdDomain.Title, 2), (back.Domain, back.Cell));
    }

    [Fact]
    public void CallSS_resumes_at_the_cell_it_names()
    {
        var film = Chain(new[] { Cell(0, 99, command: 1), Cell(100, 199), Cell(200, 299) }, cellCommands: new[] { CallVtsMenu(3) }, programs: new[] { 1, 2, 3 });
        var vm = new DvdVm(Disc(title: film));
        vm.Start();
        vm.Button(JumpTt(1), 1);
        Assert.Equal(DvdDomain.VtsMenu, Assert.IsType<DvdStep.Play>(vm.CellEnded()).Domain);
        Assert.Equal(3, Assert.IsType<DvdStep.Play>(vm.Button(LinkSub(0x10), 1)).Cell);
    }

    [Fact]
    public void SetCLnk_sets_compares_and_links()
    {
        // GPRM1 += 1; if GPRM1 == 1: link to the chain's top (type 4, direct set and compare).
        var setCLnk = new byte[] { 0x93, 0xa1, 0x00, 0x01, 0x00, 0x01, 0, 0x09 };
        var vm = new DvdVm(Disc());
        vm.Start();
        var top = Assert.IsType<DvdStep.Play>(vm.Button(setCLnk, 1));
        Assert.Equal((1, 1), ((int)vm.Gprm[1], top.Cell));
    }

    [Fact]
    public void SetSTN_chooses_the_audio_and_subpicture_streams()
    {
        // Audio 1, subpicture 2 with display on (0x40), angle left.
        var setStn = new byte[] { 0x51, 0x00, 0, 0x81, 0x80 | 0x42, 0x00, 0, 0 };
        var vm = new DvdVm(Disc());
        vm.Start();
        vm.Button(setStn, 1);
        Assert.Equal((1, 0x42, 1), ((int)vm.Sprm[1], (int)vm.Sprm[2], (int)vm.Sprm[3]));
    }

    [Fact]
    public void A_menus_buttons_are_read_from_its_navigation_pack()
    {
        var nav = new byte[2048];
        new byte[] { 0, 0, 1, 0xbf }.CopyTo(nav, 0x26);
        var p = 0x2d;
        nav[p + 0x61] = 1;
        nav[p + 0x71] = 2;
        nav[p + 0x74] = 1;
        // Scheme 1: selected, emphasis 2 colour 9, emphasis 1 colour 8, pattern 7, background 6.
        nav[p + 0x76] = 0x98;
        nav[p + 0x77] = 0x76;
        void Button(int i, int x1, int y1, int x2, int y2, int down, byte[] command)
        {
            var b = p + 0x8e + i * 18;
            nav[b] = (byte)(0x40 | (x1 >> 4));
            nav[b + 1] = (byte)(((x1 & 15) << 4) | (x2 >> 8));
            nav[b + 2] = (byte)x2;
            nav[b + 3] = (byte)(y1 >> 4);
            nav[b + 4] = (byte)(((y1 & 15) << 4) | (y2 >> 8));
            nav[b + 5] = (byte)y2;
            nav[b + 7] = (byte)down;
            command.CopyTo(nav, b + 10);
        }
        Button(0, 100, 300, 260, 340, 2, JumpTt(1));
        Button(1, 100, 360, 620, 400, 1, JumpVtsMenu(1));
        var hl = DvdButtons.Read(nav)!;
        Assert.Equal(2, hl.Buttons.Count);
        var play = hl.Buttons[0];
        Assert.Equal((100, 300, 260, 340, 2, 1), (play.X1, play.Y1, play.X2, play.Y2, play.Down, play.Scheme));
        Assert.Equal(JumpTt(1), play.Command);
        Assert.Equal((620, 400), (hl.Buttons[1].X2, hl.Buttons[1].Y2));
        Assert.Equal(new[] { 6, 7, 8, 9 }, hl.Schemes[0].SelectedColors);
        Assert.Equal(1, hl.ForcedSelect);
    }

    // ---- a menu's buttons on screen

    /// <summary>
    /// A 2×2 subpicture unit at (100, 200): every pixel code 1, which the unit colours with
    /// palette entry 5, invisible (contrast 0), as a menu's buttons are until highlighted.
    /// </summary>
    static byte[] ButtonUnit() => new byte[]
    {
        0x00, 0x1e, 0x00, 0x06,
        // Each line: two pixels of code 1 (run 2: 0b10_01), then padding to the byte.
        0x90, 0x90,
        // Its control sequence: no delay, the last one (it points at itself).
        0x00, 0x00, 0x00, 0x06,
        0x03, 0x00, 0x50,
        0x04, 0x00, 0x00,
        0x05, 0x06, 0x40, 0x65, 0x0c, 0x80, 0xc9,
        0x06, 0x00, 0x04, 0x00, 0x05,
        0x01, 0xff,
    };

    /// <summary>A highlight whose button 1 covers (100, 200)-(100, 201), scheme 1 showing code 1 in palette entry 9, opaque; button 2 under it.</summary>
    static DvdHighlight Highlight()
    {
        var scheme = new DvdColourScheme(new[] { 0, 9, 0, 0 }, new[] { 0, 15, 0, 0 }, new[] { 0, 10, 0, 0 }, new[] { 0, 15, 0, 0 });
        return new DvdHighlight(new[]
        {
            new DvdButton(1, 100, 200, 100, 201, 0, 2, 0, 0, 1, false, JumpTt(1)),
            new DvdButton(2, 100, 300, 160, 340, 1, 0, 0, 0, 1, false, JumpVtsMenu(1)),
        }, new[] { scheme, scheme, scheme }, 1, 0);
    }

    [Fact]
    public void The_selected_button_takes_its_schemes_colours_and_the_rest_stays_as_the_unit_says()
    {
        var unit = ButtonUnit();
        var menu = new DvdMenuPicture(Highlight(), unit, Pley.Tv.Core.Subtitles.Pictures.VobSubDecoder.ParseSpu(unit));
        var palette = new uint[16];
        // Entry 9 white (Y 235), entry 10 black (Y 16), as Y, Cr, Cb.
        palette[9] = (235u << 16) | (128 << 8) | 128;
        palette[10] = (16u << 16) | (128 << 8) | 128;
        var lit = DvdMenu.Draw(menu, palette, 1)!;
        Assert.Equal((100, 200, 2, 2), (lit.X, lit.Y, lit.Width, lit.Height));
        // The button's column: white, opaque; the column beside it: as the unit, unseen.
        Assert.Equal(new byte[] { 235, 235, 235, 255 }, lit.Rgba[0..4]);
        Assert.Equal(new byte[] { 235, 235, 235, 255 }, lit.Rgba[8..12]);
        Assert.Equal(0, lit.Rgba[7]);
        // Pressed: its action colours.
        Assert.Equal(new byte[] { 16, 16, 16, 255 }, DvdMenu.Draw(menu, palette, 1, pressed: true)!.Rgba[0..4]);
        // Another button selected: nothing of this one shows.
        Assert.All(Enumerable.Range(0, 4), i => Assert.Equal(0, DvdMenu.Draw(menu, palette, 2)!.Rgba[i * 4 + 3]));
    }

    [Fact]
    public void The_remote_moves_between_buttons_as_the_disc_links_them()
    {
        var hl = Highlight();
        Assert.Equal(2, DvdMenu.Move(hl, 1, Pley.Tv.Core.Ui.Direction.Down));
        Assert.Equal(1, DvdMenu.Move(hl, 2, Pley.Tv.Core.Ui.Direction.Up));
        // No link that way: it stays.
        Assert.Equal(1, DvdMenu.Move(hl, 1, Pley.Tv.Core.Ui.Direction.Left));
    }

    [Fact]
    public void A_menu_cells_buttons_and_their_picture_are_read_from_its_packs()
    {
        // A VOB of two sectors: the navigation pack with the buttons, then the unit in a private stream 1 packet.
        var vob = new byte[2 * DiscImage.Sector];
        new byte[] { 0, 0, 1, 0xba }.CopyTo(vob, 0);
        new byte[] { 0, 0, 1, 0xbf }.CopyTo(vob, 0x26);
        var p = 0x2d;
        vob[p + 0x61] = 1;
        vob[p + 0x71] = 1;
        vob[p + 0x74] = 1;
        var b = p + 0x8e;
        vob[b] = 0x40 | (100 >> 4);
        vob[b + 1] = (byte)((100 & 15) << 4);
        vob[b + 2] = 100;
        vob[b + 3] = 200 >> 4;
        vob[b + 4] = (byte)((200 & 15) << 4);
        vob[b + 5] = 201;
        JumpTt(1).CopyTo(vob, b + 10);
        var s = DiscImage.Sector;
        new byte[] { 0, 0, 1, 0xba }.CopyTo(vob, s);
        var unit = ButtonUnit();
        var pes = s + 14;
        new byte[] { 0, 0, 1, 0xbd, 0, (byte)(3 + 5 + 1 + unit.Length), 0x81, 0x80, 5, 0x21, 0, 1, 0, 1, 0x20 }.CopyTo(vob, pes);
        unit.CopyTo(vob, pes + 15);
        byte[] Read(long offset, int count) => vob.Skip((int)offset).Take(count).ToArray();

        var menu = DvdMenu.Read(Read, new DiscFile("VTS_01_0.VOB", 0, vob.Length), Cell(0, 1));
        var button = Assert.Single(menu.Highlight!.Buttons);
        Assert.Equal((100, 200, 100, 201), (button.X1, button.Y1, button.X2, button.Y2));
        Assert.Equal(unit, menu.Unit);
        Assert.Equal((100, 200, 101, 201), (menu.Control!.X1, menu.Control.Y1, menu.Control.X2, menu.Control.Y2));
    }

    // ---- a still menu's picture

    /// <summary>Bits written most significant first, as an MPEG-2 video stream lays them out.</summary>
    sealed class Bits
    {
        readonly List<byte> bytes = new();
        int bit;

        public Bits Put(string ones) => Put(System.Convert.ToInt32(ones, 2), ones.Length);

        public Bits Put(int value, int count)
        {
            for (var i = count - 1; i >= 0; i--)
            {
                if (bit == 0) bytes.Add(0);
                if (((value >> i) & 1) != 0) bytes[^1] |= (byte)(0x80 >> bit);
                bit = (bit + 1) & 7;
            }
            return this;
        }

        /// <summary>A start code, on a byte as all are.</summary>
        public Bits Code(int code)
        {
            bit = 0;
            bytes.AddRange(new byte[] { 0, 0, 1, (byte)code });
            return this;
        }

        public byte[] ToArray() => bytes.ToArray();
    }

    /// <summary>
    /// A 16×16 MPEG-2 I-picture of one macroblock and DC coefficients only: its top-left luma
    /// block at 132, the others at 128 (each DC differential from the last), grey chroma.
    /// </summary>
    static byte[] OneMacroblock() => new Bits()
        .Code(0xb3).Put(16, 12).Put(16, 12).Put(1, 4).Put(3, 4).Put(1, 18).Put(1, 1).Put(1, 10).Put(0, 1).Put(0, 1).Put(0, 1)
        // Sequence extension: main profile, progressive, 4:2:0.
        .Code(0xb5).Put(1, 4).Put(0x48, 8).Put(1, 1).Put(1, 2).Put(0, 2).Put(0, 2).Put(0, 12).Put(1, 1).Put(0, 8).Put(0, 1).Put(0, 7)
        // An I-picture, then its coding extension: 8-bit DC, a frame, frame DCT, table zero, zigzag.
        .Code(0x00).Put(0, 10).Put(1, 3).Put(0xffff, 16).Put(0, 1)
        .Code(0xb5).Put(8, 4).Put(0xffff, 16).Put(0, 2).Put(3, 2).Put(0, 1).Put(1, 1).Put(0, 1).Put(0, 1).Put(0, 1).Put(0, 1).Put(0, 1).Put(1, 1).Put(1, 1).Put(0, 1)
        // Its one slice: quantiser 1, the macroblock (increment 1, intra), six blocks.
        .Code(0x01).Put(1, 5).Put(0, 1).Put("1").Put("1")
        .Put("101").Put("100").Put("10") // Y0: size 3, +4, end
        .Put("101").Put("011").Put("10") // Y1: size 3, -4, end
        .Put("100").Put("10").Put("100").Put("10") // Y2, Y3: no difference
        .Put("00").Put("10").Put("00").Put("10") // Cb, Cr: 128
        .Code(0xb7).ToArray();

    [Fact]
    public void A_still_menus_picture_is_decoded_from_its_i_picture()
    {
        var still = MpegStill.Decode(OneMacroblock())!;
        Assert.Equal((16, 16), (still.Width, still.Height));
        // Studio range: 132 is 135 grey, 128 is 130; the top-left 8×8 block, then the one beside it.
        byte[] Pixel(int x, int y) => still.Rgba.Skip((y * 16 + x) * 4).Take(4).ToArray();
        Assert.Equal(new byte[] { 135, 135, 135, 255 }, Pixel(0, 0));
        Assert.Equal(new byte[] { 135, 135, 135, 255 }, Pixel(7, 7));
        Assert.Equal(new byte[] { 130, 130, 130, 255 }, Pixel(8, 0));
        Assert.Equal(new byte[] { 130, 130, 130, 255 }, Pixel(15, 15));
    }

    [Fact]
    public void A_picture_that_isnt_an_i_picture_isnt_decoded()
    {
        var p = OneMacroblock();
        // The picture header's coding type, from I (1) to P (2).
        for (var i = 0; i + 4 < p.Length; i++)
            if (p[i] == 0 && p[i + 1] == 0 && p[i + 2] == 1 && p[i + 3] == 0)
            {
                // temporal_reference (10 bits) then the type: bits 10-12 after the code.
                p[i + 5] = (byte)((p[i + 5] & 0xc7) | (2 << 3));
                break;
            }
        Assert.Null(MpegStill.Decode(p));
        Assert.Null(MpegStill.Decode(new byte[] { 1, 2, 3 }));
    }

    [Fact]
    public void The_discs_audio_and_subtitle_choice_names_the_titles_streams()
    {
        Pley.Tv.Core.Api.MediaStream Stream(int index, string kind) => new() { Index = index, Kind = kind };
        // Audio streams 0 and 2 of the disc (1 not in this title), subtitle streams 0 and 1.
        var title = new DvdTitle(1, 60_000, new Pley.Tv.Core.Api.Chapter[0],
            new[] { Stream(0, "video"), Stream(1, "audio"), Stream(2, "audio"), Stream(3, "subtitle"), Stream(4, "subtitle") }, new DiscSpan[0])
        {
            AudioNumbers = new[] { 0, 2 },
            SubtitleNumbers = new[] { 0, 1 },
        };
        Assert.Equal((2, 4), title.Chosen(2, 0x41));
        // Subtitles chosen but not shown (no 0x40): off.
        Assert.Equal((1, (int?)null), title.Chosen(0, 0x01));
        // A stream the title doesn't have: the player's own choice.
        Assert.Equal(((int?)null, (int?)null), title.Chosen(1, 0));
    }

    [Fact]
    public void A_title_menu_jump_from_a_title_set_menu_reaches_the_disc()
    {
        var vm = new DvdVm(Disc());
        vm.Start();
        vm.Button(JumpVtsMenu(1), 1);
        Assert.Equal(DvdDomain.VtsMenu, vm.Domain);
        Assert.Equal(DvdDomain.VmgMenu, Assert.IsType<DvdStep.Play>(vm.Button(JumpVmgMenu(), 1)).Domain);
    }
}
