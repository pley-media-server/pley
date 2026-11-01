using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;
using static Pley.Tv.Core.Tests.PagesKit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// A person's page: the Android app's personPage and roleText, which it has no test of its own
/// for; these say what TvPersonScreen shows.
/// </summary>
public class PersonModelTests
{
    readonly FakePeople people = new FakePeople();
    readonly FakeScheduler scheduler = new FakeScheduler();

    static PersonCredit Credit(ItemSummary item, params PersonRole[] roles) => new PersonCredit { Item = item, Roles = roles };

    static PersonRole Role(string role, string? character = null, int? episodes = null) => new PersonRole { Role = role, Character = character, Episodes = episodes };

    static readonly PersonDetail Lena = new PersonDetail
    {
        Id = 70,
        Name = "Lena  Okafor",
        PhotoUrl = "/api/people/70/photo",
        Credits = new[]
        {
            Credit(Film(1, "Northern Lights", 2019), Role("actor", "Neo"), Role("director")),
            Credit(Item(2, "show", "Severance") with { Year = 2022 }, Role("actor", episodes: 3)),
            Credit(Film(3, "Low Orbit", 2021), Role("writer")),
            // A title credited twice counts once.
            Credit(Film(1, "Northern Lights", 2019), Role("producer")),
        },
    };

    [Fact]
    public void A_person_has_a_row_per_role_captioned_with_the_year_and_the_part()
    {
        people.Person = Lena;
        var model = new PersonModel(people, 70, scheduler);
        var page = Assert.IsType<PersonState.Ready>(model.State.Value).Page;
        Assert.Equal("LO", page.Initials);
        Assert.Equal("http://pley/api/people/70/photo", page.Photo);
        Assert.Equal("2 movies · 1 show", page.Counts);
        Assert.Equal("2 movies · 1 show in your libraries", page.CountLine);
        Assert.Equal(new[] { "Acting", "Directing", "Writing" }, page.Rows.Select(r => r.Title));
        var acting = page.Rows[0];
        Assert.Equal(new[] { "Northern Lights", "Severance" }, acting.Tiles.Select(t => t.Title));
        Assert.Equal(new[] { "2019 · as Neo", "2022 · Guest star (3 episodes)" }, acting.Tiles.Select(t => t.Subtitle));
        Assert.Equal("2019 · Director", page.Rows[1].Tiles.Single().Subtitle);
        Assert.Equal(new PageKey.Item(3), PersonRules.Opens(page.Rows[2].Tiles.Single().Id));
    }

    [Fact]
    public void Roles_read_as_the_web_says_them()
    {
        Assert.Equal("as Neo, Director", PersonRules.RoleText(new[] { Role("actor", "Neo"), Role("director") }));
        Assert.Equal("Actor", PersonRules.RoleText(new[] { Role("actor") }));
        Assert.Equal("as Mark (1 episode)", PersonRules.RoleText(new[] { Role("actor", "Mark", episodes: 1) }));
        Assert.Equal("Crew", PersonRules.RoleText(new[] { Role("gaffer") }));
    }

    [Fact]
    public void Without_a_photo_the_circle_shows_initials()
    {
        people.Person = Lena with { PhotoUrl = null, Credits = new PersonCredit[0] };
        var page = Assert.IsType<PersonState.Ready>(new PersonModel(people, 70, scheduler).State.Value).Page;
        Assert.Null(page.Photo);
        Assert.Null(page.CountLine);
        Assert.Empty(page.Rows);
    }

    [Fact]
    public void Someone_this_profile_may_not_see_is_not_found()
    {
        people.Failure = new ApiException(404, "not_found", "no such person");
        var model = new PersonModel(people, 70, scheduler);
        Assert.IsType<PersonState.NotFound>(model.State.Value);
        var message = PersonText.Message(model.State.Value)!;
        Assert.Equal("Person not found", message.Title);
        Assert.Equal(MessageAction.Back, message.Action);
        Assert.Equal("Back", message.ActionLabel);
    }

    [Fact]
    public void A_failure_offers_retry_and_retry_asks_again()
    {
        people.Failure = new ApiException(500, "internal", "the database is locked");
        var model = new PersonModel(people, 70, scheduler);
        var message = PersonText.Message(model.State.Value)!;
        Assert.Equal("Couldn't load this person", message.Title);
        Assert.Equal("The database is locked.", message.Body);
        people.Failure = null;
        people.Person = Lena;
        model.Retry();
        Assert.IsType<PersonState.Ready>(model.State.Value);
        // A later failure keeps the page, and says so.
        people.Failure = new ApiException(500, "internal", "again");
        people.Changed();
        Assert.NotNull(Assert.IsType<PersonState.Ready>(model.State.Value).Error);
        Assert.Equal(3, people.Asks);
    }
}
