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
/// Your requests on TV, or for managers everyone's (Requests), as the Android app has them
/// (TvRequestsScreen): tabs with their numbers (Needs attention first for managers, who open on it,
/// then All, Pending, Approved, Available, Declined, Failed); for managers, movies or shows, a
/// person (Asked by) and a title's words; then a card per request with its poster, what was asked
/// for, who asked and when, its note and its stage. OK on a card opens what may be done with it.
/// The list reads on as focus comes near its end; news since the profile last looked keeps its
/// dot while the page shows.
/// </summary>
sealed class RequestsScreen : ShellPage
{
    /// <summary>The list's top, under the tabs; lower with the managers' filters, or the line about signing in again.</summary>
    const float ListTop = 128, ListTopLow = 178;
    const float CardHeight = 88, CardGap = 10, CardPitch = CardHeight + CardGap, PosterWidth = 48, PosterHeight = 72, StageWidth = 260;
    /// <summary>Cards from the end at which the next page is asked for.</summary>
    const int NearEnd = 5;

    readonly RequestsModel model;
    readonly Shell layers;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly Action<PageKey> restart;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View head = new View();
    readonly View list = new View();
    readonly View cards = new View();
    readonly FocusGroup tabs = new FocusGroup(Axis.Row, "tabs");
    readonly FocusGroup filters = new FocusGroup(Axis.Row, "filters");
    readonly FocusGroup listGroup = new FocusGroup(Axis.Column, "cards");
    /// <summary>News since the profile last looked keeps its dot while the page shows, though seeing it clears it on the server.</summary>
    readonly HashSet<long> news = new HashSet<long>();
    readonly List<long> cardIds = new List<long>();
    TextInput? titleField;
    string headShape = "";
    RequestListState? shownList;
    /// <summary>The last list that came, and its filter: its counts don't depend on the tab, so they stay on the tabs while another tab's list comes.</summary>
    RequestListState.Ready? counted;
    RequestFilter? countedFilter;
    Timer? skeletonTimer;
    object? shownNotice;
    PanelSheet? panel;
    long? panelFor;
    Animation? scroll;
    bool entered;

    public RequestsScreen(TvShell shell, Shell layers, RequestsModel model, Images images, KeyRepeatGate gate, Action<PageKey> go, Action<PageKey> restart)
        : base(shell, new PageKey.Requests(), "requests")
    {
        this.model = model;
        this.layers = layers;
        this.images = images;
        this.gate = gate;
        this.go = go;
        this.restart = restart;
        View.BackgroundColor = PleyColors.Bg;
        list.ClippingMode = ClippingModeType.ClipChildren;
        list.Add(cards);
        View.Add(list);
        View.Add(head);
        Group.Add(tabs);
        Group.Add(filters);
        Group.Add(listGroup);
        tabs.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
        filters.Exit = tabs.Exit;
        listGroup.Exit = tabs.Exit;
        listGroup.ChildFocused += Scroll;
        model.Told += Told;
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        if (entered && model.State.Value.List is RequestListState.Failed) model.Refresh();
        var first = !entered;
        entered = true;
        // At first, the first card; coming back, where focus was.
        if (first && Tree.Enter(listGroup)) return;
        if (!Tree.Enter(Group) && !Wait()) Shell.FocusRail();
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Told -= Told;
        skeletonTimer?.Stop();
        panel?.Close();
        model.Dispose();
    }

    void Told(RequestEvent e) => RequestKit.Tell(Shell, e, model.State.Value.Gate.Summary, go);

    static void Clear(View view)
    {
        foreach (var child in view.Children.ToList())
        {
            view.Remove(child);
            child.Dispose();
        }
    }

    void Show(RequestsListState s)
    {
        if (s.Problem is { } problem)
        {
            if (Equals(problem, shownNotice)) return;
            shownNotice = problem;
            headShape = "";
            shownList = null;
            Clear(head);
            // The title field went with the head: the next is made afresh.
            titleField = null;
            Clear(cards);
            tabs.Clear();
            filters.Clear();
            listGroup.Clear();
            var summary = s.Gate.Summary;
            var action = RequestKit.ProblemAction(problem, summary, model.Refresh, () => RequestKit.SignInAgain(layers, s.Origin, restart), () => { });
            head.Add(RequestKit.Notice(RequestKit.ProblemTitle(problem, summary, RequestText.ListFailed), RequestKit.ProblemBody(problem, summary),
                RequestKit.ProblemIcon(problem), action, filters));
            if (entered && !Tree.Enter(Group)) Shell.FocusRail();
            Waited();
            return;
        }
        if (shownNotice != null)
        {
            shownNotice = null;
            Clear(head);
            // The title field went with the head: the next is made afresh.
            titleField = null;
            filters.Clear();
        }
        if (s.List is RequestListState.Ready came)
        {
            counted = came;
            countedFilter = s.Filter;
        }
        Head(s);
        List(s);
        // The panel's request went (withdrawn, approved off Needs attention): the panel goes too.
        if (panel != null && panelFor is { } id && s.List is RequestListState.Ready ready && ready.Requests.All(r => r.Id != id)) panel.Close();
    }

    // ---- the head: title, tabs and filters

    void Head(RequestsListState s)
    {
        var ready = s.List as RequestListState.Ready;
        // Another tab's list on its way: the last list's counts, when only the tab changed since.
        var known = ready ?? (countedFilter != null && countedFilter with { View = s.Filter.View } == s.Filter ? counted : null);
        var views = RequestViews.All.Where(v => v != RequestView.Attention || s.Manage).ToList();
        var labels = views.Select(v => Count(v, s, ready, known) is > 0 and var n ? RequestText.Counted(RequestText.View(v), n) : RequestText.View(v)).ToList();
        var people = known?.People.FirstOrDefault(p => p.Id == s.Filter.UserId)?.Name;
        var shape = string.Join("|", labels) + $"#{s.Filter.View}#{s.Manage}#{s.SignInForApprovals}#{s.Filter.Kind}#{people ?? s.Filter.UserId?.ToString(CultureInfo.InvariantCulture)}";
        if (shape == headShape) return;
        var hadFocus = Tree.Current?.Name;
        var inTabs = Tree.Current?.Parent == tabs;
        // Tabs are told apart by their place: their labels carry numbers that change.
        var tabAt = inTabs ? tabs.Children.ToList().IndexOf(Tree.Current!) : -1;
        var inFilters = Tree.Current?.Parent == filters;
        headShape = shape;
        foreach (var child in head.Children.ToList())
        {
            if (child == titleField) continue;
            head.Remove(child);
            child.Dispose();
        }
        tabs.Clear();
        filters.Clear();
        var heading = Text.Label(s.Manage ? RequestText.Manage : RequestText.Yours, PleyType.PageTitle, PleyColors.Fg, width: 600);
        heading.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        head.Add(heading);
        var y = PleyTv.SafeVertical + PleyType.PageTitle.Line + 10;
        var x = PleyTv.Gutter;
        for (var i = 0; i < views.Count; i++)
        {
            var view = views[i];
            var on = view == s.Filter.View;
            var tab = new ButtonView(labels[i], ButtonKind.Text, () => model.View(view)) { Rest = on ? PleyColors.Surface3 : Color.Transparent, Ink = on ? PleyColors.Fg : PleyColors.Muted };
            tab.Position = new Position(Ui.Dp(x), Ui.Dp(y));
            head.Add(tab);
            tabs.Add(tab.Item);
            if (on) tabs.RememberChild(i);
            x += tab.WidthDp + 8;
        }
        y += PleyTv.ButtonHeight + 10;
        if (s.Manage)
        {
            x = PleyTv.Gutter;
            var kind = new ButtonView(s.Filter.Kind switch { TitleKind.Movie => RequestText.FilterMovies, TitleKind.Show => RequestText.FilterShows, _ => RequestText.KindAll },
                ButtonKind.Text, () => model.Kind(s.Filter.Kind switch { null => TitleKind.Movie, TitleKind.Movie => TitleKind.Show, _ => null }), iconName: "film")
            { Rest = s.Filter.Kind != null ? PleyColors.Surface3 : Color.Transparent };
            kind.Position = new Position(Ui.Dp(x), Ui.Dp(y));
            head.Add(kind);
            filters.Add(kind.Item);
            x += kind.WidthDp + 10;
            var person = new ButtonView(people ?? RequestText.FilterEveryone, ButtonKind.Text, () => People(s), iconName: "users", maxWidth: 180)
            { Rest = s.Filter.UserId != null ? PleyColors.Surface3 : Color.Transparent };
            person.Position = new Position(Ui.Dp(x), Ui.Dp(y));
            head.Add(person);
            filters.Add(person.Item);
            x += person.WidthDp + 10;
            if (titleField == null)
            {
                titleField = new TextInput(layers.Input, 240, RequestText.FilterTitle);
                titleField.Value = model.TitleQuery;
                titleField.Changed += model.TypeTitle;
                head.Add(titleField);
            }
            titleField.Position = new Position(Ui.Dp(x), Ui.Dp(y - (TextInput.Height - PleyTv.ButtonHeight) / 2));
            filters.Add(titleField.Item);
        }
        else if (s.SignInForApprovals)
        {
            var line = Text.Label("Sign in again to approve and decline here.", PleyType.Body, PleyColors.Muted);
            line.Size = new Size(line.NaturalSize2D.Width + Ui.Dp(2), Ui.Dp(PleyTv.ButtonHeight));
            line.VerticalAlignment = VerticalAlignment.Center;
            line.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
            head.Add(line);
            var sign = new ButtonView(RequestText.SignIn, ButtonKind.Pill, () => RequestKit.SignInAgain(layers, s.Origin, restart), iconName: "log-out");
            sign.Position = new Position(line.PositionX + line.SizeWidth + Ui.Dp(12), Ui.Dp(y));
            head.Add(sign);
            filters.Add(sign.Item);
        }
        var again = inFilters ? filters.Children.OfType<FocusItem>().FirstOrDefault(i => i.Name == hadFocus) : null;
        if (inTabs) Tree.Focus((FocusItem)tabs.Children[Math.Max(0, Math.Min(tabAt, tabs.Children.Count - 1))]);
        else if (inFilters && again != null) Tree.Focus(again);
        else if (inFilters) Tree.Enter(filters);
    }

    /// <summary>
    /// A tab's number: Needs attention, what waits for managers; All, every status's; a status, its own, from
    /// <paramref name="known"/>'s counts (the list showing, or the last one with the same filters). Null while not known.
    /// </summary>
    static int? Count(RequestView view, RequestsListState s, RequestListState.Ready? ready, RequestListState.Ready? known)
    {
        if (view == RequestView.Attention)
            return s.Filter.Narrowed ? s.Filter.View == RequestView.Attention ? ready?.Total : null : s.Attention;
        if (known == null) return null;
        if (view == RequestView.All) return known.Counts.Values.Sum();
        return view.Status() is { } status && known.Counts.TryGetValue(status, out var n) ? n : 0;
    }

    /// <summary>Asked by: everyone, or one person with requests, in a panel beside the page.</summary>
    void People(RequestsListState s)
    {
        var people = (s.List as RequestListState.Ready)?.People ?? Array.Empty<RequestUser>();
        var lines = new[] { new SidePanelSheet.Line(RequestText.FilterEveryone, Checked: s.Filter.UserId == null) }
            .Concat(people.Select(p => new SidePanelSheet.Line(p.Name, Checked: p.Id == s.Filter.UserId))).ToList();
        var focus = s.Filter.UserId is { } id ? Math.Max(0, people.ToList().FindIndex(p => p.Id == id) + 1) : 0;
        SidePanelSheet? sheet = null;
        sheet = new SidePanelSheet(layers, RequestText.AskedByLabel, lines, focus, i =>
        {
            model.Person(i == 0 ? null : people[i - 1].Id);
            return true;
        });
        sheet.Show();
    }

    // ---- the list

    float Top(RequestsListState s) => s.Manage || s.SignInForApprovals ? ListTopLow : ListTop;

    void List(RequestsListState s)
    {
        var top = Top(s);
        list.Position = new Position(0, Ui.Dp(top));
        list.Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight - top));
        if (ReferenceEquals(s.List, shownList) || Equals(s.List, shownList)) return;
        shownList = s.List;
        var focusedId = Tree.Current?.Parent == listGroup && Tree.Current.Name is { } n && long.TryParse(n, NumberStyles.Integer, CultureInfo.InvariantCulture, out var id) ? id : (long?)null;
        var hadListFocus = Tree.Current?.Parent == listGroup;
        var oldIds = cardIds.ToList();
        Clear(cards);
        listGroup.Clear();
        cardIds.Clear();
        scroll?.Stop();
        skeletonTimer?.Stop();
        cards.PositionY = 0;
        var pageHasFocus = Tree.Current != null && Tree.Current != Waiting && InPage(Tree.Current);
        switch (s.List)
        {
            case RequestListState.Loading loading:
                // Still cards only after a moment: a list that comes quickly (another tab's) doesn't flash them first.
                skeletonTimer = new Timer(300);
                skeletonTimer.Tick += (_, _) =>
                {
                    if (!ReferenceEquals(shownList, loading)) return false;
                    for (var i = 0; i < 4; i++)
                        cards.Add(new View
                        {
                            Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(8 + i * CardPitch)),
                            Size = new Size(Ui.Dp(PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal), Ui.Dp(CardHeight)),
                            CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                            BackgroundColor = PleyColors.Surface2,
                        });
                    return false;
                };
                skeletonTimer.Start();
                if (entered && !pageHasFocus && !Tree.Enter(tabs)) Wait();
                return;
            case RequestListState.Failed failed:
                Note(RequestKit.ProblemTitle(failed.Problem, s.Gate.Summary, RequestText.ListFailed), RequestKit.ProblemBody(failed.Problem, s.Gate.Summary),
                    new RequestKit.NoticeAction(RequestText.Retry, "refresh-cw", model.Refresh), pageHasFocus, hadListFocus);
                return;
            case RequestListState.Empty empty:
            {
                var find = s.MayAsk && !s.Manage && s.Filter.View == RequestView.All && !empty.Filtered;
                var title = empty.Filtered ? RequestText.FilteredNone : s.Filter.View == RequestView.Attention ? RequestText.AttentionNone : RequestText.NoneTitle;
                var body = empty.Filtered ? RequestText.FilteredNoneBody : s.Filter.View == RequestView.Attention ? RequestText.AttentionNoneBody
                    : s.MayAsk ? RequestText.NoneBody : RequestText.NoneBodyOff;
                Note(title, body, find ? new RequestKit.NoticeAction(RequestText.Find, "compass", () => go(new PageKey.Discover())) : null, pageHasFocus, hadListFocus);
                return;
            }
            case RequestListState.Ready ready:
                foreach (var r in ready.Requests.Where(r => r.Unseen)) news.Add(r.Id);
                for (var i = 0; i < ready.Requests.Count; i++) Card(ready.Requests[i], i, s);
                // A card that went with focus hands it to the one now in its place, else the one before, else the tab.
                var target = focusedId ?? (panel == null ? null : panelFor);
                if (target is { } t && cardIds.Contains(t))
                {
                    if (hadListFocus) Tree.Focus((FocusItem)listGroup.Children[cardIds.IndexOf(t)]);
                }
                else if (target is { } gone && hadListFocus)
                {
                    var at = oldIds.IndexOf(gone);
                    var heir = at >= 0 && at < cardIds.Count ? at : at - 1;
                    if (heir >= 0 && heir < cardIds.Count) Tree.Focus((FocusItem)listGroup.Children[heir]);
                    else Tree.Enter(tabs);
                }
                else if (entered && !pageHasFocus && listGroup.Children.Count > 0) Tree.Focus((FocusItem)listGroup.Children[0]);
                Waited();
                return;
        }
    }

    bool InPage(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == Group) return true;
        return false;
    }

    /// <summary>What the list says instead of cards: none, or why none came, with a button where one helps.</summary>
    void Note(string title, string? body, RequestKit.NoticeAction? action, bool pageHasFocus, bool hadListFocus)
    {
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(24)) };
        var heading = Text.Label(title, PleyType.Section, PleyColors.Fg, width: 600);
        view.Add(heading);
        var y = PleyType.Section.Line;
        if (body != null)
        {
            var words = PanelColumn.Lines(body, PleyType.Body, PleyColors.Muted, 520, 3);
            words.PositionY = Ui.Dp(y + 8);
            view.Add(words);
            y += 8 + words.SizeHeight / Ui.Scale;
        }
        if (action != null)
        {
            var row = new ButtonRow();
            row.Add(new ButtonView(action.Label, ButtonKind.Pill, action.Press, iconName: action.Icon));
            row.PositionY = Ui.Dp(y + 20);
            view.Add(row);
            foreach (var item in row.Group.Children.ToList()) listGroup.Add(item);
        }
        cards.Add(view);
        if (entered && (!pageHasFocus || hadListFocus))
        {
            if (listGroup.Children.Count > 0) Tree.Focus((FocusItem)listGroup.Children[0]);
            else Tree.Enter(tabs);
        }
        Waited();
    }

    /// <summary>
    /// A request as a card: its 48 dp poster with a dot for news, "Title · year", what was asked for
    /// and who asked when, the stage's line and the managers' note, and the stage at the end. Focused,
    /// it lifts to Surface3 with the ring.
    /// </summary>
    void Card(MediaRequest request, int index, RequestsListState s)
    {
        var width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal - 16;
        var item = new FocusItem(request.Id.ToString(CultureInfo.InvariantCulture)) { Selected = () => Open(request.Id) };
        var card = new View
        {
            Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(8 + index * CardPitch)),
            Size = new Size(Ui.Dp(width), Ui.Dp(CardHeight)),
            CornerRadius = Ui.Dp(PleyShape.ControlRadius),
            BackgroundColor = PleyColors.Surface,
            PivotPoint = global::Tizen.NUI.PivotPoint.Center,
        };
        var art = new ImageView
        {
            Position = new Position(Ui.Dp(12), Ui.Dp((CardHeight - PosterHeight) / 2)),
            Size = new Size(Ui.Dp(PosterWidth), Ui.Dp(PosterHeight)),
            CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
            BackgroundColor = PleyColors.Surface3,
        };
        card.Add(art);
        if (!string.IsNullOrEmpty(request.PosterUrl))
        {
            var shown = shownList;
            images.Load(new[] { Artwork.Absolute(request.PosterUrl!, s.Origin) }, PosterWidth, file =>
            {
                // The list was made again meanwhile: this card is gone.
                if (file != null && ReferenceEquals(shown, shownList)) art.ResourceUrl = file;
            });
        }
        View? dot = null;
        if (news.Contains(request.Id))
        {
            // On the poster's top right corner, ringed in the card's colour.
            dot = new View
            {
                Position = new Position(Ui.Dp(12 + PosterWidth - 8), Ui.Dp((CardHeight - PosterHeight) / 2 - 4)),
                Size = new Size(Ui.Dp(14), Ui.Dp(14)),
                CornerRadius = Ui.Dp(7),
                BackgroundColor = PleyColors.Surface,
            };
            dot.Add(new View { Position = new Position(Ui.Dp(2), Ui.Dp(2)), Size = new Size(Ui.Dp(10), Ui.Dp(10)), CornerRadius = Ui.Dp(5), BackgroundColor = PleyColors.Accent });
            card.Add(dot);
        }
        var pill = RequestRules.Pill(request, s.Manage);
        var what = request.Kind == TitleKind.Show && request.Seasons is { Count: > 0 } seasons ? RequestText.Seasons(seasons) : RequestText.Kind(request.Kind);
        var note = string.Join(" ", new[] { RequestKit.Text.PillLine(pill), s.Manage && !string.IsNullOrWhiteSpace(request.Note) ? request.Note : null }.Where(t => t != null));
        var textX = 12 + PosterWidth + 16;
        var textWidth = width - textX - StageWidth - 20 - 16;
        var title = Text.Label(request.Year is { } year ? $"{request.Title} · {year}" : request.Title, PleyType.Title, PleyColors.Fg, width: textWidth);
        var lines = PleyType.Title.Line + 3 + PleyType.Caption.Line + (note.Length > 0 ? 2 + PleyType.Caption.Line : 0);
        var textTop = (CardHeight - lines) / 2;
        title.Position = new Position(Ui.Dp(textX), Ui.Dp(textTop));
        card.Add(title);
        var asked = Text.Label($"{what} · {RequestKit.Text.AskedBy(request, s.Manage)}", PleyType.Caption, PleyColors.Muted, width: textWidth);
        asked.Position = new Position(Ui.Dp(textX), Ui.Dp(textTop + PleyType.Title.Line + 3));
        card.Add(asked);
        if (note.Length > 0)
        {
            var under = Text.Label(note, PleyType.Caption, PleyColors.Subtle, width: textWidth);
            under.Position = new Position(Ui.Dp(textX), Ui.Dp(textTop + PleyType.Title.Line + 3 + PleyType.Caption.Line + 2));
            card.Add(under);
        }
        var stage = RequestKit.StageBlock(request, s.Manage, StageWidth);
        stage.Position = new Position(Ui.Dp(width - 20) - stage.SizeWidth, (Ui.Dp(CardHeight) - stage.SizeHeight) / 2);
        card.Add(stage);
        var ring = new View
        {
            Position = new Position(-Ui.Dp(PleyShape.RingOffset), -Ui.Dp(PleyShape.RingOffset)),
            Size = new Size(Ui.Dp(width + 2 * PleyShape.RingOffset), Ui.Dp(CardHeight + 2 * PleyShape.RingOffset)),
            CornerRadius = Ui.Dp(PleyShape.ControlRadius + PleyShape.RingOffset),
            BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1,
        };
        ring.Hide();
        card.Add(ring);
        item.FocusChanged += on =>
        {
            if (!ReferenceEquals(listGroup, item.Parent)) return;
            card.BackgroundColor = on ? PleyColors.Surface3 : PleyColors.Surface;
            if (dot != null) dot.BackgroundColor = on ? PleyColors.Surface3 : PleyColors.Surface;
            if (on) ring.Show();
            else ring.Hide();
            var size = on ? 1.02f : 1f;
            card.Scale = new Vector3(size, size, 1);
        };
        cards.Add(card);
        listGroup.Add(item);
        cardIds.Add(request.Id);
    }

    /// <summary>The card focus rests on scrolls to the second line; near the end, the next page.</summary>
    void Scroll(int index)
    {
        if (index >= cardIds.Count) return;
        var target = -Ui.Dp(Math.Max(0, index - 1) * CardPitch);
        scroll?.Stop();
        scroll = Scrolling.Glide(gate);
        scroll.AnimateTo(cards, "PositionY", target);
        scroll.Play();
        if (index >= cardIds.Count - NearEnd) model.More();
    }

    // ---- what may be done with a request

    enum Step
    {
        Actions,
        Confirm,
        Reason,
    }

    /// <summary>
    /// What may be done with a request, in a side panel opened with OK on its card: at the top its
    /// title, stage, download, who asked and when, and its notes; then Open where a library has it
    /// for this profile, the server's actions in its order (Decline comes with "Decline with a
    /// reason…", Approve is plain: the settings' choices apply), and Details, its title's page. What
    /// can't be undone asks first, Cancel first; a reason is typed on the TV's keyboard, which opens at
    /// once and whose Done goes on to Decline. Back from a question goes back to the actions.
    /// </summary>
    void Open(long id)
    {
        if (model.State.Value.List is not RequestListState.Ready ready || ready.Requests.FirstOrDefault(r => r.Id == id) is not { } request) return;
        var manage = model.State.Value.Manage;
        var sheet = new PanelSheet(layers);
        panel = sheet;
        panelFor = id;
        var step = Step.Actions;
        CardAction? asking = null;
        string? used = null;
        sheet.Gone += () =>
        {
            if (panel != sheet) return;
            panel = null;
            // Focus goes back to the card, else the one in its place.
            var at = cardIds.IndexOf(id);
            if (at >= 0) Tree.Focus((FocusItem)listGroup.Children[at]);
        };
        void Act(CardAction action, string? reason)
        {
            if (model.State.Value.Busy.Contains(id)) return;
            sheet.Close();
            model.Act(request, action, reason);
        }
        void Draw()
        {
            var column = sheet.Column;
            column.Clear();
            FocusItem? first = null;
            switch (step)
            {
                case Step.Actions:
                {
                    Head(column, request, manage);
                    if (request.ItemId is { } item)
                    {
                        var open = column.Choice(new PanelChoice(RequestText.Open, () =>
                        {
                            sheet.Close();
                            go(new PageKey.Item(item));
                        }, "play"));
                        first = open.Item;
                    }
                    foreach (var action in RequestRules.CardActions(request, dialogs: false))
                    {
                        var a = action;
                        var choice = column.Choice(new PanelChoice(RequestText.Action(a), () =>
                        {
                            used = a.ToString();
                            if (a == CardAction.DeclineWithReason) step = Step.Reason;
                            else if (a.Confirms())
                            {
                                asking = a;
                                step = Step.Confirm;
                            }
                            else
                            {
                                Act(a, null);
                                return;
                            }
                            Draw();
                        }, IconOf(a), danger: a == CardAction.Delete));
                        if (used == a.ToString() || (used == null && first == null)) first = choice.Item;
                    }
                    var details = column.Choice(new PanelChoice("Details", () =>
                    {
                        sheet.Close();
                        go(new PageKey.DiscoverTitle(request.Kind, request.TmdbId));
                    }, "info"));
                    first ??= details.Item;
                    break;
                }
                case Step.Confirm:
                {
                    column.Title(RequestText.Confirm(asking!.Value, request.Title) ?? "");
                    // Cancel first, as the app's other questions: OK pressed twice undoes nothing.
                    var cancel = column.Choice(new PanelChoice(RequestText.Cancel, () =>
                    {
                        step = Step.Actions;
                        Draw();
                    }, "arrow-left"));
                    column.Choice(new PanelChoice(RequestText.Action(asking.Value), () => Act(asking.Value, null), IconOf(asking.Value), danger: true));
                    first = cancel.Item;
                    break;
                }
                case Step.Reason:
                {
                    column.Title($"Decline “{request.Title}”?");
                    if (!request.Mine) column.Sentence(RequestText.DeclineReasonHint, PleyType.Body, PleyColors.Muted);
                    var field = new TextInput(layers.Input, 288, RequestText.DeclineReason, action: InputMethod.ActionButtonTitleType.Done);
                    column.Place(field, TextInput.Height, 16, 12, field.Item);
                    PanelChoice? decline = null;
                    decline = column.Choice(new PanelChoice(RequestText.Action(CardAction.Decline), () => Act(CardAction.DeclineWithReason, field.Value), "ticket-x"), gap: 12);
                    column.Choice(new PanelChoice(RequestText.Cancel, () =>
                    {
                        step = Step.Actions;
                        Draw();
                    }, "arrow-left"));
                    // Once the words are in, Decline is next.
                    field.Submitted = () => sheet.Tree.Focus(decline.Item);
                    sheet.Tree.Focus(field.Item);
                    field.Start();
                    return;
                }
            }
            if (first != null) sheet.Tree.Focus(first);
        }
        sheet.Back = () =>
        {
            if (step == Step.Actions) return false;
            step = Step.Actions;
            Draw();
            return true;
        };
        sheet.Show();
        Draw();
    }

    /// <summary>The request at the top of its panel: title (year), stage, download, what was asked and by whom, its notes.</summary>
    static void Head(PanelColumn column, MediaRequest request, bool manage)
    {
        column.Title(request.Year is { } year ? $"{request.Title} ({year})" : request.Title);
        var pill = RequestRules.Pill(request, manage);
        var pillView = RequestKit.StagePill(pill, small: true, maxWidth: PanelColumn.Inner - 16);
        column.Place(pillView, pillView.SizeHeight / Ui.Scale, 16);
        if (request.Download is { } download)
            column.Place(RequestKit.DownloadBar(RequestRules.DownloadProgress(request) ?? 0f, download.State, PanelColumn.Inner - 32), 4, 16, 8);
        var what = request.Kind == TitleKind.Show && request.Seasons is { Count: > 0 } seasons ? RequestText.Seasons(seasons) : RequestText.Kind(request.Kind);
        column.Sentence($"{what} · {RequestKit.Text.AskedBy(request, manage)}", PleyType.Caption, PleyColors.Muted, gap: 6);
        if (RequestKit.Text.PillLine(pill) is { } line) column.Sentence(line, PleyType.Caption, PleyColors.Muted, gap: 2, lines: 3);
        if (manage && !string.IsNullOrWhiteSpace(request.Note)) column.Sentence(request.Note!, PleyType.Caption, PleyColors.Muted, gap: 2);
        if (manage && RequestText.SentTo(request) is { } sent) column.Sentence(sent, PleyType.Caption, PleyColors.Muted, gap: 2, lines: 1);
        column.Space(12);
    }

    static string IconOf(CardAction action) => action switch
    {
        CardAction.Withdraw or CardAction.Remove => "x",
        CardAction.StopFollowing => "bookmark-minus",
        CardAction.Approve or CardAction.ApproveWith => "check",
        CardAction.TryAgain => "rotate-ccw",
        CardAction.Decline or CardAction.DeclineWithReason => "ticket-x",
        CardAction.Handled => "circle-check",
        CardAction.Delete => "delete",
        _ => "plus",
    };
}
