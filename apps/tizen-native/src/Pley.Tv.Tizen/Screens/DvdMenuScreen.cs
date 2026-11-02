using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core;
using Pley.Tv.Core.Local;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Subtitles.Pictures;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A DVD played as the disc says, with its menus: <see cref="DvdVm"/> runs the disc's navigation,
/// and each cell it plays (a menu's video, a logo, a warning) plays here on the TV's player, out of
/// the disc image through the loopback server. A menu's buttons are drawn over its picture from
/// the menu's subpicture, the selected one in the disc's highlight colours; Up, Down, Left and
/// Right move as the disc links its buttons, OK presses one. A title of half a minute or more (a
/// film, an episode) plays in Pley's player, from where the disc starts it; played through, the
/// disc goes on as it says (back to its menu, the next title). Back leaves the disc.
/// </summary>
sealed class DvdMenuScreen : Sheet
{
    /// <summary>Shorter titles play here, as the disc's logos and warnings; longer ones in Pley's player.</summary>
    const long PlayerFromMs = 30_000;
    /// <summary>A menu fades out before the next opens, and the next fades in as its picture comes.</summary>
    const int FadeOutMs = 180, FadeInMs = 250;

    readonly Shell shell;
    readonly string image;
    readonly DvdDisc disc;
    readonly IReadOnlyList<DvdTitle> titles;
    readonly SpanServer server;
    readonly Action<DvdTitle, long, (int? Audio, int? Subtitle), Action<bool>> playTitle;
    readonly DvdVm vm;
    readonly TizenMediaEngine engine = new TizenMediaEngine(video: true);
    readonly IDisposable keys;
    readonly View black;
    /// <summary>Black round the picture: the window is clear over the whole video plane, and the TV's own screens would show at its sides.</summary>
    readonly View bars = new View();
    readonly PictureSubtitleView buttons = new PictureSubtitleView();
    /// <summary>
    /// The cell's first picture, decoded here: shown at once, under the buttons, until the TV's
    /// player has its own moving; a still menu's for as long as it shows, as the TV's player shows
    /// no picture of a single one.
    /// </summary>
    readonly PictureSubtitleView first = new PictureSubtitleView();
    PictureFrame? firstFrame;
    /// <summary>The TV's player shows the cell's picture: its first picture isn't wanted over it.</summary>
    bool moving;
    /// <summary>A cell's still time: held here, as a timer nothing holds is collected while it runs.</summary>
    readonly Timer still = new Timer(1000);

    DvdStep.Play? playing;
    DvdMenuPicture? menu;
    int selected;
    Animation? fade;
    /// <summary>Counts the cells played: what a cell's reading finds is dropped once another plays.</summary>
    int generation;
    bool away, gone;

    /// <param name="titles">The disc's titles as <see cref="DvdVideo"/> reads them: those the disc's navigation plays in Pley's player.</param>
    /// <param name="playTitle">Plays a title in Pley's player from a position, with the audio and subtitles the disc chose; then says whether it played through.</param>
    public DvdMenuScreen(Shell shell, string image, DvdDisc disc, IReadOnlyList<DvdTitle> titles, SpanServer server,
        Action<DvdTitle, long, (int? Audio, int? Subtitle), Action<bool>> playTitle)
        : base("dvd-menu")
    {
        Covers = true;
        this.shell = shell;
        this.image = image;
        this.disc = disc;
        this.titles = titles;
        this.server = server;
        this.playTitle = playTitle;
        vm = new DvdVm(disc);
#if PROFILE
        vm.Trace = t => Profile.Mark($"dvd vm: {t}");
#endif
        black = new View { Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight)), BackgroundColor = Color.Black };
        View.Add(black);
        View.Add(first);
        View.Add(bars);
        buttons.Opacity = 0;
        View.Add(buttons);
        // The picture and its buttons fade in together.
        engine.Started += () =>
        {
            Fade(0, FadeInMs);
            if (engine.HasVideo != true) return;
            moving = true;
            first.Show(null);
        };
        engine.VideoSize += Place;
        engine.StatusChanged += s =>
        {
            if (s == EngineStatus.Ended) CellEnded();
        };
        engine.Failed += e =>
        {
            Profile.Mark($"dvd menu: the player failed: {e.Detail}");
            CellEnded();
        };
        still.Tick += (_, _) =>
        {
            if (!gone) Go(vm.CellEnded());
            return false;
        };
        keys = shell.Input.Push(OnKey);
        shell.Shown += Shown;
        Back = () =>
        {
            shell.Pop(this);
            return true;
        };
        Gone += Ended;
    }

    public void Show()
    {
        shell.Push(this);
        Go(vm.Start());
    }

    // ---- what the disc plays

    void Go(DvdStep step)
    {
        if (gone) return;
        still.Stop();
        if (step is not DvdStep.Play play)
        {
            Profile.Mark("dvd menu: the disc stops");
            shell.Pop(this);
            return;
        }
        var marked = play.Pgc.Cells[play.Cell - 1];
        Profile.Mark($"dvd menu: {play.Domain} vts {play.Vts} chain {play.PgcNumber} cell {play.Cell}/{play.Pgc.Cells.Count}, {marked.LastSector - marked.FirstSector + 1} sectors, {marked.PlaybackMs} ms, still {marked.Still}");
        if (play.Domain == DvdDomain.Title)
            Profile.Mark($"dvd menu: the title's chain: pre [{string.Join(" ", play.Pgc.Pre.Select(Hex))}], cell commands [{string.Join(" ", play.Pgc.CellCommands.Select(Hex))}], programs [{string.Join(",", play.Pgc.ProgramCells)}], GPRM {string.Join(",", vm.Gprm.Take(8))}, title {vm.Sprm[4]} chapter {vm.Sprm[7]}");
        if (play.Domain == DvdDomain.Title && TitleOf(play) is { } title && title.DurationMs >= PlayerFromMs)
        {
            InPlayer(play, title);
            return;
        }
        var again = playing is { } was && was.Domain == play.Domain && was.Vts == play.Vts && was.PgcNumber == play.PgcNumber && was.Cell == play.Cell;
        playing = play;
        // The same cell again (a menu's video looping): from its start, its buttons as they are.
        if (again && engine.DurationMs > 0)
        {
            engine.SeekTo(0);
            engine.Play();
            return;
        }
        Cell(play);
    }

    /// <summary>
    /// The cell <paramref name="play"/> names, once the picture showing has faded out: the TV's
    /// player opens one source at a time, so the next can't start under the last one.
    /// </summary>
    void Cell(DvdStep.Play play)
    {
        var mine = ++generation;
        if (black.Opacity >= 1)
        {
            Open(play, mine);
            return;
        }
        Fade(1, FadeOutMs, () =>
        {
            if (!gone && mine == generation) Open(play, mine);
        });
    }

    /// <summary>The picture (1: black) and the buttons (as it shows) to <paramref name="to"/>, then <paramref name="then"/>.</summary>
    void Fade(float to, int ms, Action? then = null)
    {
        fade?.Stop();
        var a = new Animation(ms);
        a.AnimateTo(black, "Opacity", to);
        a.AnimateTo(buttons, "Opacity", 1 - to);
        if (then != null) a.Finished += (_, _) => then();
        fade = a;
        a.PlayHeld();
    }

    /// <summary>The cell <paramref name="play"/> names on the TV's player, its buttons read meanwhile.</summary>
    void Open(DvdStep.Play play, int mine)
    {
        menu = null;
        selected = 0;
        moving = false;
        firstFrame = null;
        buttons.Show(null);
        first.Show(null);
        var cell = play.Pgc.Cells[play.Cell - 1];
        var (spans, vob) = SpansOf(play, cell);
        if (spans.Count == 0)
        {
            Profile.Mark("dvd menu: a cell outside the disc's files");
            Go(vm.CellEnded());
            return;
        }
        black.Opacity = 1;
        buttons.Opacity = 0;
        var wide = play.Domain switch
        {
            DvdDomain.VmgMenu or DvdDomain.FirstPlay => disc.VmgMenuWide,
            DvdDomain.VtsMenu => disc.Sets.TryGetValue(play.Vts, out var s) && s.MenuWide,
            _ => Title()?.Wide == true,
        };
        var height = disc.Pal ? 576 : 480;
        var aspect = wide ? 16 / 9.0 : 4 / 3.0;
        // Where the TV will put the picture, known from the disc: the buttons and bars there from the start.
        Place((int)Math.Round(height * aspect), height);
        var url = server.Serve(image, spans);
        engine.Prepare(new EngineSource(url, Hls: false, Mime: "video/mpeg",
            Video: new EngineVideo("mpeg2video", 720, height, null, "sdr", null, 8, Aspect: aspect)), play: true);
        if (vob == null) return;
        Task.Run(() =>
        {
            using var f = new FileStream(image, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 4096);
            var read = DvdMenu.Read((offset, count) => Read(f, offset, count), vob, cell);
            return (read, read.Frame != null ? MpegStill.Decode(read.Frame) : null);
        }).ContinueWith(t =>
        {
            if (gone || mine != generation || t.Status != TaskStatus.RanToCompletion) return;
            var (read, picture) = t.Result;
            menu = read;
            if (picture != null && !moving)
            {
                firstFrame = new PictureFrame(picture.Width, picture.Height, new[] { new SubtitleBitmap(0, 0, picture.Width, picture.Height, picture.Rgba) });
                first.Show(firstFrame);
                buttons.Opacity = 1;
            }
            var hl = menu.Highlight;
            Profile.Mark($"dvd menu: {hl?.Buttons.Count ?? 0} buttons, their picture {(menu.Unit != null ? "found" : "not found")}, the first picture {(picture != null ? $"{picture.Width}x{picture.Height}" : read.Frame != null ? "not decoded" : "not found")}");
            if (hl == null || hl.Buttons.Count == 0) return;
            // The button the disc selects first; else the one it last had; else the first.
            selected = hl.ForcedSelect > 0 ? hl.ForcedSelect : play.Button;
            if (!hl.Buttons.Any(b => b.Number == selected)) selected = hl.Buttons[0].Number;
            Draw();
        }, TaskScheduler.FromCurrentSynchronizationContext());
    }

    /// <summary>A cell's stretches of the image: a menu's in its menu VOB (that VOB too, where its buttons are), a title's across its VOBs.</summary>
    (IReadOnlyList<DiscSpan> Spans, DiscFile? Vob) SpansOf(DvdStep.Play play, DvdPgcCell cell)
    {
        DiscFile? vob = play.Domain switch
        {
            DvdDomain.VmgMenu or DvdDomain.FirstPlay => disc.VmgMenuVob,
            DvdDomain.VtsMenu => disc.Sets.TryGetValue(play.Vts, out var s) ? s.MenuVob : null,
            _ => null,
        };
        if (vob != null)
        {
            var from = cell.FirstSector * DiscImage.Sector;
            var to = Math.Min(vob.Length, (cell.LastSector + 1) * DiscImage.Sector);
            return (to > from ? new[] { new DiscSpan(vob.Offset + from, to - from) } : Array.Empty<DiscSpan>(), vob);
        }
        if (play.Domain == DvdDomain.Title && disc.Sets.TryGetValue(play.Vts, out var set))
            return (DvdVideo.Spans(set.TitleVobs, cell.FirstSector, cell.LastSector).ToList(), null);
        return (Array.Empty<DiscSpan>(), null);
    }

    /// <summary>The title a title cell is of, as <see cref="DvdVideo"/> reads it.</summary>
    DvdTitle? Title() => titles.FirstOrDefault(t => t.Number == vm.Sprm[4]);

    /// <summary>
    /// The title that plays the chain <paramref name="play"/> is in: the one whose first cell is
    /// the chain's first cell, else the title the disc's registers name. A disc's titles can share
    /// a film's cells (a scene selection's chains), so a title number alone can name the wrong chain.
    /// </summary>
    DvdTitle? TitleOf(DvdStep.Play play)
    {
        if (OffsetOf(play, play.Pgc.Cells[0]) is long first)
            foreach (var t in titles)
                if (t.Cells.Count > 0 && t.Cells[0].Spans.Count > 0 && t.Cells[0].Spans[0].Offset == first) return t;
        return Title();
    }

    /// <summary>Where a title cell starts in the image; null when the title set has no such sectors.</summary>
    long? OffsetOf(DvdStep.Play play, DvdPgcCell cell) =>
        disc.Sets.TryGetValue(play.Vts, out var set) ? DvdVideo.Spans(set.TitleVobs, cell.FirstSector, cell.LastSector).Select(s => (long?)s.Offset).FirstOrDefault() : null;

    /// <summary>The cell played: it stays as long as the disc says (until a button, for 255), then the disc goes on.</summary>
    void CellEnded()
    {
        if (gone || away || playing is not { } play) return;
        var cell = play.Pgc.Cells[play.Cell - 1];
        if (cell.Still == 255) return;
        if (cell.Still > 0)
        {
            still.Interval = (uint)(cell.Still * 1000);
            still.Start();
            return;
        }
        Go(vm.CellEnded());
    }

    /// <summary>A title in Pley's player, from the cell the disc starts it at; played through, its chain ends as the disc says.</summary>
    void InPlayer(DvdStep.Play play, DvdTitle title)
    {
        away = true;
        playing = null;
        generation++;
        engine.Stop();
        buttons.Show(null);
        fade?.Stop();
        black.Opacity = 1;
        buttons.Opacity = 0;
        // Where the cell the disc starts at is in the title: found by its sectors, as the title may leave out cells (other angles).
        var at = OffsetOf(play, play.Pgc.Cells[play.Cell - 1]);
        var startMs = title.Cells.FirstOrDefault(c => c.Spans.Count > 0 && c.Spans[0].Offset == at)?.StartMs ?? 0;
        Profile.Mark($"dvd menu: title {title.Number} in the player from {startMs} ms");
        // The audio and subtitles the disc's setup chose (its system registers 1 and 2).
        var tracks = title.Chosen(vm.Sprm[1], vm.Sprm[2]);
        Profile.Mark($"dvd menu: the disc chose audio {vm.Sprm[1]}, subpicture {vm.Sprm[2]:x}: tracks {tracks.Audio?.ToString() ?? "-"}, {tracks.Subtitle?.ToString() ?? "off"}");
        playTitle(title, startMs, tracks, finished =>
        {
            away = false;
            if (gone) return;
            // Left before its end: the title set's menu, as the remote's Menu key gives.
            Go(finished ? vm.ChainEnded() : vm.Menu());
        });
    }

    // ---- the buttons

    /// <summary>The buttons over the picture, where the TV puts it.</summary>
    void Place(int width, int height)
    {
        var (sw, sh) = PictureArea.Screen;
        var (x, y, w, h) = PictureArea.Of(width, height, sw, sh);
        buttons.Position = new Position(x, y);
        buttons.Size = new Size(w, h);
        first.Position = new Position(x, y);
        first.Size = new Size(w, h);
        first.Show(null);
        if (!moving) first.Show(firstFrame);
        foreach (var bar in bars.Children.ToList())
        {
            bars.Remove(bar);
            bar.Dispose();
        }
        // As the player's: round the rectangle the TV puts the picture in, a pixel into it.
        void Bar(float bx, float by, float bw, float bh)
        {
            if (bw >= 1 && bh >= 1) bars.Add(new View { Position = new Position(bx, by), Size = new Size(bw, bh), BackgroundColor = Color.Black });
        }
        Bar(0, 0, sw, y + 1);
        Bar(0, y + h - 1, sw, sh - y - h + 1);
        Bar(0, 0, x + 1, sh);
        Bar(x + w - 1, 0, sw - x - w + 1, sh);
        // Drawn again at the new place: the same picture shown again keeps where it was.
        buttons.Show(null);
        Draw();
    }

    void Draw(bool pressed = false)
    {
        if (menu == null || playing is not { } play) return;
        var bitmap = DvdMenu.Draw(menu, play.Pgc.Palette, selected, pressed);
        buttons.Show(bitmap == null ? null : new PictureFrame(720, disc.Pal ? 576 : 480, new[] { bitmap }));
    }

    bool OnKey(string key, bool down)
    {
        if (gone || shell.Top != this) return false;
        var hl = menu?.Highlight;
        Direction? direction = key switch
        {
            "Up" => Direction.Up,
            "Down" => Direction.Down,
            "Left" => Direction.Left,
            "Right" => Direction.Right,
            _ => null,
        };
        if (direction is { } d)
        {
            if (down && hl != null && hl.Buttons.Count > 0)
            {
                var next = DvdMenu.Move(hl, selected, d);
                if (next != selected)
                {
                    selected = next;
                    Draw();
                    // A button that acts as it's selected.
                    if (hl.Buttons.First(b => b.Number == next).AutoAction) Press();
                }
            }
            return true;
        }
        if (key is "Return" or "KP_Enter")
        {
            if (!down) Press();
            return true;
        }
        if (RemoteKeys.Menu(key))
        {
            if (!down && playing != null) Go(vm.Menu());
            return true;
        }
        return RemoteKeys.Transport(key);
    }

    void Press()
    {
        if (menu?.Highlight?.Buttons.FirstOrDefault(b => b.Number == selected) is not { } button) return;
        Profile.Mark($"dvd menu: button {button.Number} pressed, its command {Hex(button.Command)}");
        Draw(pressed: true);
        Go(vm.Button(button.Command, button.Number));
    }

    // ---- the screen

    void Shown(bool shown)
    {
        if (gone || away) return;
        // Hidden, the TV's player is let go; back, the cell plays again.
        if (!shown)
        {
            generation++;
            engine.Stop();
            fade?.Stop();
            black.Opacity = 1;
            buttons.Opacity = 0;
        }
        else if (playing is { } play) Cell(play);
    }

    void Ended()
    {
        if (gone) return;
        gone = true;
        still.Stop();
        fade?.Stop();
        keys.Dispose();
        shell.Shown -= Shown;
        // The server isn't stopped: the TV lets go of its player after this returns, reading from
        // it meanwhile, and the next disc served replaces what it serves, as after a title.
        engine.Release();
        buttons.Show(null);
        first.Show(null);
    }

    static string Hex(byte[] bytes) => BitConverter.ToString(bytes).Replace("-", "");

    static byte[] Read(FileStream f, long offset, int count)
    {
        if (offset >= f.Length) return new byte[0];
        f.Seek(offset, SeekOrigin.Begin);
        var bytes = new byte[(int)Math.Min(count, f.Length - offset)];
        var read = 0;
        while (read < bytes.Length)
        {
            var n = f.Read(bytes, read, bytes.Length - read);
            if (n == 0) break;
            read += n;
        }
        if (read < bytes.Length) Array.Resize(ref bytes, read);
        return bytes;
    }
}
