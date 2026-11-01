using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A TV grid of tiles (the Android app's TvPosterGrid, design spec §4.2): <see cref="Columns"/>
/// wells from the gutter, a tile's gap apart, 20 dp between a caption and the next row.
/// <list type="bullet">
/// <item>It has a focus item for every position, loaded or not: a cell whose page hasn't come is a
/// still skeleton that takes focus like a tile, so the D-pad runs through a big library's unloaded
/// stretches too. Only the rows near the screen have views; the rest are made as focus comes.</item>
/// <item>The focused row's wells stay <see cref="TileRoom"/> below the grid's top, so the next row
/// always peeks in; a held key moves a row at a time, gliding.</item>
/// <item>Pictures load for the rows near the screen, not under a held key: where it lands loads when
/// it comes up.</item>
/// </list>
/// </summary>
sealed class GridView : View
{
    /// <summary>Between a caption and the next row's wells.</summary>
    public const float RowGap = 20;
    /// <summary>Room above the first row for the focused tile's growth and ring.</summary>
    public const float TileRoom = 14;

    readonly TileKind kind;
    readonly KeyRepeatGate gate;
    readonly Images images;
    readonly Func<int, TileModel?> tileAt;
    readonly Func<int, string?>? badgeAt;
    readonly View content = new View();
    readonly Dictionary<int, View> cells = new Dictionary<int, View>();
    readonly Dictionary<int, TileModel?> drawn = new Dictionary<int, TileModel?>();
    readonly float pitch;
    readonly float height;
    FocusItem[] items = Array.Empty<FocusItem>();
    Animation? scroll;
    int topRow;
    bool heldHere;
    /// <summary>
    /// While Up or Down is held, one ring stays still on the top row and the rows slide up or down
    /// under it, the tiles keeping their resting look (a row's held Left and Right, RowView.Slide):
    /// a tile growing and shrinking at each step, the ring with it, made the page stutter.
    /// </summary>
    readonly View slideRing;
    bool quiet;

    /// <param name="height">The grid's height in dp, from its top (the wells' top less <see cref="TileRoom"/>).</param>
    /// <param name="badgeAt">A count on a tile's top right (a collection's titles).</param>
    public GridView(TileKind kind, int columns, float height, KeyRepeatGate gate, Images images, Func<int, TileModel?> tileAt, Func<int, string?>? badgeAt = null)
    {
        this.kind = kind;
        this.gate = gate;
        this.images = images;
        this.tileAt = tileAt;
        this.badgeAt = badgeAt;
        this.height = height;
        Columns = columns;
        pitch = kind.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight + RowGap;
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(height));
        ClippingMode = ClippingModeType.ClipChildren;
        content.PositionY = Ui.Dp(TileRoom);
        Add(content);
        var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
        slideRing = new View
        {
            Size = new Size(Ui.Dp(kind.Width) + 2 * reach, Ui.Dp(kind.Height) + 2 * reach),
            PositionY = Ui.Dp(TileRoom) - reach,
            BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1f,
            CornerRadius = kind.Round ? (Ui.Dp(kind.Width) + 2 * reach) / 2 : Ui.Dp(PleyShape.TileRadius) + reach,
        };
        slideRing.Hide();
        Add(slideRing);
        Group = new FocusGroup(Axis.Grid, "grid") { Columns = columns };
        Group.ChildFocused += Focused;
        gate.HoldEnded += Landed;
    }

    public FocusGroup Group { get; }

    public int Columns { get; }

    public int Total => items.Length;

    /// <summary>The focused position; -1 before any.</summary>
    public int Current { get; private set; } = -1;

    /// <summary>OK on a loaded tile, by position.</summary>
    public Action<int, TileModel>? Open { get; set; }

    /// <summary>OK held or Menu on a loaded tile.</summary>
    public Action<int, TileModel>? Actions { get; set; }

    /// <summary>Focus moved to a position: the jump's letter follows it.</summary>
    public event Action<int>? Moved;

    /// <summary>The first position of the rows on screen changed: a listing reads around it.</summary>
    public event Action<int>? Showing;

    /// <summary>The focus item of <paramref name="position"/>, to focus it.</summary>
    public FocusItem? ItemAt(int position) => position >= 0 && position < items.Length ? items[position] : null;

    /// <summary>
    /// The grid holds <paramref name="total"/> positions, its tiles from <c>tileAt</c> as they are
    /// now (a page came in, a tile changed). The positions' focus items stay while the total does,
    /// so focus stays where it was.
    /// </summary>
    public void Update(int total)
    {
        if (total != items.Length)
        {
            foreach (var cell in cells.Values) cell.Dispose();
            cells.Clear();
            drawn.Clear();
            // The positions' focus items go: tiles that listen to them go too.
            while (spare.Count > 0) spare.Pop().Dispose();
            counts.Clear();
            Group.Clear();
            items = new FocusItem[total];
            for (var i = 0; i < total; i++)
            {
                var at = i;
                var item = new FocusItem("cell" + i)
                {
                    Selected = () =>
                    {
                        if (tileAt(at) is { } tile) Open?.Invoke(at, tile);
                    },
                };
                item.LongSelected = () =>
                {
                    if (tileAt(at) is { } tile) Actions?.Invoke(at, tile);
                };
                item.Menu = item.LongSelected;
                items[i] = Group.Add(item);
            }
            if (Current >= total) Current = total - 1;
        }
        Draw();
    }

    /// <summary>Back to the first row (a new sort or filter): focus, if it is here, goes to the first tile.</summary>
    public void ScrollToTop(FocusTree tree)
    {
        Group.ForgetLast();
        Current = -1;
        topRow = 0;
        scroll?.Stop();
        content.PositionY = Ui.Dp(TileRoom);
        if (tree.Current != null && tree.Current.Parent == Group && items.Length > 0) tree.Focus(items[0]);
        Draw();
    }

    /// <summary>Brings <paramref name="position"/>'s row to the top at once (a jump), before focusing it.</summary>
    public void Jump(int position)
    {
        if (position < 0 || position >= items.Length) return;
        topRow = position / Columns;
        scroll?.Stop();
        content.PositionY = Ui.Dp(TileRoom - topRow * pitch);
        Draw();
    }

    void Focused(int index)
    {
        var took = Profile.Start();
        Current = index;
        var row = index / Columns;
        if (row != topRow)
        {
            // A held Up or Down: the rows slide under a still ring (quiet).
            if (gate.Holding && !quiet) Quieten(true);
            topRow = row;
            scroll?.Stop();
            scroll = Scrolling.Glide(gate);
            scroll.AnimateTo(content, "PositionY", Ui.Dp(TileRoom - row * pitch));
            scroll.Play();
            Showing?.Invoke(row * Columns);
        }
        if (gate.Holding) heldHere = true;
        if (quiet) slideRing.PositionX = Ui.Dp(PleyTv.Gutter + index % Columns * (kind.Width + PleyTv.TileGap)) - Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
        Draw();
        Moved?.Invoke(index);
        Profile.Took("grid: a move", took);
    }

    void Landed()
    {
        if (Disposed) return;
        if (quiet) Quieten(false);
        if (!heldHere) return;
        heldHere = false;
        LoadPictures();
    }

    /// <summary>The tiles keep their resting look under the still ring (on), or show their focus again (off).</summary>
    void Quieten(bool on)
    {
        quiet = on;
        if (on) slideRing.Show();
        else slideRing.Hide();
        foreach (var cell in cells.Values)
            if (cell is TileView tile) tile.Quiet = on;
    }

    /// <summary>Tiles off screen, kept to show the rows coming on: making a row's tiles was most of a move's cost.</summary>
    readonly Stack<TileView> spare = new Stack<TileView>();

    /// <summary>A position's count badge (a collection's titles), on the tile showing it.</summary>
    readonly Dictionary<int, View> counts = new Dictionary<int, View>();

    /// <summary>Rows on screen and one either side have views; others go, their tiles kept for the rows coming.</summary>
    void Draw()
    {
        if (Disposed) return;
        var rows = (int)Math.Ceiling(height / pitch) + 1;
        var first = Math.Max(0, topRow - 1) * Columns;
        var last = Math.Min(items.Length - 1, (topRow + rows) * Columns + Columns - 1);
        foreach (var position in cells.Keys.ToList())
        {
            if (position >= first && position <= last) continue;
            Let(position);
        }
        for (var p = first; p <= last; p++)
        {
            var tile = tileAt(p);
            if (cells.TryGetValue(p, out var cell))
            {
                if (ReferenceEquals(drawn[p], tile)) continue;
                Let(p);
            }
            cell = tile != null ? Tile(p, tile) : new SkeletonCell(kind, items[p], gate);
            cell.Position = new Position(Ui.Dp(PleyTv.Gutter + p % Columns * (kind.Width + PleyTv.TileGap)), Ui.Dp(p / Columns * pitch));
            if (cell.GetParent() == null) content.Add(cell);
            cells[p] = cell;
            drawn[p] = tile;
        }
        // Focused tiles draw over their neighbours.
        if (Current >= 0 && cells.TryGetValue(Current, out var focused)) focused.RaiseToTop();
        if (!gate.Holding) LoadPictures();
    }

    /// <summary>The cell at <paramref name="position"/> goes: a tile is kept for another, hidden; a skeleton goes.</summary>
    void Let(int position)
    {
        var cell = cells[position];
        if (counts.TryGetValue(position, out var count))
        {
            count.Dispose();
            counts.Remove(position);
        }
        if (cell is TileView tile)
        {
            tile.Hide();
            spare.Push(tile);
        }
        else cell.Dispose();
        cells.Remove(position);
        drawn.Remove(position);
    }

    View Tile(int position, TileModel tile)
    {
        TileView view;
        if (spare.Count > 0)
        {
            view = spare.Pop();
            view.Bind(tile, items[position]);
            view.Show();
        }
        else
        {
            view = new TileView(tile, kind, items[position], gate, images);
            view.Sync();
        }
        view.Quiet = quiet;
        if (badgeAt?.Invoke(position) is { } badge)
        {
            var text = Text.Label(badge, PleyType.Badge, PleyColors.Fg);
            var pill = new View
            {
                Size = new Size(Math.Max(Ui.Dp(18), Ui.Dp(12) + text.NaturalSize2D.Width), Ui.Dp(18)),
                BackgroundColor = PleyColors.OnArt,
                CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
            };
            pill.Position = new Position(Ui.Dp(kind.Width - 6) - pill.SizeWidth, Ui.Dp(6));
            text.Size = new Size(pill.SizeWidth, pill.SizeHeight);
            text.HorizontalAlignment = HorizontalAlignment.Center;
            text.VerticalAlignment = VerticalAlignment.Center;
            pill.Add(text);
            view.Add(pill);
            counts[position] = pill;
        }
        return view;
    }

    void LoadPictures()
    {
        foreach (var cell in cells.Values)
            if (cell is TileView tile) tile.LoadPicture();
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        gate.HoldEnded -= Landed;
        scroll?.Stop();
        base.Dispose(type);
    }

    /// <summary>A cell whose page hasn't come: a still well and two caption bars, with the tile's focus ring.</summary>
    sealed class SkeletonCell : View
    {
        readonly FocusItem focus;
        readonly View frame;
        readonly View ring;

        public SkeletonCell(TileKind kind, FocusItem focus, KeyRepeatGate gate)
        {
            this.focus = focus;
            var w = Ui.Dp(kind.Width);
            var h = Ui.Dp(kind.Height);
            Size = new Size(w, h + Ui.Dp(PleyTv.TilesToCaption + PleyTv.CaptionHeight));
            frame = new View { Size = new Size(w, h), PivotPoint = global::Tizen.NUI.PivotPoint.Center, PositionUsesPivotPoint = false };
            var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
            var radius = kind.Round ? w / 2 : Ui.Dp(PleyShape.TileRadius);
            ring = new View
            {
                Size = new Size(w + 2 * reach, h + 2 * reach),
                Position = new Position(-reach, -reach),
                BackgroundColor = Color.Transparent,
                BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
                BorderlineColor = PleyColors.AccentBright,
                BorderlineOffset = -1f,
                CornerRadius = kind.Round ? (w + 2 * reach) / 2 : radius + reach,
                Opacity = 0,
            };
            frame.Add(ring);
            frame.Add(new View { Size = new Size(w, h), CornerRadius = radius, BackgroundColor = PleyColors.Surface2 });
            Add(frame);
            var top = h + Ui.Dp(PleyTv.TilesToCaption);
            Add(new View { Position = new Position(0, top + Ui.Dp(3)), Size = new Size(w * 0.6f, Ui.Dp(12)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
            Add(new View { Position = new Position(0, top + Ui.Dp(23)), Size = new Size(w * 0.35f, Ui.Dp(12)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
            focus.FocusChanged += Look;
            Look(focus.Focused);
        }

        void Look(bool on)
        {
            if (Disposed) return;
            if (on) RaiseToTop();
            ring.Opacity = on ? 1 : 0;
            var scale = on ? PleyShape.TileScale : 1f;
            frame.Scale = new Vector3(scale, scale, 1);
        }

        protected override void Dispose(DisposeTypes type)
        {
            if (Disposed) return;
            focus.FocusChanged -= Look;
            base.Dispose(type);
        }
    }
}
