using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;

namespace Pley.Tv.Core.Ui;

/// <summary>A person's page (apps/web/src/pages/PersonPage.tsx; the Android app's PersonUiState).</summary>
public abstract record PersonState
{
    /// <summary>Nothing known yet: the header's still blocks.</summary>
    public sealed record Loading : PersonState;

    /// <summary>"Person not found": no such person, or nothing of theirs this profile may see (404).</summary>
    public sealed record NotFound : PersonState;

    /// <summary>It didn't load: "Couldn't load this person", the error's sentence, and Retry.</summary>
    public sealed record Failed(Exception Error) : PersonState;

    /// <summary>The page; <paramref name="Error"/>: the server couldn't be asked again, and what shows is what it said before.</summary>
    public sealed record Ready(PersonPage Page, Exception? Error = null) : PersonState;
}

/// <summary>An actor's or director's page: what they are in, in this server's libraries.</summary>
/// <param name="Photo">The photo's whole address; null without one (the circle shows <paramref name="Initials"/>).</param>
/// <param name="Initials">"Idris Vale" → "IV".</param>
/// <param name="Counts">"3 movies · 2 shows"; empty when there are none.</param>
/// <param name="Rows">A row per role: acting first, then directing, creating, writing, producing.</param>
public sealed record PersonPage(long Id, string Name, string? Photo, string Initials, string Counts, IReadOnlyList<PersonRow> Rows)
{
    /// <summary>"3 movies · 2 shows in your libraries" under the name; null without any.</summary>
    public string? CountLine => Counts.Length > 0 ? PersonText.InLibraries(Counts) : null;
}

/// <summary>
/// Titles of one role: posters captioned with the year and the part ("2019 · as Neo"). A tile
/// opens its item's page (<see cref="PersonRules.Opens"/>); holding OK opens its actions.
/// </summary>
/// <param name="Role">"actor", "director", "creator", "writer" or "producer".</param>
/// <param name="Title">"Acting", "Directing"…</param>
public sealed record PersonRow(string Role, string Title, IReadOnlyList<TileModel> Tiles)
{
    /// <summary>The tile the row draws: posters.</summary>
    public TileKind Kind => TileKind.Poster;
}

/// <summary>What a person's page reads: the app's state, or a fake in tests.</summary>
public interface IPersonSource
{
    /// <summary>The server's origin, which pictures are made whole against.</summary>
    string Origin { get; }

    Task<PersonDetail> Person(long id);

    /// <summary>Calls <paramref name="changed"/> when what pages show may have changed (the server's events).</summary>
    IDisposable OnChange(Action changed);
}

/// <summary>A person's page's source in the app: <see cref="AppState"/> and its API.</summary>
public sealed class AppPersonSource : IPersonSource
{
    readonly AppState app;

    public AppPersonSource(AppState app) => this.app = app;

    public string Origin => app.Server.Value ?? "";

    public Task<PersonDetail> Person(long id) => app.Api.Value is { } api ? api.Person(id) : Task.FromException<PersonDetail>(new SignedOutException());

    public IDisposable OnChange(Action changed) => AppChanges.Follow(app.Changes, changed);
}

/// <summary>Following a count the app bumps, for the sources' OnChange.</summary>
internal static class AppChanges
{
    /// <summary>Calls <paramref name="changed"/> on each change of the counts after now.</summary>
    public static IDisposable Follow(Signal<int> count, Action changed, Signal<int>? also = null)
    {
        // Watch calls at once with the count as it is: only the changes after count.
        var first = true;
        var one = count.Watch(_ =>
        {
            if (first) first = false;
            else changed();
        });
        if (also == null) return one;
        var firstToo = true;
        var two = also.Watch(_ =>
        {
            if (firstToo) firstToo = false;
            else changed();
        });
        return new Both(one, two);
    }

    sealed class Both : IDisposable
    {
        readonly IDisposable one, two;

        public Both(IDisposable one, IDisposable two)
        {
            this.one = one;
            this.two = two;
        }

        public void Dispose()
        {
            one.Dispose();
            two.Dispose();
        }
    }
}

/// <summary>A person's page's rules (the Android app's personPage and roleText).</summary>
public static class PersonRules
{
    /// <summary>The roles' rows, in their order.</summary>
    public static readonly IReadOnlyList<string> RoleOrder = new[] { "actor", "director", "creator", "writer", "producer" };

    static readonly Dictionary<string, string> RoleNames = new Dictionary<string, string>
    {
        ["actor"] = "Actor",
        ["creator"] = "Creator",
        ["director"] = "Director",
        ["writer"] = "Writer",
        ["producer"] = "Producer",
    };

    /// <summary>The page of <paramref name="person"/>, whose pictures are on <paramref name="origin"/>.</summary>
    public static PersonPage Page(PersonDetail person, string origin)
    {
        var seen = new HashSet<long>();
        var credits = person.Credits.Where(c => seen.Add(c.Item.Id)).ToList();
        var movies = credits.Count(c => c.Item.Kind == "movie");
        var shows = credits.Count(c => c.Item.Kind == "show");
        var counts = string.Join(" · ", new[]
        {
            movies > 0 ? Format.Pluralize(movies, "movie") : null,
            shows > 0 ? Format.Pluralize(shows, "show") : null,
        }.Where(s => s != null));
        var rows = new List<PersonRow>();
        foreach (var role in RoleOrder)
        {
            var titles = credits.Where(c => c.Roles.Any(r => r.Role == role)).ToList();
            if (titles.Count == 0) continue;
            rows.Add(new PersonRow(role, PersonText.RoleTitle(role), titles.Select(c =>
                TileModel.Of(c.Item, TileKind.Poster, origin) with { Subtitle = Subtitle(c.Item, c.Roles.Where(r => r.Role == role).ToList()) }).ToList()));
        }
        return new PersonPage(
            person.Id,
            person.Name,
            string.IsNullOrEmpty(person.PhotoUrl) ? null : Artwork.Absolute(person.PhotoUrl!, origin),
            Initials(person.Name),
            counts,
            rows);
    }

    /// <summary>Where a tile goes: its item's page.</summary>
    public static PageKey Opens(long itemId) => new PageKey.Item(itemId);

    /// <summary>"2019 · as Neo".</summary>
    public static string Subtitle(ItemSummary item, IReadOnlyList<PersonRole> roles) =>
        string.Join(" · ", new[] { item.Year?.ToString(CultureInfo.InvariantCulture) ?? "", RoleText(roles) }.Where(s => s.Length > 0));

    /// <summary>"as Neo", "Director", "Guest star (3 episodes)", as the web says it.</summary>
    public static string RoleText(IReadOnlyList<PersonRole> roles) => string.Join(", ", roles.Select(r =>
    {
        var episodes = r.Episodes ?? 0;
        var name = r.Role == "actor"
            ? !string.IsNullOrEmpty(r.Character) ? $"as {r.Character}" : episodes > 0 ? "Guest star" : "Actor"
            : RoleNames.TryGetValue(r.Role, out var n) ? n : "Crew";
        return episodes > 0 ? $"{name} ({Format.Pluralize(episodes, "episode")})" : name;
    }));

    /// <summary>"Idris Vale" → "IV": the photo's stand-in.</summary>
    public static string Initials(string name) =>
        string.Concat(name.Split(' ').Where(w => !string.IsNullOrWhiteSpace(w)).Take(2).Select(w => char.ToUpperInvariant(w[0])));
}

/// <summary>
/// A person's page (the Android app's PersonViewModel, as the TV uses it): the photo, the name
/// and what of theirs is in the libraries, then a row per role. It asks again when the app hears
/// that things changed.
/// </summary>
public sealed class PersonModel : IDisposable
{
    readonly IPersonSource source;
    readonly IScheduler scheduler;
    readonly IDisposable changes;
    PersonDetail? person;
    Exception? error;
    int asks;
    bool gone;

    public PersonModel(IPersonSource source, long personId, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
        PersonId = personId;
        changes = source.OnChange(Fetch);
        Fetch();
    }

    public long PersonId { get; }

    public Signal<PersonState> State { get; } = new Signal<PersonState>(new PersonState.Loading());

    public void Retry() => Fetch();

    public void Dispose()
    {
        gone = true;
        changes.Dispose();
    }

    void Fetch()
    {
        if (gone) return;
        var at = ++asks;
        Plays.When(scheduler, () => source.Person(PersonId), p =>
        {
            if (gone || at != asks) return;
            person = p;
            error = null;
            Compute();
        }, e =>
        {
            if (gone || at != asks) return;
            error = e;
            Compute();
        });
    }

    void Compute()
    {
        State.Value = person != null ? new PersonState.Ready(PersonRules.Page(person, source.Origin), error)
            : error is ApiException { Status: 404 } ? new PersonState.NotFound()
            : error != null ? new PersonState.Failed(error)
            : new PersonState.Loading();
    }
}

/// <summary>What the TV's person page says (the Android app's tv_person_* strings).</summary>
public static class PersonText
{
    public const string Acting = "Acting";
    public const string Directing = "Directing";
    public const string Creating = "Creating";
    public const string Writing = "Writing";
    public const string Producing = "Producing";
    public const string NotFound = "Person not found";
    public const string NotFoundBody = "No one by that link has anything in your libraries.";
    public const string Failed = "Couldn't load this person";

    /// <summary>"3 movies · 2 shows in your libraries".</summary>
    public static string InLibraries(string counts) => $"{counts} in your libraries";

    /// <summary>A role's row title.</summary>
    public static string RoleTitle(string role) => role switch
    {
        "director" => Directing,
        "creator" => Creating,
        "writer" => Writing,
        "producer" => Producing,
        _ => Acting,
    };

    /// <summary>The page's message in place of its rows: not found (Back), or failed (Retry); null otherwise.</summary>
    public static LibraryMessage? Message(PersonState state) => state switch
    {
        PersonState.NotFound => new LibraryMessage(NotFound, NotFoundBody, MessageAction.Back),
        PersonState.Failed f => new LibraryMessage(LibraryText.FailedTitle(f.Error, Failed), LibraryText.FailedBody(f.Error), MessageAction.Retry),
        _ => null,
    };
}
