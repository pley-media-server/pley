using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Discover on TV (the Android app's TvDiscoverScreen), the rail's way to ask for movies and shows.
/// At rest, the hero for the first trending title (the next row's first when trending has none for
/// this profile): its backdrop, facts, a line of overview, its main action and Details. Down goes
/// into TMDB's rows of posters, each read as it comes near the screen, with their badges, then the
/// profile's requests; the backdrop and an info block follow the tile focus rests on, as on Home. At
/// the top, the way to the requests with their number, for a profile that has requests to see. Back
/// from the rows goes to the hero, then the rail. TMDB's search is in Search.
/// </summary>
sealed class DiscoverScreen : ShellPage
{
    /// <summary>The text column over backdrops: x 96 to 560.</summary>
    const float TextColumn = 464;
    /// <summary>The first row's header at rest, under the hero.</summary>
    const float FirstRowTop = 356;
    /// <summary>The hero's title box: two lines end at y = 172 whatever their number.</summary>
    const float HeroTop = 84, TitleBox = 88;
    /// <summary>Tiles from a row's end at which its next page is asked for.</summary>
    const int NearEnd = 6;
    const int SkeletonTiles = 7;

    readonly DiscoverModel model;
    readonly Shell layers;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly Action<PageKey> restart;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly BackdropView backdrop;
    readonly ScrimView scrim;
    readonly View hero = new View();
    readonly View infoBlock = new View { Opacity = 0 };
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly View chipSlot = new View();
    /// <summary>The way to the requests at the top, while the profile has some to see.</summary>
    readonly FocusGroup chipGroup = new FocusGroup(Axis.Row, "requests");
    readonly FocusGroup heroGroup = new FocusGroup(Axis.Row, "hero");
    /// <summary>Each row's place in the column: still wells until its first page comes, then its row.</summary>
    readonly Dictionary<DiscoverList, RowSlot> slots = new Dictionary<DiscoverList, RowSlot>();
    RowSlot? yoursSlot;
    /// <summary>Each row's tiles by title, so a row that reads on or changes a badge rebinds only what changed.</summary>
    readonly Dictionary<DiscoverList, Dictionary<DiscoverTitle, TileModel>> tileCache = new Dictionary<DiscoverList, Dictionary<DiscoverTitle, TileModel>>();
    readonly ContextScheduler post = new ContextScheduler();
    bool gone;
    RowColumnView? column;
    TitleActionsView? actions;
    /// <summary>What the column was made for: which rows show, and whether each has titles yet.</summary>
    string shape = "";
    DiscoverHero? shownHero;
    (RequestsEntry Entry, bool Shown)? shownChip;
    object? shownNotice;
    bool inRows, entered, hadProblem;
    Timer? skeletonTimer, rest;
    /// <summary>What focus rests on in the rows: a title or one of the profile's requests.</summary>
    object? about;
    Animation? reveal;

    public DiscoverScreen(TvShell shell, Shell layers, DiscoverModel model, Images images, KeyRepeatGate gate, Action<PageKey> go, Action<PageKey> restart)
        : base(shell, new PageKey.Discover(), "discover")
    {
        this.model = model;
        this.layers = layers;
        this.images = images;
        this.gate = gate;
        this.go = go;
        this.restart = restart;
        View.BackgroundColor = PleyColors.Bg;
        backdrop = new BackdropView(images);
        View.Add(backdrop);
        scrim = new ScrimView();
        View.Add(scrim);
        View.Add(infoBlock);
        hero.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(HeroTop));
        View.Add(hero);
        View.Add(body);
        chipSlot.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(chipSlot);
        Group.Add(chipGroup);
        Group.Add(heroGroup);
        Group.ChildFocused += i => SetInRows(Group.Children[i] != heroGroup && Group.Children[i] != chipGroup);
        heroGroup.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        chipGroup.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
        model.Told += Told;
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        // Back on the page, what failed is asked again.
        if (entered && hadProblem) model.Retry();
        var first = !entered;
        entered = true;
        // At first, the hero's button; coming back, where focus was.
        if (first && Tree.Enter(heroGroup)) return;
        if (Tree.Enter(Group)) return;
        if (model.State.Value.Hero == null && model.State.Value.Problem == null && !model.State.Value.Nothing && Wait()) return;
        Shell.FocusRail();
    }

    public override void Gone()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        model.Told -= Told;
        model.Dispose();
        DropRows();
        skeletonTimer?.Stop();
        rest?.Stop();
    }

    /// <summary>Back from the rows: the hero again, on the button it had; the next Back goes to the rail.</summary>
    public override bool Back()
    {
        if (!inRows) return false;
        SetInRows(false);
        if (!Tree.Enter(heroGroup)) Shell.FocusRail();
        return true;
    }

    void Told(RequestEvent e) => RequestKit.Tell(Shell, e, model.State.Value.Gate.Summary, go);

    void Show(DiscoverState s)
    {
        hadProblem = s.Problem != null || s.Rows.Any(r => r.Pages.Problem != null);
        if (s.Problem is { } problem)
        {
            Notice(problem, s);
            return;
        }
        if (s.Hero == null)
        {
            if (s.Nothing) Notice(null, s);
            else ShowWaiting();
            return;
        }
        if (shownNotice != null) ClearAll();
        skeletonTimer?.Stop();
        Chip(s);
        Hero(s.Hero, s);
        Rows(s);
    }

    // ---- the page's states

    void ClearAll()
    {
        shownNotice = null;
        shape = "";
        shownHero = null;
        actions = null;
        DropRows();
        column = null;
        heroGroup.Clear();
        Clear(hero);
        Clear(body);
        Clear(infoBlock);
        Group.Clear();
        Group.Add(chipGroup);
        Group.Add(heroGroup);
    }

    static void Clear(View view)
    {
        foreach (var child in view.Children.ToList())
        {
            view.Remove(child);
            child.Dispose();
        }
    }

    /// <summary>Why the page can't show (with what can be done), or "Nothing here right now".</summary>
    void Notice(RequestProblem? problem, DiscoverState s)
    {
        var key = (object?)problem ?? "nothing";
        if (Equals(key, shownNotice)) return;
        var hadFocus = Tree.Current != null && Within(Tree.Current);
        ClearAll();
        shownNotice = key;
        skeletonTimer?.Stop();
        backdrop.Show(null);
        Chip(s, hidden: true);
        var summary = s.Gate.Summary;
        View notice;
        if (problem == null)
        {
            notice = RequestKit.Notice(RequestText.NothingTitle, RequestText.NothingBody, "compass", null, Group);
        }
        else
        {
            var action = RequestKit.ProblemAction(problem, summary, model.Retry, () => RequestKit.SignInAgain(layers, s.Origin, restart), () => go(new PageKey.Requests()));
            notice = RequestKit.Notice(RequestKit.ProblemTitle(problem, summary, RequestText.DiscoverFailed), RequestKit.ProblemBody(problem, summary), RequestKit.ProblemIcon(problem), action, Group);
        }
        body.Add(notice);
        // With nothing to press, focus goes to the rail, so the remote is never dead.
        // Only when focus was on the page: one in the open rail stays there.
        if (entered && (hadFocus || Tree.Current == null) && !Tree.Enter(Group)) Shell.FocusRail();
    }

    bool Within(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == Group) return true;
        return false;
    }

    /// <summary>Discover while its first list comes, after a moment: still blocks where the hero and the first row go. No spinner.</summary>
    void ShowWaiting()
    {
        if (shownNotice is "waiting" || skeletonTimer?.IsRunning() == true) return;
        ClearAll();
        shownNotice = "waiting";
        if (entered && !Wait()) Shell.FocusRail();
        skeletonTimer = new Timer(300);
        skeletonTimer.Tick += (_, _) =>
        {
            if (shownNotice is not "waiting") return false;
            void Block(float x, float y, float w, float h) =>
                body.Add(new View { Position = new Position(Ui.Dp(x), Ui.Dp(y)), Size = new Size(Ui.Dp(w), Ui.Dp(h)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
            var g = PleyTv.Gutter;
            Block(g, 131, 300, 38);
            Block(g, 180, 200, 18);
            Block(g, 212, 440, 15);
            Block(g, 272, 128, PleyTv.ButtonHeight);
            Block(g, 356, 180, 18);
            var tile = TileKind.Poster;
            for (var i = 0; i < SkeletonTiles; i++) Block(g + (tile.Width + tile.Gap) * i, 392, tile.Width, tile.Height);
            return false;
        };
        skeletonTimer.Start();
    }

    // ---- the way to the requests

    /// <summary>
    /// The way to the requests at the top, where other pages have their title: Your requests
    /// (Requests for managers) with their number, red while some couldn't be sent; for a profile with
    /// requests to see. It fades while focus is in the rows.
    /// </summary>
    void Chip(DiscoverState s, bool hidden = false)
    {
        var summary = s.Gate.Summary;
        var entry = RequestsEntry.Of(s.Gate);
        var shown = !hidden && summary is { RequestsEntry: true } && (summary.Own > 0 || summary.Manage);
        if (shownChip == (entry, shown)) return;
        shownChip = (entry, shown);
        var hadFocus = chipGroup.Children.Any(c => c is FocusItem { Focused: true });
        Clear(chipSlot);
        chipGroup.Clear();
        if (!shown || summary == null) return;
        var chip = new CountChip(summary.Manage ? RequestText.Manage : RequestText.Yours, entry.Badge, entry.Alarm, () => go(new PageKey.Requests()));
        chipSlot.Add(chip);
        chipGroup.Add(chip.Item);
        if (hadFocus) Tree.Focus(chip.Item);
    }

    // ---- the hero

    /// <summary>
    /// The hero: the title's two lines ending at y = 172 whatever their number, clear of the way to
    /// the requests above; its facts; a line of overview; the main action (its first line of words
    /// over it) and Details, which opens the title's page.
    /// </summary>
    void Hero(DiscoverHero h, DiscoverState s)
    {
        if (shownHero == h && actions != null) return;
        var focusedAt = heroGroup.Children.ToList().FindIndex(c => c is FocusGroup g ? g.Children.Any(i => i is FocusItem { Focused: true }) : c is FocusItem { Focused: true });
        var hadFocus = focusedAt >= 0 || (Tree.Current != null && Tree.Current.Parent == actions?.Group);
        var wasMain = actions?.Main is { Focused: true };
        shownHero = h;
        Clear(hero);
        heroGroup.Clear();
        var title = h.Title;
        var name = PanelColumn.Lines(title.Title, PleyType.HeroTitle, PleyColors.Fg, TextColumn, 2);
        name.SizeHeight = Ui.Dp(TitleBox);
        name.VerticalAlignment = VerticalAlignment.Bottom;
        hero.Add(name);
        var y = TitleBox + 8;
        var meta = MetaLine.Of(RequestKit.TitleMeta(title));
        meta.PositionY = Ui.Dp(y);
        hero.Add(meta);
        y += 18;
        if (!string.IsNullOrWhiteSpace(title.Overview))
        {
            y += 10;
            var overview = Text.Label(title.Overview, PleyType.BodyLarge, Summary, width: TextColumn);
            overview.PositionY = Ui.Dp(y);
            hero.Add(overview);
            y += PleyType.BodyLarge.Line;
        }
        y += 20;
        var handlers = new TitleHandlers(
            model.HeroAction,
            model.HeroMore,
            model.HeroTellMeToo,
            model.HeroStopFollowing,
            _ => model.HeroAction(),
            () => RequestKit.SignInAgain(layers, s.Origin, restart));
        actions = new TitleActionsView(h.Action, title.Kind, s.Allowance, s.Manage, handlers, compact: true,
            _ => new[] { new ExtraButton("Details", "info", () => go(new PageKey.DiscoverTitle(title.Kind, title.TmdbId))) });
        actions.PositionY = Ui.Dp(y);
        hero.Add(actions);
        heroGroup.Add(actions.Group);
        // Focus stays in the hero as its action changes: on the main button, which keeps its place.
        if (hadFocus || (Tree.Current == null && !inRows) || Tree.Current == base.Waiting)
        {
            var target = (wasMain ? actions.Main : null) ?? actions.Main ?? actions.Extras.FirstOrDefault();
            if (target != null) Tree.Focus(target);
        }
        Waited();
        if (!inRows) FollowFocus();
    }

    // ---- the rows

    void Rows(DiscoverState s)
    {
        var shown = s.Rows.Where(r => !(r.Pages.Titles.Count == 0 && r.Pages.Settled)).ToList();
        // Made again only as rows come and go (one TMDB gave nothing for, Your requests): a row's
        // first page, and the pages after it, change the row in place. Your requests are made again
        // as their number changes too: a row changed in place keeps the tiles past a shorter list's end.
        var next = string.Join(",", shown.Select(r => r.List)) + $"|{s.Yours.Count}";
        if (next != shape)
        {
            shape = next;
            Build(s, shown);
        }
        foreach (var row in shown)
        {
            if (!slots.TryGetValue(row.List, out var slot)) continue;
            if (row.Pages.Titles.Count > 0) slot.Show(TitleTiles(row.List, row.Pages.Titles, s.Origin), () => MakeRow(row.List, s));
        }
        if (yoursSlot != null && s.Yours.Count > 0)
            yoursSlot.Show(s.Yours.Select(r => RequestKit.RequestTile(r, s.Origin, s.Manage)).ToList(), () => MakeYours(s));
    }

    /// <summary>A row's tiles, made again only for the titles that changed.</summary>
    IReadOnlyList<TileModel> TitleTiles(DiscoverList list, IReadOnlyList<DiscoverTitle> titles, string origin)
    {
        if (!tileCache.TryGetValue(list, out var cache)) tileCache[list] = cache = new Dictionary<DiscoverTitle, TileModel>();
        return titles.Select(t =>
        {
            if (!cache.TryGetValue(t, out var tile)) cache[t] = tile = RequestKit.TitleTile(t, origin);
            return tile;
        }).ToList();
    }

    RowView MakeRow(DiscoverList list, DiscoverState s) =>
        new RowView(RequestText.Row(list), TitleTiles(list, s.Rows.First(r => r.List == list).Pages.Titles, s.Origin), TileKind.Poster, gate, images,
            tile =>
            {
                if (Find(list, tile.Id) is { } t) go(new PageKey.DiscoverTitle(t.Kind, t.TmdbId));
            },
            null,
            Shell.FocusRail,
            tile =>
            {
                var pages = model.State.Value.Rows.First(r => r.List == list).Pages;
                var at = pages.Titles.ToList().FindIndex(t => RequestKit.TileId(t) == tile.Id);
                if (at < 0) return;
                Rest(pages.Titles[at]);
                if (at >= pages.Titles.Count - NearEnd) Later(() => model.More(list));
            },
            Tree.Focus,
            (RequestText.SeeAll, () => go(new PageKey.DiscoverList(list))));

    RowView MakeYours(DiscoverState s) =>
        new RowView(RequestText.Yours, s.Yours.Select(r => RequestKit.RequestTile(r, s.Origin, s.Manage)).ToList(), TileKind.Landscape, gate, images,
            tile =>
            {
                if (model.State.Value.Yours.FirstOrDefault(r => r.Id == tile.Id) is { } request)
                    go(RequestsModel.Opens(request, model.State.Value.Gate.MayAsk) ?? new PageKey.DiscoverTitle(request.Kind, request.TmdbId));
            },
            null,
            Shell.FocusRail,
            tile =>
            {
                if (model.State.Value.Yours.FirstOrDefault(r => r.Id == tile.Id) is { } request) Rest(request);
            },
            Tree.Focus,
            (RequestText.SeeAll, () => go(new PageKey.Requests())));

    /// <summary>Runs <paramref name="work"/> after what runs now: a model that answers at once changes the page outside a focus move or a build.</summary>
    void Later(Action work) => post.Post(() =>
    {
        if (!gone) work();
    });

    /// <summary>The rows go: each disposed, so it stops making tiles and hearing the remote.</summary>
    void DropRows()
    {
        foreach (var slot in slots.Values) slot.Dispose();
        yoursSlot?.Dispose();
        slots.Clear();
        yoursSlot = null;
    }

    /// <summary>The column of rows, made again as rows come and go: focus stays on the tile it was on.</summary>
    void Build(DiscoverState s, IReadOnlyList<DiscoverRow> shown)
    {
        var focusedName = Tree.Current?.Name;
        var inColumn = column != null && Tree.Current != null && InColumn(Tree.Current);
        var old = column;
        if (old != null)
        {
            Group.Remove(old.Group);
            body.Remove(old);
        }
        DropRows();
        column = new RowColumnView(gate, top: FirstRowTop, firstAtTop: false);
        var lists = new List<DiscoverList>();
        foreach (var row in shown)
        {
            lists.Add(row.List);
            var slot = new RowSlot(RequestText.Row(row.List), TileKind.Poster);
            slots[row.List] = slot;
            column.Add(slot, slot.Group);
        }
        if (s.Yours.Count > 0)
        {
            yoursSlot = new RowSlot(RequestText.Yours, TileKind.Landscape);
            column.Add(yoursSlot, yoursSlot.Group);
        }
        var credit = RequestKit.Credit();
        var foot = new View { Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyType.Caption.Line + PleyTv.SafeVertical)) };
        foot.Add(credit);
        column.Add(foot, new FocusGroup(Axis.Row, "credit"));
        // A row coming into view is read, and the one after it, so its posters are on their way before focus gets there.
        column.Group.ChildFocused += i => Later(() =>
        {
            for (var k = i; k <= i + 2 && k < lists.Count; k++) model.ShowRow(lists[k]);
        });
        Later(() =>
        {
            foreach (var list in lists.Take(2)) model.ShowRow(list);
        });
        body.Add(column);
        Group.Add(column.Group);
        old?.Dispose();
        // The rows' tiles, as they are now, before focus goes back to the one it was on.
        foreach (var row in shown)
            if (row.Pages.Titles.Count > 0) slots[row.List].Show(TitleTiles(row.List, row.Pages.Titles, s.Origin), () => MakeRow(row.List, s));
        if (yoursSlot != null) yoursSlot.Show(s.Yours.Select(r => RequestKit.RequestTile(r, s.Origin, s.Manage)).ToList(), () => MakeYours(s));
        if (inColumn && focusedName != null && FindItem(column.Group, focusedName) is { } again) Tree.Focus(again);
        else if (inColumn) Tree.Enter(column.Group);
    }

    bool InColumn(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == column?.Group) return true;
        return false;
    }

    static FocusItem? FindItem(FocusGroup group, string name)
    {
        foreach (var child in group.Children)
        {
            if (child is FocusItem item && item.Name == name) return item;
            if (child is FocusGroup g && FindItem(g, name) is { } found) return found;
        }
        return null;
    }

    DiscoverTitle? Find(DiscoverList list, long tileId) =>
        model.State.Value.Rows.First(r => r.List == list).Pages.Titles.FirstOrDefault(t => RequestKit.TileId(t) == tileId);

    // ---- following focus

    void SetInRows(bool on)
    {
        if (on == inRows) return;
        inRows = on;
        if (!on)
        {
            about = null;
            column?.ScrollToTop();
        }
        reveal?.Stop();
        reveal = new Animation(PleyMotion.RevealMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        reveal.AnimateTo(hero, "Opacity", on ? 0f : 1f);
        reveal.AnimateTo(chipSlot, "Opacity", on ? 0f : 1f);
        reveal.AnimateTo(infoBlock, "Opacity", on ? 1f : 0f);
        Profile.Slowed(reveal);
        reveal.Play();
        // The chip fades with the hero, and can't take focus while it's gone.
        foreach (var item in chipGroup.Children.OfType<FocusItem>()) item.Shown = !on;
        scrim.Set(on ? ScrimView.Rows : ScrimView.Hero);
        FollowFocus();
    }

    /// <summary>Focus moved onto <paramref name="what"/> in the rows: once it rests there 400 ms, the backdrop and the info block follow it.</summary>
    void Rest(object what)
    {
        rest?.Stop();
        rest = new Timer(PleyMotion.RestMs);
        rest.Tick += (_, _) =>
        {
            about = what;
            FollowFocus();
            return false;
        };
        rest.Start();
    }

    void FollowFocus()
    {
        var s = model.State.Value;
        var picture = (inRows ? about : null) switch
        {
            DiscoverTitle t => t.BackdropUrl,
            MediaRequest r => r.BackdropUrl,
            _ => s.Hero?.Title.BackdropUrl,
        };
        backdrop.Show(string.IsNullOrEmpty(picture) ? null : new[] { Artwork.Absolute(picture!, s.Origin) });
        if (inRows) Info(about, s.Manage);
    }

    /// <summary>
    /// The block about where focus rests in the rows: a title's name, facts and two lines of
    /// overview; a request's name, its stage with the seasons asked for, who asked and when, and its stage's line.
    /// </summary>
    void Info(object? what, bool manage)
    {
        Clear(infoBlock);
        if (what == null) return;
        infoBlock.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(28));
        var name = Text.Label(what is DiscoverTitle t ? t.Title : ((MediaRequest)what).Title, PleyType.PageTitle, PleyColors.Fg, width: TextColumn);
        name.VerticalAlignment = VerticalAlignment.Bottom;
        name.SizeHeight = Ui.Dp(52);
        infoBlock.Add(name);
        var y = 52f + 10;
        if (what is DiscoverTitle title)
        {
            var meta = MetaLine.Of(RequestKit.TitleMeta(title));
            meta.PositionY = Ui.Dp(y);
            infoBlock.Add(meta);
            if (!string.IsNullOrWhiteSpace(title.Overview))
            {
                var overview = PanelColumn.Lines(title.Overview, PleyType.Body, Summary, TextColumn, 2);
                overview.PositionY = Ui.Dp(y + 18 + 8);
                infoBlock.Add(overview);
            }
            return;
        }
        var request = (MediaRequest)what;
        var pill = RequestRules.Pill(request, manage);
        var pillView = RequestKit.StagePill(pill, small: true);
        pillView.PositionY = Ui.Dp(y);
        infoBlock.Add(pillView);
        if (request.Seasons is { Count: > 0 } seasons)
        {
            var asked = Text.Label(RequestText.Seasons(seasons), PleyType.Label, PleyColors.Muted, width: 200);
            asked.Position = new Position(pillView.SizeWidth + Ui.Dp(8), Ui.Dp(y + 2));
            infoBlock.Add(asked);
        }
        y += 22 + 8;
        var who = Text.Label(RequestKit.Text.AskedBy(request, manage), PleyType.Caption, PleyColors.Muted, width: TextColumn);
        who.PositionY = Ui.Dp(y);
        infoBlock.Add(who);
        if (RequestKit.Text.PillLine(pill) is { } line)
        {
            var under = Text.Label(line, PleyType.Caption, PleyColors.Muted, width: TextColumn);
            under.PositionY = Ui.Dp(y + PleyType.Caption.Line + 2);
            infoBlock.Add(under);
        }
    }

    static readonly Color Summary = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.85f);
}

/// <summary>
/// A chip (design spec §2.6) with a number after its label (the Android app's TvCountChip): the list
/// icon, the label, and the number in a small pill, red while failed requests wait for a manager.
/// </summary>
sealed class CountChip : View
{
    readonly TextLabel label;
    readonly ImageView icon;
    readonly CountBadge? badge;
    Animation? focusAnimation;

    public CountChip(string text, int count, bool alarm, Action onClick)
    {
        Item = new FocusItem(text) { Selected = onClick };
        CornerRadius = Ui.Dp(PleyShape.ControlRadius);
        PivotPoint = global::Tizen.NUI.PivotPoint.Center;
        icon = Icons.Icon("list-video", PleyTv.IconSize, PleyColors.Fg);
        icon.Position = new Position(Ui.Dp(16), Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2));
        Add(icon);
        label = Text.Label(text, PleyType.Label, PleyColors.Fg);
        var labelWidth = label.NaturalSize2D.Width + Ui.Dp(2);
        label.Size = new Size(labelWidth, Ui.Dp(PleyTv.ButtonHeight));
        label.VerticalAlignment = VerticalAlignment.Center;
        label.Position = new Position(Ui.Dp(16 + PleyTv.IconSize + 8), 0);
        Add(label);
        var width = label.PositionX + labelWidth + Ui.Dp(16);
        if (count > 0)
        {
            badge = new CountBadge(count, alarm);
            badge.Position = new Position(width - Ui.Dp(8), (Ui.Dp(PleyTv.ButtonHeight) - badge.SizeHeight) / 2);
            Add(badge);
            width += badge.SizeWidth + Ui.Dp(8) - Ui.Dp(8);
            width += Ui.Dp(8);
        }
        Size = new Size(width, Ui.Dp(PleyTv.ButtonHeight));
        Item.FocusChanged += Look;
        Look(false);
    }

    public FocusItem Item { get; }

    void Look(bool focused)
    {
        if (Disposed) return;
        BackgroundColor = focused ? PleyColors.Accent : PleyColors.Surface3;
        var ink = focused ? PleyColors.OnAccent : PleyColors.Fg;
        label.TextColor = ink;
        icon.Color = ink;
        badge?.Focused(focused);
        focusAnimation?.Stop();
        var scale = focused ? PleyShape.PillScale : 1f;
        focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs);
        focusAnimation.AnimateTo(this, "Scale", new Vector3(scale, scale, 1));
        focusAnimation.Play();
    }
}

/// <summary>
/// A Discover row's place in the column, as tall as its row: its title and still wells (nothing in
/// it takes focus) until its first page comes, then the row, which takes the pages after it in place.
/// </summary>
sealed class RowSlot : View
{
    View? skeleton;
    RowView? row;

    public RowSlot(string title, TileKind kind)
    {
        Group = new FocusGroup(Axis.Row, title);
        var top = PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles;
        skeleton = new View();
        var header = Text.Label(title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        header.Position = new Position(Ui.Dp(PleyTv.Gutter), 0);
        skeleton.Add(header);
        for (var i = 0; i * (kind.Width + kind.Gap) < PleyTv.CanvasWidth - PleyTv.Gutter; i++)
            skeleton.Add(new View
            {
                Position = new Position(Ui.Dp(PleyTv.Gutter + (kind.Width + kind.Gap) * i), Ui.Dp(top)),
                Size = new Size(Ui.Dp(kind.Width), Ui.Dp(kind.Height)),
                CornerRadius = Ui.Dp(PleyShape.TileRadius),
                BackgroundColor = PleyColors.Surface2,
            });
        Add(skeleton);
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(top + kind.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight));
    }

    public FocusGroup Group { get; }

    /// <summary>The row's tiles are <paramref name="tiles"/>: the row is made (by <paramref name="make"/>) the first time, and changed in place after.</summary>
    public void Show(IReadOnlyList<TileModel> tiles, Func<RowView> make)
    {
        if (row != null)
        {
            row.Update(tiles);
            return;
        }
        row = make();
        if (skeleton != null)
        {
            Remove(skeleton);
            skeleton.Dispose();
            skeleton = null;
        }
        Add(row);
        Group.Add(row.Group);
        row.ShowPictures();
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        // Only when disposed by the app: collected, its row may have gone first (Kit/Held.cs).
        if (type == DisposeTypes.Explicit) row?.Dispose();
        base.Dispose(type);
    }
}
