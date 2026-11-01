using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Live TV (the Android app's TvLiveTvScreen): "Live TV" with the Guide and
/// Channels tabs; down the left the categories under the search (Favourites, Recently watched,
/// All channels, then the provider's groups, those of a region under it); beside them the chosen
/// category's guide (the page's first focus) or tiles, a page of channels loaded at a time,
/// opening on the channel last watched. OK on a channel watches it in the live player; holding OK
/// (or Menu, or Info) opens what's on in a side panel, with Watch and the favourite.
/// </summary>
sealed class LiveTvScreen : ShellPage
{
    const float HeaderHeight = 40, ContentTop = PleyTv.SafeVertical + HeaderHeight + 16, CategoriesLeft = PleyTv.Gutter - LiveCategories.Inset;
    const float ContentLeft = CategoriesLeft + LiveCategories.Width + 16, MessageWidth = 520;

    readonly Shell layers;
    readonly LiveTvModel model;
    readonly Action<LiveWatch, Action> play;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View tabsView = new View();
    readonly FocusGroup tabs = new FocusGroup(Axis.Row, "tabs");
    readonly ButtonView guideTab;
    readonly ButtonView channelsTab;
    readonly FocusGroup body = new FocusGroup(Axis.Row, "body");
    readonly TextInput search;
    readonly CategoryColumnView categories;
    readonly View contentView = new View();
    readonly FocusGroup content = new FocusGroup(Axis.Column, "content");
    readonly View notice;
    readonly TextLabel noticeText;
    readonly Timer noticeTimer = new Timer((uint)LiveTvModel.NoticeMs);
    View? message;
    GuideView? guide;
    ChannelCardsView? cards;
    /// <summary>What the content shows now: drawn again when it changes.</summary>
    string? shown;
    bool allowedShown = true;
    (LiveGroups? Groups, LiveCategory Category, IReadOnlyList<LiveGroup> Found)? categoriesShown;
    /// <summary>Focus was sent to the channels (the page opened, a category chosen, a search made): they take it as they show.</summary>
    bool toContent = true;
    bool gone;

    /// <param name="play">Opens the live player on a channel; the action given is called once it closes.</param>
    public LiveTvScreen(TvShell shell, Shell layers, LiveTvModel model, Images images, KeyRepeatGate gate, Action<LiveWatch, Action> play)
        : base(shell, new PageKey.LiveTv(), "live")
    {
        this.layers = layers;
        this.model = model;
        this.play = play;
        this.images = images;
        this.gate = gate;

        var title = Text.Label(LiveText.Title, PleyType.PageTitle, PleyColors.Fg, width: 400);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical + (HeaderHeight - PleyType.PageTitle.Line) / 2));
        View.Add(title);
        guideTab = new ButtonView(LiveText.Guide, ButtonKind.Text, () => model.SetView(LiveView.Guide));
        channelsTab = new ButtonView(LiveText.Channels, ButtonKind.Text, () => model.SetView(LiveView.Channels));
        channelsTab.PositionX = guideTab.SizeWidth + Ui.Dp(8);
        tabsView.Add(guideTab);
        tabsView.Add(channelsTab);
        var tabsWidth = guideTab.WidthDp + 8 + channelsTab.WidthDp;
        tabsView.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - tabsWidth), Ui.Dp(PleyTv.SafeVertical));
        View.Add(tabsView);
        tabs.Add(guideTab.Item);
        tabs.Add(channelsTab.Item);
        tabs.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);

        search = new TextInput(layers.Input, LiveCategories.Width - (LiveCategories.Inset - 10), LiveText.Search, iconName: "search");
        search.Changed += q =>
        {
            if (!gone) model.SetQuery(q);
        };
        search.Submitted = () =>
        {
            toContent = true;
            Tree.Enter(content);
        };
        categories = new CategoryColumnView(Tree, LiveCategories.Width, PleyTv.CanvasHeight - ContentTop, search)
        {
            Position = new Position(Ui.Dp(CategoriesLeft), Ui.Dp(ContentTop)),
            Chosen = Choose,
        };
        categories.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        View.Add(categories);
        contentView.Position = new Position(Ui.Dp(ContentLeft), Ui.Dp(ContentTop));
        View.Add(contentView);

        Group.Add(tabs);
        Group.Add(body);
        // Up from the top of the categories or the guide goes to the tabs on a press; a held Up
        // stops there, as a held Left stops at a row's start rather than open the rail.
        body.Exit = d => d == Direction.Up && gate.Holding;
        body.Add(categories.Group);
        body.Add(content);

        notice = new View { CornerRadius = Ui.Dp(PleyShape.ControlRadius), BackgroundColor = PleyColors.Surface3 };
        noticeText = Text.Label("", PleyType.Body, PleyColors.Fg);
        noticeText.Position = new Position(Ui.Dp(20), 0);
        notice.Add(noticeText);
        notice.Hide();
        View.Add(notice);
        noticeTimer.Tick += (_, _) =>
        {
            if (!gone) notice.Hide();
            return false;
        };

        model.WatchRequested += Watch;
        model.Told += Told;
        watches.Add(model.State.Watch(OnState));
        watches.Add(model.Channels.Watch(_ => ShowContent()));
        watches.Add(model.Guide.Watch(g => guide?.SetPrograms(g)));
        watches.Add(model.Now.Watch(now =>
        {
            guide?.SetNow(now);
            cards?.SetNow(now);
        }));
        watches.Add(model.Landing.Watch(l =>
        {
            guide?.Land(l);
            cards?.Land(l);
        }));
    }

    public override void Enter()
    {
        if (Tree.Enter(content)) return;
        if (allowedShown && categories.FocusShown()) return;
        Shell.FocusRail();
    }

    /// <summary>
    /// Back goes up a step at a time: the guide back to now first, then from the guide or the
    /// channels to the categories, then (the shell's) to the rail; not from the guide to the rail.
    /// </summary>
    public override bool Back()
    {
        if (guide?.Back() == true) return true;
        return allowedShown && Tree.Current is { } at && InContent(at) && categories.FocusShown();
    }

    public override void Gone()
    {
        gone = true;
        noticeTimer.Stop();
        model.WatchRequested -= Watch;
        model.Told -= Told;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        model.Dispose();
    }

    void OnState(LivePage page)
    {
        Profile.Mark("live page: state");
        if (gone) return;
        if (page.Allowed != allowedShown)
        {
            allowedShown = page.Allowed;
            if (!page.Allowed)
            {
                var hadFocus = InPage(Tree.Current);
                tabsView.Hide();
                categories.Hide();
                tabs.Clear();
                body.Clear();
                shown = null;
                ClearContent();
                var m = Message(LiveText.NotAllowedTitle, LiveText.NotAllowedBody, null);
                m.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ContentTop + 24));
                View.Add(m);
                message = m;
                if (hadFocus) Shell.FocusRail();
                return;
            }
            // Open again (an administrator changed it): the page as it was.
            message?.Dispose();
            message = null;
            tabsView.Show();
            categories.Show();
            tabs.Add(guideTab.Item);
            tabs.Add(channelsTab.Item);
            body.Add(categories.Group);
            body.Add(content);
        }
        if (!page.Allowed) return;
        Tab(guideTab, page.View == LiveView.Guide);
        Tab(channelsTab, page.View == LiveView.Channels);
        tabs.RememberChild(page.View == LiveView.Guide ? 0 : 1);
        var wanted = (page.Groups, page.Category, page.Found);
        if (categoriesShown is not { } was || was.Groups != wanted.Groups || was.Category != wanted.Category || !was.Found.SequenceEqual(wanted.Found))
        {
            categoriesShown = wanted;
            categories.Show(page.Groups, page.Category, page.Found);
        }
        ShowContent();
    }

    static void Tab(ButtonView tab, bool on)
    {
        tab.Rest = on ? PleyColors.Surface3 : Color.Transparent;
        tab.Ink = on ? PleyColors.Fg : PleyColors.Muted;
    }

    /// <summary>The channels as the category's state says: why there are none, or the guide or the tiles.</summary>
    void ShowContent()
    {
        if (gone) return;
        var page = model.State.Value;
        if (!page.Allowed) return;
        var pages = model.Channels.Value;
        var kind = pages.Total == null ? pages.Error != null ? "failed" : "loading" : pages.Total == 0 ? "empty" : "list";
        var key = $"{kind}|{page.View}|{page.Listed.Key}|{(kind == "empty" ? page.Query.Trim() : "")}";
        if (key != shown)
        {
            var hadFocus = Tree.Current != null && InContent(Tree.Current);
            var elsewhere = Tree.Current != null && !hadFocus && !InPage(Tree.Current);
            shown = key;
            ClearContent();
            switch (kind)
            {
                case "failed":
                    var e = pages.Error!;
                    var failed = Message(Errors.Unreachable(e) ? Errors.OfflineTitle : LiveText.Failed, Errors.Unreachable(e) ? Errors.OfflineBody(e) : Errors.Message(e), model.Retry);
                    failed.PositionY = Ui.Dp(24);
                    contentView.Add(failed);
                    message = failed;
                    break;
                case "empty":
                    var (t, b) = Empty(page);
                    var empty = Message(t, b, null);
                    empty.PositionY = Ui.Dp(24);
                    contentView.Add(empty);
                    message = empty;
                    break;
                case "list":
                    MakeList(page);
                    break;
            }
            // Shown as a search is typed, the list leaves focus in the search; sent there, or with focus gone, it takes it.
            if (!elsewhere && (toContent || hadFocus || Tree.Current == null || !categories.HoldsFocus) && content.CanTakeFocus)
            {
                if (Tree.Enter(content)) toContent = false;
            }
            else if (hadFocus && !content.CanTakeFocus) categories.FocusShown();
        }
        guide?.SetPages(pages);
        cards?.SetPages(pages);
    }

    void MakeList(LivePage page)
    {
        var width = PleyTv.CanvasWidth - ContentLeft - PleyTv.SafeHorizontal;
        var height = PleyTv.CanvasHeight - ContentTop;
        if (page.View == LiveView.Guide)
        {
            guide = new GuideView(model, images, gate, page.Listed, page.Origin, width, height, categories.FocusShown, model.Watch, Details);
            contentView.Add(guide);
            content.Add(guide.Item);
            guide.SetPages(model.Channels.Value);
            guide.SetPrograms(model.Guide.Value);
            guide.Land(model.Landing.Value);
        }
        else
        {
            cards = new ChannelCardsView(model, images, gate, page.Listed, page.Origin, width + PleyTv.SafeHorizontal, height, categories.FocusShown, model.Watch, Details);
            contentView.Add(cards);
            content.Add(cards.Item);
            cards.SetPages(model.Channels.Value);
            cards.Land(model.Landing.Value);
        }
    }

    void ClearContent()
    {
        content.Clear();
        foreach (var child in contentView.Children.ToList())
        {
            contentView.Remove(child);
            child.Dispose();
        }
        if (message != null && message.GetParent() != null && message.GetParent() != contentView)
        {
            message.Unparent();
            message.Dispose();
        }
        message = null;
        guide = null;
        cards = null;
    }

    /// <summary>A category or search without channels: why, in its own words.</summary>
    static (string Title, string Body) Empty(LivePage page)
    {
        if (page.Searching) return (LiveText.NoMatchTitle, LiveText.NoMatchSearch(page.Query));
        return page.Category switch
        {
            LiveCategory.Favorites => (LiveText.NoneFavoritesTitle, LiveText.NoneFavoritesBody),
            LiveCategory.Recent => (LiveText.NoneRecentTitle, LiveText.NoneRecentBody),
            LiveCategory.Group => (LiveText.NoMatch, ""),
            _ => (LiveText.NoneTitle, LiveText.NoneBody),
        };
    }

    /// <summary>The title in Section, the sentence in Body Muted, and Try again when there is something to try.</summary>
    View Message(string title, string text, Action? retry)
    {
        var view = new View();
        view.Add(Text.Label(title, PleyType.Section, PleyColors.Fg, width: MessageWidth));
        if (text.Length > 0)
        {
            var words = Text.Label(text, PleyType.Body, PleyColors.Muted, maxLines: 3, width: MessageWidth);
            words.VerticalAlignment = VerticalAlignment.Top;
            words.PositionY = Ui.Dp(PleyType.Section.Line + 8);
            view.Add(words);
        }
        if (retry != null)
        {
            var buttons = new ButtonRow();
            buttons.Add(new ButtonView(LiveText.Retry, ButtonKind.Pill, retry, iconName: "refresh-cw"));
            buttons.PositionY = Ui.Dp(PleyType.Section.Line + 8 + PleyType.Body.Line * 2 + 24);
            view.Add(buttons);
            content.Add(buttons.Group);
        }
        return view;
    }

    /// <summary>A category chosen: its channels show, and take focus; a search ends.</summary>
    void Choose(LiveCategory category)
    {
        toContent = true;
        if (search.Value.Length > 0) search.Value = "";
        model.SetCategory(category);
        if (content.CanTakeFocus && Tree.Enter(content)) toContent = false;
    }

    void Details(LiveChannel channel, LiveProgram? program)
    {
        var page = model.State.Value;
        var name = LiveRules.ChannelName(channel, page.Listed);
        new ProgramPanelSheet(layers, images, channel, name, program, page.Origin, model.Now.Value, () => model.Watch(channel), model.ToggleFavorite).Show();
    }

    void Watch(LiveWatch watch)
    {
        if (gone) return;
        // Back from the player, the list opens at the channel watched.
        play(watch, () =>
        {
            if (!gone) model.Returned();
        });
    }

    void Told(Exception e)
    {
        if (gone) return;
        noticeText.Text = LiveText.FavoriteFailed(e);
        var w = noticeText.NaturalSize.Width;
        noticeText.Size = new Size(w + 2, Ui.Dp(PleyType.Body.Line + 24));
        notice.Size = new Size(w + Ui.Dp(40), Ui.Dp(PleyType.Body.Line + 24));
        notice.Position = new Position((Ui.Dp(PleyTv.CanvasWidth) - notice.SizeWidth) / 2, Ui.Dp(PleyTv.CanvasHeight - 32 - PleyType.Body.Line - 24));
        notice.Show();
        notice.RaiseToTop();
        noticeTimer.Stop();
        noticeTimer.Start();
    }

    bool InContent(FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == content) return true;
        return false;
    }

    bool InPage(FocusNode? node)
    {
        if (node == null) return false;
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == Group) return true;
        return false;
    }
}
