using System;
using System.Collections.Generic;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A row of tiles (the Android app's TvRow, design spec §2.4, D-pad rules 2 to 5): a header, then
/// the tiles, whose focused tile's left edge stays at the gutter (x = 96) while the row moves under
/// it, unless every tile fits on screen unscrolled. It is one focus group that comes back to the
/// tile focused last; Left from its first tile enters the rail, Right stops at its end. Under a held
/// key it slides on at a flat pace beneath a ring that stays still, and settles when the key comes up.
/// </summary>
sealed class RowView : View
{
    /// <summary>The tiles past the screen's edge start being made this long after the row, once its first frame and pictures are in.</summary>
    const uint MakerDelayMs = 500;
    /// <summary>Then one is made every this often until the row has a screen more (<see cref="RowReach.Soon"/>).</summary>
    const uint MakerMs = 32;

    readonly TileKind kind;
    readonly KeyRepeatGate gate;
    readonly Images images;
    /// <summary>Puts focus on a tile: where a slide lands.</summary>
    readonly Func<FocusItem, bool>? focusItem;
    readonly View strip;
    readonly IReadOnlyList<TileModel> models;
    /// <summary>The tiles a screen's width holds.</summary>
    readonly int perScreen;
    /// <summary>Each tile's place in the focus tree, there from the start, whether its tile is made yet or not.</summary>
    readonly List<FocusItem> items = new List<FocusItem>();
    /// <summary>The tiles made so far, from the row's start (<see cref="Make"/>).</summary>
    readonly List<TileView> tiles = new List<TileView>();
    /// <summary>Makes the tiles past the screen's edge one at a time, once the row is shown and as focus moves along it.</summary>
    Timer? maker;
    /// <summary>The row's last slot when it has more than it shows: "See all".</summary>
    readonly SeeAllTile? seeAll;
    /// <summary>The ring that stays still at the gutter while the row slides under a held key.</summary>
    readonly View slideRing;
    Animation? scroll;
    Animation? slide;
    int focused = -1;
    /// <summary>Which way the row is sliding under a held key: 1, -1, or 0 when it isn't.</summary>
    int sliding;
    /// <summary>Focus moved in this row under a held key: its pictures are asked for when the key comes up.</summary>
    bool heldHere;

    public RowView(string? title, IReadOnlyList<TileModel> models, TileKind kind, KeyRepeatGate gate, Images images,
        Action<TileModel> onOpen, Action<TileModel>? onActions, Func<bool> toRail, Action<TileModel>? onFocus = null, Func<FocusItem, bool>? focusItem = null,
        (string Label, Action Open)? seeAll = null)
    {
        this.kind = kind;
        this.gate = gate;
        this.images = images;
        this.focusItem = focusItem;
        this.models = models;
        perScreen = RowReach.PerScreen(kind);
        Group = new FocusGroup(Axis.Row, title);
        // D-pad rule 3: Left from the first tile goes to the rail's current item; Right stops at the end.
        Group.Exit = d => d == Direction.Right || (d == Direction.Left && Scrolling.Left(gate, toRail));
        var top = 0f;
        if (title != null)
        {
            var header = Text.Label(title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
            header.Position = new Position(Ui.Dp(PleyTv.Gutter), 0);
            Add(header);
            top = Ui.Dp(PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles);
        }
        strip = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), top) };
        Add(strip);
        var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
        var w = Ui.Dp(kind.Width);
        var h = Ui.Dp(kind.Height);
        slideRing = new View
        {
            Size = new Size(w + 2 * reach, h + 2 * reach),
            Position = new Position(Ui.Dp(PleyTv.Gutter) - reach, top - reach),
            BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1f,
            CornerRadius = kind.Round ? (w + 2 * reach) / 2 : Ui.Dp(PleyShape.TileRadius) + reach,
        };
        slideRing.Hide();
        var step = Ui.Dp(kind.Width + kind.Gap);
        foreach (var model in models)
        {
            var item = Group.Add(new FocusItem($"{title}/{model.Id}"));
            item.Selected = () => onOpen(model);
            if (onActions != null)
            {
                item.LongSelected = () => onActions(model);
                item.Menu = () => onActions(model);
            }
            if (onFocus != null) item.FocusChanged += on => { if (on) onFocus(model); };
            items.Add(item);
        }
        if (seeAll is { } more)
        {
            var item = Group.Add(new FocusItem($"{title}/see-all") { Selected = more.Open });
            this.seeAll = new SeeAllTile(kind, more.Label, item) { Position = new Position(models.Count * step, 0) };
            strip.Add(this.seeAll);
        }
        // A tile costs a few milliseconds on a TV, and search's rows hold up to 200: the ones on
        // screen are made now, a screen more one at a time after, the rest as focus comes near
        // them (RowReach), or at once when focus gets there first.
        Make(Ahead(0));
        MakeSoon(MakerDelayMs);
        Add(slideRing);
        Group.ChildFocused += ScrollTo;
        gate.HoldEnded += Landed;
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), top + Ui.Dp(kind.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight));
    }

    public FocusGroup Group { get; }

    /// <summary>The places along the row: its tiles, and See all.</summary>
    int Slots => models.Count + (seeAll != null ? 1 : 0);

    /// <summary>The first tile on screen with focus at <paramref name="index"/>: the one cut off at the gutter's left.</summary>
    int First(int index) => Math.Max(0, (int)(-OffsetFor(index) / Ui.Dp(kind.Width + kind.Gap)) - 1);

    /// <summary>
    /// How many tiles from the row's start are on screen with focus at <paramref name="index"/>,
    /// with the few past the edge whose pictures are asked for ahead.
    /// </summary>
    int Ahead(int index) => RowReach.Now(First(index), perScreen, models.Count);

    /// <summary>
    /// Makes the tiles up to a screen past those on screen with focus where it is, one at a time
    /// from <paramref name="delayMs"/> on, unless they're made or being made.
    /// </summary>
    void MakeSoon(uint delayMs)
    {
        if (tiles.Count >= Soon() || maker?.IsRunning() == true) return;
        if (maker == null)
        {
            var timer = new Timer(delayMs);
            timer.Tick += (s, e) =>
            {
                if (Disposed) return false;
                if (timer.Interval != MakerMs) timer.Interval = MakerMs;
                Make(tiles.Count + 1);
                return tiles.Count < Soon();
            };
            maker = timer;
        }
        else maker.Interval = delayMs;
        maker.Start();
    }

    /// <summary>How many tiles from the row's start it makes while it waits, with focus where it is.</summary>
    int Soon() => RowReach.Soon(First(Math.Max(0, focused)), perScreen, models.Count);

    /// <summary>Makes the row's first <paramref name="count"/> tiles, those not made yet, each looking as its focus is.</summary>
    void Make(int count)
    {
        var step = Ui.Dp(kind.Width + kind.Gap);
        for (var i = tiles.Count; i < Math.Min(count, models.Count); i++)
        {
            var tile = new TileView(models[i], kind, items[i], gate, images) { Position = new Position(i * step, 0) };
            if (sliding != 0) tile.Quiet = true;
            else if (items[i].Focused) tile.Sync();
            tiles.Add(tile);
            strip.Add(tile);
        }
    }

    /// <summary>
    /// Where the strip sits for the tile at <paramref name="index"/>: its left edge at the gutter,
    /// or as far as the row goes; a row whose tiles all fit stays where it starts.
    /// </summary>
    float OffsetFor(int index)
    {
        var step = Ui.Dp(kind.Width + kind.Gap);
        var content = Slots * step - Ui.Dp(kind.Gap);
        var room = Ui.Dp(PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        if (content <= room) return 0;
        // The row's end padding (the safe area and a tile) lets the last tile come in from the edge.
        var most = content + Ui.Dp(PleyTv.SafeHorizontal + kind.Width) - Ui.Dp(PleyTv.CanvasWidth - PleyTv.Gutter);
        return -Math.Min(index * step, Math.Max(0, most));
    }

    void ScrollTo(int index)
    {
        var from = focused;
        focused = index;
        Make(Ahead(index));
        MakeSoon(MakerMs);
        // Under a held key, pictures wait: tiles flown past ask for none, and where focus lands loads them.
        if (gate.Holding) heldHere = true;
        else LoadPictures(index);
        var direction = from < 0 ? 0 : Math.Sign(index - from);
        // A held key: the row slides on by itself, at the hold's pace, until the key comes up.
        if (gate.Holding && direction != 0 && CanSlide(index, direction))
        {
            if (sliding != direction) Slide(direction);
            return;
        }
        if (sliding != 0) EndSlide(land: false);
        var x = Ui.Dp(PleyTv.Gutter) + OffsetFor(index);
        scroll?.Stop();
        scroll = Scrolling.Glide(gate);
        scroll.AnimateTo(strip, "PositionX", x);
        scroll.Play();
    }

    /// <summary>
    /// Whether the row can slide <paramref name="direction"/> with focus at <paramref name="index"/>:
    /// the focused tile is at the gutter, where the slide's ring stays, and the strip has room to
    /// move. Near the row's end, where tiles don't come to the gutter, focus steps across them
    /// first, so the ring never leaps from the tile to the gutter.
    /// </summary>
    bool CanSlide(int index, int direction)
    {
        var atGutter = Math.Abs(OffsetFor(index) + index * Ui.Dp(kind.Width + kind.Gap)) < 0.5f;
        return atGutter && (direction > 0 ? OffsetFor(index) > OffsetFor(Slots - 1) : index > 0);
    }

    /// <summary>
    /// The PSP's XMB way: the strip slides to the row's end as one animation, at a tile per move of
    /// the hold (NUI runs it on its own thread, so nothing the app does makes it hitch), under a ring
    /// that stays still at the gutter; the tiles keep their resting look meanwhile.
    /// </summary>
    void Slide(int direction)
    {
        scroll?.Stop();
        slide?.Stop();
        sliding = direction;
        // The tiles it reaches are made as focus moves along under the hold (ScrollTo), ahead of the screen's edge.
        foreach (var t in tiles) t.Quiet = true;
        slideRing.Show();
        var target = Ui.Dp(PleyTv.Gutter) + OffsetFor(direction > 0 ? Slots - 1 : 0);
        var from = strip.CurrentPosition.X;
        var perMs = Ui.Dp(kind.Width + kind.Gap) / gate.Interval;
        var ms = (int)(Math.Abs(target - from) / perMs);
        if (ms <= 0) return;
        slide = new Animation(ms) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.Linear) };
        slide.AnimateTo(strip, "PositionX", target);
        slide.Play();
    }

    /// <summary>
    /// The slide ends. The key came up (<paramref name="land"/>): focus lands on the tile under the
    /// ring, or the next one the way it was going, so the row only ever settles forward; the focus
    /// the hold's clock moved along can be a tile either side of it. Or the slide reached where the
    /// tiles stop coming to the gutter, and focus steps on from there as it is.
    /// </summary>
    void EndSlide(bool land)
    {
        if (sliding == 0) return;
        var direction = sliding;
        sliding = 0;
        slide?.Stop();
        slide = null;
        slideRing.Hide();
        foreach (var t in tiles) t.Quiet = false;
        var target = Math.Max(0, focused);
        if (land)
        {
            var under = (Ui.Dp(PleyTv.Gutter) - strip.CurrentPosition.X) / Ui.Dp(kind.Width + kind.Gap);
            target = direction > 0 ? (int)Math.Ceiling(under - 0.01) : (int)Math.Floor(under + 0.01);
            target = Math.Max(0, Math.Min(Slots - 1, target));
            // Focus there scrolls the row to it (ScrollTo), the hold being over.
            if (target != focused && focusItem != null && focusItem(target < items.Count ? items[target] : seeAll!.Focus)) return;
        }
        scroll?.Stop();
        scroll = new Animation(PleyMotion.FocusMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        scroll.AnimateTo(strip, "PositionX", Ui.Dp(PleyTv.Gutter) + OffsetFor(target));
        scroll.Play();
    }

    void Landed()
    {
        EndSlide(land: true);
        if (!heldHere) return;
        heldHere = false;
        LoadPictures(Math.Max(0, focused));
    }

    /// <summary>Asks for the pictures on screen: the page brings the row near.</summary>
    public void ShowPictures() => LoadPictures(Math.Max(0, focused));

    /// <summary>
    /// A row a page lets go of (search's, as the next answer comes) stops making tiles; its tiles go
    /// with it, so the one focus leaves has no animation running once nothing holds it.
    /// </summary>
    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        gate.HoldEnded -= Landed;
        // Only when disposed by the app: collected, the timer and animations may have gone first (Kit/Held.cs).
        if (type == DisposeTypes.Explicit)
        {
            maker?.Stop();
            maker?.Dispose();
            scroll?.Stop();
            slide?.Stop();
            foreach (var tile in tiles) tile.Dispose();
        }
        base.Dispose(type);
    }

    /// <summary>Asks for the pictures of the tiles on screen with focus at <paramref name="index"/>, and a few past them.</summary>
    void LoadPictures(int index)
    {
        var count = Ahead(index);
        Make(count);
        for (var i = First(index); i < count; i++) tiles[i].LoadPicture();
    }
}

/// <summary>
/// A page of rows (the Android app's TvRowColumn, design spec §2.4, §4.1): rows stacked with a 28 dp
/// gap; the focused row's header comes to y = 172, and while a key repeats the page glides, faster
/// the longer the key is held. Rows scrolled above the focused one aren't drawn, leaving the page's own
/// header (or backdrop) clear.
/// </summary>
sealed class RowColumnView : View
{
    readonly KeyRepeatGate gate;
    readonly float headerY;
    readonly bool firstAtTop;
    readonly View content;
    readonly List<View> rows = new List<View>();
    /// <summary>Each section's line, where its top comes when it has focus.</summary>
    readonly List<float> lines = new List<float>();
    /// <summary>Rows below the focused one that ask for their pictures ahead.</summary>
    const int RowsNear = 2;
    int current;
    /// <summary>Focus moved between rows under a held key: rows near where it lands load when the key comes up.</summary>
    bool heldHere;
    Animation? scroll;

    /// <param name="firstAtTop">The first row stays where the page put it (a page with a header); false: it comes to the line too (Home, whose hero goes as the rows come up).</param>
    public RowColumnView(KeyRepeatGate gate, float top, float headerY = PleyTv.FocusedRowHeaderY, bool firstAtTop = true)
    {
        this.gate = gate;
        this.headerY = headerY;
        this.firstAtTop = firstAtTop;
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
        content = new View { PositionY = Ui.Dp(top) };
        Top = Ui.Dp(top);
        Add(content);
        Group = new FocusGroup(Axis.Column, "rows");
        Group.ChildFocused += ScrollTo;
        gate.HoldEnded += Landed;
    }

    public FocusGroup Group { get; }

    /// <summary>Where the first row starts while the page is at its top, in pixels.</summary>
    public float Top { get; }

    public void Add(RowView row) => Add(row, row.Group);

    /// <summary>A section that isn't a plain row (an item's seasons and episodes), with its focus group.</summary>
    /// <param name="line">Where its top comes when it has focus; the page's line (y = 172) without.</param>
    public void Add(View row, FocusGroup group, float? line = null)
    {
        lines.Add(line ?? headerY);
        var y = 0f;
        foreach (var r in rows) y += r.SizeHeight + Ui.Dp(PleyTv.RowGap);
        row.PositionY = y;
        rows.Add(row);
        content.Add(row);
        Group.Add(group);
        // The rows on screen at first ask for their pictures; the rest, as they come near.
        if (rows.Count <= RowsNear) (row as RowView)?.ShowPictures();
    }

    void ScrollTo(int index)
    {
        current = index;
        if (gate.Holding) heldHere = true;
        else Near(index);
        // The first row stays where the page put it; others bring their header to the line.
        var target = index == 0 && firstAtTop ? Top : Ui.Dp(lines[index]) - rows[index].PositionY;
        scroll?.Stop();
        scroll = Scrolling.Glide(gate);
        scroll.AnimateTo(content, "PositionY", target);
        scroll.Play();
        // Once the page has scrolled, nothing is drawn above the focused row's header (less a
        // little room): the rows above it go, leaving the page's header or backdrop clear.
        for (var i = 0; i < rows.Count; i++)
        {
            if (index > 0 && i < index) rows[i].Hide();
            else rows[i].Show();
        }
    }

    /// <summary>Rows near the screen's one with focus ask for their pictures: the row above and two below.</summary>
    void Near(int index)
    {
        for (var i = Math.Max(0, index - 1); i <= Math.Min(rows.Count - 1, index + RowsNear); i++) (rows[i] as RowView)?.ShowPictures();
    }

    void Landed()
    {
        if (!heldHere) return;
        heldHere = false;
        Near(current);
    }

    protected override void Dispose(global::Tizen.NUI.DisposeTypes type)
    {
        if (Disposed) return;
        gate.HoldEnded -= Landed;
        base.Dispose(type);
    }

    /// <summary>Back to the page's top: the first row where the page put it, every row drawn.</summary>
    public void ScrollToTop()
    {
        scroll?.Stop();
        scroll = new Animation(PleyMotion.FocusMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        scroll.AnimateTo(content, "PositionY", Top);
        scroll.Play();
        foreach (var r in rows) r.Show();
    }
}

/// <summary>How rows and pages move to their focused tile or row.</summary>
static class Scrolling
{
    /// <summary>
    /// Left from the start of a row (D-pad rule 3): the rail on a press. A held Left stops there, so
    /// scrolling back along a row never opens the rail by itself; true either way, focus staying put.
    /// </summary>
    public static bool Left(KeyRepeatGate gate, Func<bool> toRail) => gate.Holding || toRail();

    /// <summary>
    /// A move's animation: eased over 150 ms for a press; for a held key, evenly over a little more
    /// than the time to the next move (<see cref="KeyRepeatGate.GlideMs"/>), so each step runs into
    /// the next without a pause and the row glides, faster the longer the key is held.
    /// </summary>
    public static Animation Glide(KeyRepeatGate gate) => gate.Holding
        ? new Animation(gate.GlideMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.Linear) }
        : new Animation(PleyMotion.FocusMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
}
