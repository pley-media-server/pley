using System;
using System.Collections.Generic;
using Pley.Tv.Core;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>A page in the shell's back stack: Home, an item, a library.</summary>
abstract class ShellPage
{
    protected ShellPage(TvShell shell, PageKey key, string name)
    {
        Shell = shell;
        Key = key;
        View = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
        Group = new FocusGroup(Axis.Column, name);
    }

    protected TvShell Shell { get; }
    protected FocusTree Tree => Shell.Tree;
    public PageKey Key { get; }
    public View View { get; }
    public FocusGroup Group { get; }

    /// <summary>Back on the page: true when the page took it (Home's rows going back to its hero).</summary>
    public virtual bool Back() => false;

    /// <summary>Focus comes to the page: opened, or back on top. The group's remembered child by default.</summary>
    public virtual void Enter() => Tree.Enter(Group);

    /// <summary>The page left the stack: timers and requests stop.</summary>
    public virtual void Gone()
    {
    }

    /// <summary>
    /// The remote is in the middle of something on the page (libraries being sorted, a playlist's
    /// entry lifted): requests' news waits, as a press meant for it would close the news instead.
    /// </summary>
    public virtual bool Busy => false;

    /// <summary>A focus that shows nothing, for a page with nothing to focus yet.</summary>
    protected FocusItem Waiting { get; } = new FocusItem("waiting");

    /// <summary>
    /// Nothing on the page can take focus yet (it loads): focus waits on the page, showing
    /// nothing, and the page takes it on once it can. Given to the rail instead, the rail opened
    /// for a moment on the way to every page that loads (a search result's item).
    /// </summary>
    protected bool Wait()
    {
        if (Waiting.Parent != Group) Group.Add(Waiting);
        return Tree.Focus(Waiting);
    }

    /// <summary>The page can take focus now: the waiting focus goes, so entering the page never lands on it.</summary>
    protected void Waited()
    {
        if (Waiting.Parent == Group) Group.Remove(Waiting);
    }
}

/// <summary>
/// The TV's shell (the Android app's TvApp and TvNavigator): the rail and one back stack of pages,
/// started over by the rail. Back (tv-addendum §3.3, design spec §5.2 rule 6): the page's own
/// first; a page opened from another goes back to it; a page the rail opened gives focus to the
/// rail's current entry; Back on the rail leaves the app. Pages under the top one stay as they
/// were, so going back lands where focus was.
/// </summary>
sealed class TvShell : Sheet
{
    readonly AppState state;
    readonly FocusGroup slot = new FocusGroup(Axis.Column, "page");
    readonly View pages = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly List<ShellPage> stack = new List<ShellPage>();
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View toast;
    readonly TextLabel toastText;
    readonly Timer toastTimer = new Timer(3_500);
    IReadOnlyList<RailItem> railItems = Array.Empty<RailItem>();
    LiveTvEntry? liveTv;
    bool nowPlaying;
    bool storage;
    /// <summary>The way into requests and its number, while the profile has any.</summary>
    RequestsEntry requests = RequestsEntry.None;

    public TvShell(AppState state, Action<RailItem> onRail, Action onProfile) : base("shell", Axis.Row)
    {
        this.state = state;
        View.BackgroundColor = PleyColors.Bg;
        View.Add(pages);
        // The pages' fade under the collapsed rail: Bg to x = 56, clear from 72.
        View.Add(new View
        {
            Size = new Size(Ui.Dp(PleyTv.RailFadeClearFrom), Ui.Dp(PleyTv.CanvasHeight)),
            Background = Gradients.Horizontal(PleyColors.Bg, new Color(PleyColors.Bg.R, PleyColors.Bg.G, PleyColors.Bg.B, 0), PleyTv.RailFadeSolidUntil / PleyTv.RailFadeClearFrom),
        });
        Rail = new RailView(Tree, () => state.Settings.Value.ReduceMotion, onRail, onProfile);
        View.Add(Rail.Scrim);
        View.Add(Rail);
        Root.Add(Rail.Group);
        Root.Add(slot);

        toast = new View
        {
            Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.CanvasHeight - PleyTv.SafeVertical - 38)),
            CornerRadius = Ui.Dp(PleyShape.ControlRadius),
            BackgroundColor = PleyColors.Surface3,
        };
        toastText = Text.Label("", PleyType.Label, PleyColors.Fg);
        toastText.Position = new Position(Ui.Dp(20), 0);
        toast.Add(toastText);
        toast.Hide();
        View.Add(toast);
        toastTimer.Tick += (s, e) =>
        {
            toast.Hide();
            return false;
        };

        Back = OnBack;
        watches.Add(state.Libraries.Watch(_ => RailItems()));
        watches.Add(state.Me.Watch(me => Rail.SetProfile(me == null ? null : new RailProfile(me.Name, AvatarView.Rgb(ProfileColors.Of(me.Name, me.AvatarColor))))));
        Gone += () =>
        {
            foreach (var w in watches) w.Dispose();
            foreach (var p in stack) p.Gone();
            stack.Clear();
            toastTimer.Stop();
        };
    }

    public RailView Rail { get; }

    /// <summary>The page on top.</summary>
    public ShellPage? Top => stack.Count > 0 ? stack[stack.Count - 1] : null;

    /// <summary>The page the stack starts at, which the rail shows as current.</summary>
    public PageKey? RootKey => stack.Count > 0 ? stack[0].Key : null;

    /// <summary>Opens <paramref name="page"/> over the one on top.</summary>
    public void Open(ShellPage page)
    {
        if (Top is { } below) below.View.Hide();
        stack.Add(page);
        Show(page);
    }

    /// <summary>Makes the page for a key: set by the app, which knows every page.</summary>
    public Func<PageKey, ShellPage?>? PageOf { get; set; }

    /// <summary>Plays what a page asks for, in the player over everything; the app sets it.</summary>
    public Action<PlayRequest>? Play { get; set; }

    /// <summary>A tile's actions (OK held, Menu): the item, and whether its tile is in Continue Watching; the app sets it.</summary>
    public Action<long, bool>? Actions { get; set; }

    /// <summary>Opens the page of <paramref name="key"/> over the one on top; false when the app has none for it yet.</summary>
    public bool Open(PageKey key)
    {
        var opened = Profile.Start();
        var page = PageOf?.Invoke(key);
        if (page == null) return false;
        Open(page);
        Profile.Took($"page {key.GetType().Name} shown", opened);
        return true;
    }

    /// <summary>Shows the page of <paramref name="key"/> in place of the one on top, which turned out to be another (an extra opens its movie).</summary>
    public void ReplaceTop(PageKey key)
    {
        var page = PageOf?.Invoke(key);
        if (page == null || stack.Count == 0) return;
        var top = stack[stack.Count - 1];
        stack.RemoveAt(stack.Count - 1);
        top.View.Unparent();
        top.Gone();
        top.View.Dispose();
        stack.Add(page);
        Show(page);
    }

    /// <summary>Starts the stack over at <paramref name="page"/> (the rail's entries).</summary>
    public void Restart(ShellPage page)
    {
        foreach (var p in stack)
        {
            p.View.Unparent();
            p.Gone();
            p.View.Dispose();
        }
        stack.Clear();
        stack.Add(page);
        // Marked before the page enters: a page with nothing to focus yet gives focus to the rail's
        // current entry, which was still the page before (Home, on the first one opened).
        MarkCurrent();
        Show(page);
    }

    /// <summary>Focus to the rail's current entry, which opens the rail.</summary>
    public bool FocusRail() => Rail.FocusCurrent();

    /// <summary>A short message at the foot of the page, gone after 3.5 s, or <paramref name="ms"/> (a cast's "Playing from …", 4 s).</summary>
    public void Toast(string text, long ms = 3_500)
    {
        toastText.Text = text;
        var width = toastText.NaturalSize.Width;
        toastText.Size = new Size(width + 2, Ui.Dp(38));
        toast.Size = new Size(width + Ui.Dp(40), Ui.Dp(38));
        toast.Show();
        toastTimer.Stop();
        toastTimer.Interval = (uint)ms;
        toastTimer.Start();
    }

    void Show(ShellPage page)
    {
        if (page.View.GetParent() == null) pages.Add(page.View);
        page.View.Show();
        slot.Clear();
        slot.Add(page.Group);
        page.Enter();
    }

    /// <summary>The rail's entries for the libraries, with Live TV while it has channels and Now Playing while music has a queue.</summary>
    void RailItems()
    {
        railItems = Pley.Tv.Core.Ui.Rail.Items(state.Libraries.Value ?? Array.Empty<Library>(), nowPlaying: nowPlaying, liveTv: liveTv?.Channels.Value > 0, storage: storage,
            requests: Pley.Tv.Core.Ui.Rail.RequestsItem(requests));
        Rail.SetItems(railItems);
        MarkCurrent();
    }

    /// <summary>Live TV shows in the rail while <paramref name="entry"/> says it has channels.</summary>
    public void Follow(LiveTvEntry entry)
    {
        liveTv = entry;
        watches.Add(entry.Channels.Watch(_ => RailItems()));
    }

    /// <summary>Now Playing shows in the rail while <paramref name="now"/> has a track; the rail changes only as it comes and goes.</summary>
    public void Follow(Signal<NowPlaying?> now)
    {
        watches.Add(now.Watch(n =>
        {
            if ((n != null) == nowPlaying) return;
            nowPlaying = n != null;
            RailItems();
        }));
    }

    /// <summary>The way into requests shows in the rail as <paramref name="source"/>'s summary says, with its number.</summary>
    public void Follow(IRequestsSource source)
    {
        watches.Add(source.State.Watch(s =>
        {
            var entry = RequestsEntry.Of(s);
            if (entry == requests) return;
            requests = entry;
            RailItems();
        }));
    }

    /// <summary>The USB drive shows in the rail while one is plugged in.</summary>
    public void Follow(Signal<string?> drive)
    {
        watches.Add(drive.Watch(root =>
        {
            if ((root != null) == storage) return;
            storage = root != null;
            RailItems();
        }));
    }

    void MarkCurrent() => Rail.SetCurrent(RootKey == null ? null : Core.Ui.Rail.Current(RootKey, railItems));

    bool OnBack()
    {
        var current = Tree.Current;
        // Back on the rail leaves the app.
        if (current != null && current.Parent == Rail.Group) return false;
        if (Top?.Back() == true) return true;
        if (stack.Count > 1)
        {
            var top = stack[stack.Count - 1];
            stack.RemoveAt(stack.Count - 1);
            top.View.Unparent();
            top.Gone();
            top.View.Dispose();
            Show(stack[stack.Count - 1]);
            return true;
        }
        return FocusRail();
    }
}
