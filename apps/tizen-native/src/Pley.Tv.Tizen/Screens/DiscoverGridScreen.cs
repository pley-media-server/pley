using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A grid of TMDB's titles on TV (the Android app's TvDiscoverGridScreen): the See all of a
/// Discover row, the whole list under its name, or of Search's Discover row, every title TMDB finds
/// under "Results for “dune”", each with its badge. It reads four rows' worth first, five pages at
/// most (for a profile with an age limit, page after page can come empty), then three rows more as
/// focus comes within two rows of its end, with still wells while they come. At the end, Show more
/// once five pages brought too few, which reads five more, or Retry after a page failed. Back goes
/// back to the page it came from.
/// </summary>
sealed class DiscoverGridScreen : ShellPage
{
    /// <summary>The grid's posters: the library grid's, seven columns.</summary>
    public const int Columns = 7;

    /// <summary>Rows read before the first answer shows whole.</summary>
    public const int FirstRows = 4;

    /// <summary>The grid under the page's title: it scrolls up to here.</summary>
    const float GridTop = 72;

    static readonly TileKind Kind = TileKind.LibraryGrid;

    readonly DiscoverGridModel model;
    readonly Shell layers;
    readonly Action<PageKey> go;
    readonly Action<PageKey> restart;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    readonly GridView grid;
    readonly View aside = new View();
    IReadOnlyList<DiscoverTitle> titles = Array.Empty<DiscoverTitle>();
    /// <summary>The grid ends in Show more (or Retry, with <see cref="retry"/>).</summary>
    bool end, retry, entered, failed;
    object? shownNotice;

    public DiscoverGridScreen(TvShell shell, Shell layers, DiscoverGridModel model, Images images, KeyRepeatGate gate, Action<PageKey> go, Action<PageKey> restart)
        : base(shell, model.State.Value.List is { } list ? new PageKey.DiscoverList(list) : new PageKey.DiscoverResults(model.Query), "discover-grid")
    {
        this.model = model;
        this.layers = layers;
        this.go = go;
        this.restart = restart;
        View.BackgroundColor = PleyColors.Bg;
        View.Add(body);
        var state = model.State.Value;
        var heading = Text.Label(state.List is { } l ? RequestText.Row(l) : RequestText.ResultsFor(model.Query), PleyType.PageTitle, PleyColors.Fg,
            width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        heading.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(heading);
        grid = new GridView(Kind, Columns, PleyTv.CanvasHeight - GridTop, gate, images, TileAt) { PositionY = Ui.Dp(GridTop) };
        grid.Open = (at, _) =>
        {
            if (at < titles.Count) go(RequestRules.Opens(titles[at]));
            else if (end) ShowMore();
        };
        // Two rows from the end: the next pages, until three rows more are in.
        grid.Moved += at =>
        {
            if (at >= titles.Count - 2 * Columns) model.Want(titles.Count + 3 * Columns);
        };
        grid.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        aside.Position = new Position(Ui.Dp(PleyTv.Gutter + Kind.Width + 32), Ui.Dp(GridTop + 68));
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        // Back on the page, what failed is asked again.
        if (entered && failed) model.Retry();
        entered = true;
        if (!Tree.Enter(Group) && !Wait()) Shell.FocusRail();
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Dispose();
    }

    /// <summary>The tiles by title, and Show more's: the grid draws a cell again only when its tile is another.</summary>
    readonly Dictionary<DiscoverTitle, TileModel> tiles = new Dictionary<DiscoverTitle, TileModel>();
    TileModel? endTile;

    TileModel? TileAt(int at)
    {
        if (at < titles.Count)
        {
            var title = titles[at];
            if (!tiles.TryGetValue(title, out var tile)) tiles[title] = tile = RequestKit.TitleTile(title, model.State.Value.Origin);
            return tile;
        }
        if (!end) return null;
        var label = retry ? RequestText.Retry : RequestText.ShowMore;
        if (endTile?.Title != label) endTile = new TileModel(-1, label, "", Array.Empty<string>(), 0f, false, 0, Array.Empty<string>(), label);
        return endTile;
    }

    void ShowMore()
    {
        var at = titles.Count;
        model.ShowMore();
        // The well in Show more's place takes focus, and keeps it as its title comes.
        if (grid.ItemAt(at) is { } item) Tree.Focus(item);
    }

    void Show(DiscoverGridState s)
    {
        var problem = s.Problem ?? (s.Search as DiscoverSearch.Failed)?.Problem;
        failed = problem != null;
        if (problem != null)
        {
            Notice(problem, () =>
            {
                var summary = s.Gate.Summary;
                var action = RequestKit.ProblemAction(problem, summary, model.Retry, () => RequestKit.SignInAgain(layers, s.Origin, restart), () => go(new PageKey.Requests()));
                var title = RequestKit.ProblemTitle(problem, summary, s.List != null ? RequestText.ListFailedShort : RequestText.SearchFailed);
                return RequestKit.Notice(title, RequestKit.ProblemBody(problem, summary), RequestKit.ProblemIcon(problem), action, Group, top: 96);
            });
            return;
        }
        switch (s.Search)
        {
            case DiscoverSearch.Empty:
                Notice("empty", () => s.List != null
                    ? RequestKit.Notice(RequestText.NothingTitle, RequestText.ListNothingBody, "compass", null, Group, top: 96)
                    : RequestKit.Notice(RequestText.SearchNone(s.Query), RequestText.SearchNoneBody, "search", null, Group, top: 96));
                return;
            case DiscoverSearch.Ready ready:
                Grid(ready.Pages, s);
                return;
            default:
                Grid(Pages.None with { Loading = true }, s);
                return;
        }
    }

    /// <summary>A sentence in place of the grid; with nothing to press, focus goes to the rail.</summary>
    void Notice(object key, Func<View> make)
    {
        if (Equals(key, shownNotice)) return;
        shownNotice = key;
        var hadFocus = Tree.Current != null && Tree.Current.Parent != Shell.Rail.Group;
        foreach (var child in body.Children.ToList())
        {
            body.Remove(child);
            if (child != grid && child != aside) child.Dispose();
        }
        Group.Clear();
        body.Add(make());
        if (entered && !Tree.Enter(Group) && hadFocus) Shell.FocusRail();
    }

    /// <summary>
    /// The titles as posters; while more come, still wells to the end of the row (the first of them
    /// can hold focus), and at the end Show more or Retry. Beside a lone Show more or Retry, why there
    /// are no titles yet.
    /// </summary>
    void Grid(Pages pages, DiscoverGridState s)
    {
        if (shownNotice != null || grid.GetParent() == null)
        {
            shownNotice = null;
            foreach (var child in body.Children.ToList())
            {
                body.Remove(child);
                if (child != grid && child != aside) child.Dispose();
            }
            Group.Clear();
            body.Add(grid);
            body.Add(aside);
            Group.Add(grid.Group);
        }
        var focused = grid.Current;
        var had = Tree.Current != null && Tree.Current.Parent == grid.Group;
        titles = pages.Titles;
        var coming = !pages.Settled;
        end = !coming && (pages.Stopped || pages.Problem != null);
        retry = pages.Problem != null;
        var count = coming ? (titles.Count + 1 + Columns - 1) / Columns * Columns : titles.Count + (end ? 1 : 0);
        grid.Update(count);
        if (had && grid.ItemAt(Math.Max(0, Math.Min(focused, count - 1))) is { } again) Tree.Focus(again);
        else if (Tree.Current == Waiting && grid.ItemAt(0) is { } first) Tree.Focus(first);
        Waited();
        Aside(pages, s);
    }

    /// <summary>Beside a grid's lone Show more or Retry: why it has no titles yet.</summary>
    void Aside(Pages pages, DiscoverGridState s)
    {
        foreach (var child in aside.Children.ToList())
        {
            aside.Remove(child);
            child.Dispose();
        }
        if (titles.Count > 0 || !end) return;
        var summary = s.Gate.Summary;
        var problem = pages.Problem;
        var title = problem != null ? RequestKit.ProblemTitle(problem, summary, s.List != null ? RequestText.ListFailedShort : RequestText.SearchFailed)
            : s.List != null ? RequestText.NoneYet : RequestText.SearchNoneYet(s.Query);
        var body = problem != null ? RequestKit.ProblemBody(problem, summary) : RequestText.NoneYetBody;
        var heading = PanelColumn.Lines(title, PleyType.Title, PleyColors.Fg, 520, 2);
        aside.Add(heading);
        if (body != null)
        {
            var words = PanelColumn.Lines(body, PleyType.Body, PleyColors.Muted, 520, 4);
            words.PositionY = heading.SizeHeight + Ui.Dp(8);
            aside.Add(words);
        }
    }
}
