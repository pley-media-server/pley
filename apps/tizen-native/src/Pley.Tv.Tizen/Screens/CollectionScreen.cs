using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A collection's page on TV (design spec §4.9; the Android app's TvCollectionScreen): the item
/// page's top, a backdrop with the text column at x = 96 ("Collection" and its TMDB or Smart
/// badge, the title, what it holds in words, the summary, then Play with focus and Shuffle), and
/// its titles in a grid below, which scrolls up over the backdrop as focus moves down, the
/// backdrop dimming under it.
/// </summary>
sealed class CollectionScreen : ShellPage
{
    /// <summary>The text column: x 96 to 560, in the backdrop's 85 % scrim.</summary>
    const float TextColumn = PleyTv.TextColumnEnd - PleyTv.Gutter;
    const float HeroTop = 58;
    /// <summary>The page's top, to the grid's first row of wells (design spec §4.3: what is below starts at 452).</summary>
    const float HeroHeight = 452 - 16;
    /// <summary>Under a round button, the room its label takes while it has focus.</summary>
    const float ButtonLabelRoom = 24;
    const float BadgeHeight = 18;
    /// <summary>The grid's top at rest, its wells a row's gap under the hero, and once focus is in it, its focused row's wells at y = 58.</summary>
    const float GridTop = HeroHeight + GridView.RowGap - GridView.TileRoom, GridTopScrolled = HeroTop - GridView.TileRoom;
    /// <summary>The dimming over the backdrop once the grid has passed over it.</summary>
    const float Dim = 0.92f;

    readonly CollectionModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly BackdropView backdrop;
    readonly View dim;
    readonly View scroller = new View();
    readonly View hero = new View();
    readonly View heroText = new View();
    readonly View buttons = new View();
    readonly FocusGroup buttonGroup = new FocusGroup(Axis.Row, "buttons");
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly Timer messageTimer = new Timer((uint)CollectionText.MessageMs);
    ButtonView? play;
    RoundButton? shuffle;
    TextLabel? said;
    TextLabel? empty;
    GridView? grid;
    CollectionPage? shown;
    string shownKind = "";
    string? shownBackdrop;
    bool scrolled, gone;
    Animation? scroll;
    /// <summary>Where focus was while the page loaded (the rail): Play takes it from there once the page comes.</summary>
    FocusItem? waitingOn;
    bool waiting;

    public CollectionScreen(TvShell shell, CollectionModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new PageKey.Collection(model.CollectionId), "collection")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.BackgroundColor = PleyColors.Bg;
        backdrop = new BackdropView(images);
        View.Add(backdrop);
        View.Add(new ScrimView(ScrimView.Item));
        dim = new View
        {
            Size = new global::Tizen.NUI.Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight)),
            BackgroundColor = PleyColors.Bg,
            Opacity = 0,
        };
        View.Add(dim);
        hero.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(HeroTop));
        hero.Add(heroText);
        hero.Add(buttons);
        scroller.Add(hero);
        View.Add(scroller);
        View.Add(body);
        buttonGroup.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        Group.ChildFocused += i => Scroll(grid != null && Group.Children[i] == grid.Group);
        messageTimer.Tick += (s, e) =>
        {
            Say(null);
            return false;
        };
        model.Told += OnTold;
        model.PlayRequested += OnPlay;
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) ToRail();
    }

    /// <summary>
    /// Nothing on the page can take focus yet: it waits on the page (the rail keeps it if it
    /// already has it), and the page takes it once it can, unless someone has moved on meanwhile.
    /// </summary>
    void ToRail()
    {
        if (Tree.Current == null || Tree.Current.Parent != Shell.Rail.Group) Wait();
        waiting = true;
        waitingOn = Tree.Current;
    }

    public override void Gone()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        model.Told -= OnTold;
        model.PlayRequested -= OnPlay;
        messageTimer.Stop();
        model.Dispose();
    }

    void OnTold(CollectionEvent e)
    {
        Say(CollectionText.Said(e));
        messageTimer.Stop();
        messageTimer.Start();
    }

    void OnPlay(PlayRequest request) => Shell.Play?.Invoke(request);

    void Show(Loadable<CollectionPage> state)
    {
        if (state.Value is { } page)
        {
            if (shownKind != "ready") Clear();
            shownKind = "ready";
            ShowPage(page);
            return;
        }
        if (CollectionText.CollectionMessage(state) is { } message)
        {
            shownKind = "failed";
            Clear();
            Message(message);
            return;
        }
        if (shownKind == "loading") return;
        shownKind = "loading";
        Clear();
        Skeleton();
    }

    /// <summary>Takes the page's content away (the loading blocks, a message, the page itself).</summary>
    void Clear()
    {
        var hadFocus = Tree.Current != null && Within(Group, Tree.Current);
        Group.Clear();
        buttonGroup.Clear();
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        foreach (var child in buttons.Children.ToList())
        {
            buttons.Remove(child);
            child.Dispose();
        }
        foreach (var child in heroText.Children.ToList())
        {
            heroText.Remove(child);
            child.Dispose();
        }
        if (grid != null)
        {
            grid.Unparent();
            grid.Dispose();
            grid = null;
        }
        empty?.Unparent();
        empty?.Dispose();
        play = null;
        shuffle = null;
        said = null;
        empty = null;
        shown = null;
        hero.Hide();
        Scroll(false);
        backdrop.Show(null);
        shownBackdrop = null;
        if (hadFocus) ToRail();
    }

    /// <summary>The loading page: still blocks where the column's lines and Play will be, 14 apart. No spinner.</summary>
    void Skeleton()
    {
        var y = HeroTop;
        void Block(float w, float h, float radius = PleyShape.BadgeRadius)
        {
            body.Add(new View
            {
                Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y)),
                Size = new global::Tizen.NUI.Size(Ui.Dp(w), Ui.Dp(h)),
                CornerRadius = Ui.Dp(radius),
                BackgroundColor = PleyColors.Surface2,
            });
            y += h + 14;
        }
        Block(120, 18);
        Block(360, 44);
        Block(240, 18);
        Block(TextColumn, 70);
        y += 20 + 14;
        Block(128, PleyTv.ButtonHeight, PleyShape.ControlRadius);
    }

    /// <summary>It failed with nothing to show: the title in Section, the sentence in Body Muted, 8 apart, and 24 under it Retry with focus.</summary>
    void Message(LibraryMessage m)
    {
        const float width = 520;
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(HeroTop)) };
        view.Add(Text.Label(m.Title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - 2 * PleyTv.Gutter));
        var words = Text.Label(m.Body, PleyType.Body, PleyColors.Muted, maxLines: 3, width: width);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.PositionY = Ui.Dp(PleyType.Section.Line + 8);
        view.Add(words);
        body.Add(view);
        var row = new ButtonRow();
        row.Add(new ButtonView(m.ActionLabel ?? LibraryText.Retry, ButtonKind.Pill, model.Retry));
        row.PositionY = Ui.Dp(PleyType.Section.Line + 8 + 8 + 16 + 8) + words.GetHeightForWidth(Ui.Dp(width));
        view.Add(row);
        Group.Add(row.Group);
        Tree.Enter(row.Group);
        waiting = false;
    }

    void ShowPage(CollectionPage page)
    {
        var first = shown == null;
        shown = page;
        hero.Show();
        if (page.Backdrop != shownBackdrop)
        {
            shownBackdrop = page.Backdrop;
            backdrop.Show(page.Backdrop == null ? null : new[] { page.Backdrop });
        }
        var bottom = ShowText(page);
        ShowButtons(page, bottom);
        ShowGrid(page);
        if (first)
        {
            Group.Clear();
            Group.Add(buttonGroup);
            if (grid != null) Group.Add(grid.Group);
        }
        // Play has focus once the page comes, else Shuffle; not once someone has gone to the rail meanwhile.
        if (first && (Tree.Current == null || (waiting && Tree.Current == waitingOn) || !InTree(Tree.Current)))
        {
            if (!Tree.Enter(buttonGroup) && (grid == null || !Tree.Enter(grid.Group))) Shell.FocusRail();
        }
        if (first) waiting = false;
        // What had focus can't take it now (the last title went): the page's start has it.
        if (Tree.Current is { } current && Within(Group, current) && !current.CanTakeFocus && !Tree.Enter(Group)) Shell.FocusRail();
    }

    /// <summary>
    /// The column's text, from y = 58: the layers icon, "Collection" and its badges, 8 apart; 10;
    /// the title in HeroTitle (2 lines at most); 12; the facts in Label Muted; 12; the summary in
    /// BodyLarge at 85 % (3 lines at most). Whether the page is empty, for the line under the buttons.
    /// </summary>
    bool ShowText(CollectionPage page)
    {
        foreach (var child in heroText.Children.ToList())
        {
            heroText.Remove(child);
            child.Dispose();
        }
        float x = 0;
        void Place(View v, float widthDp, float heightDp)
        {
            v.Position = new Position(Ui.Dp(x), Ui.Dp((BadgeHeight - heightDp) / 2));
            heroText.Add(v);
            x += widthDp + 8;
        }
        Place(Icons.Icon("layers", 18, PleyColors.Muted), 18, 18);
        var label = Text.Label(CollectionText.Label, PleyType.Label, PleyColors.Muted);
        label.Size = new global::Tizen.NUI.Size(label.NaturalSize.Width + 2, Text.LineHeight(PleyType.Label));
        Place(label, label.SizeWidth / Ui.Scale, label.SizeHeight / Ui.Scale);
        if (page.Automatic)
        {
            var badge = Badge(CollectionText.Tmdb);
            Place(badge, badge.SizeWidth / Ui.Scale, BadgeHeight);
        }
        if (page.Smart)
        {
            var badge = Badge(CollectionText.Smart);
            Place(badge, badge.SizeWidth / Ui.Scale, BadgeHeight);
        }
        var y = BadgeHeight + 10;
        y += Lines(page.Title, PleyType.HeroTitle, PleyColors.Fg, 2, y);
        y += 12;
        y += Lines(CollectionText.FactsLine(page), PleyType.Label, PleyColors.Muted, 1, y);
        if (page.Summary is { } summary) Lines(summary, PleyType.BodyLarge, Faded, 3, y + 12);
        return page.Tiles.Count == 0;
    }

    /// <summary>Text of up to <paramref name="maxLines"/> lines at <paramref name="y"/>; its height in dp.</summary>
    float Lines(string text, TextStyle style, Color color, int maxLines, float y)
    {
        var label = Text.Label(text, style, color, maxLines: maxLines, width: TextColumn);
        var line = Text.LineHeight(style);
        var lines = 1;
        if (maxLines > 1)
        {
            label.VerticalAlignment = VerticalAlignment.Top;
            lines = (int)Math.Ceiling(label.GetHeightForWidth(Ui.Dp(TextColumn)) / line - 0.05f);
            lines = Math.Max(1, Math.Min(maxLines, lines));
            label.SizeHeight = line * lines;
        }
        label.PositionY = Ui.Dp(y);
        heroText.Add(label);
        return style.Line * lines;
    }

    /// <summary>A TMDB or Smart badge (the Android app's QualityBadge): Badge text in Muted, a 1 dp Outline edge, 6 either side.</summary>
    static View Badge(string text)
    {
        var t = Text.Label(text, PleyType.Badge, PleyColors.Muted);
        var box = new View
        {
            Size = new global::Tizen.NUI.Size(t.NaturalSize.Width + Ui.Dp(12), Ui.Dp(BadgeHeight)),
            BorderlineWidth = Ui.Dp(1),
            BorderlineColor = PleyColors.Outline,
            BorderlineOffset = -1,
            CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
        };
        t.Size = new global::Tizen.NUI.Size(box.SizeWidth, box.SizeHeight);
        t.HorizontalAlignment = HorizontalAlignment.Center;
        box.Add(t);
        return box;
    }

    /// <summary>
    /// Play (a pill) and Shuffle (round), 10 apart at the column's foot, then the room for
    /// Shuffle's label and 12, then "Nothing in this collection yet." when it is empty. Made once,
    /// so a button keeps focus when the page is read again.
    /// </summary>
    void ShowButtons(CollectionPage page, bool isEmpty)
    {
        var height = HeroHeight - HeroTop;
        var y = height - (isEmpty ? PleyType.Body.Line : 0) - 12 - ButtonLabelRoom - PleyTv.ButtonHeight;
        buttons.Position = new Position(0, Ui.Dp(y));
        if (play == null || shuffle == null)
        {
            play = new ButtonView(CollectionText.Play, ButtonKind.Pill, () => model.Play(false), iconName: "play");
            shuffle = new RoundButton(CollectionText.Shuffle, "shuffle", () => model.Play(true));
            shuffle.PositionX = Ui.Dp(play.WidthDp + PleyTv.ButtonGap);
            buttons.Add(play);
            buttons.Add(shuffle);
            buttonGroup.Add(play.Item);
            buttonGroup.Add(shuffle.Item);
            said = Text.Label("", PleyType.Label, PleyColors.DangerText, maxLines: 2, width: Math.Max(80, TextColumn - (play.WidthDp + PleyTv.ButtonGap + PleyTv.ButtonHeight + 12)));
            said.VerticalAlignment = VerticalAlignment.Top;
            said.Position = new Position(Ui.Dp(play.WidthDp + PleyTv.ButtonGap + PleyTv.ButtonHeight + 12), Ui.Dp(14));
            buttons.Add(said);
        }
        play.Enabled = page.CanPlay;
        shuffle.Enabled = page.CanShuffle;
        if (page.CanPlay) buttonGroup.RememberChild(0);
        if (isEmpty && empty == null)
        {
            empty = Text.Label(CollectionText.Empty, PleyType.Body, PleyColors.Muted, width: TextColumn);
            hero.Add(empty);
        }
        else if (!isEmpty && empty != null)
        {
            empty.Unparent();
            empty.Dispose();
            empty = null;
        }
        if (empty != null) empty.PositionY = Ui.Dp(height - PleyType.Body.Line);
    }

    void Say(string? text)
    {
        if (said != null) said.Text = text ?? "";
    }

    /// <summary>The titles: the library grid's posters, 7 across, from a row's gap under the hero. Kept while their number is, so focus stays.</summary>
    void ShowGrid(CollectionPage page)
    {
        var total = page.Tiles.Count;
        if (total == 0)
        {
            if (grid == null) return;
            var focusWasIn = Tree.Current != null && Tree.Current.Parent == grid.Group;
            Group.Remove(grid.Group);
            if (focusWasIn && !Tree.Enter(buttonGroup)) Shell.FocusRail();
            grid.Unparent();
            grid.Dispose();
            grid = null;
            Scroll(false);
            return;
        }
        if (grid == null)
        {
            var made = new GridView(TileKind.LibraryGrid, page.Columns, PleyTv.CanvasHeight - GridTopScrolled, gate, images,
                p => shown != null && p < shown.Tiles.Count ? shown.Tiles[p] : null);
            made.PositionY = Ui.Dp(GridTop);
            made.Open = (p, _) =>
            {
                if (model.Opens(p) is { } key) go(key);
            };
            made.Actions = (_, tile) => Shell.Actions?.Invoke(tile.Id, false);
            made.Group.Exit = d => d switch
            {
                Direction.Left => Scrolling.Left(gate, Shell.FocusRail),
                // The grid's last row and column: focus stays.
                Direction.Down or Direction.Right => true,
                _ => false,
            };
            made.Moved += _ => Dimming();
            scroller.Add(made);
            grid = made;
            // Made after the page first showed (it had no titles): it goes under the buttons.
            if (shown != null && Group.Children.Contains(buttonGroup)) Group.Add(made.Group);
        }
        var hadFocus = Tree.Current != null && Tree.Current.Parent == grid.Group;
        var before = grid.Current;
        grid.Update(total);
        if (hadFocus && Tree.Current?.Parent != grid.Group)
        {
            // Its positions were made again (the number changed): focus goes back where it was.
            var at = Math.Max(0, Math.Min(before, grid.Total - 1));
            if (grid.ItemAt(at) is { } item)
            {
                grid.Jump(at);
                Tree.Focus(item);
            }
        }
    }

    /// <summary>The page moves up when focus goes into the grid, and back down when it comes back to the buttons.</summary>
    void Scroll(bool down)
    {
        if (scrolled == down) return;
        scrolled = down;
        scroll?.Stop();
        scroll = Scrolling.Glide(gate);
        scroll.AnimateTo(scroller, "PositionY", down ? Ui.Dp(GridTopScrolled - GridTop) : 0f);
        scroll.AnimateTo(dim, "Opacity", DimFor(grid?.Current ?? 0));
        scroll.Play();
    }

    /// <summary>The backdrop dims as the grid passes over it: by how far the hero has gone, fully once the first row has gone too.</summary>
    float DimFor(int position)
    {
        if (!scrolled) return 0;
        var row = grid == null ? 0 : position / Math.Max(1, grid.Columns);
        return row > 0 ? Dim : Dim * Math.Min(1f, (GridTop - GridTopScrolled) / HeroHeight);
    }

    void Dimming()
    {
        if (gone || !scrolled || grid == null) return;
        var target = DimFor(grid.Current);
        if (Math.Abs(dim.Opacity - target) < 0.001f) return;
        var fade = Scrolling.Glide(gate);
        fade.AnimateTo(dim, "Opacity", target);
        fade.Play();
    }

    bool InTree(FocusItem item)
    {
        FocusNode node = item;
        while (node.Parent != null) node = node.Parent;
        return node == Tree.Root;
    }

    static bool Within(FocusGroup group, FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == group) return true;
        return false;
    }

    static readonly Color Faded = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.85f);
}
