using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// An item's page on TV (design spec §4.3, §4.4; the Android app's TvItemScreen): a movie, show,
/// season or episode over its full-bleed backdrop. Focus starts on Resume or Play. Down goes to the
/// episodes (shows, seasons, episodes) and the rows (Extras, Cast, More Like This); once focus has
/// left the buttons, an info block stays at the top, about the focused episode in the episode row.
/// </summary>
sealed class ItemScreen : ShellPage
{
    const float TextColumn = 464, HeroTop = 52, TitleBox = 72, SummaryLines = 3;

    readonly Shell layers;
    readonly ItemModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly BackdropView backdrop;
    readonly View hero = new View();
    readonly Redrawn heroText;
    readonly ButtonRow heroButtons = new ButtonRow();
    readonly View infoBlock = new View { Opacity = 0 };
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly Timer episodeRest = new Timer((uint)KeyRepeatGate.RestMs);
    /// <summary>
    /// A page shown already is drawn again once what it says has settled: back from a film, four or
    /// five answers came within 400 ms (its progress, its details, its seasons), each drawn whole,
    /// 50 to 180 ms each, as the TV's player was let go.
    /// </summary>
    readonly Timer redraw = new Timer(RedrawMs);
    const uint RedrawMs = 60;
    ItemPage? waiting;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly Dictionary<FocusItem, HeroButtonKind> buttonKinds = new Dictionary<FocusItem, HeroButtonKind>();
    RowColumnView? column;
    SeasonsView? seasons;
    SeasonPostersView? seasonPosters;
    ItemPage? shown;
    /// <summary>What requests say of a movie or show: its request's stage, and Request more seasons.</summary>
    readonly ItemRequestModel? requests;
    FocusItem? moreSeasons;
    string shownSections = "";
    EpisodeLine? focusedEpisode;
    EpisodeLine? restedEpisode;
    bool inSections;
    bool summaryCut;
    Animation? reveal;
    Animation? reading;
    string shownKind = "";

    /// <param name="layers">The app's sheets, for the side panels and dialogs.</param>
    /// <param name="go">Opens another page: an item, a person, a collection.</param>
    public ItemScreen(TvShell shell, Shell layers, ItemModel model, Images images, KeyRepeatGate gate, Action<PageKey> go, ItemRequestModel? requests = null)
        : base(shell, new PageKey.Item(model.ItemId), "item")
    {
        this.requests = requests;
        this.layers = layers;
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.BackgroundColor = PleyColors.Bg;
        backdrop = new BackdropView(images);
        View.Add(backdrop);
        View.Add(new ScrimView(ScrimView.Item));
        hero.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(HeroTop));
        heroText = new Redrawn(hero);
        hero.Add(heroButtons);
        View.Add(hero);
        View.Add(body);
        View.Add(infoBlock);
        Group.ChildFocused += i => SetInSections(Group.Children[i] != heroButtons.Group);
        heroButtons.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        redraw.Tick += (s, e) =>
        {
            if (waiting is { } page && model.State.Value is ItemState.Ready) Draw(page);
            waiting = null;
            return false;
        };
        episodeRest.Tick += (s, e) =>
        {
            restedEpisode = focusedEpisode;
            ShowInfoBlock();
            return false;
        };

        model.Told += OnTold;
        model.PlayRequested += OnPlay;
        model.OpenRequested += go;
        watches.Add(model.State.Watch(Show));
        if (requests != null) watches.Add(requests.State.Watch(_ =>
        {
            if (shown == null) return;
            ShowHero(shown, false);
            // The hero grew or shrank by the request's line: the rows under it follow.
            column?.MoveTop(RowsTop());
        }));
    }

    /// <summary>Where the rows start while the page is at its top: under the hero's buttons, in dp.</summary>
    float RowsTop() => HeroTop + heroButtons.PositionY / Ui.Scale + PleyTv.ButtonHeight + 8 + PleyTv.RowGap;

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Told -= OnTold;
        model.PlayRequested -= OnPlay;
        model.OpenRequested -= go;
        episodeRest.Stop();
        redraw.Stop();
        heroText.Stop();
        model.Dispose();
        requests?.Dispose();
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Wait();
    }

    void OnTold(ItemEvent e) => Shell.Toast(ItemText.Toast(e).Text);

    void OnPlay(PlayRequest request) => Shell.Play?.Invoke(request);

    void Show(ItemState state)
    {
        switch (state)
        {
            case ItemState.Ready ready:
                if (shownKind != "ready")
                {
                    Clear();
                    shownKind = "ready";
                    redraw.Stop();
                    waiting = null;
                    Draw(ready.Page);
                    return;
                }
                if (waiting == null) redraw.Start();
                waiting = ready.Page;
                return;
            case ItemState.Loading:
                if (shownKind == "loading") return;
                shownKind = "loading";
                Clear();
                Skeleton();
                return;
            case ItemState.NotFound:
                shownKind = "notFound";
                Clear();
                Message(ItemText.NotFound, ItemText.NotFoundBody, ItemText.Back, () => Shell.Back?.Invoke());
                return;
            case ItemState.Failed f:
                shownKind = "failed";
                Clear();
                Message(ItemText.FailedTitleOf(f.Error), ItemText.FailedBody(f.Error), ItemText.Retry, model.Retry);
                return;
            case ItemState.Elsewhere e:
                Shell.ReplaceTop(e.Key);
                return;
        }
    }

    void Clear()
    {
        Group.Clear();
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        hero.Hide();
        column = null;
        seasons = null;
        shown = null;
        shownSections = "";
    }

    /// <summary>The loading page (design spec §8): still blocks where the hero's text and buttons will be. No spinner.</summary>
    void Skeleton()
    {
        float y = 82;
        void Block(float x, float w, float h, float radius = PleyShape.BadgeRadius)
        {
            body.Add(new View { Position = new Position(Ui.Dp(x), Ui.Dp(y)), Size = new Size(Ui.Dp(w), Ui.Dp(h)), CornerRadius = Ui.Dp(radius), BackgroundColor = PleyColors.Surface2 });
        }
        var g = PleyTv.Gutter;
        Block(g, 320, 40);
        y += 40 + 14;
        Block(g, 220, 18);
        y += 18 + 24;
        Block(g, 464, 15);
        y += 15 + 7;
        Block(g, 440, 15);
        y += 15 + 7;
        Block(g, 300, 15);
        y += 15 + 38;
        Block(g, 140, PleyTv.ButtonHeight, PleyShape.ControlRadius);
        for (var i = 0; i < 3; i++) Block(g + 150 + i * (PleyTv.ButtonHeight + PleyTv.ButtonGap), PleyTv.ButtonHeight, PleyTv.ButtonHeight, PleyShape.ControlRadius);
    }

    void Message(string title, string text, string action, Action onAction)
    {
        var width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
        var heading = Text.Label(title, PleyType.PageTitle, PleyColors.Fg, width: width);
        heading.HorizontalAlignment = HorizontalAlignment.Center;
        heading.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(190));
        var words = Text.Label(text, PleyType.Body, PleyColors.Muted, maxLines: 3, width: TextColumn);
        words.HorizontalAlignment = HorizontalAlignment.Center;
        words.VerticalAlignment = VerticalAlignment.Top;
        words.Position = new Position(Ui.Dp(PleyTv.Gutter + (width - TextColumn) / 2), Ui.Dp(190 + PleyType.PageTitle.Line + 12));
        body.Add(heading);
        body.Add(words);
        var row = new ButtonRow();
        row.Add(new ButtonView(action, ButtonKind.Pill, onAction));
        row.Position = new Position(Ui.Dp(PleyTv.Gutter + (width - row.WidthDp) / 2), Ui.Dp(190 + PleyType.PageTitle.Line + 12 + PleyType.Body.Line * 3 + 24));
        body.Add(row);
        Group.Add(row.Group);
        Tree.Enter(row.Group);
    }

    void Draw(ItemPage page)
    {
        var drawn = Profile.Start();
        ShowPage(page);
        Profile.Took("item page drawn", drawn);
    }

    void ShowPage(ItemPage page)
    {
        var first = shown == null;
        shown = page;
        hero.Show();
        backdrop.Show(HomeRules.Backdrop(page.Item, page.Origin));
        ShowHero(page, first);
        ShowSections(page, first);
        // Drawn: what requests say of it may be asked now.
        requests?.Shown(page.Item.Kind);
        if (inSections) ShowInfoBlock();
    }

    /// <summary>
    /// The hero (the Android app's TvHero): the title bottom-aligned in the logo's 72 dp box, an
    /// episode's context line, the metadata line, the time left, genres, three lines of summary,
    /// the credits, then the buttons. Focus starts on Resume or Play.
    /// </summary>
    void ShowHero(ItemPage page, bool first)
    {
        // Drawn over the one before, which goes once this is drawn: emptied first, the hero blinked.
        var block = heroText.Next();
        float y = 0;
        var title = Text.Label(ItemText.HeroTitle(page), PleyType.HeroTitle, PleyColors.Fg, maxLines: 2, width: TextColumn);
        title.VerticalAlignment = VerticalAlignment.Bottom;
        title.SizeHeight = Ui.Dp(TitleBox);
        block.Add(title);
        y += TitleBox;
        void Line(TextLabel label, float gap, float height)
        {
            y += gap;
            label.PositionY = Ui.Dp(y);
            block.Add(label);
            y += height;
        }
        if (ItemText.ContextLine(page) is { } context) Line(Text.Label(context, PleyType.Title, PleyColors.Fg, width: TextColumn), 8, PleyType.Title.Line);
        y += 12;
        var meta = MetaLine.Of(page.Meta);
        meta.PositionY = Ui.Dp(y);
        block.Add(meta);
        y += 18;
        // Its request's stage under the facts: the small pill, and a download's 120 dp bar beside it.
        if (requests?.State.Value is { Request: { } asked } request)
        {
            y += 10;
            var pill = RequestKit.StagePill(RequestRules.Pill(asked, request.Manage), small: true);
            pill.PositionY = Ui.Dp(y);
            block.Add(pill);
            if (asked.Download is { } download)
            {
                var bar = RequestKit.DownloadBar(RequestRules.DownloadProgress(asked) ?? 0f, download.State, 120);
                bar.Position = new Position(pill.SizeWidth + Ui.Dp(12), Ui.Dp(y + 9));
                block.Add(bar);
            }
            y += 22;
        }
        if (ItemText.TimeLeft(page) is { } left) Line(Text.Label(left, PleyType.Caption, PleyColors.Fg, width: TextColumn), 8, PleyType.Caption.Line);
        var genres = ItemText.GenresLine(page);
        if (genres.Length > 0) Line(Text.Label(genres, PleyType.Caption, PleyColors.Muted, width: TextColumn), 6, PleyType.Caption.Line);
        if (page.Summary is { } summary)
        {
            var text = Text.Label(summary, PleyType.BodyLarge, Faded, maxLines: (int)SummaryLines, width: TextColumn);
            text.VerticalAlignment = VerticalAlignment.Top;
            // Longer than its lines: More offers it all.
            summaryCut = text.GetHeightForWidth(Ui.Dp(TextColumn)) > Text.LineHeight(PleyType.BodyLarge) * SummaryLines + 1;
            Line(text, 10, PleyType.BodyLarge.Line * SummaryLines);
        }
        if (ItemText.Credits(page) is { } credits)
        {
            var label = Text.Label("", PleyType.Caption, PleyColors.Fg, width: TextColumn);
            label.EnableMarkup = true;
            label.Text = CreditsMarkup(credits);
            Line(label, 8, PleyType.Caption.Line);
        }
        y += 22;
        heroButtons.PositionY = Ui.Dp(y);
        ShowButtons(page, first);
    }

    /// <summary>"Directed by Lena Okafor · Starring Idris Vale, Maren Holt": the labels in Muted.</summary>
    static string CreditsMarkup(CreditsLine credits)
    {
        static string Escape(string s) => s.Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;");
        var muted = "#A1A9B6";
        var parts = new List<string>();
        if (credits.Made != null && credits.Makers.Length > 0) parts.Add($"<color value='{muted}'>{Escape(credits.Made)} </color>{Escape(credits.Makers)}");
        if (credits.Starring.Length > 0) parts.Add($"<color value='{muted}'>{Escape(ItemText.Starring)} </color>{Escape(credits.Starring)}");
        return string.Join("<color value='#8B95A5'> · </color>", parts);
    }

    /// <summary>The buttons drawn.</summary>
    IReadOnlyList<HeroButton>? buttonsShown;

    void ShowButtons(ItemPage page, bool first)
    {
        // The same buttons stay: made again, their labels blinked as the page came back from a film.
        var buttons = ItemText.Buttons(page);
        if (!first && buttonsShown != null && buttons.SequenceEqual(buttonsShown)) return;
        buttonsShown = buttons;
        HeroButtonKind? focusedKind = Tree.Current != null && buttonKinds.TryGetValue(Tree.Current, out var k) ? k : null;
        var wasMore = moreSeasons != null && Tree.Current == moreSeasons;
        moreSeasons = null;
        buttonKinds.Clear();
        // Let go only once focus is on their replacements: the one with focus hears it leave.
        var old = heroButtons.Children.ToList();
        foreach (var child in old) heroButtons.Remove(child);
        heroButtons.Group.Clear();
        float x = 0;
        FocusItem? start = null;
        FocusItem? again = null;
        foreach (var b in buttons)
        {
            View view;
            FocusItem item;
            if (b.Kind == HeroButtonKind.Primary)
            {
                var pill = new ButtonView(b.Label, ButtonKind.Pill, () => Press(b.Kind), iconName: b.Icon) { Enabled = b.Enabled };
                // The page as it is then: the buttons outlive the page they were made for.
                pill.Item.LongSelected = () =>
                {
                    var now = shown ?? page;
                    Panel(ItemText.ActionsPanel(now.Item), now.Item);
                };
                pill.Item.Menu = pill.Item.LongSelected;
                view = pill;
                item = pill.Item;
            }
            else
            {
                var round = new RoundButton(b.Label, b.Icon ?? "info", () => Press(b.Kind));
                view = round;
                item = round.Item;
            }
            // Known by kind, so a button keeps focus when the page changes under it (Watched, My List).
            buttonKinds[item] = b.Kind;
            view.PositionX = Ui.Dp(x);
            heroButtons.Add(view);
            heroButtons.Group.Add(item);
            x += view.SizeWidth / Ui.Scale + PleyTv.ButtonGap;
            if (b.Kind == ItemText.FirstFocus(page)) start = item;
            if (b.Kind == focusedKind) again = item;
            // After Play, a show with seasons to ask for: its Discover page, Which seasons? open.
            if (b.Kind == HeroButtonKind.Primary && requests?.State.Value is { More: true } request)
            {
                var more = new ButtonView(RequestText.MoreSeasons, ButtonKind.Pill, () => go(request.Picker), iconName: "plus");
                more.PositionX = Ui.Dp(x);
                heroButtons.Add(more);
                heroButtons.Group.Add(more.Item);
                x += more.WidthDp + PleyTv.ButtonGap;
                if (wasMore) again = more.Item;
                moreSeasons = more.Item;
            }
        }
        heroButtons.SizeWidth = Ui.Dp(x);
        if (first)
        {
            Group.Clear();
            Group.Add(heroButtons.Group);
            if (start != null) heroButtons.Group.RememberChild(heroButtons.Group.Children.ToList().IndexOf(start));
        }
        if (again != null) Tree.Focus(again);
        // The focused button went (Trailer, From start, Request more seasons once asked): focus to where the page starts.
        else if ((first || focusedKind != null || wasMore) && start != null) Tree.Focus(start);
        foreach (var child in old) child.Dispose();
    }

    void Press(HeroButtonKind kind)
    {
        var page = shown;
        if (page == null) return;
        switch (kind)
        {
            case HeroButtonKind.Primary:
                model.Play(false);
                break;
            case HeroButtonKind.FromStart:
                model.Play(true);
                break;
            case HeroButtonKind.Watched:
                model.ToggleWatched();
                break;
            case HeroButtonKind.Tracks:
                if (ItemText.TracksPanel(page) is { } tracks) Panel(tracks, page.Item);
                break;
            case HeroButtonKind.MyList:
                model.ToggleMyList();
                break;
            case HeroButtonKind.Trailer:
                if (page.Trailer != null) model.Play(page.Trailer, fromStart: true);
                break;
            case HeroButtonKind.AddToPlaylist:
                go(new AddToPlaylistPage(page.Id));
                break;
            case HeroButtonKind.More:
                Panel(ItemText.MorePanel(page, summaryCut), page.Item);
                break;
        }
    }

    /// <summary>
    /// A side panel over the page: Audio and subtitles, More, or a tile's actions. Choosing a version
    /// or a track keeps Audio and subtitles open, its check on the new choice (the Android app's
    /// TvPanels: focus stays in the panel until Back closes it); any other row closes its panel.
    /// </summary>
    void Panel(SidePanel panel, ItemSummary item)
    {
        SidePanelSheet? sheet = null;
        sheet = new SidePanelSheet(layers, panel, row =>
        {
            var track = false;
            switch (row.Kind)
            {
                case PanelRowKind.Version when row.Value is { } file:
                    model.ChooseVersion(file);
                    track = true;
                    break;
                case PanelRowKind.Audio:
                    model.ChooseAudio(row.Value is { } audio ? (int)audio : null);
                    track = true;
                    break;
                case PanelRowKind.Subtitle:
                    model.ChooseSubtitle(row.Value is { } subtitle ? (int)subtitle : null);
                    track = true;
                    break;
                case PanelRowKind.Rating:
                    model.Rate(row.Value is { } stars ? (int)stars : null);
                    break;
                case PanelRowKind.FullSummary when shown != null:
                    FullSummary(shown);
                    break;
                case PanelRowKind.Open when row.Key != null:
                    go(row.Key);
                    break;
                case PanelRowKind.Action when row.Action is { } action:
                    model.Perform(action, item);
                    break;
            }
            if (!track || shown == null || ItemText.TracksPanel(shown) is not { } again) return false;
            // Shown again over itself with the choice made, focus on the row chosen.
            var at = panel.Rows.ToList().IndexOf(row);
            sheet!.Close();
            Panel(again with { Focus = at >= 0 && at < again.Rows.Count ? at : again.Focus }, item);
            return true;
        });
        sheet.Show();
    }

    void FullSummary(ItemPage page)
    {
        var dialog = new Dialog(layers, page.Title, ItemText.FullSummaryText(page));
        dialog.Button(ItemText.Close, dialog.Close, initial: true);
        dialog.Show();
    }

    /// <summary>
    /// The sections under the hero: the seasons and episodes, then Extras, Cast &amp; Crew and More
    /// Like This. Made again only when what they show changed, so focus in them stays put.
    /// </summary>
    void ShowSections(ItemPage page, bool first)
    {
        var key = SectionsKey(page);
        if (key == shownSections)
        {
            if (page.Seasons != null)
            {
                seasonPosters?.Update(page.Seasons);
                seasons?.Update(page.Seasons, page.Origin);
            }
            return;
        }
        shownSections = key;
        var focusedName = Tree.Current?.Name;
        // Let go only once focus is on the new sections: the tile with focus hears it leave.
        var old = column;
        var focusWasIn = old != null && Tree.Current != null && Find(old.Group, Tree.Current.Name ?? "") == Tree.Current;
        if (old != null)
        {
            Group.Remove(old.Group);
            old.Unparent();
        }
        column = new RowColumnView(gate, top: RowsTop(), firstAtTop: false);
        seasons = null;
        seasonPosters = null;
        if (page.Seasons is { } s)
        {
            if (s.Posters)
            {
                seasonPosters = new SeasonPostersView(s, page.Origin, gate, images, Shell.FocusRail, model.SelectSeason, Tree.Focus);
                column.Add(seasonPosters, seasonPosters.Group, 218);
            }
            seasons = new SeasonsView(page, s, gate, images, Shell.FocusRail, OpenEpisode, FocusEpisode, model.SelectSeason, Tree.Focus);
            // The season tabs (or posters) at y = 218 and their episodes at 286 (a season's under its name at 272), under the info block and the focused episode's lines.
            column.Add(seasons, seasons.Group, ItemText.ShowsTabs(s) ? 218 : 272 - PleyTv.RowHeaderHeight - PleyTv.HeaderToTiles);
        }
        var origin = page.Origin;
        if (page.Extras.Count > 0)
        {
            var byId = page.Extras.ToDictionary(e => e.Id);
            column.Add(new RowView(ItemText.Extras, ItemText.ExtraTiles(page), TileKind.Landscape, gate, images,
                m => { if (byId.TryGetValue(m.Id, out var extra)) model.Play(extra, fromStart: true); }, null, Shell.FocusRail, focusItem: Tree.Focus));
        }
        if (ItemText.HasCastRow(page))
        {
            var people = ItemText.CastAndCrew(page).Select(p => new TileModel(p.Id, p.Name, p.Subtitle,
                p.Photo == null ? Array.Empty<string>() : new[] { Artwork.Absolute(p.Photo, origin) }, 0, false, 0, Array.Empty<string>(), p.Name)).ToList();
            column.Add(new RowView(ItemText.CastRowTitle(page), people, TileKind.Person, gate, images,
                m => go(new PageKey.Person(m.Id)), null, Shell.FocusRail, focusItem: Tree.Focus));
        }
        if (page.Related.Count > 0)
        {
            var byId = page.Related.ToDictionary(r => r.Id);
            column.Add(new RowView(ItemText.MoreLikeThis, page.Related.Select(r => TileModel.Of(r, TileKind.Poster, origin)).ToList(), TileKind.Poster, gate, images,
                m => go(new PageKey.Item(m.Id)),
                m => { if (byId.TryGetValue(m.Id, out var r)) Panel(ItemText.ActionsPanel(r), r); },
                Shell.FocusRail, focusItem: Tree.Focus));
        }
        if (ItemText.HasDetails(page))
        {
            var details = new ItemDetailsView(page, model.ChooseVersion);
            details.Group.Exit = d =>
            {
                if (d == Direction.Up) details.Rest();
                return d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d is Direction.Right or Direction.Down;
            };
            // Reading on through the details, the info block steps aside: the text has the screen.
            details.Reading += on => Fade(infoBlock, on ? 0f : 1f);
            column.Add(details, details.Group);
        }
        body.Add(column);
        if (Group.Children.Count == 0) Group.Add(heroButtons.Group);
        Group.Add(column.Group);
        if (!first && focusedName != null && Find(column.Group, focusedName) is { } again) Tree.Focus(again);
        else if (focusWasIn && !Tree.Enter(column.Group)) Tree.Enter(heroButtons.Group);
        old?.Dispose();
    }

    static string SectionsKey(ItemPage page) =>
        string.Join("|", new[]
        {
            page.Seasons == null ? "-" : string.Join(",", page.Seasons.Seasons.Select(t => t.Id)) + (page.Seasons.Posters ? ";posters" : ""),
            string.Join(",", page.Extras.Select(e => e.Id)),
            string.Join(",", page.Cast.Select(c => c.Id)) + ";" + string.Join(",", page.Details.Crew.SelectMany(c => c.People).Select(p => p.Id)),
            string.Join(",", page.Related.Select(r => r.Id)),
            // The details: their facts, the version that plays, the files.
            string.Join(";", ItemText.Facts(page.Details).Select(f => f.Label + "=" + f.Value)),
            string.Join(",", page.Details.Versions.Select(v => v.Id + (v.Selected ? "*" : ""))),
            string.Join(",", page.MediaInfo.Select(m => m.Id)),
        });

    static FocusItem? Find(FocusGroup group, string name)
    {
        foreach (var child in group.Children)
        {
            if (child is FocusItem item && item.Name == name) return item;
            if (child is FocusGroup g && Find(g, name) is { } found) return found;
        }
        return null;
    }

    void OpenEpisode(EpisodeLine episode) => go(new PageKey.Item(episode.Item.Id));

    /// <summary>The info block follows the focused episode once focus has rested on it.</summary>
    void FocusEpisode(EpisodeLine? episode)
    {
        focusedEpisode = episode;
        episodeRest.Stop();
        if (episode == null)
        {
            restedEpisode = null;
            ShowInfoBlock();
        }
        else
        {
            episodeRest.Start();
        }
    }

    /// <summary>The info block steps aside while reading on through the details, and back after.</summary>
    void Fade(View view, float to)
    {
        reading?.Stop();
        reading = new Animation(PleyMotion.RevealMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        reading.AnimateTo(view, "Opacity", to);
        reading.Play();
    }

    void SetInSections(bool on)
    {
        if (on == inSections) return;
        inSections = on;
        reveal?.Stop();
        reveal = new Animation(PleyMotion.RevealMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        reveal.AnimateTo(hero, "Opacity", on ? 0f : 1f);
        reveal.AnimateTo(infoBlock, "Opacity", on ? 1f : 0f);
        reveal.Play();
        if (on) ShowInfoBlock();
        else
        {
            column?.ScrollToTop();
            FocusEpisode(null);
        }
    }

    /// <summary>
    /// The info block once focus has left the hero, from y = 28: the title and metadata line, then
    /// the focused episode's title, line and two lines of summary in the episode row, or the item's
    /// own summary by the other rows.
    /// </summary>
    void ShowInfoBlock()
    {
        var page = shown;
        if (page == null) return;
        foreach (var child in infoBlock.Children.ToList())
        {
            infoBlock.Remove(child);
            child.Dispose();
        }
        infoBlock.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(28));
        float y = 0;
        var title = Text.Label(ItemText.HeroTitle(page), PleyType.HeroTitle, PleyColors.Fg, width: TextColumn);
        infoBlock.Add(title);
        y += PleyType.HeroTitle.Line + 10;
        var meta = MetaLine.Of(page.Meta);
        meta.PositionY = Ui.Dp(y);
        infoBlock.Add(meta);
        y += 18;
        var episode = (seasons?.FocusInside ?? false) || (seasonPosters?.FocusInside ?? false) ? restedEpisode : null;
        if (episode != null)
        {
            y += 14;
            var name = Text.Label(episode.Item.Title, PleyType.Title, PleyColors.Fg, width: TextColumn);
            name.PositionY = Ui.Dp(y);
            infoBlock.Add(name);
            y += PleyType.Title.Line + 4;
            var line = Text.Label(ItemText.EpisodeInfo(episode), PleyType.Caption, PleyColors.Muted, width: TextColumn);
            line.PositionY = Ui.Dp(y);
            infoBlock.Add(line);
            y += PleyType.Caption.Line;
            if (episode.Summary is { } summary)
            {
                var text = Text.Label(summary, PleyType.Body, Faded, maxLines: 2, width: TextColumn);
                text.VerticalAlignment = VerticalAlignment.Top;
                text.PositionY = Ui.Dp(y + 6);
                infoBlock.Add(text);
            }
        }
        else if (page.Summary is { } summary)
        {
            var text = Text.Label(summary, PleyType.Body, Faded, maxLines: 2, width: TextColumn);
            text.VerticalAlignment = VerticalAlignment.Top;
            text.PositionY = Ui.Dp(y + 8);
            infoBlock.Add(text);
        }
    }

    static readonly Color Faded = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.85f);
}

/// <summary>
/// The seasons and episodes (design spec §2.4, §4.4; the Android app's TvEpisodesSection): the
/// season tabs when the show has more than one season, else the season's name, then its episodes as
/// tiles captioned "5. Glasshouse" and "26 min left". Focus comes into the row on the episode to
/// watch. A tab focus rests on shows its season. Under season posters (<see cref="SeasonPostersView"/>)
/// it is the season's name and its episodes.
/// </summary>
sealed class SeasonsView : View
{
    const float TabHeight = 40;

    readonly ItemPage page;
    readonly KeyRepeatGate gate;
    readonly Images images;
    readonly Func<bool> toRail;
    readonly Action<EpisodeLine> open;
    readonly Action<EpisodeLine?> focus;
    readonly Action<long> select;
    readonly Func<FocusItem, bool> focusItem;
    readonly View tabs = new View();
    readonly FocusGroup tabGroup = new FocusGroup(Axis.Row, "seasons");
    readonly Timer tabRest = new Timer((uint)KeyRepeatGate.RestMs);
    readonly List<(SeasonTab Tab, ButtonView Button)> tabButtons = new List<(SeasonTab, ButtonView)>();
    TextLabel? heading;
    RowView? row;
    TextLabel? empty;
    SeasonsSection? section;
    string shownEpisodes = "";
    long? restingTab;
    float tabScroll;
    Animation? tabAnimation;
    readonly float rowTop;

    public SeasonsView(ItemPage page, SeasonsSection seasons, KeyRepeatGate gate, Images images, Func<bool> toRail,
        Action<EpisodeLine> open, Action<EpisodeLine?> focus, Action<long> select, Func<FocusItem, bool> focusItem)
    {
        this.page = page;
        this.gate = gate;
        this.images = images;
        this.toRail = toRail;
        this.open = open;
        this.focus = focus;
        this.select = select;
        this.focusItem = focusItem;
        Group = new FocusGroup(Axis.Column, "episodes");
        if (ItemText.ShowsTabs(seasons))
        {
            tabs.Position = new Position(Ui.Dp(PleyTv.Gutter - 20), 0);
            Add(tabs);
            Group.Add(tabGroup);
            tabGroup.Exit = d => d == Direction.Right || (d == Direction.Left && Scrolling.Left(gate, toRail));
            // 28 to the episodes: room for the focused tile's growth and ring, and a gap past them.
            rowTop = TabHeight + 28;
        }
        else
        {
            rowTop = PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles;
        }
        tabRest.Tick += (s, e) =>
        {
            if (restingTab is { } id && id != section?.Selected) select(id);
            return false;
        };
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(rowTop + TileKind.Episode.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight));
        Update(seasons, page.Origin);
    }

    public FocusGroup Group { get; }

    /// <summary>Whether focus is on a tab or an episode.</summary>
    public bool FocusInside => tabButtons.Any(t => t.Button.Item.Focused) || (row?.Group.Children.OfType<FocusItem>().Any(i => i.Focused) ?? false);

    public void Update(SeasonsSection seasons, string origin)
    {
        section = seasons;
        ShowTabs(seasons);
        var key = seasons.Episodes == null ? "loading" : string.Join(",", seasons.Episodes.Select(e => $"{e.Item.Id}:{e.Length}:{e.Watched}"));
        if (key == shownEpisodes) return;
        shownEpisodes = key;
        var hadFocus = row?.Group.Children.OfType<FocusItem>().FirstOrDefault(i => i.Focused)?.Name;
        if (row != null)
        {
            Group.Remove(row.Group);
            row.Unparent();
            row.Dispose();
            row = null;
        }
        empty?.Unparent();
        empty?.Dispose();
        empty = null;
        var episodes = seasons.Episodes;
        if (episodes == null) return;
        if (episodes.Count == 0)
        {
            empty = Text.Label(ItemText.NoEpisodes, PleyType.Body, PleyColors.Muted, width: 500);
            empty.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(rowTop));
            Add(empty);
            return;
        }
        var byId = episodes.ToDictionary(e => e.Item.Id);
        var tiles = episodes.Select(e => TileModel.Of(e.Item, TileKind.Episode, origin) with { Title = e.Title, Subtitle = e.Length }).ToList();
        row = new RowView(null, tiles, TileKind.Episode, gate, images,
            m => open(byId[m.Id]), null, toRail,
            m => focus(byId.TryGetValue(m.Id, out var line) ? line : null), focusItem);
        row.PositionY = Ui.Dp(rowTop);
        Add(row);
        Group.Add(row.Group);
        // The section is the first under the hero: its episodes are near from the start.
        row.ShowPictures();
        // Focus comes into the row on the episode to watch.
        var at = seasons.Focus is { } id ? episodes.ToList().FindIndex(e => e.Item.Id == id) : -1;
        if (at >= 0) row.Group.RememberChild(at);
        if (hadFocus != null && row.Group.Children.OfType<FocusItem>().FirstOrDefault(i => i.Name == hadFocus) is { } again) focusItem(again);
    }

    /// <summary>The season tabs (design spec §2.6): 40 dp pills in Muted; the shown season on Surface3 in Fg; focused, Accent.</summary>
    void ShowTabs(SeasonsSection seasons)
    {
        if (!ItemText.ShowsTabs(seasons))
        {
            if (heading == null)
            {
                heading = Text.Label(ItemText.EpisodesHeading(page, seasons), PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
                heading.Position = new Position(Ui.Dp(PleyTv.Gutter), 0);
                Add(heading);
            }
            // Under season posters, the season shown changes.
            else heading.Text = ItemText.EpisodesHeading(page, seasons);
            return;
        }
        if (tabButtons.Count == 0)
        {
            float x = 0;
            foreach (var tab in seasons.Seasons)
            {
                var button = new ButtonView(tab.Title, ButtonKind.Text, () => select(tab.Id)) { PositionX = Ui.Dp(x) };
                var left = x;
                button.Item.FocusChanged += on =>
                {
                    tabRest.Stop();
                    if (!on) return;
                    ScrollTabs(left, button.WidthDp);
                    restingTab = tab.Id;
                    tabRest.Start();
                };
                tabs.Add(button);
                tabGroup.Add(button.Item);
                tabButtons.Add((tab, button));
                x += button.WidthDp + 4;
            }
        }
        for (var i = 0; i < tabButtons.Count; i++)
        {
            var (tab, button) = tabButtons[i];
            var current = tab.Id == seasons.Selected;
            button.Rest = current ? PleyColors.Surface3 : Color.Transparent;
            button.Ink = current ? PleyColors.Fg : PleyColors.Muted;
            if (current) tabGroup.RememberChild(i);
        }
    }

    /// <summary>
    /// More tabs than the screen holds slide, just far enough to show the focused one between its
    /// place at the start and the right safe edge (the Android app's TvScrolling.inView).
    /// </summary>
    void ScrollTabs(float left, float width)
    {
        var room = PleyTv.CanvasWidth - (PleyTv.Gutter - 20) - PleyTv.SafeHorizontal;
        var scroll = tabScroll;
        if (left < scroll) scroll = left;
        else if (left + width > scroll + room) scroll = left + width - room;
        if (scroll == tabScroll) return;
        tabScroll = scroll;
        tabAnimation?.Stop();
        tabAnimation = Scrolling.Glide(gate);
        tabAnimation.AnimateTo(tabs, "PositionX", Ui.Dp(PleyTv.Gutter - 20 - tabScroll));
        tabAnimation.Play();
    }

    protected override void Dispose(global::Tizen.NUI.DisposeTypes type)
    {
        if (Disposed) return;
        tabAnimation?.Stop();
        tabRest.Stop();
        tabRest.Dispose();
        base.Dispose(type);
    }
}

/// <summary>
/// The seasons as poster tiles (the Android app's SeasonPosters), as the web shows them: each
/// season's poster, else the show's, its name and episodes under it, in a row of the kit. As with
/// the tabs, a season focus rests on, or Select chooses, is shown under the row, and focus comes
/// back into the row on the season shown.
/// </summary>
sealed class SeasonPostersView : View
{
    readonly RowView row;
    readonly Action<long> select;
    readonly Timer rest = new Timer((uint)KeyRepeatGate.RestMs);
    SeasonsSection seasons;
    long? resting;

    public SeasonPostersView(SeasonsSection seasons, string origin, KeyRepeatGate gate, Images images, Func<bool> toRail,
        Action<long> select, Func<FocusItem, bool> focusItem)
    {
        this.seasons = seasons;
        this.select = select;
        var tiles = seasons.Seasons
            .Where(t => t.Item != null)
            .Select(t => TileModel.Of(t.Item! with { Title = t.Title }, TileKind.Poster, origin))
            .ToList();
        row = new RowView(null, tiles, TileKind.Poster, gate, images, m => Choose(m.Id), null, toRail, m =>
        {
            resting = m.Id;
            rest.Stop();
            rest.Start();
        }, focusItem);
        rest.Tick += (s, e) =>
        {
            // Only while focus is still on the season it rested on.
            if (resting is { } id && id != this.seasons.Selected && FocusInside) select(id);
            return false;
        };
        Size = row.Size;
        Add(row);
        // The first section under the hero: its pictures are near from the start.
        row.ShowPictures();
        Update(seasons);
    }

    public FocusGroup Group => row.Group;

    /// <summary>Whether focus is on a season: the info block keeps the episode, as over the tabs.</summary>
    public bool FocusInside => row.Group.Children.OfType<FocusItem>().Any(i => i.Focused);

    public void Update(SeasonsSection seasons)
    {
        this.seasons = seasons;
        var at = seasons.Seasons.ToList().FindIndex(t => t.Id == seasons.Selected);
        if (at >= 0) row.Group.RememberChild(at);
    }

    void Choose(long id)
    {
        if (id != seasons.Selected) select(id);
    }

    protected override void Dispose(global::Tizen.NUI.DisposeTypes type)
    {
        if (Disposed) return;
        rest.Stop();
        rest.Dispose();
        base.Dispose(type);
    }
}
