using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

namespace Pley.Tv.Core.Requests;

/// <summary>Search's Discover row: TMDB's first titles for the words that no library has.</summary>
public abstract record TmdbSection
{
    /// <summary>No row: nothing typed, asking doesn't work, or TMDB has none the libraries lack.</summary>
    public sealed record Hidden : TmdbSection;

    public sealed record Loading(string Query) : TmdbSection;

    public sealed record Ready(string Query, IReadOnlyList<DiscoverTitle> Titles) : TmdbSection;

    /// <summary>TMDB couldn't be searched: <paramref name="Problem"/> says why, in words for <paramref name="Summary"/>'s account.</summary>
    public sealed record Failed(string Query, RequestProblem Problem, RequestSummary? Summary) : TmdbSection;
}

/// <summary>
/// Discover in search (the Android app's SearchViewModel.tmdb, as the TV has it): beside the
/// libraries' search, never before it nor holding it up, TMDB's first <see cref="Shown"/> titles that
/// no library has (<c>missing=1</c>), read from at most <see cref="MaxPages"/> of its pages, their
/// badges following requests_changed. The words are cut to what the server takes. Hidden when asking
/// doesn't work or TMDB has none; when TMDB couldn't be searched, it says why.
/// </summary>
public sealed class SearchDiscoverModel : IDisposable
{
    /// <summary>How many of TMDB's titles a TV's Discover row shows; its See all has the rest.</summary>
    public const int Shown = 20;

    /// <summary>The most of TMDB's pages the row reads for them.</summary>
    public const int MaxPages = 3;

    readonly IRequestsSource source;
    readonly List<IDisposable> watches = new List<IDisposable>();
    string query = "";
    string asked = "";
    int runs;
    Task patching = Task.CompletedTask;
    bool gone;

    public SearchDiscoverModel(IRequestsSource source)
    {
        this.source = source;
        watches.Add(source.State.Watch(_ => Refresh()));
        source.Changed += Heard;
    }

    public Signal<TmdbSection> State { get; } = new Signal<TmdbSection>(new TmdbSection.Hidden());

    public void Dispose()
    {
        gone = true;
        runs++;
        foreach (var w in watches) w.Dispose();
        source.Changed -= Heard;
    }

    /// <summary>The libraries are searched for <paramref name="words"/> now (empty: nothing typed).</summary>
    public void Search(string words)
    {
        query = RequestRules.SearchText(words);
        Refresh();
    }

    /// <summary>Asks TMDB again, after the row failed.</summary>
    public void Retry()
    {
        asked = "";
        Refresh();
    }

    void Refresh()
    {
        if (gone) return;
        var q = source.State.Value.MayAsk ? query : "";
        if (q == asked) return;
        asked = q;
        var mine = ++runs;
        if (q.Length == 0)
        {
            State.Value = new TmdbSection.Hidden();
            return;
        }
        State.Value = new TmdbSection.Loading(q);
        _ = Read(q, mine);
    }

    async Task Read(string q, int mine)
    {
        // A short page, even an empty one (its titles all in a library), isn't the end: read on for enough to show.
        var pages = Pages.None;
        var read = 0;
        try
        {
            do
            {
                var answer = await source.Search(q, pages.Page + 1, missing: true);
                if (mine != runs) return;
                pages = pages.With(answer);
            }
            while (pages.Short(Shown) && ++read < MaxPages);
        }
        catch (Exception e)
        {
            if (mine != runs) return;
            // What came before shows; with nothing, why.
            if (pages.Titles.Count == 0)
            {
                State.Value = new TmdbSection.Failed(q, RequestProblem.Of(e), source.State.Value.Summary);
                return;
            }
        }
        var titles = pages.Titles.Take(Shown).ToList();
        State.Value = titles.Count == 0 ? new TmdbSection.Hidden() : new TmdbSection.Ready(q, titles);
    }

    void Heard(RequestChange change)
    {
        var mine = runs;
        if (!gone && State.Value is TmdbSection.Ready) patching = RequestsSources.After(patching, () => Patch(change, mine));
    }

    async Task Patch(RequestChange change, int mine)
    {
        if (State.Value is not TmdbSection.Ready ready) return;
        var patch = await source.PatchFor(change, ready.Titles);
        if (patch == null || gone || mine != runs || State.Value is not TmdbSection.Ready now) return;
        State.Value = now with { Titles = now.Titles.Select(patch).ToList() };
    }
}
