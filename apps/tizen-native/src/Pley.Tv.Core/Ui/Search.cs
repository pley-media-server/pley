using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Live;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// The groups results come in, in the order they show (the web's SearchPage; the Android app's
/// SearchGroupKind): each kind of item, then people. Live TV's channels aren't a group of
/// tiles: they come beside the groups (<see cref="SearchResultsState.Ready.Channels"/>), in a row
/// before People.
/// </summary>
public enum SearchGroupKind
{
    Movies,
    Shows,
    Seasons,
    Episodes,
    Artists,
    Albums,
    Tracks,
    People,
}

/// <summary>One group of results: its tiles, and the ids beside them.</summary>
public sealed record SearchGroup(SearchGroupKind Kind, IReadOnlyList<TileModel> Tiles, IReadOnlyList<long> Ids)
{
    /// <summary>"People", "Movies"…: the row's title.</summary>
    public string Title => SearchText.GroupName(Kind);

    /// <summary>The tile the row draws on TV: people as circles, posters at their row size.</summary>
    public TileKind TileKind => SearchRules.TileKindOf(Kind);

    public int Count => Tiles.Count;
}

/// <summary>What the results part of the page shows (the Android app's SearchResultsState).</summary>
public abstract record SearchResultsState
{
    /// <summary>Nothing typed: recent searches, or what search finds.</summary>
    public sealed record Idle : SearchResultsState;

    /// <summary>The first answer for <paramref name="Query"/> is on its way, with nothing earlier to keep showing.</summary>
    public sealed record Loading(string Query) : SearchResultsState;

    /// <summary>
    /// Results for <paramref name="Query"/>, grouped, with the Live TV <paramref name="Channels"/>
    /// found (shown after the titles, before People, as on the web). While <paramref name="Refreshing"/>, a newer
    /// query's answer is on its way and these stay until it comes.
    /// </summary>
    public sealed record Ready(string Query, IReadOnlyList<SearchGroup> Groups, IReadOnlyList<LiveChannel> Channels, bool Refreshing) : SearchResultsState;

    /// <summary>No results for "north".</summary>
    public sealed record Empty(string Query) : SearchResultsState;

    /// <summary>"Search failed", with the error's sentence.</summary>
    public sealed record Failed(string Query, Exception Error) : SearchResultsState;
}

/// <summary>The page: what is typed, the profile's recent searches (newest first) and the results.</summary>
public sealed record SearchState(string Query, IReadOnlyList<string> Recent, SearchResultsState Results)
{
    /// <summary>An answer is on its way: "Searching…" beside the field.</summary>
    public bool Searching => Results is SearchResultsState.Loading or SearchResultsState.Ready { Refreshing: true };

    /// <summary>
    /// Down from the field has somewhere to go: the rows, Retry, or the recent searches' chips.
    /// Otherwise Down stays on the field.
    /// </summary>
    public bool HasBelow => Results is SearchResultsState.Ready or SearchResultsState.Failed || (Results is SearchResultsState.Idle && Recent.Count > 0);
}

/// <summary>What search reads and keeps: the server's answers and the profile's recent searches; the app's, or a fake in tests.</summary>
public interface ISearchSource
{
    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    /// <summary>Results for <paramref name="query"/>; <paramref name="cancel"/> gives it up when a newer query takes over.</summary>
    Task<SearchAnswer> Search(string query, CancellationToken cancel);

    /// <summary>This profile's recent searches on this TV, newest first.</summary>
    IReadOnlyList<string> Recent();

    /// <summary>Keeps <paramref name="recent"/> as this profile's recent searches (empty forgets them).</summary>
    void KeepRecent(IReadOnlyList<string> recent);
}

/// <summary>Search's source in the app: <see cref="AppState"/> and its API, recent searches kept on the TV per server and profile.</summary>
public sealed class AppSearchSource : ISearchSource
{
    const string RecentKey = "pley.searches";

    readonly AppState app;

    public AppSearchSource(AppState app) => this.app = app;

    public string Origin => app.Server.Value ?? "";

    public Task<SearchAnswer> Search(string query, CancellationToken cancel) =>
        app.Api.Value is { } api ? api.Search(query, cancel) : Task.FromException<SearchAnswer>(new SignedOutException());

    public IReadOnlyList<string> Recent()
    {
        if (KeyOf() is not { } key || app.Platform.Storage.Get(key) is not { Length: > 0 } text) return Array.Empty<string>();
        try
        {
            return (JsonSerializer.Deserialize<List<string>>(text) ?? new List<string>()).Take(SearchRules.RecentMax).ToList();
        }
        catch (JsonException)
        {
            return Array.Empty<string>();
        }
    }

    public void KeepRecent(IReadOnlyList<string> recent)
    {
        // Without a profile (signed out, not known yet) nothing is kept.
        if (KeyOf() is not { } key) return;
        if (recent.Count == 0) app.Platform.Storage.Remove(key);
        else app.Platform.Storage.Set(key, JsonSerializer.Serialize(recent));
    }

    string? KeyOf()
    {
        var server = app.Server.Value;
        var me = app.Me.Value?.Id;
        if (server == null || me == null) return null;
        return string.Join(".", RecentKey, server, me.Value.ToString(CultureInfo.InvariantCulture));
    }
}

/// <summary>Search's rules that aren't words (the Android app's SearchViewModel companion and tvSearchTiles).</summary>
public static class SearchRules
{
    /// <summary>How long typing pauses before what is typed is searched.</summary>
    public const long DebounceMs = 250;

    /// <summary>Recent searches kept per profile.</summary>
    public const int RecentMax = 10;

    /// <summary>How often what's on a channel found is looked at again.</summary>
    public const long NowTickMs = 30_000;

    /// <summary>
    /// Where the Live TV row goes among <paramref name="groups"/>' rows: after the titles and
    /// before People, as on the web and Android TV, so last when there are no people.
    /// </summary>
    public static int ChannelsAt(IReadOnlyList<SearchGroup> groups) => groups.Count(g => g.Kind != SearchGroupKind.People);

    /// <summary>
    /// Watching a channel found: the live player changes channel among every channel, as a
    /// search in Live TV does.
    /// </summary>
    public static LiveWatch Watch(LiveChannel channel) => new LiveWatch(channel.Id, new LiveCategory.All().Key, channel);

    /// <summary>The item kind each group holds; people have none.</summary>
    static string? KindOf(SearchGroupKind group) => group switch
    {
        SearchGroupKind.Movies => "movie",
        SearchGroupKind.Shows => "show",
        SearchGroupKind.Seasons => "season",
        SearchGroupKind.Episodes => "episode",
        SearchGroupKind.Artists => "artist",
        SearchGroupKind.Albums => "album",
        SearchGroupKind.Tracks => "track",
        _ => null,
    };

    /// <summary>How a group's tiles are drawn on TV: people as circles, episodes as stills, music as squares, the rest posters at their row size.</summary>
    public static TileKind TileKindOf(SearchGroupKind group) => group switch
    {
        SearchGroupKind.People => TileKind.Person,
        SearchGroupKind.Episodes => TileKind.Landscape,
        SearchGroupKind.Artists or SearchGroupKind.Albums or SearchGroupKind.Tracks => TileKind.Square,
        _ => TileKind.Poster,
    };

    /// <summary>
    /// <paramref name="answer"/> in its groups and the groups' order (movies, shows, seasons,
    /// episodes, artists, albums, tracks, people), each keeping the server's order of relevance;
    /// empty groups left out. Items of other kinds (extras) aren't shown, as on the web.
    /// </summary>
    public static IReadOnlyList<SearchGroup> Groups(SearchAnswer answer, string origin)
    {
        var groups = new List<SearchGroup>();
        foreach (SearchGroupKind group in Enum.GetValues(typeof(SearchGroupKind)))
        {
            if (KindOf(group) is not { } kind) continue;
            var items = answer.Items.Where(i => i.Kind == kind).ToList();
            if (items.Count == 0) continue;
            var tileKind = TileKindOf(group);
            groups.Add(new SearchGroup(group, items.Select(i => TileModel.Of(i, tileKind, origin)).ToList(), items.Select(i => i.Id).ToList()));
        }
        var people = answer.People ?? Array.Empty<PersonSummary>();
        if (people.Count > 0) groups.Add(new SearchGroup(SearchGroupKind.People, people.Select(p => PersonTile(p, origin)).ToList(), people.Select(p => p.Id).ToList()));
        return groups;
    }

    /// <summary>A person's tile: a circle with their photo, and their name.</summary>
    public static TileModel PersonTile(PersonSummary person, string origin) => new TileModel(
        person.Id,
        person.Name,
        "",
        string.IsNullOrEmpty(person.PhotoUrl) ? Array.Empty<string>() : new[] { Artwork.Absolute(person.PhotoUrl!, origin) },
        0f,
        false,
        0,
        Array.Empty<string>(),
        person.Name);

    /// <summary>Where a result opens: artists and albums their music pages, a track its album, the rest their item page.</summary>
    public static PageKey Opens(ItemSummary item) => item.Kind == "track"
        ? item.ParentId is { } album ? new AlbumPage(album) : new PageKey.Item(item.Id)
        : LibraryRules.Opens(item);

    /// <summary>
    /// <paramref name="recent"/> with <paramref name="query"/> on top, once whatever its case,
    /// at most <see cref="RecentMax"/>; as it was for a blank query.
    /// </summary>
    public static IReadOnlyList<string> Remembered(IReadOnlyList<string> recent, string query)
    {
        var q = query.Trim();
        if (q.Length == 0) return recent;
        return new[] { q }.Concat(recent.Where(r => !string.Equals(r, q, StringComparison.OrdinalIgnoreCase))).Take(RecentMax).ToList();
    }
}

/// <summary>
/// Search (apps/web/src/pages/SearchPage.tsx; the Android app's SearchViewModel, as the TV uses
/// it). What is typed is asked for once typing has paused 250 ms, and only the latest query's
/// answer counts: a newer one gives up a request still on its way. The results already shown
/// stay while the next ones load. Results come in groups: Movies, Shows, Seasons, Episodes,
/// Artists, Albums, Tracks, Live TV, People, the server's closest matches first in each; a
/// channel found plays at once. A query is kept among the profile's last 10 searches when it is
/// submitted or one of its results is opened.
///
/// The TV types with the system's keyboard over the field: the screen hands each change of the
/// field's text to <see cref="Type"/>, and the keyboard's Search key to <see cref="Submit"/>.
/// </summary>
public sealed class SearchModel : IDisposable
{
    readonly ISearchSource source;
    readonly IScheduler scheduler;
    /// <summary>The trimmed query last taken from the field.</summary>
    string taken = "";
    IDisposable? timer;
    CancellationTokenSource? running;
    int asks;
    /// <summary>The last answer, which asking for its query again shows at once.</summary>
    (string Query, SearchAnswer Answer)? last;
    /// <summary>The results shown, by id, for opening them.</summary>
    (string Query, Dictionary<long, ItemSummary> Items, HashSet<long> People, Dictionary<long, LiveChannel> Channels) shown =
        ("", new Dictionary<long, ItemSummary>(), new HashSet<long>(), new Dictionary<long, LiveChannel>());
    /// <summary>Moves <see cref="Now"/> on while channels are shown.</summary>
    IDisposable? ticker;
    bool gone;

    /// <param name="query">What the field starts with (<see cref="PageKey.Search.Query"/>).</param>
    public SearchModel(ISearchSource source, IScheduler scheduler, string query = "")
    {
        this.source = source;
        this.scheduler = scheduler;
        Now = new Signal<long>(scheduler.Now / 1000);
        State = new Signal<SearchState>(new SearchState(query, source.Recent(), new SearchResultsState.Idle()));
        Type(query);
    }

    public Signal<SearchState> State { get; }

    /// <summary>The time (unix seconds) the channels found show what's on at, and how far it is.</summary>
    public Signal<long> Now { get; }

    /// <summary>The server's origin: the channels' logos are on it.</summary>
    public string Origin => source.Origin;

    /// <summary>The field changed: <paramref name="text"/> as it shows it.</summary>
    public void Type(string text)
    {
        if (gone) return;
        if (text != State.Value.Query) State.Value = State.Value with { Query = text };
        var q = text.Trim();
        if (q == taken) return;
        taken = q;
        timer?.Dispose();
        timer = null;
        if (q.Length == 0)
        {
            // Nothing typed goes back to the recent searches at once.
            Drop();
            Results(new SearchResultsState.Idle());
            return;
        }
        timer = scheduler.After(SearchRules.DebounceMs, () =>
        {
            timer = null;
            Ask(q);
        });
    }

    /// <summary>Asks for the query again, after it failed.</summary>
    public void Retry()
    {
        if (taken.Length == 0 || timer != null) return;
        Ask(taken);
    }

    /// <summary>Search was pressed on the keyboard: the query is kept among the recent ones.</summary>
    public void Submit() => Remember(State.Value.Query);

    /// <summary>A recent search was chosen: it is searched again, and moves to the top. The screen puts it in the field too.</summary>
    public void SearchAgain(string query)
    {
        Type(query);
        Submit();
    }

    public void ClearRecent()
    {
        source.KeepRecent(Array.Empty<string>());
        State.Value = State.Value with { Recent = Array.Empty<string>() };
    }

    /// <summary>
    /// Where opening the result <paramref name="id"/> of <paramref name="group"/> goes (null when
    /// it isn't among the results); the query is kept among the recent searches, since one of its
    /// results was wanted.
    /// </summary>
    public PageKey? Open(SearchGroupKind group, long id)
    {
        var now = shown;
        PageKey? key = group == SearchGroupKind.People
            ? now.People.Contains(id) ? new PageKey.Person(id) : null
            : now.Items.TryGetValue(id, out var item) ? SearchRules.Opens(item) : null;
        if (key != null) Remember(now.Query);
        return key;
    }

    /// <summary>
    /// What watching the Live TV channel <paramref name="id"/> found opens (null when it isn't
    /// among the results): the live player on it, changing channel among every channel. The query
    /// is kept among the recent searches.
    /// </summary>
    public LiveWatch? Watch(long id)
    {
        var now = shown;
        if (!now.Channels.TryGetValue(id, out var channel)) return null;
        Remember(now.Query);
        return SearchRules.Watch(channel);
    }

    public void Dispose()
    {
        gone = true;
        timer?.Dispose();
        timer = null;
        ticker?.Dispose();
        ticker = null;
        Drop();
    }

    void Remember(string query)
    {
        var recent = SearchRules.Remembered(State.Value.Recent, query);
        if (ReferenceEquals(recent, State.Value.Recent)) return;
        source.KeepRecent(recent);
        State.Value = State.Value with { Recent = recent };
    }

    /// <summary>A request still on its way is given up, and its answer won't count.</summary>
    void Drop()
    {
        asks++;
        running?.Cancel();
        running = null;
    }

    void Ask(string q)
    {
        if (gone) return;
        Drop();
        var at = asks;
        if (last is { } l && l.Query == q)
        {
            Answered(q, l.Answer);
            return;
        }
        // What is shown stays while the new answer comes.
        Results(State.Value.Results is SearchResultsState.Ready ready ? ready with { Refreshing = true } : new SearchResultsState.Loading(q));
        var cancel = new CancellationTokenSource();
        running = cancel;
        Plays.When(scheduler, () => source.Search(q, cancel.Token), answer =>
        {
            if (gone || at != asks) return;
            running = null;
            last = (q, answer);
            Answered(q, answer);
        }, e =>
        {
            if (gone || at != asks) return;
            running = null;
            Results(new SearchResultsState.Failed(q, e));
        });
    }

    void Answered(string q, SearchAnswer answer)
    {
        var groups = SearchRules.Groups(answer, source.Origin);
        var channels = answer.Channels ?? Array.Empty<LiveChannel>();
        var items = new Dictionary<long, ItemSummary>();
        foreach (var item in answer.Items) items[item.Id] = item;
        var found = new Dictionary<long, LiveChannel>();
        foreach (var channel in channels) found[channel.Id] = channel;
        shown = (q, items, new HashSet<long>((answer.People ?? Array.Empty<PersonSummary>()).Select(p => p.Id)), found);
        // Channels alone are results: not "No results".
        Results(groups.Count == 0 && channels.Count == 0 ? new SearchResultsState.Empty(q) : new SearchResultsState.Ready(q, groups, channels, false));
    }

    void Results(SearchResultsState results)
    {
        // What's on a channel found moves on while it shows; nothing ticks otherwise.
        if (results is SearchResultsState.Ready { Channels.Count: > 0 })
        {
            Now.Value = scheduler.Now / 1000;
            if (ticker == null) Tick();
        }
        else
        {
            ticker?.Dispose();
            ticker = null;
        }
        State.Value = State.Value with { Results = results };
    }

    void Tick()
    {
        ticker = scheduler.After(SearchRules.NowTickMs, () =>
        {
            if (gone) return;
            Now.Value = scheduler.Now / 1000;
            Tick();
        });
    }
}

/// <summary>What the TV's search says (the Android app's strings_tv_search.xml).</summary>
public static class SearchText
{
    public static string Field => Strings.T("search.field");
    public static string Recent => Strings.T("search.recent.title");
    public static string RecentClear => Strings.T("search.recent.clear");
    public static string EmptyTitle => Strings.T("search.empty.title");
    public static string EmptyBody => Strings.T("search.empty.body");
    public static string NoneBody => Strings.T("search.none.body");
    /// <summary>The libraries found nothing, and Discover's row shows under it.</summary>
    public static string NothingInLibraries => Strings.T("search.nothingInLibraries");
    public static string Failed => Strings.T("search.failed");
    public static string Retry => Strings.T("search.retry");
    public static string Searching => Strings.T("search.searching");

    public static string People => Strings.T("search.groups.people");
    /// <summary>The channels' row, after the titles and before People.</summary>
    public static string LiveTv => Strings.T("search.groups.liveTv");
    public static string Movies => Strings.T("search.groups.movies");
    public static string Shows => Strings.T("search.groups.shows");
    public static string Seasons => Strings.T("search.groups.seasons");
    public static string Episodes => Strings.T("search.groups.episodes");
    public static string Artists => Strings.T("search.groups.artists");
    public static string Albums => Strings.T("search.groups.albums");
    public static string Tracks => Strings.T("search.groups.tracks");

    /// <summary>No results for “north”.</summary>
    public static string NoneTitle(string query) => Strings.T("search.none.title", ("query", query));

    public static string GroupName(SearchGroupKind group) => group switch
    {
        SearchGroupKind.People => People,
        SearchGroupKind.Movies => Movies,
        SearchGroupKind.Shows => Shows,
        SearchGroupKind.Seasons => Seasons,
        SearchGroupKind.Episodes => Episodes,
        SearchGroupKind.Artists => Artists,
        SearchGroupKind.Albums => Albums,
        _ => Tracks,
    };

    /// <summary>
    /// What shows in place of rows: nothing typed and no recent searches ("Search your
    /// libraries"), no results, or a failure with Retry; null while rows, chips or skeletons show.
    /// </summary>
    public static LibraryMessage? Message(SearchState state) => state.Results switch
    {
        SearchResultsState.Idle when state.Recent.Count == 0 => new LibraryMessage(EmptyTitle, EmptyBody),
        SearchResultsState.Empty e => new LibraryMessage(NoneTitle(e.Query), NoneBody),
        SearchResultsState.Failed f => new LibraryMessage(LibraryText.FailedTitle(f.Error, Failed), LibraryText.FailedBody(f.Error), MessageAction.Retry),
        _ => null,
    };
}
