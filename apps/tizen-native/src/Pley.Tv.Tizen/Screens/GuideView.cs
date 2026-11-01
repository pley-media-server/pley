using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The programme guide (the Android app's TvGuide): what has focus described above the grid,
/// channels down and time across, now marked by a line. Focus is one item, moved by the guide
/// itself (<see cref="GuideCursor"/>) rather than by geometry, so only the rows in view have
/// views, however many channels there are. It opens on the channel last watched, when it is in
/// the category.
/// </summary>
sealed class GuideView : View
{
    const float TileWidth = 120, TileHeight = 68, TileGap = 24;

    readonly LiveTvModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly LiveCategory listed;
    readonly string origin;
    readonly Func<bool> toCategories;
    readonly Action<LiveChannel> watch;
    readonly Action<LiveChannel, LiveProgram?> details;
    readonly GuideCursor cursor;
    readonly LiveClock clock = LiveClock.Local;
    readonly float width;
    readonly float rowsHeight;
    readonly float timeWidth;

    readonly View info = new View();
    readonly TextLabel title;
    readonly TextLabel meta;
    readonly TextLabel words;
    View? badge;
    ChannelLogoView? tile;
    long? tileChannel;

    readonly TextLabel day;
    readonly View headerLayer = new View();
    readonly View rowsLayer = new View();
    readonly View timeLayer = new View();
    readonly View rowsTimeLayer = new View();
    readonly View nowLine;
    readonly Dictionary<int, GuideChannelCell> cells = new Dictionary<int, GuideChannelCell>();
    readonly Dictionary<int, (ProgramsLook Look, View View)> rows = new Dictionary<int, (ProgramsLook, View)>();

    ChannelPages pages = ChannelPages.Empty;
    GuidePrograms guide = GuidePrograms.None;
    long now;
    int top;
    long drawnLeft;
    Landing? placed;
    Animation? slide;

    /// <param name="widthDp">From the guide's left edge to the right safe edge.</param>
    /// <param name="toCategories">Left from the guide's start: focus to the category shown.</param>
    public GuideView(LiveTvModel model, Images images, KeyRepeatGate gate, LiveCategory listed, string origin, float widthDp, float heightDp,
        Func<bool> toCategories, Action<LiveChannel> watch, Action<LiveChannel, LiveProgram?> details)
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.listed = listed;
        this.origin = origin;
        this.toCategories = toCategories;
        this.watch = watch;
        this.details = details;
        width = widthDp;
        timeWidth = widthDp - GuideLayout.ChannelWidth;
        var rowsTop = GuideLayout.InfoHeight + GuideLayout.HeaderHeight;
        rowsHeight = heightDp - rowsTop;
        now = model.Now.Value;
        cursor = new GuideCursor(model.GuideStart, now);
        drawnLeft = cursor.Left;
        Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp));

        // What has focus: its title, channel and times, and what the guide says of it; the channel's tile at the right.
        var textWidth = widthDp - TileWidth - TileGap;
        title = Text.Label("", PleyType.Section, PleyColors.Fg, width: textWidth);
        title.PositionY = Ui.Dp(4);
        info.Add(title);
        meta = Text.Label("", PleyType.Label, PleyColors.Muted, width: textWidth);
        meta.PositionY = Ui.Dp(4 + PleyType.Section.Line + 2);
        info.Add(meta);
        words = Text.Label("", PleyType.Body, PleyColors.Muted, maxLines: 2, width: Math.Min(560, textWidth));
        words.VerticalAlignment = VerticalAlignment.Top;
        words.PositionY = Ui.Dp(4 + PleyType.Section.Line + 2 + PleyType.Label.Line + 6);
        info.Add(words);
        Add(info);

        // The times across the top, the day at the left.
        day = Text.Label("", PleyType.Caption, PleyColors.Fg, width: GuideLayout.ChannelWidth - GuideLayout.ChannelInset);
        day.Position = new Position(Ui.Dp(GuideLayout.ChannelInset), Ui.Dp(GuideLayout.InfoHeight + (GuideLayout.HeaderHeight - PleyType.Caption.Line) / 2));
        Add(day);
        var header = new View
        {
            Position = new Position(Ui.Dp(GuideLayout.ChannelWidth), Ui.Dp(GuideLayout.InfoHeight)),
            Size = new Size(Ui.Dp(timeWidth), Ui.Dp(GuideLayout.HeaderHeight)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        header.Add(headerLayer);
        Add(header);

        // The rows: the channels' cells, and over them their programmes in time, which slide as the grid moves.
        var grid = new View
        {
            Position = new Position(0, Ui.Dp(rowsTop)),
            Size = new Size(Ui.Dp(widthDp), Ui.Dp(rowsHeight)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        grid.Add(rowsLayer);
        var times = new View
        {
            Position = new Position(Ui.Dp(GuideLayout.ChannelWidth), 0),
            Size = new Size(Ui.Dp(timeWidth), Ui.Dp(rowsHeight)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        timeLayer.Add(rowsTimeLayer);
        nowLine = new View
        {
            Size = new Size(Ui.Dp(GuideLayout.NowLineWidth), Ui.Dp(rowsHeight)),
            BackgroundColor = LiveViews.Alpha(PleyColors.Accent, 0.85f),
        };
        timeLayer.Add(nowLine);
        times.Add(timeLayer);
        grid.Add(times);
        Add(grid);
        timeLayer.PositionX = -Ui.Dp(GuideLayout.Scroll(cursor.Left, cursor.Start));
        headerLayer.PositionX = timeLayer.PositionX;

        catchUp.Tick += (s, e) => CatchUp();
        Item = new FocusItem("guide")
        {
            Selected = Ok,
            OwnKey = OnKey,
        };
        Item.LongSelected = Details;
        Item.Menu = Details;
        Item.FocusChanged += _ => Draw();
        // Shown again (this or a page it is on): what changed meanwhile.
        VisibilityChanged += (_, e) =>
        {
            if (!e.Visibility || !stale || Disposed || !Showing()) return;
            stale = false;
            Draw();
        };
        model.ShowingTimes(cursor.Left, cursor.Left + GuideLayout.ViewS);
        Header();
    }

    /// <summary>The guide's one focus item.</summary>
    public FocusItem Item { get; }

    int Total => pages.Total ?? 0;

    public void SetPages(ChannelPages value)
    {
        Profile.Mark("live guide: pages");
        pages = value;
        cursor.Fit(Total);
        ShowingRows();
        Draw();
    }

    public void SetPrograms(GuidePrograms value)
    {
        Profile.Mark("live guide: programmes");
        guide = value;
        Draw();
    }

    public void SetNow(long value)
    {
        now = value;
        Draw();
        MoveBars();
    }

    /// <summary>The list opens at the channel last watched (once per landing).</summary>
    public void Land(Landing? landing)
    {
        Profile.Mark("live guide: land");
        if (landing == null || landing.Category != listed || placed == landing) return;
        placed = landing;
        cursor.Place(landing.Index ?? 0, Total);
        Scroll(animate: false);
        Draw();
    }

    /// <summary>Back away from now: the guide comes back to now first.</summary>
    public bool Back()
    {
        if (!Item.Focused || pages[cursor.Row] == null || !cursor.AwayFromNow(Programs(cursor.Row), now)) return false;
        cursor.BackToNow(now);
        Moved();
        return true;
    }

    IReadOnlyList<LiveProgram>? Programs(int row) => pages[row] is { } c ? guide.Of(c.Id) : null;

    /// <summary>How long after Left last moved within the guide a Left at its start still stays: taps in a row.</summary>
    const int EdgePauseMs = 500;

    /// <summary>When Left last moved within the guide (Environment.TickCount).</summary>
    int leftMovedAt = Environment.TickCount - EdgePauseMs;

    bool OnKey(string key, bool down)
    {
        if (Disposed) return false;
        var programs = Programs(cursor.Row);
        switch (key)
        {
            case "Left":
                if (!down) return true;
                if (cursor.MoveLeft(programs))
                {
                    leftMovedAt = Environment.TickCount;
                    Moved();
                }
                // Out to the categories on a press of its own: not under a held key, nor straight
                // after Left went back through the programmes (taps in a row stop at the guide's start).
                else if (unchecked(Environment.TickCount - leftMovedAt) >= EdgePauseMs) Scrolling.Left(gate, toCategories);
                return true;
            case "Right":
                if (!down) return true;
                cursor.MoveRight(programs);
                Moved();
                return true;
            case "Up":
                if (!down) return true;
                // Up from the first row leaves for the tabs.
                if (!cursor.MoveUp()) return false;
                RowMoved();
                return true;
            case "Down":
                if (!down) return true;
                cursor.MoveDown(Total);
                RowMoved();
                return true;
        }
        if (RemoteKeys.ChannelUp(key) || RemoteKeys.ChannelDown(key))
        {
            if (!down) return true;
            if (RemoteKeys.ChannelUp(key)) cursor.PageUp();
            else cursor.PageDown(Total);
            RowMoved();
            return true;
        }
        if (RemoteKeys.Info(key))
        {
            if (!down) Details();
            return true;
        }
        return false;
    }

    /// <summary>OK on a programme on now (or a channel without guide) watches it; on another, its details.</summary>
    void Ok()
    {
        var channel = pages[cursor.Row];
        var programs = Programs(cursor.Row);
        switch (cursor.Ok(channel, programs, now))
        {
            case GuideOk.Watch:
                watch(channel!);
                break;
            case GuideOk.Details:
                details(channel!, cursor.Current(programs));
                break;
        }
    }

    /// <summary>Holding OK, Menu or Info: the details of the programme with focus.</summary>
    void Details()
    {
        if (pages[cursor.Row] is { } channel && cursor.Details(Programs(cursor.Row)) is { } program) details(channel, program);
    }

    /// <summary>The cursor moved in time: the grid follows by half hours, its programmes asked for.</summary>
    void Moved()
    {
        if (cursor.Left != drawnLeft)
        {
            drawnLeft = cursor.Left;
            model.ShowingTimes(cursor.Left, cursor.Left + GuideLayout.ViewS);
            Header();
            slide?.Stop();
            slide = new Animation(PleyMotion.OverlayMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
            var x = -Ui.Dp(GuideLayout.Scroll(cursor.Left, cursor.Start));
            slide.AnimateTo(timeLayer, "PositionX", x);
            slide.AnimateTo(headerLayer, "PositionX", x);
            slide.Play();
        }
        Draw();
    }

    void RowMoved()
    {
        Scroll(animate: true);
        Draw();
    }

    /// <summary>The rows scroll when the focused one reaches the first or the last in view, bringing it second.</summary>
    void Scroll(bool animate)
    {
        var shown = (int)Math.Ceiling(rowsHeight / GuideLayout.RowHeight);
        var row = cursor.Row;
        if (row <= top || row >= top + shown - 1) top = Math.Max(0, row - 1);
        // The last row stays at the foot, above the room under it.
        var full = (int)((rowsHeight - GuideLayout.BottomPadding) / GuideLayout.RowHeight);
        top = Math.Max(0, Math.Min(top, Total - full));
        var y = -Ui.Dp(top * GuideLayout.RowHeight);
        if (animate)
        {
            var a = Scrolling.Glide(gate);
            a.AnimateTo(rowsLayer, "PositionY", y);
            a.AnimateTo(rowsTimeLayer, "PositionY", y);
            a.Play();
        }
        else
        {
            rowsLayer.PositionY = y;
            rowsTimeLayer.PositionY = y;
        }
        ShowingRows();
    }

    void ShowingRows()
    {
        var shown = (int)Math.Ceiling(rowsHeight / GuideLayout.RowHeight);
        model.ShowingRows(top, top + shown);
    }

    /// <summary>The half hours across the top, a screen either way of the grid's edge, in time as the programmes are.</summary>
    void Header()
    {
        foreach (var child in headerLayer.Children.ToList())
        {
            headerLayer.Remove(child);
            child.Dispose();
        }
        foreach (var (time, x) in GuideLayout.Slots(cursor.Left, cursor.Start))
        {
            var midnight = clock.Midnight(time);
            var label = Text.Label(midnight ? clock.Day(time, now) : clock.Time(time), PleyType.Caption, midnight ? PleyColors.Fg : PleyColors.Muted, width: 100);
            label.Position = new Position(Ui.Dp(x + 6), Ui.Dp((GuideLayout.HeaderHeight - PleyType.Caption.Line) / 2));
            headerLayer.Add(label);
        }
    }

    /// <summary>Something changed while the guide was hidden: drawn when it shows again.</summary>
    bool stale;

    /// <summary>Whether the guide and every view it is on are shown: a player hides the pages under it.</summary>
    bool Showing()
    {
        for (var v = (View?)this; v != null; v = v.GetParent() as View)
            if (!v.Visibility) return false;
        return true;
    }

    /// <summary>Draws what changed: the focused programme's description, and the rows in view (and one either side).</summary>
    void Draw()
    {
        if (Disposed) return;
        // Under a player, its pages hidden: drawn once they show again, not as each channel tuned
        // there changes Recent or the clock moves, which held the UI thread as the channel started.
        if (!Showing())
        {
            stale = true;
            return;
        }
        Profile.Mark("live guide: draw");
        var focused = Item.Focused;
        var channel = pages[cursor.Row];
        var programs = Programs(cursor.Row);
        var current = channel != null ? cursor.Current(programs) : null;
        Info(channel, current);
        Text.Show(day, clock.Day(Math.Max(cursor.Left, cursor.Start), now));
        nowLine.PositionX = Ui.Dp(GuideLayout.X(now, cursor.Start) - GuideLayout.NowLineWidth / 2);

        var shown = (int)Math.Ceiling(rowsHeight / GuideLayout.RowHeight);
        var first = Math.Max(0, top - 1);
        var last = Math.Min(Total - 1, top + shown + 1);
        foreach (var i in cells.Keys.Where(i => i < first || i > last).ToList())
        {
            cells[i].Dispose();
            cells.Remove(i);
        }
        foreach (var i in rows.Keys.Where(i => i < first || i > last).ToList())
        {
            rows[i].View.Dispose();
            bars.Remove(i);
            rows.Remove(i);
        }
        for (var i = first; i <= last; i++)
        {
            var c = pages[i];
            var rowFocused = focused && i == cursor.Row;
            if (!cells.TryGetValue(i, out var cell) || !SameChannel(cell.Channel, c))
            {
                cell?.Dispose();
                cell = new GuideChannelCell(c, c != null ? LiveRules.ChannelName(c, listed) : "", origin, images, width);
                cell.PositionY = Ui.Dp(i * GuideLayout.RowHeight);
                rowsLayer.Add(cell);
                cells[i] = cell;
            }
            cell.Focus(rowFocused);
            var look = LookOf(i, rowFocused);
            later.Remove(i);
            if (rows.TryGetValue(i, out var drawn) && Same(drawn.Look, look)) continue;
            // Now: a row coming into view, and the rows focus came to and left. The others (an answer
            // filling a screen of rows, ten at 10 to 15 ms each) catch up a few at a time, frame by frame.
            if (drawn.View != null && !rowFocused && drawn.Look.Focus == null)
            {
                later.Add(i);
                continue;
            }
            // A row coming into view (a screen of them when the guide pages) shows its still bars
            // at once, and its programmes as it catches up.
            if (drawn.View == null && !rowFocused)
            {
                Build(i, look with { Programs = null, Loaded = false });
                later.Add(i);
                continue;
            }
            Build(i, look);
        }
        if (later.Count > 0 && !catchingUp)
        {
            catchingUp = true;
            catchUp.Start();
        }
    }

    /// <summary>How row <paramref name="i"/> is to look now.</summary>
    ProgramsLook LookOf(int i, bool rowFocused)
    {
        var c = pages[i];
        var list = c != null ? guide.Of(c.Id) : null;
        return new ProgramsLook(c, list, guide.Loaded, cursor.Left, now, rowFocused ? cursor.Current(list)?.Start ?? -1 : null);
    }

    void Build(int i, ProgramsLook look)
    {
        if (rows.TryGetValue(i, out var drawn)) drawn.View.Dispose();
        barsMade.Clear();
        var view = ProgramsRow(look);
        view.PositionY = Ui.Dp(i * GuideLayout.RowHeight);
        rowsTimeLayer.Add(view);
        rows[i] = (look, view);
        bars[i] = barsMade.ToList();
    }

    /// <summary>The bars of the programmes airing in each row made, to move with the time without making the row again.</summary>
    readonly Dictionary<int, List<(View Bar, float Left, float Width)>> bars = new Dictionary<int, List<(View Bar, float Left, float Width)>>();
    readonly List<(View Bar, float Left, float Width)> barsMade = new List<(View Bar, float Left, float Width)>();

    /// <summary>The airing programmes' bars to now, in place (a row's bars go with it, so none is disposed).</summary>
    /// <summary>
    /// An airing programme's bar, in dp: from its cell's left (<paramref name="left"/>, on the guide's
    /// time line) to now's line. As the share played of the whole programme, a programme begun before
    /// the guide (its cell starting at the guide's edge) had its bar run past now.
    /// </summary>
    float BarWidth(float left, float width, long at) => Math.Max(0, Math.Min(width, GuideLayout.X(at, cursor.Start) - left));

    void MoveBars()
    {
        foreach (var row in bars.Values)
            foreach (var (bar, left, width) in row)
                bar.SizeWidth = Ui.Dp(BarWidth(left, width, now));
    }

    /// <summary>Rows to make again, a few a frame, nearest the focused row first.</summary>
    readonly HashSet<int> later = new HashSet<int>();

    /// <summary>The time a frame's catching up may take, in ms: what is left of a 60 Hz frame.</summary>
    const double CatchUpMs = 8;

    readonly Timer catchUp = new Timer(16);
    bool catchingUp;

    /// <summary>Makes again the rows waiting, nearest the focused row first, until a frame's time is spent.</summary>
    bool CatchUp()
    {
        Profile.Mark("live guide: catch up");
        if (Disposed)
        {
            catchingUp = false;
            return false;
        }
        var watch = System.Diagnostics.Stopwatch.StartNew();
        var focused = Item.Focused;
        while (later.Count > 0 && watch.Elapsed.TotalMilliseconds < CatchUpMs)
        {
            var i = later.OrderBy(r => Math.Abs(r - cursor.Row)).First();
            later.Remove(i);
            // Gone out of view meanwhile.
            if (!rows.TryGetValue(i, out var drawn)) continue;
            var look = LookOf(i, focused && i == cursor.Row);
            if (!Same(drawn.Look, look)) Build(i, look);
        }
        catchingUp = later.Count > 0;
        return catchingUp;
    }

    void Info(LiveChannel? channel, LiveProgram? program)
    {
        if (channel == null)
        {
            info.Hide();
            return;
        }
        info.Show();
        var name = LiveRules.ChannelName(channel, listed);
        badge?.Dispose();
        badge = program != null ? LiveViews.BadgeOf(program) : null;
        var x = 0f;
        if (badge != null)
        {
            badge.Position = new Position(0, Ui.Dp(4) + (Text.LineHeight(PleyType.Section) - badge.SizeHeight) / 2);
            info.Add(badge);
            x = badge.SizeWidth + Ui.Dp(8);
        }
        Text.Show(title, program?.Title ?? name);
        title.PositionX = x;
        title.SizeWidth = Ui.Dp(width - TileWidth - TileGap) - x;
        Text.Show(meta, LiveText.GuideMeta(channel, name, program, now, clock));
        words.Text = LiveText.GuideText(program) ?? "";
        if (tileChannel != channel.Id || tile == null)
        {
            tile?.Dispose();
            tile = new ChannelLogoView(channel, origin, images, TileWidth, TileHeight, name: true);
            tile.Position = new Position(Ui.Dp(width - TileWidth), Ui.Dp(4));
            info.Add(tile);
            tileChannel = channel.Id;
        }
    }

    /// <summary>What a row of programmes shows: drawn again only when it changes.</summary>
    readonly record struct ProgramsLook(LiveChannel? Channel, IReadOnlyList<LiveProgram>? Programs, bool Loaded, long Left, long Now, long? Focus);

    /// <summary>
    /// Whether a row drawn as <paramref name="a"/> looks as <paramref name="b"/> would: its
    /// programmes compared by those in view. Each guide answer (one for every move up or down,
    /// for the rows coming into view) brings new lists of the same programmes, and compared as
    /// lists every row in view was made again at each move.
    /// </summary>
    bool Same(ProgramsLook a, ProgramsLook b)
    {
        if (!SameChannel(a.Channel, b.Channel) || a.Left != b.Left || a.Focus != b.Focus) return false;
        var x = GuideLayout.Visible(a.Programs, a.Left, cursor.Start);
        var y = ReferenceEquals(a.Programs, b.Programs) ? x : GuideLayout.Visible(b.Programs, b.Left, cursor.Start);
        if (x.Count != y.Count) return false;
        // Whether the guide is still loading tells only a row with nothing in view what to say;
        // it changes with every answer asked for, and compared for every row made them all again.
        if (x.Count == 0) return a.Loaded == b.Loaded;
        for (var i = 0; i < x.Count; i++)
        {
            if (x[i].Start != y[i].Start || x[i].Stop != y[i].Stop || x[i].Title != y[i].Title) return false;
            // The time only matters where a programme began or ended between the two (its colour,
            // its bar); the bars of those airing move in place (MoveBars). Compared as it was, every
            // row was made again at each 15 s tick, all the while the guide showed.
            if (x[i].AiringAt(a.Now) != y[i].AiringAt(b.Now) || (x[i].Stop <= a.Now) != (y[i].Stop <= b.Now)) return false;
        }
        return true;
    }

    /// <summary>
    /// Whether two answers' records of a channel draw the same: by what is drawn of it, as its
    /// records hold lists (its programmes' categories) that make each answer's records unequal.
    /// </summary>
    static bool SameChannel(LiveChannel? a, LiveChannel? b) =>
        a == null || b == null
            ? a == b
            : a.Id == b.Id && a.Name == b.Name && a.Number == b.Number && a.Logo == b.Logo && a.HasGuide == b.HasGuide && a.Favorite == b.Favorite;

    /// <summary>A channel's programmes in time, x from the guide's start: the focused one in the accent, what airs marked, what ended dimmed.</summary>
    View ProgramsRow(ProgramsLook look)
    {
        var start = cursor.Start;
        var row = new View { Size = new Size(Ui.Dp(GuideLayout.X(start + LiveRules.GuideSpanS, start)), Ui.Dp(GuideLayout.RowHeight)) };
        var leftX = GuideLayout.X(look.Left, start);
        var cellHeight = GuideLayout.RowHeight - 2 * GuideLayout.CellInsetY;
        if (look.Channel == null)
        {
            row.Add(new View
            {
                Position = new Position(Ui.Dp(leftX + GuideLayout.CellInsetX), Ui.Dp(GuideLayout.CellInsetY)),
                Size = new Size(Ui.Dp(timeWidth - 2 * GuideLayout.CellInsetX), Ui.Dp(cellHeight)),
                CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                BackgroundColor = PleyColors.Surface2,
            });
            return row;
        }
        var visible = GuideLayout.Visible(look.Programs, look.Left, start);
        if (visible.Count == 0)
        {
            if (!look.Loaded) return row;
            // Nothing in view: the channel says why, across the grid.
            var on = look.Focus == -1;
            var filler = new View
            {
                Position = new Position(Ui.Dp(leftX + GuideLayout.CellInsetX), Ui.Dp(GuideLayout.CellInsetY)),
                Size = new Size(Ui.Dp(timeWidth - 2 * GuideLayout.CellInsetX), Ui.Dp(cellHeight)),
                CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                BackgroundColor = on ? PleyColors.Accent : LiveViews.Alpha(PleyColors.Surface, 0.6f),
            };
            var label = Text.Label(look.Channel.HasGuide ? LiveText.NoInfo : LiveRules.ChannelName(look.Channel, listed), PleyType.Caption, on ? PleyColors.OnAccent : PleyColors.Subtle,
                width: timeWidth - 2 * GuideLayout.CellInsetX - 24);
            label.Position = new Position(Ui.Dp(12), Ui.Dp((cellHeight - PleyType.Caption.Line) / 2));
            filler.Add(label);
            row.Add(filler);
            return row;
        }
        foreach (var program in visible)
        {
            var (x, w) = GuideLayout.Cell(program, start);
            var focused = look.Focus == program.Start;
            var airing = program.AiringAt(look.Now);
            var cellWidth = Math.Max(0, w - 2 * GuideLayout.CellInsetX);
            var cell = new View
            {
                Position = new Position(Ui.Dp(x + GuideLayout.CellInsetX), Ui.Dp(GuideLayout.CellInsetY)),
                Size = new Size(Ui.Dp(cellWidth), Ui.Dp(cellHeight)),
                CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                BackgroundColor = focused ? PleyColors.Accent : airing ? LiveViews.PrimaryContainer : PleyColors.Surface2,
                Opacity = program.Stop <= look.Now && !focused ? GuideLayout.PastAlpha : 1f,
            };
            if (airing && !focused)
            {
                cell.BorderlineWidth = Ui.Dp(1);
                cell.BorderlineColor = LiveViews.Alpha(PleyColors.Accent, 0.35f);
                cell.BorderlineOffset = -1;
            }
            if (w > GuideLayout.MinTextWidth)
            {
                // A programme begun left of the grid keeps its title in view.
                var offset = Math.Max(0, Math.Min(leftX - x, Math.Max(0, w - GuideLayout.TitleRoom)));
                var textWidth = Math.Max(10, cellWidth - offset - 2 * GuideLayout.CellPadding);
                var name = Text.Label(program.Title, PleyType.Label, focused ? PleyColors.OnAccent : PleyColors.Fg, width: textWidth);
                if (airing || focused) name.FontStyle = Text.Weight(PleyType.Bold);
                var block = PleyType.Label.Line + PleyType.Caption.Line;
                name.Position = new Position(Ui.Dp(offset + GuideLayout.CellPadding), Ui.Dp((cellHeight - block) / 2));
                cell.Add(name);
                var span = Text.Label(clock.Span(program), PleyType.Caption, focused ? LiveViews.Alpha(PleyColors.OnAccent, 0.8f) : PleyColors.Subtle, width: textWidth);
                span.Position = new Position(Ui.Dp(offset + GuideLayout.CellPadding), Ui.Dp((cellHeight - block) / 2 + PleyType.Label.Line));
                cell.Add(span);
            }
            if (airing)
            {
                var bar = new View
                {
                    Position = new Position(0, Ui.Dp(cellHeight - GuideLayout.ProgressHeight)),
                    Size = new Size(Ui.Dp(BarWidth(x + GuideLayout.CellInsetX, cellWidth, look.Now)), Ui.Dp(GuideLayout.ProgressHeight)),
                    BackgroundColor = focused ? PleyColors.OnAccent : PleyColors.Accent,
                    // The cell doesn't clip (a GPU pass a cell): the bar takes its bottom left corner itself.
                    CornerRadius = new Vector4(0, 0, 0, Ui.Dp(PleyShape.ControlRadius)),
                };
                cell.Add(bar);
                barsMade.Add((bar, x + GuideLayout.CellInsetX, cellWidth));
            }
            row.Add(cell);
        }
        return row;
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        slide?.Stop();
        catchUp.Stop();
        base.Dispose(type);
    }
}

/// <summary>
/// A channel's cell at the left of its guide row: its logo, number (a star when a favourite) and
/// name; the focused row's filled with Surface3. Under the whole row, its hairline.
/// </summary>
sealed class GuideChannelCell : View
{
    readonly View box;
    readonly TextLabel? name;

    public GuideChannelCell(LiveChannel? channel, string label, string origin, Images images, float rowWidthDp)
    {
        Channel = channel;
        Size = new Size(Ui.Dp(rowWidthDp), Ui.Dp(GuideLayout.RowHeight));
        Add(new View
        {
            Position = new Position(0, Ui.Dp(GuideLayout.RowHeight) - 1),
            Size = new Size(Ui.Dp(rowWidthDp), 1),
            BackgroundColor = LiveViews.Alpha(PleyColors.Surface4, 0.6f),
        });
        var inner = GuideLayout.RowHeight - 2 * GuideLayout.CellInsetY;
        box = new View
        {
            Position = new Position(0, Ui.Dp(GuideLayout.CellInsetY)),
            Size = new Size(Ui.Dp(channel == null ? GuideLayout.ChannelWidth - 8 : GuideLayout.ChannelWidth), Ui.Dp(inner)),
            CornerRadius = Ui.Dp(PleyShape.ControlRadius),
            BackgroundColor = channel == null ? PleyColors.Surface2 : Color.Transparent,
        };
        Add(box);
        if (channel == null) return;
        var logo = new ChannelLogoView(channel, origin, images, 56, 34);
        logo.Position = new Position(Ui.Dp(GuideLayout.ChannelInset), Ui.Dp((inner - 34) / 2));
        box.Add(logo);
        var x = GuideLayout.ChannelInset + 56 + 10;
        var textWidth = GuideLayout.ChannelWidth - x - 8;
        var top = (inner - PleyType.Caption.Line - PleyType.Label.Line) / 2;
        var number = LiveViews.Fitted(channel.Number ?? "", PleyType.Caption, PleyColors.Subtle, textWidth - 16);
        number.Position = new Position(Ui.Dp(x), Ui.Dp(top));
        box.Add(number);
        if (channel.Favorite)
        {
            var star = Icons.Icon("star", 12, PleyColors.Accent);
            star.Position = new Position(Ui.Dp(x) + number.SizeWidth + Ui.Dp(4), Ui.Dp(top + (PleyType.Caption.Line - 12) / 2));
            box.Add(star);
        }
        name = Text.Label(label, PleyType.Label, PleyColors.Muted, width: textWidth);
        name.Position = new Position(Ui.Dp(x), Ui.Dp(top + PleyType.Caption.Line));
        box.Add(name);
    }

    public LiveChannel? Channel { get; }

    public void Focus(bool on)
    {
        if (Disposed || name == null) return;
        box.BackgroundColor = on ? PleyColors.Surface3 : Color.Transparent;
        name.TextColor = on ? PleyColors.Fg : PleyColors.Muted;
    }
}
