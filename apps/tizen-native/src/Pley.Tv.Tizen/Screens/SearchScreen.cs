using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Search on TV (design spec §4.5; the Android app's TvSearchScreen): the field at the top, 520
/// wide, with initial focus; OK opens the TV's keyboard, and Back or its Search key closes it.
/// Before typing, the recent searches as chips. While typing (250 ms after the last key), the
/// results as rows, grouped (Movies, Shows, Seasons, Episodes, Artists, Albums, Tracks, Live TV,
/// People), the first header at y = 96; a channel found plays at once. Down from the field goes
/// into the rows, the chips or Retry, Up from the first row back to it; Left goes to the rail.
/// </summary>
sealed class SearchScreen : ShellPage
{
    const float FieldWidth = 520, ResultsTop = 96, NoticeTop = 120, NoticeWidth = 560, ChipsGap = 12, ChipMaxWidth = 320;
    /// <summary>The chips' row sits 16 dp under its header, plus the room Android's row keeps above them for their growth.</summary>
    const float ChipsTop = ResultsTop + PleyTv.RowHeaderHeight + 16 + 6;
    const int SkeletonRows = 2, SkeletonTiles = 7;

    readonly SearchModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly Action<LiveWatch> watch;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly TextInput field;
    readonly TextLabel searching;
    /// <summary>What is under the field: the rows, the chips or Retry.</summary>
    readonly FocusGroup below = new FocusGroup(Axis.Column, "below");
    /// <summary>The results' rows, while shown: disposed as they go, so they stop making tiles and give up their channels' logos.</summary>
    readonly List<View> rows = new List<View>();

    /// <summary>What the body shows now: the groups, the recent searches, a message, or "skeleton".</summary>
    object? shown;
    View? chipStrip;
    float chipScroll;
    Animation? chipAnimation;
    /// <summary>The Live TV row, while the results have one.</summary>
    ChannelRowView? channelRow;
    /// <summary>TMDB's titles for the words, after the libraries' results, for a profile that may ask.</summary>
    readonly SearchDiscoverModel? discover;
    TmdbRowView? tmdbRow;

    /// <param name="layers">The app's sheets: the field's keyboard takes the remote's keys through their input.</param>
    /// <param name="go">Opens another page: an item, a person, an album.</param>
    /// <param name="watch">Plays a Live TV channel found.</param>
    public SearchScreen(TvShell shell, Shell layers, SearchModel model, Images images, KeyRepeatGate gate, Action<PageKey> go, Action<LiveWatch> watch,
        SearchDiscoverModel? discover = null)
        : base(shell, new PageKey.Search(model.State.Value.Query), "search")
    {
        this.discover = discover;
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        this.watch = watch;
        View.BackgroundColor = PleyColors.Bg;
        // The rows scroll up under the field, which is drawn over them.
        View.Add(body);
        field = new TextInput(layers.Input, FieldWidth, SearchText.Field, iconName: "search")
        {
            Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical)),
        };
        field.Value = model.State.Value.Query;
        field.Changed += model.Type;
        field.Submitted = model.Submit;
        View.Add(field);
        searching = Text.Label(SearchText.Searching, PleyType.Caption, PleyColors.Muted, width: 200);
        searching.Position = new Position(Ui.Dp(PleyTv.Gutter + FieldWidth + 20), Ui.Dp(PleyTv.SafeVertical + (TextInput.Height - PleyType.Caption.Line) / 2));
        searching.Hide();
        View.Add(searching);

        Group.Add(field.Item);
        Group.Add(below);
        // Left from the field, a chip's or Retry's start goes to the rail; Right stops at their end.
        Group.Exit = d => d switch
        {
            Direction.Left => Scrolling.Left(gate, Shell.FocusRail),
            Direction.Right => true,
            _ => false,
        };
        watches.Add(model.State.Watch(Show));
        watches.Add(model.Now.Watch(now => channelRow?.SetNow(now)));
        if (discover != null) watches.Add(discover.State.Watch(Tmdb));
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Shell.FocusRail();
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        field.Changed -= model.Type;
        discover?.Dispose();
        chipAnimation?.Stop();
        DropRows();
        model.Dispose();
    }

    /// <summary>The results' rows go, each disposed here: a view's Dispose leaves its children, and a row left would make tiles and load logos on for an answer gone.</summary>
    void DropRows()
    {
        foreach (var row in rows) row.Dispose();
        rows.Clear();
    }

    void Show(SearchState s)
    {
        if (s.Searching) searching.Show();
        else searching.Hide();
        discover?.Search(s.Results switch
        {
            SearchResultsState.Loading l => l.Query,
            SearchResultsState.Ready r => r.Query,
            SearchResultsState.Empty e => e.Query,
            SearchResultsState.Failed f => f.Query,
            _ => "",
        });
        // The libraries have nothing: the line saying so, and Discover's row under it, where it stays.
        if (s.Results is SearchResultsState.Empty empty && discover != null && discover.State.Value is not TmdbSection.Hidden)
        {
            if (!Equals(shown, ("tmdb", empty.Query))) OnlyTmdb(empty.Query);
            return;
        }

        if (SearchText.Message(s) is { } message)
        {
            if (!Equals(message, shown)) Notice(message);
            return;
        }
        switch (s.Results)
        {
            case SearchResultsState.Idle:
                if (!ReferenceEquals(s.Recent, shown)) Recent(s.Recent);
                break;
            case SearchResultsState.Loading:
                if (!Equals(shown, "skeleton")) Skeletons();
                break;
            case SearchResultsState.Ready ready:
                if (!ReferenceEquals(ready.Groups, shown)) Rows(ready);
                break;
        }
    }

    /// <summary>What was under the field goes; focus that was in it goes back to the field.</summary>
    void ClearBody(object? next)
    {
        var hadFocus = Tree.Current != null && Below(Tree.Current);
        chipAnimation?.Stop();
        chipAnimation = null;
        chipStrip = null;
        chipScroll = 0;
        channelRow = null;
        tmdbRow = null;
        DropRows();
        below.Clear();
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            child.Dispose();
        }
        shown = next;
        if (hadFocus) Tree.Focus(field.Item);
    }

    bool Below(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == below) return true;
        return false;
    }

    /// <summary>
    /// The results' rows: people as circles, Live TV's channels as its channel tiles, episodes as
    /// stills, music as squares, the rest posters.
    /// </summary>
    void Rows(SearchResultsState.Ready ready)
    {
        var groups = ready.Groups;
        ClearBody(groups);
        var column = new RowColumnView(gate, top: ResultsTop, headerY: ResultsTop);
        // Live TV's row comes after the titles and before People, as on the web.
        var channelsAt = ready.Channels.Count > 0 ? SearchRules.ChannelsAt(groups) : -1;
        for (var i = 0; i <= groups.Count; i++)
        {
            if (i == channelsAt)
            {
                channelRow = new ChannelRowView(ready.Channels, model.Origin, model.Now.Value, gate, images, Watch, Shell.FocusRail);
                column.Add(channelRow, channelRow.Group);
                rows.Add(channelRow);
            }
            if (i == groups.Count) break;
            var g = groups[i];
            var row = new RowView(g.Title, g.Tiles, g.TileKind, gate, images,
                tile =>
                {
                    if (model.Open(g.Kind, tile.Id) is { } key) go(key);
                },
                g.Kind == SearchGroupKind.People ? null : tile => Shell.Actions?.Invoke(tile.Id, false),
                Shell.FocusRail, focusItem: Tree.Focus);
            column.Add(row);
            rows.Add(row);
        }
        if (discover != null)
        {
            tmdbRow = TmdbRow();
            column.Add(tmdbRow, tmdbRow.Group);
            rows.Add(tmdbRow);
            tmdbRow.Show(discover.State.Value);
        }
        body.Add(column);
        below.Add(column.Group);
    }

    TmdbRowView TmdbRow() => new TmdbRowView(gate, images, model.Origin, go, () => discover?.Retry(), Shell.FocusRail, Tree.Focus, () => Tree.Focus(field.Item));

    /// <summary>The Discover row's state changed: the row shows it, or the page is drawn again when it comes or goes with no library results.</summary>
    void Tmdb(TmdbSection section)
    {
        if (tmdbRow != null) tmdbRow.Show(section);
        if (model.State.Value.Results is SearchResultsState.Empty && (section is TmdbSection.Hidden) == shown is ValueTuple<string, string>) Show(model.State.Value);
    }

    /// <summary>The libraries found nothing: "Nothing in your libraries matches", and Discover's row under it.</summary>
    void OnlyTmdb(string query)
    {
        ClearBody(("tmdb", query));
        var line = Text.Label("Nothing in your libraries matches", PleyType.Body, PleyColors.Muted, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        line.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ResultsTop));
        body.Add(line);
        var top = ResultsTop + 20 + PleyTv.RowGap;
        var column = new RowColumnView(gate, top: top, headerY: top);
        tmdbRow = TmdbRow();
        column.Add(tmdbRow, tmdbRow.Group);
        rows.Add(tmdbRow);
        tmdbRow.Show(discover!.State.Value);
        body.Add(column);
        below.Add(column.Group);
    }

    void Watch(LiveChannel channel)
    {
        if (model.Watch(channel.Id) is { } w) watch(w);
    }

    /// <summary>
    /// Before typing: "Recent searches" at y = 96 and a chip for each (a history icon, at most 320
    /// wide), 12 apart, then Clear. Choosing one searches it again; focus goes back to the field,
    /// since the chips go once the results come.
    /// </summary>
    void Recent(IReadOnlyList<string> recent)
    {
        ClearBody(recent);
        var header = Text.Label(SearchText.Recent, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        header.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ResultsTop));
        body.Add(header);
        var strip = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ChipsTop)) };
        var chips = new FocusGroup(Axis.Row, "recent");
        var lefts = new List<float>();
        var widths = new List<float>();
        float x = 0;
        void Place(ButtonView button)
        {
            button.PositionX = Ui.Dp(x);
            strip.Add(button);
            chips.Add(button.Item);
            lefts.Add(x);
            widths.Add(button.WidthDp);
            x += button.WidthDp + ChipsGap;
        }
        foreach (var query in recent)
        {
            var q = query;
            Place(new ButtonView(q, ButtonKind.Text, () => SearchAgain(q), iconName: "history", maxWidth: ChipMaxWidth) { Rest = PleyColors.Surface3 });
        }
        Place(new ButtonView(SearchText.RecentClear, ButtonKind.Text, () =>
        {
            Tree.Focus(field.Item);
            model.ClearRecent();
        }));
        chips.ChildFocused += i => ScrollChips(lefts[i], widths[i]);
        body.Add(strip);
        chipStrip = strip;
        below.Add(chips);
    }

    void SearchAgain(string query)
    {
        Tree.Focus(field.Item);
        field.Value = query;
        model.SearchAgain(query);
    }

    /// <summary>Keeps the focused chip between the gutter and the right safe edge.</summary>
    void ScrollChips(float left, float width)
    {
        if (chipStrip == null) return;
        var room = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
        if (left < chipScroll) chipScroll = left;
        else if (left + width > chipScroll + room) chipScroll = left + width - room;
        chipAnimation?.Stop();
        chipAnimation = Scrolling.Glide(gate);
        chipAnimation.AnimateTo(chipStrip, "PositionX", Ui.Dp(PleyTv.Gutter - chipScroll));
        chipAnimation.Play();
    }

    /// <summary>
    /// "No results for “north”", "Search failed", or what search finds before anything is typed:
    /// at the gutter, y = 120, at most 560 wide, the title in Title over the sentence in Body
    /// Muted. Retry sits 12 dp under them; focus stays on the field until Down.
    /// </summary>
    void Notice(LibraryMessage message)
    {
        ClearBody(message);
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(NoticeTop)) };
        var title = Text.Label(message.Title, PleyType.Title, PleyColors.Fg, width: NoticeWidth);
        view.Add(title);
        var words = Text.Label(message.Body, PleyType.Body, PleyColors.Muted, maxLines: 4, width: NoticeWidth);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.SizeHeight = Math.Min(words.GetHeightForWidth(Ui.Dp(NoticeWidth)), Text.LineHeight(PleyType.Body) * 4);
        words.PositionY = Ui.Dp(PleyType.Title.Line + 8);
        view.Add(words);
        if (message.ActionLabel is { } action)
        {
            var buttons = new ButtonRow();
            buttons.Add(new ButtonView(action, ButtonKind.Pill, model.Retry, iconName: "rotate-ccw"));
            buttons.PositionY = words.PositionY + words.SizeHeight + Ui.Dp(8 + 12 + 8);
            view.Add(buttons);
            below.Add(buttons.Group);
        }
        body.Add(view);
    }

    /// <summary>Two rows of still poster skeletons while the first answer comes (no shimmer on TV).</summary>
    void Skeletons()
    {
        ClearBody("skeleton");
        var tile = TileKind.Poster;
        var y = ResultsTop;
        for (var r = 0; r < SkeletonRows; r++)
        {
            Block(PleyTv.Gutter, y, 140, 22, PleyShape.BadgeRadius);
            y += 22 + PleyTv.HeaderToTiles;
            for (var i = 0; i < SkeletonTiles; i++) Block(PleyTv.Gutter + i * (tile.Width + PleyTv.TileGap), y, tile.Width, tile.Height, PleyShape.TileRadius);
            y += tile.Height + PleyTv.RowGap;
        }
    }

    void Block(float x, float y, float w, float h, float radius) =>
        body.Add(new View { Position = new Position(Ui.Dp(x), Ui.Dp(y)), Size = new Size(Ui.Dp(w), Ui.Dp(h)), CornerRadius = Ui.Dp(radius), BackgroundColor = PleyColors.Surface2 });
}

/// <summary>
/// Search's Live TV row (the Android app's ChannelRow; the web's ChannelResults): the channels
/// found as Live TV's channel tiles, the logo with the number and how far what's on is, captioned
/// with the name and what's on. OK plays the channel; there are no details on a long press, as on
/// Android TV. As in a row of tiles, the focused tile's left edge stays at the gutter while the
/// row moves under it, Left from the first tile goes to the rail and Right stops at the end.
/// </summary>
sealed class ChannelRowView : View
{
    static readonly TileKind Kind = ChannelCard.Kind;
    static readonly int PerScreen = RowReach.PerScreen(Kind);

    readonly IReadOnlyList<LiveChannel> channels;
    readonly string origin;
    readonly KeyRepeatGate gate;
    readonly Images images;
    readonly View strip;
    /// <summary>Each channel's place in the focus tree, there from the start, whether its card is made yet or not.</summary>
    readonly List<FocusItem> items = new List<FocusItem>();
    /// <summary>The cards made so far, from the row's start (<see cref="Make"/>).</summary>
    readonly List<ChannelCard> cards = new List<ChannelCard>();
    /// <summary>The logos asked for, given up when the row goes.</summary>
    readonly CancellationTokenSource logos = new CancellationTokenSource();
    long now;
    Animation? scroll;

    public ChannelRowView(IReadOnlyList<LiveChannel> channels, string origin, long now, KeyRepeatGate gate, Images images, Action<LiveChannel> onWatch, Func<bool> toRail)
    {
        this.channels = channels;
        this.origin = origin;
        this.now = now;
        this.gate = gate;
        this.images = images;
        Group = new FocusGroup(Axis.Row, SearchText.LiveTv);
        Group.Exit = d => d == Direction.Right || (d == Direction.Left && Scrolling.Left(gate, toRail));
        var header = Text.Label(SearchText.LiveTv, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        header.Position = new Position(Ui.Dp(PleyTv.Gutter), 0);
        Add(header);
        var top = Ui.Dp(PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles);
        strip = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), top) };
        Add(strip);
        for (var i = 0; i < channels.Count; i++)
        {
            var channel = channels[i];
            var at = i;
            var item = Group.Add(new FocusItem($"{SearchText.LiveTv}/{channel.Id}") { Selected = () => onWatch(channel) });
            item.FocusChanged += on =>
            {
                if (at < cards.Count) cards[at].Focus(on);
            };
            items.Add(item);
        }
        // A card costs a few milliseconds on a TV and its logo a download, and search finds up to
        // 200 channels: the cards on screen are made now, the rest as focus comes near them
        // (RowReach), each logo asked for as its card is made.
        Make(RowReach.Now(0, PerScreen, channels.Count));
        Group.ChildFocused += ScrollTo;
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), top + Ui.Dp(Kind.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight));
    }

    public FocusGroup Group { get; }

    /// <summary>What's on each channel whose card is made, and how far it is, at <paramref name="now"/>; cards made later start from it.</summary>
    public void SetNow(long now)
    {
        if (Disposed) return;
        this.now = now;
        foreach (var card in cards) card.Now(now);
    }

    /// <summary>Makes the row's first <paramref name="count"/> cards, those not made yet, each looking as its focus is.</summary>
    void Make(int count)
    {
        var step = Ui.Dp(Kind.Width + Kind.Gap);
        for (var i = cards.Count; i < Math.Min(count, channels.Count); i++)
        {
            var channel = channels[i];
            var card = new ChannelCard(channel, channel.Name, origin, images, now, logos.Token) { Position = new Position(i * step, 0) };
            strip.Add(card);
            cards.Add(card);
            if (items[i].Focused) card.Focus(true);
        }
    }

    /// <summary>Where the strip sits for the tile at <paramref name="index"/>, as in <see cref="RowView"/>: its left edge at the gutter, or as far as the row goes.</summary>
    float OffsetFor(int index)
    {
        var step = Ui.Dp(Kind.Width + Kind.Gap);
        var content = channels.Count * step - Ui.Dp(Kind.Gap);
        var room = Ui.Dp(PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        if (content <= room) return 0;
        var most = content + Ui.Dp(PleyTv.SafeHorizontal + Kind.Width) - Ui.Dp(PleyTv.CanvasWidth - PleyTv.Gutter);
        return -Math.Min(index * step, Math.Max(0, most));
    }

    void ScrollTo(int index)
    {
        // The first on screen is the one cut off at the gutter's left, as in RowView.
        var first = Math.Max(0, (int)(-OffsetFor(index) / Ui.Dp(Kind.Width + Kind.Gap)) - 1);
        Make(RowReach.Now(first, PerScreen, channels.Count));
        scroll?.Stop();
        scroll = Scrolling.Glide(gate);
        scroll.AnimateTo(strip, "PositionX", Ui.Dp(PleyTv.Gutter) + OffsetFor(index));
        scroll.Play();
    }

    /// <summary>
    /// The row goes (the next answer came, or the page went): its logos still coming are given up,
    /// and its cards go with it, so the one focus leaves has no animation running once nothing holds it.
    /// </summary>
    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        // Only when disposed by the app: collected, the cards' animations may have gone first (Kit/Held.cs).
        if (type == DisposeTypes.Explicit)
        {
            logos.Cancel();
            scroll?.Stop();
            foreach (var card in cards) card.Dispose();
        }
        base.Dispose(type);
    }
}

/// <summary>
/// Search's Discover row (the Android app's TvTmdbRow): TMDB's titles for the words as posters with
/// their status badges, each opening its title's page, then See all for every title TMDB finds for
/// them; a still row while TMDB is asked; or why it can't be searched, with Retry where trying again
/// may help. It keeps its place among the rows as what it shows changes.
/// </summary>
sealed class TmdbRowView : View
{
    readonly KeyRepeatGate gate;
    readonly Images images;
    readonly string origin;
    readonly Action<PageKey> go;
    readonly Action retry;
    readonly Func<bool> toRail;
    readonly Func<FocusItem, bool> focus;
    readonly Action away;
    RowView? row;
    TmdbSection? shown;

    /// <param name="away">Focus was in the row, and what it shows now takes none (Retry pressed, TMDB asked again): where focus goes.</param>
    public TmdbRowView(KeyRepeatGate gate, Images images, string origin, Action<PageKey> go, Action retry, Func<bool> toRail, Func<FocusItem, bool> focus, Action away)
    {
        this.away = away;
        this.gate = gate;
        this.images = images;
        this.origin = origin;
        this.go = go;
        this.retry = retry;
        this.toRail = toRail;
        this.focus = focus;
        Group = new FocusGroup(Axis.Column, RequestText.SearchRow);
        Group.Exit = d => d == Direction.Right || (d == Direction.Left && Scrolling.Left(gate, toRail));
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles + TileKind.Poster.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight));
    }

    public FocusGroup Group { get; }

    public void Show(TmdbSection section)
    {
        if (Equals(section, shown)) return;
        var hadFocus = Group.Children.Any(Holds);
        if (shown is TmdbSection.Ready before && section is TmdbSection.Ready ready && before.Query == ready.Query && row != null)
        {
            // Badges that followed a request: the row stays, its tiles take their new look.
            shown = section;
            row.Update(ready.Titles.Select(t => RequestKit.TitleTile(t, origin)).ToList());
            return;
        }
        shown = section;
        row = null;
        foreach (var child in Children.ToList())
        {
            Remove(child);
            child.Dispose();
        }
        Group.Clear();
        switch (section)
        {
            case TmdbSection.Ready r:
            {
                var titles = r.Titles;
                row = new RowView(RequestText.SearchRow, titles.Select(t => RequestKit.TitleTile(t, origin)).ToList(), TileKind.Poster, gate, images,
                    tile =>
                    {
                        var title = titles.FirstOrDefault(t => RequestKit.TileId(t) == tile.Id);
                        if (title != null) go(RequestRules.Opens(title));
                    },
                    null, toRail, focusItem: focus, seeAll: (RequestText.SeeAll, () => go(new PageKey.DiscoverResults(r.Query))));
                Add(row);
                Group.Add(row.Group);
                row.ShowPictures();
                break;
            }
            case TmdbSection.Failed f:
            {
                var header = Text.Label(RequestText.SearchRow, PleyType.Section, PleyColors.Fg, width: 600);
                header.PositionX = Ui.Dp(PleyTv.Gutter);
                Add(header);
                var y = PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles;
                var title = Text.Label(RequestKit.ProblemTitle(f.Problem, f.Summary, RequestText.SearchFailed), PleyType.Title, PleyColors.Fg, width: 560);
                title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
                Add(title);
                y += PleyType.Title.Line;
                if (RequestKit.ProblemBody(f.Problem, f.Summary) is { } body)
                {
                    var words = PanelColumn.Lines(body, PleyType.Body, PleyColors.Muted, 560, 2);
                    words.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y + 4));
                    Add(words);
                    y += 4 + words.SizeHeight / Ui.Scale;
                }
                if (f.Problem.Retry)
                {
                    var buttons = new ButtonRow();
                    buttons.Add(new ButtonView(RequestText.Retry, ButtonKind.Pill, retry, iconName: "refresh-cw"));
                    buttons.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y + 16));
                    Add(buttons);
                    Group.Add(buttons.Group);
                }
                break;
            }
            case TmdbSection.Loading:
            {
                // A still row while TMDB is asked: its title and wells; nothing in it takes focus.
                var header = Text.Label(RequestText.SearchRow, PleyType.Section, PleyColors.Fg, width: 600);
                header.PositionX = Ui.Dp(PleyTv.Gutter);
                Add(header);
                var tile = TileKind.Poster;
                for (var i = 0; i < 7; i++)
                    Add(new View
                    {
                        Position = new Position(Ui.Dp(PleyTv.Gutter + (tile.Width + tile.Gap) * i), Ui.Dp(PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles)),
                        Size = new Size(Ui.Dp(tile.Width), Ui.Dp(tile.Height)),
                        CornerRadius = Ui.Dp(PleyShape.TileRadius),
                        BackgroundColor = PleyColors.Surface2,
                    });
                break;
            }
        }
        if (hadFocus)
        {
            if (Group.Children.FirstOrDefault(c => c.CanTakeFocus) is FocusGroup g && g.Children.OfType<FocusItem>().FirstOrDefault(i => i.CanTakeFocus) is { } item) focus(item);
            else away();
        }
    }

    static bool Holds(FocusNode node) => node switch
    {
        FocusItem item => item.Focused,
        FocusGroup group => group.Children.Any(Holds),
        _ => false,
    };

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        // Only when disposed by the app: collected, its row may have gone first (Kit/Held.cs).
        if (type == DisposeTypes.Explicit) row?.Dispose();
        base.Dispose(type);
    }
}
