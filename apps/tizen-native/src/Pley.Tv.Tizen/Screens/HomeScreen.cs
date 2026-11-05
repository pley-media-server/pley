using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Home on TV (design spec §2.2, §4.1; the Android app's TvHomeScreen). At rest: the hero for the
/// first Continue Watching item (its backdrop, Resume, with initial focus), then the server's rows.
/// Down moves into the rows: the hero's buttons go, the rows come up under a compact info block
/// about the focused tile, and the backdrop and the info block follow the tile once focus has
/// rested 400 ms. Back from the rows puts the page back at its hero and focus on the rail's Home.
/// </summary>
sealed class HomeScreen : ShellPage
{
    /// <summary>The text column over backdrops: x 96 to 560.</summary>
    const float TextColumn = 464;
    /// <summary>The first row's header at rest: y = 384, under the hero.</summary>
    const float FirstRowTop = 384;
    const float HeroTop = 52, TitleBox = 72;

    readonly HomeModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<ItemSummary> open;
    readonly FocusGroup page;
    readonly BackdropView backdrop;
    readonly ScrimView scrim;
    readonly View hero = new View();
    readonly Redrawn heroText;
    /// <summary>What the hero says, as last drawn: the same again isn't drawn again.</summary>
    (string, string, HomeMeta, string, bool, string, bool) heroShown;
    readonly ButtonRow heroButtons;
    readonly ButtonView resume;
    readonly RoundButton fromStart;
    readonly RoundButton info;
    readonly RoundButton watched;
    readonly View infoBlock = new View { Opacity = 0 };
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly List<IDisposable> watches = new List<IDisposable>();
    RowColumnView? column;
    /// <summary>The rows drawn, in order.</summary>
    readonly List<RowView> rowViews = new List<RowView>();
    IReadOnlyList<HomeRow> shownRows = Array.Empty<HomeRow>();
    readonly Dictionary<(string Row, long Id), ItemSummary> itemsByTile = new Dictionary<(string, long), ItemSummary>();
    HomeInfo? heroInfo;
    long? backdropFor;
    bool inRows;
    Timer? skeletonTimer;
    Animation? reveal;
    string shownKind = "";

    /// <param name="open">Opens an item's page.</param>
    public HomeScreen(TvShell shell, HomeModel model, Images images, KeyRepeatGate gate, Action<ItemSummary> open) : base(shell, new PageKey.Home(), "home")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.open = open;
        page = Group;
        View.BackgroundColor = PleyColors.Bg;

        backdrop = new BackdropView(images);
        View.Add(backdrop);
        scrim = new ScrimView();
        View.Add(scrim);
        View.Add(infoBlock);
        hero.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(HeroTop));
        heroText = new Redrawn(hero);
        heroButtons = new ButtonRow();
        resume = heroButtons.Add(new ButtonView("Resume", ButtonKind.Pill, () => Play(false), iconName: "play"));
        fromStart = new RoundButton("From start", "rotate-ccw", () => Play(true));
        info = new RoundButton("Info", "info", () =>
        {
            if (heroInfo != null) open(heroInfo.Item);
        });
        watched = new RoundButton("Mark as watched", "circle-check", () =>
        {
            if (heroInfo != null) model.SetWatched(heroInfo.Item, !heroInfo.Watched);
        });
        hero.Add(heroButtons);
        View.Add(hero);
        View.Add(body);

        page.ChildFocused += i => SetInRows(page.Children[i] != heroButtons.Group);
        heroButtons.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);

        watches.Add(model.State.Watch(Show));
        watches.Add(model.Info.Watch(_ => FollowFocus()));
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Dispose();
        skeletonTimer?.Stop();
        heroText.Stop();
    }

    /// <summary>Focus back on Home: on the tile it left from, or Resume.</summary>
    public override void Enter()
    {
        if (!Tree.Enter(Group)) Shell.FocusRail();
    }

    /// <summary>Resume (or Play) and From start on the hero: a film or episode plays; anything else opens its page.</summary>
    void Play(bool fromStart)
    {
        var item = heroInfo?.Item;
        if (item == null) return;
        if (item.Kind is "movie" or "episode" or "extra")
            Shell.Play?.Invoke(new PlayRequest(item.Id, fromStart || item.UserState.Watched ? 0 : item.UserState.PositionMs, fromStart));
        else open(item);
    }

    /// <summary>Back from the rows: the page goes back to its hero (focus on Resume), and focus to the rail.</summary>
    public override bool Back()
    {
        if (!inRows) return false;
        page.ForgetLast();
        heroButtons.Group.ForgetLast();
        SetInRows(false);
        Shell.FocusRail();
        return true;
    }

    void Show(HomeState s)
    {
        var kind = s.GetType().Name;
        if (s is HomeState.Ready ready)
        {
            if (shownKind != kind) ClearBody();
            shownKind = kind;
            var drawn = Profile.Start();
            ShowReady(ready);
            Profile.Took("Home drawn", drawn);
            return;
        }
        if (shownKind == kind && s is not HomeState.Empty) return;
        shownKind = kind;
        ClearBody();
        hero.Hide();
        infoBlock.Hide();
        backdrop.Show(null);
        switch (s)
        {
            case HomeState.Loading:
                // After a moment, so Home read from what was saved never flashes one.
                skeletonTimer = new Timer(300);
                skeletonTimer.Tick += (_, _) =>
                {
                    if (model.State.Value is HomeState.Loading) ShowSkeleton();
                    return false;
                };
                skeletonTimer.Start();
                break;
            case HomeState.Failed f:
                Notice(Errors.Unreachable(f.Error) ? Errors.OfflineTitle : "Couldn't load your home screen",
                    Errors.Unreachable(f.Error) ? Errors.OfflineBody(f.Error) : Errors.Message(f.Error), retry: true);
                break;
            case HomeState.NoLibraries:
                Notice("No libraries yet", "Ask the server owner to add a library.", retry: false);
                break;
            case HomeState.Empty e:
                Notice(e.Scanning ? "Scanning your libraries…" : "Nothing here yet",
                    e.Scanning ? "New items will show up here as soon as they're found." : "Your libraries are empty. Add media to the library folders and run a scan.", retry: false);
                break;
        }
    }

    void ClearBody()
    {
        skeletonTimer?.Stop();
        page.Clear();
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        column = null;
        rowViews.Clear();
        shownRows = Array.Empty<HomeRow>();
        itemsByTile.Clear();
    }

    /// <summary>A sentence instead of rows: centred, with Retry focused when there is one; without, focus goes to the rail.</summary>
    void Notice(string title, string text, bool retry)
    {
        var left = PleyTv.Gutter;
        var width = PleyTv.CanvasWidth - left - PleyTv.SafeHorizontal;
        var heading = Text.Label(title, PleyType.PageTitle, PleyColors.Fg, width: width);
        heading.HorizontalAlignment = HorizontalAlignment.Center;
        var words = Text.Label(text, PleyType.Body, PleyColors.Muted, maxLines: 3, width: TextColumn);
        words.HorizontalAlignment = HorizontalAlignment.Center;
        words.VerticalAlignment = VerticalAlignment.Top;
        var total = PleyType.PageTitle.Line + 12 + PleyType.Body.Line * 3 + (retry ? 24 + PleyTv.ButtonHeight : 0);
        var top = (PleyTv.CanvasHeight - total) / 2;
        heading.Position = new Position(Ui.Dp(left), Ui.Dp(top));
        words.Position = new Position(Ui.Dp(left + (width - TextColumn) / 2), Ui.Dp(top + PleyType.PageTitle.Line + 12));
        body.Add(heading);
        body.Add(words);
        if (retry)
        {
            var row = new ButtonRow();
            row.Add(new ButtonView("Retry", ButtonKind.Pill, model.Refresh, iconName: "refresh-cw"));
            row.Position = new Position(Ui.Dp(left + (width - row.WidthDp) / 2), Ui.Dp(top + PleyType.PageTitle.Line + 12 + PleyType.Body.Line * 3 + 24));
            body.Add(row);
            page.Add(row.Group);
            Tree.Enter(row.Group);
        }
        else
        {
            Shell.FocusRail();
        }
    }

    /// <summary>Static blocks in the hero's and the first row's shapes. No spinner.</summary>
    void ShowSkeleton()
    {
        void Block(float x, float y, float w, float h, float radius = PleyShape.BadgeRadius) =>
            body.Add(new View { Position = new Position(Ui.Dp(x), Ui.Dp(y)), Size = new Size(Ui.Dp(w), Ui.Dp(h)), CornerRadius = Ui.Dp(radius), BackgroundColor = PleyColors.Surface2 });
        var g = PleyTv.Gutter;
        Block(g, 84, 260, 38);
        Block(g, 132, 210, 18);
        Block(g, 168, 440, 15);
        Block(g, 190, 400, 15);
        Block(g, 212, 260, 15);
        Block(g, 250, 128, PleyTv.ButtonHeight, PleyShape.ControlRadius);
        Block(g, 387, 180, 17);
        var tile = TileKind.Landscape;
        for (var i = 0; i < 4; i++) Block(g + (tile.Width + tile.Gap) * i, 422, tile.Width, tile.Height, PleyShape.TileRadius);
        Shell.FocusRail();
    }

    void ShowReady(HomeState.Ready s)
    {
        if (s.Hero != null) ShowHero(s.Hero);
        if (SameRows(s.Rows, shownRows)) { }
        else if (SameItems(s.Rows, shownRows) && rowViews.Count == s.Rows.Count) UpdateRows(s);
        else ShowRows(s);
        FollowFocus();
    }

    /// <summary>The same rows of the same items, in the same order: only what the tiles say may differ.</summary>
    static bool SameItems(IReadOnlyList<HomeRow> a, IReadOnlyList<HomeRow> b) =>
        a.Count == b.Count && a.Zip(b, (x, y) => x.Key == y.Key && x.Title == y.Title && x.Items.Select(i => i.Id).SequenceEqual(y.Items.Select(i => i.Id))).All(same => same);

    /// <summary>
    /// The tiles take their new look (the progress a film just played left, a badge), in the rows
    /// they are in: Home drawn again whole as the player closed held the UI 190 to 470 ms, as the
    /// TV's player was let go.
    /// </summary>
    void UpdateRows(HomeState.Ready s)
    {
        shownRows = s.Rows;
        for (var r = 0; r < s.Rows.Count; r++)
        {
            var row = s.Rows[r];
            foreach (var item in row.Items) itemsByTile[(row.Key, item.Id)] = item;
            rowViews[r].Update(row.Items.Select(i => TileModel.Of(i, row.Kind, s.Origin)).ToList());
        }
    }

    static bool SameRows(IReadOnlyList<HomeRow> a, IReadOnlyList<HomeRow> b) =>
        a.Count == b.Count && a.Zip(b, (x, y) => x.Key == y.Key && x.Title == y.Title && x.Items.SequenceEqual(y.Items)).All(same => same);

    void ShowRows(HomeState.Ready s)
    {
        var hadFocus = Tree.Current;
        var focusedKey = hadFocus?.Name;
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        page.Clear();
        itemsByTile.Clear();
        rowViews.Clear();
        shownRows = s.Rows;
        page.Add(heroButtons.Group);
        hero.Show();
        infoBlock.Show();
        column = new RowColumnView(gate, top: FirstRowTop, firstAtTop: false);
        foreach (var row in s.Rows)
        {
            var kind = row.Kind;
            var models = row.Items.Select(i => TileModel.Of(i, kind, s.Origin)).ToList();
            foreach (var item in row.Items) itemsByTile[(row.Key, item.Id)] = item;
            var rowKey = row.Key;
            var view = new RowView(row.Title, models, kind, gate, images,
                m =>
                {
                    if (itemsByTile.TryGetValue((rowKey, m.Id), out var item)) open(item);
                },
                m => Shell.Actions?.Invoke(m.Id, row.ContinueWatching),
                Shell.FocusRail,
                m =>
                {
                    if (!itemsByTile.TryGetValue((rowKey, m.Id), out var item)) return;
                    model.Focus(item, row.After(item));
                    // Its backdrop loads now, focus may rest on it next; not for every tile a held key passes.
                    if (!gate.Holding) images.Load(HomeRules.Backdrop(item, s.Origin), BackdropView.WidthDp, _ => { });
                },
                Tree.Focus,
                row.SeeAll is { } more ? (LibraryText.SeeAll, () =>
                {
                    if (!Shell.Open(new PageKey.Library(more.LibraryId, LibraryTab.Library, more.View))) Shell.Toast("That page comes next in the native app.");
                }) : null);
            column.Add(view);
            rowViews.Add(view);
        }
        page.Add(column.Group);
        body.Add(column);
        // Focus where it was (a row whose items changed keeps its tile), else on Resume.
        FocusItem? again = null;
        if (focusedKey != null) again = Find(column.Group, focusedKey);
        if (again != null) Tree.Focus(again);
        else if (hadFocus == null || hadFocus.Parent != Shell.Rail.Group) Tree.Focus(resume.Item);
    }

    static FocusItem? Find(FocusGroup group, string name)
    {
        foreach (var child in group.Children)
        {
            if (child is FocusItem item && item.Name == name) return item;
            if (child is FocusGroup g && Find(g, name) is { } found) return found;
        }
        return null;
    }

    /// <summary>
    /// The hero's stack at x = 96: the title, an episode's "S2 · E5 · The Long Night", the
    /// metadata line, the time left, three lines of summary, then Resume (or Play) and the round
    /// buttons From start, Info and Watched. A missing piece closes its gap.
    /// </summary>
    void ShowHero(HomeInfo h)
    {
        if (heroInfo == h) return;
        var said = (h.Title, h.Context, h.Meta, h.Summary, h.InProgress, h.TimeLeft, h.Watched);
        var same = heroInfo != null && said.Equals(heroShown);
        heroInfo = h;
        if (same) return;
        heroShown = said;
        var text = heroText.Next();
        var title = Text.Label(h.Title, PleyType.HeroTitle, PleyColors.Fg, maxLines: 2, width: TextColumn);
        title.VerticalAlignment = VerticalAlignment.Bottom;
        title.SizeHeight = Ui.Dp(TitleBox);
        text.Add(title);
        float y = TitleBox;
        if (h.Context.Length > 0)
        {
            y += 10;
            var context = Text.Label(h.Context, PleyType.Title, PleyColors.Fg, width: TextColumn);
            context.PositionY = Ui.Dp(y);
            text.Add(context);
            y += PleyType.Title.Line;
        }
        if (!h.Meta.IsEmpty)
        {
            y += 8;
            var meta = MetaLine.Of(h.Meta);
            meta.PositionY = Ui.Dp(y);
            text.Add(meta);
            y += 18;
        }
        if (h.InProgress && h.TimeLeft.Length > 0)
        {
            y += 10;
            var left = Text.Label(h.TimeLeft, PleyType.Caption, PleyColors.Fg, width: TextColumn);
            left.PositionY = Ui.Dp(y);
            text.Add(left);
            y += PleyType.Caption.Line;
        }
        if (h.Summary.Length > 0)
        {
            y += 10;
            var summary = Text.Label(h.Summary, PleyType.BodyLarge, Summary, maxLines: 3, width: TextColumn);
            summary.VerticalAlignment = VerticalAlignment.Top;
            summary.PositionY = Ui.Dp(y);
            text.Add(summary);
            y += PleyType.BodyLarge.Line * 3;
        }
        y += 20;
        heroButtons.PositionY = Ui.Dp(y);
        resume.SetText(h.InProgress ? "Resume" : "Play");
        watched.SetText(h.Watched ? "Mark as unwatched" : "Mark as watched");
        LayoutHeroButtons(h.InProgress);
    }

    void LayoutHeroButtons(bool inProgress)
    {
        // The round buttons sit after Resume, a button's gap apart; From start only while in progress.
        var focusedRound = Tree.Current;
        foreach (var b in new View[] { fromStart, info, watched })
            if (b.GetParent() != null) heroButtons.Remove(b);
        heroButtons.Group.Clear();
        heroButtons.Group.Add(resume.Item);
        var x = resume.WidthDp;
        void AddRound(RoundButton b)
        {
            x += PleyTv.ButtonGap;
            b.PositionX = Ui.Dp(x);
            heroButtons.Add(b);
            heroButtons.Group.Add(b.Item);
            x += PleyTv.ButtonHeight;
        }
        if (inProgress) AddRound(fromStart);
        AddRound(info);
        AddRound(watched);
        heroButtons.SizeWidth = Ui.Dp(x);
        if (focusedRound == fromStart.Item && !inProgress) Tree.Focus(resume.Item);
    }

    void SetInRows(bool on)
    {
        if (on == inRows) return;
        inRows = on;
        if (!on)
        {
            model.Focus(null);
            // Up from the first row, as Back: the rows go back down under the hero.
            column?.ScrollToTop();
        }
        reveal?.Stop();
        reveal = new Animation(PleyMotion.RevealMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        reveal.AnimateTo(hero, "Opacity", on ? 0f : 1f);
        reveal.AnimateTo(infoBlock, "Opacity", on ? 1f : 0f);
        Profile.Slowed(reveal);
        reveal.Play();
        scrim.Set(on ? ScrimView.Rows : ScrimView.Hero);
        FollowFocus();
    }

    /// <summary>In the rows, the backdrop and the info block show the tile focus rests on; at rest, the hero's.</summary>
    void FollowFocus()
    {
        if (model.State.Value is not HomeState.Ready ready || heroInfo == null) return;
        var about = inRows ? model.Info.Value ?? heroInfo : heroInfo;
        if (backdropFor != about.Item.Id)
        {
            backdropFor = about.Item.Id;
            backdrop.Show(HomeRules.Backdrop(about.Item, ready.Origin));
        }
        if (inRows) ShowInfoBlock(about);
    }

    /// <summary>The compact block about the focused tile, from y = 28: the title in a 52 dp box, the metadata line 10 below, two lines of summary 8 below.</summary>
    void ShowInfoBlock(HomeInfo about)
    {
        foreach (var child in infoBlock.Children.ToList())
        {
            infoBlock.Remove(child);
            child.Dispose();
        }
        infoBlock.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(28));
        var title = Text.Label(about.Title, PleyType.PageTitle, PleyColors.Fg, width: TextColumn);
        title.VerticalAlignment = VerticalAlignment.Bottom;
        title.SizeHeight = Ui.Dp(52);
        infoBlock.Add(title);
        var meta = MetaLine.Of(about.Meta, about.Context);
        meta.PositionY = Ui.Dp(52 + 10);
        infoBlock.Add(meta);
        if (about.Summary.Length > 0)
        {
            var summary = Text.Label(about.Summary, PleyType.Body, Summary, maxLines: 2, width: TextColumn);
            summary.VerticalAlignment = VerticalAlignment.Top;
            summary.PositionY = Ui.Dp(52 + 10 + 18 + 8);
            infoBlock.Add(summary);
        }
    }

    static readonly Color Summary = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.85f);
}

/// <summary>
/// The metadata line (design spec §2.2, §2.6): year · content rating (a badge) · runtime · ★ rating ·
/// quality badges, in Label Muted with Subtle dots; <c>lead</c> ("S1 · E3 · Glasshouse") first, in Fg.
/// </summary>
static class MetaLine
{
    const float Width = 464, Height = 18, Gap = 8;

    public static View Of(HomeMeta meta, string lead = "")
    {
        var parts = new List<MetaPart>();
        if (meta.Year.Length > 0) parts.Add(new MetaPart.Text(meta.Year));
        if (meta.ContentRating != null) parts.Add(new MetaPart.ContentRating(meta.ContentRating));
        if (meta.Runtime.Length > 0) parts.Add(new MetaPart.Text(meta.Runtime));
        if (meta.Rating != null) parts.Add(new MetaPart.Rating(meta.Rating));
        if (meta.QualityBadges.Count > 0) parts.Add(new MetaPart.Badges(meta.QualityBadges));
        return Of(parts, lead);
    }

    /// <summary>The line of <paramref name="parts"/>, in their order (an item page's own).</summary>
    public static View Of(IReadOnlyList<MetaPart> parts, string lead = "")
    {
        var line = new View { Size = new Size(Ui.Dp(Width), Ui.Dp(Height)) };
        float x = 0;
        var first = true;
        void Separate()
        {
            if (!first) Place(Label("·", PleyColors.Subtle));
            first = false;
        }
        void Place(View v)
        {
            v.PositionX = Ui.Dp(x);
            line.Add(v);
            x += v.SizeWidth / Ui.Scale + Gap;
        }
        TextLabel Label(string text, Color color)
        {
            var l = Text.Label(text, PleyType.Label, color);
            // The font's line may be a touch taller than the 18 dp line: centred on it all the same.
            var h = Text.LineHeight(PleyType.Label);
            l.Size = new Size(l.NaturalSize.Width + 2, h);
            l.PositionY = (Ui.Dp(Height) - h) / 2;
            return l;
        }
        View Badge(string text)
        {
            var t = Text.Label(text, PleyType.Badge, PleyColors.Fg);
            var box = new View
            {
                Size = new Size(t.NaturalSize.Width + Ui.Dp(12), Ui.Dp(18)),
                BorderlineWidth = Ui.Dp(1),
                BorderlineColor = PleyColors.Control,
                BorderlineOffset = -1,
                CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
            };
            t.Size = new Size(box.SizeWidth, box.SizeHeight);
            t.HorizontalAlignment = HorizontalAlignment.Center;
            box.Add(t);
            return box;
        }
        if (lead.Length > 0)
        {
            Separate();
            var l = Label(lead, PleyColors.Fg);
            if (l.SizeWidth > Ui.Dp(Width * 0.6f)) l.SizeWidth = Ui.Dp(Width * 0.6f);
            Place(l);
        }
        foreach (var part in parts)
        {
            switch (part)
            {
                case MetaPart.Text t:
                    Separate();
                    Place(Label(t.Value, PleyColors.Muted));
                    break;
                case MetaPart.ContentRating c:
                    Separate();
                    Place(Badge(c.Value));
                    break;
                case MetaPart.Rating r:
                    Separate();
                    var star = Icons.Icon("star", 14, PleyColors.Accent);
                    star.PositionY = Ui.Dp((Height - 14) / 2);
                    Place(star);
                    x -= Gap - 4;
                    Place(Label(r.Value, PleyColors.Fg));
                    break;
                case MetaPart.Badges b when b.Values.Count > 0:
                    Separate();
                    foreach (var value in b.Values)
                    {
                        Place(Badge(value));
                        x -= Gap - 6;
                    }
                    break;
            }
        }
        return line;
    }
}

/// <summary>
/// The page's backdrop (design spec §2.2): one picture under everything, filling the screen, with a
/// 300 ms crossfade once the next picture has loaded. Never blurred.
/// </summary>
sealed class BackdropView : View
{
    /// <summary>Backdrops are asked for at most 1280 pixels wide.</summary>
    public const float WidthDp = 640;

    readonly Images images;
    ImageView? shown;
    int asked;

    public BackdropView(Images images)
    {
        this.images = images;
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
    }

    public void Show(IReadOnlyList<string>? candidates)
    {
        var mine = ++asked;
        if (candidates == null || candidates.Count == 0)
        {
            Swap(null);
            return;
        }
        images.Load(candidates, WidthDp, file =>
        {
            if (mine != asked || Disposed) return;
            Swap(file);
        });
    }

    void Swap(string? file)
    {
        var before = shown;
        shown = null;
        if (file != null)
        {
            shown = new ImageView
            {
                ResourceUrl = file,
                Size = Size,
                FittingMode = FittingModeType.ScaleToFill,
                Opacity = 0,
            };
            Add(shown);
        }
        var fade = new Animation(PleyMotion.BackdropMs);
        if (shown != null) fade.AnimateTo(shown, "Opacity", 1f);
        if (before != null) fade.AnimateTo(before, "Opacity", 0f);
        fade.Finished += (s, e) =>
        {
            if (before == null) return;
            before.Unparent();
            before.Dispose();
        };
        Profile.Slowed(fade);
        fade.PlayHeld();
    }
}

/// <summary>
/// The backdrop's two gradients: from the left, Bg at 100 % (x = 0), 92 % (300), 85 % (560),
/// 0 % (800); from the top, Bg at 0 % at <c>from</c> to 100 % at <c>to</c>, and solid below.
/// </summary>
sealed class ScrimView : View
{
    /// <summary>Home at rest, under the hero.</summary>
    public static readonly (float From, float To) Hero = (220, 410);
    /// <summary>Home in the rows, under the info block.</summary>
    public static readonly (float From, float To) Rows = (120, 300);
    /// <summary>Item pages.</summary>
    public static readonly (float From, float To) Item = (260, 500);

    readonly View fade;
    readonly View solid;
    Animation? move;

    public ScrimView((float From, float To)? place = null)
    {
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
        var offsets = new PropertyArray();
        var colors = new PropertyArray();
        foreach (var (at, alpha) in new[] { (0f, 1f), (300f / 800, 0.92f), (560f / 800, 0.85f), (1f, 0f) })
        {
            offsets.Add(new PropertyValue(at));
            colors.Add(new PropertyValue(new Color(PleyColors.Bg.R, PleyColors.Bg.G, PleyColors.Bg.B, alpha)));
        }
        var left = new PropertyMap();
        left.Add(Visual.Property.Type, new PropertyValue((int)Visual.Type.Gradient));
        left.Add(GradientVisualProperty.StartPosition, new PropertyValue(new Vector2(-0.5f, 0)));
        left.Add(GradientVisualProperty.EndPosition, new PropertyValue(new Vector2(0.5f, 0)));
        left.Add(GradientVisualProperty.StopOffset, new PropertyValue(offsets));
        left.Add(GradientVisualProperty.StopColor, new PropertyValue(colors));
        Add(new View { Size = new Size(Ui.Dp(800), Ui.Dp(PleyTv.CanvasHeight)), Background = left });

        var top = new PropertyMap();
        var topOffsets = new PropertyArray();
        topOffsets.Add(new PropertyValue(0f));
        topOffsets.Add(new PropertyValue(1f));
        var topColors = new PropertyArray();
        topColors.Add(new PropertyValue(new Color(PleyColors.Bg.R, PleyColors.Bg.G, PleyColors.Bg.B, 0)));
        topColors.Add(new PropertyValue(PleyColors.Bg));
        top.Add(Visual.Property.Type, new PropertyValue((int)Visual.Type.Gradient));
        top.Add(GradientVisualProperty.StartPosition, new PropertyValue(new Vector2(0, -0.5f)));
        top.Add(GradientVisualProperty.EndPosition, new PropertyValue(new Vector2(0, 0.5f)));
        top.Add(GradientVisualProperty.StopOffset, new PropertyValue(topOffsets));
        top.Add(GradientVisualProperty.StopColor, new PropertyValue(topColors));
        fade = new View { Background = top, Size = new Size(Ui.Dp(PleyTv.CanvasWidth), 0) };
        solid = new View { BackgroundColor = PleyColors.Bg, Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight)) };
        Add(fade);
        Add(solid);
        Place(place ?? Hero, animate: false);
    }

    public void Set((float From, float To) where) => Place(where, animate: true);

    void Place((float From, float To) where, bool animate)
    {
        move?.Stop();
        fade.PositionY = Ui.Dp(where.From);
        fade.SizeHeight = Ui.Dp(where.To - where.From);
        if (!animate)
        {
            solid.PositionY = Ui.Dp(where.To);
            return;
        }
        move = new Animation(PleyMotion.RevealMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        move.AnimateTo(solid, "PositionY", Ui.Dp(where.To));
        Profile.Slowed(move);
        move.Play();
    }
}
