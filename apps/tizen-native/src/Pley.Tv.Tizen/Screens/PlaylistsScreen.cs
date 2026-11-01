using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The playlists page on TV (design spec §4.9; the Android app's TvPlaylistsScreen): "Playlists"
/// with New playlist at the right safe edge, then a grid of square covers (a mosaic of the first
/// four entries), five across, captioned with the title and "12 items · 3h 12m". Focus starts on
/// the first cover, or on New playlist when there are none, and comes back to the one opened.
/// </summary>
sealed class PlaylistsScreen : ShellPage
{
    const int Columns = PlaylistRules.Columns;
    const float HeaderHeight = PleyTv.ButtonHeight, GridTop = 28, RowGap = 22, MessageWidth = 520;

    readonly PlaylistsModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly IDisposable watch;
    readonly ButtonRow header = new ButtonRow();
    readonly View body = new View();
    readonly View covers = new View();
    readonly FocusGroup grid = new FocusGroup(Axis.Grid, "playlists") { Columns = Columns };
    readonly FocusGroup content = new FocusGroup(Axis.Column, "content");
    IReadOnlyList<PlaylistCard>? shown;
    string? lastCover;
    float scroll;

    public PlaylistsScreen(TvShell shell, PlaylistsModel model, Images images, KeyRepeatGate gate, Action<PageKey> go) : base(shell, new PageKey.Playlists(), "playlists")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        var title = Text.Label(PlaylistText.PlaylistsTitle, PleyType.PageTitle, PleyColors.Fg, width: 480);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical + (HeaderHeight - PleyType.PageTitle.Line) / 2));
        View.Add(title);
        header.Add(new ButtonView(PlaylistText.New, ButtonKind.Pill, () => go(new NewPlaylistPage()), iconName: "plus"));
        header.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - header.WidthDp), Ui.Dp(PleyTv.SafeVertical));
        View.Add(header);
        var top = PleyTv.SafeVertical + HeaderHeight;
        body.Position = new Position(0, Ui.Dp(top));
        body.Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight - top));
        body.ClippingMode = ClippingModeType.ClipChildren;
        View.Add(body);
        Group.Add(header.Group);
        Group.Add(content);
        // Left goes to the rail; Right stops at the edge.
        header.Group.Exit = d => d == Direction.Left ? Shell.FocusRail() : d == Direction.Right;
        grid.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
        grid.ChildFocused += i =>
        {
            if (shown != null && i < shown.Count) lastCover = "playlist/" + shown[i].Id;
            Reveal(i / Columns);
        };
        watch = model.State.Watch(Show);
    }

    public override void Enter()
    {
        if (!Tree.Enter(content) && !Tree.Enter(Group)) Shell.FocusRail();
    }

    public override void Gone()
    {
        watch.Dispose();
        model.Dispose();
        // Not on the page while a message shows: it goes with the page otherwise.
        if (covers.GetParent() == null) covers.Dispose();
    }

    float Pitch => TileKind.Square.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight + RowGap;

    void Show(Loadable<IReadOnlyList<PlaylistCard>> state)
    {
        var list = state.Value;
        var focusedName = Tree.Current?.Name;
        var hadFocus = Tree.Current != null && InPage(Tree.Current);
        if (PlaylistText.ShowsNew(state)) header.Show();
        else header.Hide();
        foreach (var child in header.Group.Children.OfType<FocusItem>()) child.Shown = PlaylistText.ShowsNew(state);
        if (list != null && shown != null && Same(list, shown)) return;
        var wasLoading = shown == null;
        shown = list;

        // Let go only once focus is on the new covers: the one with focus hears it leave.
        if (covers.GetParent() != null) body.Remove(covers);
        var old = body.Children.ToList();
        foreach (var child in old) body.Remove(child);
        content.Clear();
        grid.Clear();
        foreach (var child in covers.Children.ToList())
        {
            covers.Remove(child);
            old.Add(child);
        }
        scroll = 0;
        covers.PositionY = Ui.Dp(GridTop);

        if (PlaylistText.PlaylistsMessage(state) is { } message)
        {
            Message(message, hadFocus);
        }
        else if (list == null)
        {
            Skeletons();
        }
        else
        {
            body.Add(covers);
            content.Add(grid);
            for (var i = 0; i < list.Count; i++)
            {
                var card = list[i];
                var cover = new CoverTile(card, images);
                cover.Position = new Position(Ui.Dp(PleyTv.Gutter + i % Columns * (TileKind.Square.Width + PleyTv.TileGap)), Ui.Dp(i / Columns * Pitch));
                cover.Item.Selected = () => go(new PageKey.Playlist(card.Id));
                covers.Add(cover);
                grid.Add(cover.Item);
            }
        }

        // Focus where it was, the cover opened last when coming back; else where the page starts.
        var name = hadFocus ? focusedName : lastCover;
        // New playlist keeps focus, unless it had it only while the covers loaded.
        var keepsNew = hadFocus && Tree.Current?.Parent == header.Group && !wasLoading && header.Group.CanTakeFocus;
        var again = name == null ? null : grid.Children.OfType<FocusItem>().FirstOrDefault(c => c.Name == name);
        if (again != null)
        {
            grid.RememberChild(grid.Children.ToList().IndexOf(again));
            if (hadFocus) Tree.Focus(again);
        }
        else if (!keepsNew && (hadFocus || Tree.Current == null)) Enter();
        foreach (var child in old) child.Dispose();
    }

    /// <summary>Whether the covers would look the same: a refresh that changed nothing keeps them.</summary>
    static bool Same(IReadOnlyList<PlaylistCard> a, IReadOnlyList<PlaylistCard> b) =>
        a.Count == b.Count && a.Zip(b, (x, y) => x.Id == y.Id && x.Title == y.Title && x.Caption == y.Caption && x.Video == y.Video && x.Artwork.SequenceEqual(y.Artwork)).All(same => same);

    bool InPage(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == Group) return true;
        return false;
    }

    /// <summary>Keeps the focused cover's row in view, with the grid's padding above and below.</summary>
    void Reveal(int row)
    {
        var room = PleyTv.CanvasHeight - PleyTv.SafeVertical - HeaderHeight;
        var top = GridTop + row * Pitch;
        // The caption's bottom, and room for the next row's ring to peek in.
        var bottom = top + Pitch;
        var target = scroll;
        if (top - GridTop < target) target = top - GridTop;
        else if (bottom > target + room) target = bottom - room;
        if (target == scroll) return;
        scroll = target;
        var move = Scrolling.Glide(gate);
        move.AnimateTo(covers, "PositionY", Ui.Dp(GridTop - scroll));
        move.Play();
    }

    /// <summary>None yet: a heading and a sentence, 40 under the header; or a failure with Retry.</summary>
    void Message(LibraryMessage m, bool hadFocus)
    {
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(40)) };
        var title = Text.Label(m.Title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        view.Add(title);
        var words = PanelColumn.Lines(m.Body, PleyType.Body, PleyColors.Muted, m.ActionLabel == null ? PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal : MessageWidth, 3);
        words.PositionY = Ui.Dp(PleyType.Section.Line + 8);
        view.Add(words);
        body.Add(view);
        if (m.ActionLabel is { } action)
        {
            var row = new ButtonRow();
            row.Add(new ButtonView(action, ButtonKind.Pill, model.Retry, iconName: "refresh-cw"));
            row.PositionY = Ui.Dp(PleyType.Section.Line + 8 + 16) + words.SizeHeight;
            view.Add(row);
            row.Group.Exit = d => d == Direction.Left && Shell.FocusRail();
            content.Add(row.Group);
        }
        // With none, New playlist takes focus.
        else if (hadFocus || Tree.Current == null) Tree.Enter(header.Group);
    }

    /// <summary>One still row of five covers while nothing is known.</summary>
    void Skeletons()
    {
        var row = new View { PositionY = Ui.Dp(GridTop) };
        var size = TileKind.Square.Width;
        for (var i = 0; i < Columns; i++)
        {
            var x = PleyTv.Gutter + i * (size + PleyTv.TileGap);
            row.Add(new View { Position = new Position(Ui.Dp(x), 0), Size = new Size(Ui.Dp(size), Ui.Dp(size)), CornerRadius = Ui.Dp(PleyShape.TileRadius), BackgroundColor = PleyColors.Surface2 });
            var caption = size + PleyTv.TilesToCaption;
            row.Add(new View { Position = new Position(Ui.Dp(x), Ui.Dp(caption + 3)), Size = new Size(Ui.Dp(size * 0.6f), Ui.Dp(12)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
            row.Add(new View { Position = new Position(Ui.Dp(x), Ui.Dp(caption + 23)), Size = new Size(Ui.Dp(size * 0.35f), Ui.Dp(12)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
        }
        body.Add(row);
    }

    /// <summary>
    /// A playlist's cover in the grid (the Android app's TvTileFrame around TvPlaylistArt): the
    /// square art, and its title and "12 items · 3h 12m · Smart" under it. Focused, the art grows
    /// to 1.08 with the accent ring and the title turns bold.
    /// </summary>
    sealed class CoverTile : View
    {
        readonly View frame;
        readonly View ring;
        readonly TextLabel title;
        Animation? focusAnimation;

        public CoverTile(PlaylistCard card, Images images)
        {
            var size = TileKind.Square.Width;
            var w = Ui.Dp(size);
            Size = new Size(w, Ui.Dp(size + PleyTv.TilesToCaption + PleyTv.CaptionHeight));
            Item = new FocusItem("playlist/" + card.Id);
            frame = new View { Size = new Size(w, w), PivotPoint = global::Tizen.NUI.PivotPoint.Center, PositionUsesPivotPoint = false };
            var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
            ring = new View
            {
                Size = new Size(w + 2 * reach, w + 2 * reach),
                Position = new Position(-reach, -reach),
                BackgroundColor = Color.Transparent,
                BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
                BorderlineColor = PleyColors.AccentBright,
                BorderlineOffset = -1f,
                CornerRadius = Ui.Dp(PleyShape.TileRadius) + reach,
                Opacity = 0,
            };
            frame.Add(ring);
            frame.Add(new PlaylistArtView(card, images, size));
            Add(frame);
            var captionTop = Ui.Dp(size + PleyTv.TilesToCaption);
            title = Text.Label(card.Title, PleyType.Caption, PleyColors.Fg, width: size);
            title.PositionY = captionTop;
            Add(title);
            if (card.Caption.Length > 0)
            {
                var line = Text.Label(card.Caption, PleyType.Caption, PleyColors.Muted, width: size);
                line.PositionY = captionTop + Ui.Dp(PleyType.Caption.Line);
                Add(line);
            }
            Item.FocusChanged += Look;
        }

        public FocusItem Item { get; }

        void Look(bool on)
        {
            // Let go already: a focus change can still reach it on its way to the view in its place.
            if (Disposed) return;
            if (on) RaiseToTop();
            title.FontStyle = Text.Weight(on ? PleyType.Bold : PleyType.Caption.Weight);
            var scale = on ? PleyShape.TileScale : 1f;
            focusAnimation?.Stop();
            focusAnimation = new Animation(on ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
            {
                DefaultAlphaFunction = new AlphaFunction(on ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
            };
            focusAnimation.AnimateTo(frame, "Scale", new Vector3(scale, scale, 1));
            focusAnimation.AnimateTo(ring, "Opacity", on ? 1f : 0f);
            focusAnimation.Play();
        }

        protected override void Dispose(DisposeTypes type)
        {
            if (Disposed) return;
            Item.FocusChanged -= Look;
            focusAnimation?.Stop();
            base.Dispose(type);
        }
    }
}

/// <summary>
/// A playlist's square art (the Android app's TvPlaylistArt; the web's PlaylistArt): a 2 by 2
/// mosaic of its first four entries' pictures, or its first entry's picture over the title's own
/// gradient, or, empty, its kind's glyph (list-video or list-music) on a Surface3 to Surface2
/// gradient; with the tile's 1 dp hairline.
/// </summary>
sealed class PlaylistArtView : View
{
    public PlaylistArtView(PlaylistCard card, Images images, float size, float glyph = 48, float radius = PleyShape.TileRadius)
    {
        var w = Ui.Dp(size);
        Size = new Size(w, w);
        CornerRadius = Ui.Dp(radius);
        BackgroundColor = PleyColors.Surface2;
        ClippingMode = ClippingModeType.ClipChildren;
        var art = card.Artwork;
        if (art.Count >= 4)
        {
            var half = size / 2;
            for (var i = 0; i < 4; i++) Add(Picture(images, art[i], half, (i % 2) * half, (i / 2) * half));
        }
        else if (art.Count > 0)
        {
            var (from, to) = Placeholder.Colors(card.Title);
            var well = new View { Size = new Size(w, w), Background = Gradients.Diagonal(AvatarView.Rgb(from), AvatarView.Rgb(to)) };
            if (size >= 80)
            {
                var name = Text.Label(card.Title, PleyType.Title, new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.7f), maxLines: 4);
                name.Size = new Size(w - Ui.Dp(24), w - Ui.Dp(24));
                name.Position = new Position(Ui.Dp(12), Ui.Dp(12));
                name.HorizontalAlignment = HorizontalAlignment.Center;
                well.Add(name);
            }
            Add(well);
            Add(Picture(images, art[0], size, 0, 0));
        }
        else
        {
            var well = new View { Size = new Size(w, w), Background = Gradients.Diagonal(PleyColors.Surface3, PleyColors.Surface2) };
            var icon = Icons.Icon(card.Video ? "list-video" : "list-music", glyph, PleyColors.Subtle);
            icon.Position = new Position(Ui.Dp((size - glyph) / 2), Ui.Dp((size - glyph) / 2));
            well.Add(icon);
            Add(well);
        }
        Add(new View
        {
            Size = new Size(w, w),
            CornerRadius = Ui.Dp(radius),
            BackgroundColor = Color.Transparent,
            BorderlineWidth = Ui.Dp(1),
            BorderlineColor = PleyColors.TileHairline,
            BorderlineOffset = -1,
        });
    }

    /// <summary>One picture, <paramref name="size"/> square at (x, y), fading in once it has loaded.</summary>
    ImageView Picture(Images images, string url, float size, float x, float y)
    {
        var px = Ui.Dp(size);
        var picture = new ImageView
        {
            Size = new Size(px, px),
            Position = new Position(Ui.Dp(x), Ui.Dp(y)),
            DesiredWidth = (int)px,
            DesiredHeight = (int)px,
            FittingMode = FittingModeType.ScaleToFill,
            Opacity = 0,
        };
        images.Load(new[] { url }, size, file =>
        {
            if (file == null || Disposed) return;
            picture.ResourceUrl = file;
            var fade = new Animation(PleyMotion.ImageFadeMs);
            fade.AnimateTo(picture, "Opacity", 1f);
            fade.Play();
        });
        return picture;
    }
}
