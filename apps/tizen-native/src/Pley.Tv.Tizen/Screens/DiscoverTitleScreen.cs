using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A movie or show found on TMDB, on TV (the Android app's TvDiscoverTitleScreen), over its backdrop
/// (its poster at the right without one): the hero with its facts, network and genres, tagline and
/// overview, and its one main action, with Your requests where that takes no focus, and Overview
/// when the overview is cut; then its seasons, each with its state or its request's stage, and its
/// cast. OK on a season that may be asked for opens Which seasons? at it; Request more seasons
/// elsewhere opens it with the page. Focus starts on the main action.
/// </summary>
sealed class DiscoverTitleScreen : ShellPage
{
    const float TextColumn = 464;
    /// <summary>The hero's top, and the title's box: the title sits at its bottom.</summary>
    const float HeroTop = 52, TitleBox = 72;
    /// <summary>A short hero still puts the first row's header where its captions start under the screen's foot.</summary>
    const float HeroMin = 372;
    /// <summary>Without a backdrop, the poster at the right of the hero.</summary>
    const float PosterX = 664, PosterWidth = 200, PosterHeight = 300;
    const int OverviewLines = 3;

    readonly DiscoverTitleModel model;
    readonly Shell layers;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly Action<PageKey> restart;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly BackdropView backdrop;
    readonly View poster = new View();
    readonly View info = new View { Opacity = 0 };
    readonly View hero = new View();
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly FocusGroup heroGroup = new FocusGroup(Axis.Row, "hero");
    RowColumnView? column;
    /// <summary>The seasons' row, while the page has one: read again, its tiles take their new states.</summary>
    RowView? seasonsRow;
    TitleActionsView? actions;
    TitleState.Ready? shown;
    object? shownNotice;
    RequestProblem? toldFailure;
    PanelSheet? picker;
    bool inHero = true, entered, gone;
    int posterAsks;
    Animation? reveal;

    public DiscoverTitleScreen(TvShell shell, Shell layers, DiscoverTitleModel model, PageKey.DiscoverTitle key, Images images, KeyRepeatGate gate,
        Action<PageKey> go, Action<PageKey> restart) : base(shell, key, "title")
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
        View.Add(new ScrimView(ScrimView.Item));
        poster.Position = new Position(Ui.Dp(PosterX), Ui.Dp(HeroTop));
        View.Add(poster);
        info.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(28));
        View.Add(info);
        hero.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(HeroTop));
        View.Add(hero);
        View.Add(body);
        Group.ChildFocused += i => InHero(Group.Children[i] == heroGroup);
        heroGroup.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        model.Told += Told;
        watches.Add(model.State.Watch(Show));
        watches.Add(model.Picker.Watch(Picker));
    }

    public override void Enter()
    {
        if (entered && model.State.Value is TitleState.Problem) model.Retry();
        entered = true;
        if (Tree.Enter(Group)) return;
        if (model.State.Value is TitleState.Loading) Wait();
        else Shell.FocusRail();
    }

    public override void Gone()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        model.Told -= Told;
        picker?.Close();
        model.Dispose();
    }

    /// <summary>Back from the rows: the hero again; the next Back leaves the page.</summary>
    public override bool Back()
    {
        if (inHero || actions == null) return false;
        InHero(true);
        if (!Tree.Enter(heroGroup)) return false;
        return true;
    }

    void Told(RequestEvent e) => RequestKit.Tell(Shell, e, (model.State.Value as TitleState.Ready)?.Summary ?? (model.State.Value as TitleState.Problem)?.Summary, go);

    void Clear(View view)
    {
        foreach (var child in view.Children.ToList())
        {
            view.Remove(child);
            child.Dispose();
        }
    }

    void ClearAll()
    {
        shown = null;
        actions = null;
        column = null;
        seasonsRow = null;
        Clear(hero);
        Clear(body);
        Clear(info);
        Clear(poster);
        heroGroup.Clear();
        Group.Clear();
    }

    void Show(TitleState s)
    {
        switch (s)
        {
            case TitleState.Loading:
                if (shownNotice is "loading") return;
                ClearAll();
                shownNotice = "loading";
                Skeleton();
                return;
            case TitleState.Problem p:
                if (Equals(shownNotice, p.Value)) return;
                ClearAll();
                shownNotice = p.Value;
                backdrop.Show(null);
                View notice;
                if (p.Value is RequestProblem.NotFound)
                {
                    notice = RequestKit.Notice("This title isn't on TMDB", "It may have been taken off since.", "compass",
                        new RequestKit.NoticeAction(RequestText.Back, "arrow-left", () => Shell.Back?.Invoke()), Group);
                }
                else
                {
                    var action = RequestKit.ProblemAction(p.Value, p.Summary, model.Retry, () => RequestKit.SignInAgain(layers, model.Origin, restart), () => go(new PageKey.Requests()));
                    notice = RequestKit.Notice(RequestKit.ProblemTitle(p.Value, p.Summary, RequestText.TitleFailed), RequestKit.ProblemBody(p.Value, p.Summary), RequestKit.ProblemIcon(p.Value), action, Group);
                }
                body.Add(notice);
                if (entered && !Tree.Enter(Group)) Shell.FocusRail();
                Waited();
                return;
            case TitleState.Ready ready:
                // Read again and refused, the page keeps what it showed and says why (the shell says so when the server is away).
                if (ready.Failed is { } failed and not RequestProblem.Offline && !Equals(failed, toldFailure)) Shell.Toast(RequestKit.Text.Problem(failed, ready.Summary));
                toldFailure = ready.Failed;
                Page(ready);
                // Which seasons? asked for with the page, before it could show.
                if (picker == null && model.Picker.Value is { } waiting) Picker(waiting);
                return;
        }
    }

    /// <summary>The loading page: still blocks where the hero's text and button will be. No spinner.</summary>
    void Skeleton()
    {
        void Block(float y, float w, float h) =>
            body.Add(new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y)), Size = new Size(Ui.Dp(w), Ui.Dp(h)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
        Block(82, 320, 40);
        Block(136, 220, 18);
        Block(166, 160, 14);
        Block(196, 464, 15);
        Block(218, 440, 15);
        Block(240, 300, 15);
        Block(282, 140, PleyTv.ButtonHeight);
    }

    void Page(TitleState.Ready page)
    {
        var detail = page.Detail;
        var same = shown != null && shown.Detail == detail && shown.Action == page.Action && shown.Busy == page.Busy && shown.Allowance == page.Allowance;
        if (same) return;
        var hadFocusName = Tree.Current?.Name;
        var heroFocus = actions != null && Tree.Current != null && Tree.Current.Parent == actions.Group;
        var wasLoading = shownNotice != null;
        shownNotice = null;
        var sameTitle = shown?.Detail.TmdbId == detail.TmdbId && shown.Detail.Kind == detail.Kind && shown.Detail.Seasons?.Count == detail.Seasons?.Count && shown.Detail.Cast.Count == detail.Cast.Count;
        if (wasLoading)
        {
            Clear(body);
            Group.Clear();
        }
        shown = page;
        var origin = model.Origin;
        backdrop.Show(string.IsNullOrEmpty(detail.BackdropUrl) ? null : new[] { Artwork.Absolute(detail.BackdropUrl!, origin) });
        Clear(poster);
        var asked = ++posterAsks;
        if (string.IsNullOrEmpty(detail.BackdropUrl) && !string.IsNullOrEmpty(detail.PosterUrl))
        {
            var art = new ImageView { Size = new Size(Ui.Dp(PosterWidth), Ui.Dp(PosterHeight)), CornerRadius = Ui.Dp(PleyShape.ControlRadius), BackgroundColor = PleyColors.Surface2 };
            poster.Add(art);
            images.Load(new[] { Artwork.Absolute(detail.PosterUrl!, origin) }, PosterWidth, file =>
            {
                // The page went, or drew another poster, meanwhile: this one is gone.
                if (file != null && !gone && asked == posterAsks) art.ResourceUrl = file;
            });
        }
        Info(detail);
        var heroHeight = Hero(page);
        if (!sameTitle || column == null) Rows(page, Math.Max(heroHeight, HeroMin));
        else
        {
            // Read again (a season asked for, a download's progress): the seasons take their new states, the rows follow the hero.
            seasonsRow?.Update(SeasonTiles(page, origin));
            column.MoveTop(Math.Max(heroHeight, HeroMin));
        }
        if (Group.Children.Count == 0 || Group.Children[0] != heroGroup) Group.Insert(0, heroGroup);
        // Focus where it was in the hero (its main button keeps its place as Request turns into Requested), else on the main action.
        if (heroFocus || wasLoading || Tree.Current == Waiting)
        {
            var again = hadFocusName != null ? actions!.Group.Children.OfType<FocusItem>().FirstOrDefault(i => i.Name == hadFocusName && i.CanTakeFocus) : null;
            var target = again ?? actions!.Main ?? actions.Extras.FirstOrDefault() ?? actions.Group.Children.OfType<FocusItem>().FirstOrDefault(i => i.CanTakeFocus);
            if (target != null) Tree.Focus(target);
            else if (entered && !Tree.Enter(Group)) Shell.FocusRail();
        }
        Waited();
    }

    /// <summary>
    /// The hero, in the 464 dp text column at x = 96: the title bottom-aligned in the logo's box, its
    /// facts, network and genres, the tagline, three lines of overview, then the main action. After
    /// it, Your requests when the main action takes no focus, and Overview when the overview runs past
    /// its lines. Its height, from the page's top.
    /// </summary>
    float Hero(TitleState.Ready page)
    {
        var detail = page.Detail;
        Clear(hero);
        heroGroup.Clear();
        var name = PanelColumn.Lines(detail.Title, PleyType.HeroTitle, PleyColors.Fg, TextColumn, 2);
        var nameHeight = Math.Max(TitleBox, name.SizeHeight / Ui.Scale);
        name.SizeHeight = Ui.Dp(nameHeight);
        name.VerticalAlignment = VerticalAlignment.Bottom;
        hero.Add(name);
        var y = nameHeight + 12;
        var meta = MetaLine.Of(RequestKit.TitleMeta(detail));
        meta.PositionY = Ui.Dp(y);
        hero.Add(meta);
        y += 18;
        var facts = string.Join(" · ", new[] { detail.Network }.Where(n => !string.IsNullOrWhiteSpace(n)).Concat(detail.Genres));
        if (facts.Length > 0)
        {
            y += 6;
            var line = Text.Label(facts, PleyType.Caption, PleyColors.Muted, width: TextColumn);
            line.PositionY = Ui.Dp(y);
            hero.Add(line);
            y += PleyType.Caption.Line;
        }
        if (!string.IsNullOrWhiteSpace(detail.Tagline))
        {
            y += 10;
            var tagline = Text.Label(detail.Tagline!, PleyType.Body, new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.75f), width: TextColumn);
            tagline.PositionY = Ui.Dp(y);
            hero.Add(tagline);
            y += PleyType.Body.Line;
        }
        var cut = false;
        if (!string.IsNullOrWhiteSpace(detail.Overview))
        {
            y += 10;
            var overview = PanelColumn.Lines(detail.Overview, PleyType.BodyLarge, Summary, TextColumn, OverviewLines);
            cut = overview.GetHeightForWidth(Ui.Dp(TextColumn)) > Kit.Text.LineHeight(PleyType.BodyLarge) * OverviewLines + 1;
            overview.PositionY = Ui.Dp(y);
            hero.Add(overview);
            y += overview.SizeHeight / Ui.Scale;
        }
        y += 22;
        var summary = page.Summary;
        var handlers = new TitleHandlers(model.Request, () => model.OpenPicker(), model.TellMeToo, model.StopFollowing,
            id => go(new PageKey.Item(id)), () => RequestKit.SignInAgain(layers, model.Origin, restart));
        actions = new TitleActionsView(page.Action, detail.Kind, page.Allowance, page.Manage, handlers, compact: false, claimed =>
        {
            var extras = new List<ExtraButton>();
            if (!claimed && summary is { RequestsEntry: true })
                extras.Add(new ExtraButton(summary.Manage ? RequestText.Manage : RequestText.Yours, "list-video", () => go(new PageKey.Requests())));
            if (cut) extras.Add(new ExtraButton(RequestText.Overview, "ellipsis", () => Overview(detail)));
            return extras;
        });
        actions.PositionY = Ui.Dp(y);
        hero.Add(actions);
        heroGroup.Add(actions.Group);
        return HeroTop + y + actions.HeightDp + 8;
    }

    /// <summary>The seasons, specials last, as posters with their badges and states; the cast; TMDB's credit.</summary>
    void Rows(TitleState.Ready page, float top)
    {
        if (column != null)
        {
            Group.Remove(column.Group);
            body.Remove(column);
            column.Dispose();
        }
        var detail = page.Detail;
        var origin = model.Origin;
        column = new RowColumnView(gate, top: top, firstAtTop: false);
        seasonsRow = null;
        var seasons = SeasonTiles(page, origin);
        if (seasons.Count > 0)
        {
            column.Add(seasonsRow = new RowView(RequestText.SeasonsHeading, seasons, TileKind.Poster, gate, images,
                tile =>
                {
                    var current = model.State.Value as TitleState.Ready;
                    var season = current?.Detail.Seasons?.FirstOrDefault(s => s.Number == tile.Id);
                    if (current != null && season != null && CanPick(current.Detail, current.Action) && RequestRules.Askable(season, current.Detail)) model.OpenPicker(season.Number);
                },
                null, Shell.FocusRail, focusItem: Tree.Focus));
        }
        if (detail.Cast.Count > 0)
        {
            var tiles = detail.Cast.Select((m, i) => new TileModel(i, m.Name, m.Character ?? "",
                string.IsNullOrEmpty(m.PhotoUrl) ? Array.Empty<string>() : new[] { Artwork.Absolute(m.PhotoUrl!, origin) }, 0f, false, 0, Array.Empty<string>(),
                string.Join(", ", new[] { m.Name, m.Character }.Where(s => !string.IsNullOrWhiteSpace(s))))).ToList();
            // TMDB's people have no page here: the row is there to read.
            column.Add(new RowView(RequestText.Cast, tiles, TileKind.Person, gate, images, _ => { }, null, Shell.FocusRail, focusItem: Tree.Focus));
        }
        var foot = new View { Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyType.Caption.Line + PleyTv.SafeVertical)) };
        foot.Add(RequestKit.Credit());
        column.Add(foot, new FocusGroup(Axis.Row, "credit"));
        body.Add(column);
        Group.Add(column.Group);
    }

    /// <summary>The seasons' tiles, specials last.</summary>
    static List<TileModel> SeasonTiles(TitleState.Ready page, string origin) =>
        (page.Detail.Seasons ?? Array.Empty<DiscoverSeason>()).OrderBy(s => s.Number == 0).ThenBy(s => s.Number).Select(s => SeasonTile(s, page, origin)).ToList();

    /// <summary>A season's poster (else the show's): its badge, and under it its request's stage, else its state.</summary>
    static TileModel SeasonTile(DiscoverSeason season, TitleState.Ready page, string origin)
    {
        var detail = page.Detail;
        var request = RequestRules.RequestOf(season, detail);
        var asked = request != null && season.State == SeasonState.Requested;
        var line = asked ? RequestKit.Text.Pill(RequestRules.Pill(request!, page.Manage)) : RequestText.SeasonState(season);
        TitleBadge? badge = season.State switch
        {
            SeasonState.Library => TitleBadge.InLibrary,
            SeasonState.Partial => TitleBadge.PartlyInLibrary,
            SeasonState.Requested => TitleBadge.Requested,
            _ => season.Declined ? TitleBadge.Declined : null,
        };
        var badgeText = badge is { } b ? RequestText.Badge(b) : null;
        var name = RequestText.SeasonName(season);
        var art = new[] { season.PosterUrl, detail.PosterUrl }.Where(u => !string.IsNullOrEmpty(u)).Select(u => Artwork.Absolute(u!, origin)).ToList();
        return new TileModel(season.Number, name, line, art, asked ? RequestRules.DownloadProgress(request!) ?? 0f : 0f, false, 0,
            badgeText == null ? Array.Empty<string>() : new[] { badgeText }, string.Join(", ", new[] { name, badgeText, line }.Where(s => s != null)));
    }

    /// <summary>The block over the rows once focus leaves the hero: the title, its facts, two lines of overview.</summary>
    void Info(DiscoverDetail detail)
    {
        Clear(info);
        var name = Text.Label(detail.Title, PleyType.HeroTitle, PleyColors.Fg, width: TextColumn);
        info.Add(name);
        var meta = MetaLine.Of(RequestKit.TitleMeta(detail));
        meta.PositionY = Ui.Dp(PleyType.HeroTitle.Line + 10);
        info.Add(meta);
        if (!string.IsNullOrWhiteSpace(detail.Overview))
        {
            var overview = PanelColumn.Lines(detail.Overview, PleyType.Body, Summary, TextColumn, 2);
            overview.PositionY = Ui.Dp(PleyType.HeroTitle.Line + 10 + 18 + 8);
            info.Add(overview);
        }
    }

    void InHero(bool on)
    {
        if (on == inHero) return;
        inHero = on;
        if (on) column?.ScrollToTop();
        reveal?.Stop();
        reveal = new Animation(PleyMotion.RevealMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        reveal.AnimateTo(hero, "Opacity", on ? 1f : 0f);
        reveal.AnimateTo(poster, "Opacity", on ? 1f : 0f);
        reveal.AnimateTo(info, "Opacity", on ? 0f : 1f);
        Profile.Slowed(reveal);
        reveal.Play();
    }

    /// <summary>The whole overview, with the tagline over it, in a panel beside the page.</summary>
    void Overview(DiscoverDetail detail)
    {
        var sheet = new PanelSheet(layers);
        sheet.Column.Title(detail.Title);
        var text = string.Join("\n\n", new[] { detail.Tagline, detail.Overview }.Where(s => !string.IsNullOrWhiteSpace(s)));
        sheet.Column.Sentence(text, PleyType.Body, PleyColors.Fg, lines: 24);
        var close = sheet.Column.Choice(new PanelChoice(RequestText.Close, () => sheet.Close(), "x"), gap: 16);
        sheet.Show(close.Item);
    }

    // ---- Which seasons?

    /// <summary>
    /// Which seasons?, a side panel: the title under its name, every season with its line, those that
    /// may be asked for ticked or not, then the limits when more are ticked than may be asked for
    /// (the first line marked), and Request N seasons. Focus starts on the season it was opened at,
    /// else the first that may be asked for, and stays in the panel; Back closes it.
    /// </summary>
    void Picker(SeasonPicker? open)
    {
        if (open == null)
        {
            var sheet = picker;
            picker = null;
            sheet?.Close();
            return;
        }
        var page = model.State.Value as TitleState.Ready;
        if (page == null) return;
        var fresh = picker == null;
        var focusedName = picker?.Tree.Current?.Name;
        if (picker == null)
        {
            picker = new PanelSheet(layers);
            var mine = picker;
            mine.Gone += () =>
            {
                if (picker != mine) return;
                picker = null;
                model.ClosePicker();
            };
        }
        var column = picker.Column;
        column.Clear();
        column.Title(RequestText.PickerTitle, page.Detail.Title);
        FocusItem? start = null;
        var startAt = open.Start is { } n && open.Seasons.Any(s => s.Askable && s.Season.Number == n) ? n : open.Seasons.FirstOrDefault(s => s.Askable)?.Season.Number;
        foreach (var row in open.Seasons)
        {
            var number = row.Season.Number;
            var choice = new PanelChoice(RequestText.SeasonName(row.Season), () => model.Toggle(number), line: RequestText.SeasonState(row.Season),
                chosen: row.Askable && open.Picked.Contains(number)) { Enabled = row.Askable };
            column.Choice(choice, gap: 2);
            if (number == startAt) start = choice.Item;
        }
        var lines = open.OverLimit || open.Left is <= 0 ? RequestKit.Text.Allowance(page.Allowance, TitleKind.Show) : Array.Empty<string>();
        for (var i = 0; i < lines.Count; i++)
        {
            // More ticked than may be asked for: the line saying how many may stands out.
            var marked = open.OverLimit && i == 0;
            column.Sentence(lines[i], PleyType.Caption, marked ? PleyColors.Fg : PleyColors.Muted, gap: i == 0 ? 12 : 4, lines: 3);
        }
        var ask = new PanelChoice(RequestText.PickerButton(open), model.AskPicked, open.Count > 0 ? "plus" : null) { Enabled = open.Ready };
        column.Choice(ask, gap: 16);
        if (fresh)
        {
            picker.Show(start);
            return;
        }
        // Made again as a season is ticked: focus stays on the row it was on.
        var again = column.Group.Children.OfType<FocusItem>().FirstOrDefault(i => i.Name == focusedName && i.CanTakeFocus);
        if (again != null) picker.Tree.Focus(again);
        else picker.Tree.Enter(column.Group);
    }

    /// <summary>Whether a show's seasons may be picked to ask for now: Request, or Request more seasons, even at the limit (the picker says so).</summary>
    static bool CanPick(DiscoverDetail detail, MainAction action)
    {
        if (detail.Kind != TitleKind.Show || !(detail.Seasons ?? Array.Empty<DiscoverSeason>()).Any(s => RequestRules.Askable(s, detail))) return false;
        return action is MainAction.Request { Ask: not Ask.NoAccess } || action is MainAction.Partial { More: Ask.Yes or Ask.Limit };
    }

    static readonly Color Summary = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.85f);
}
