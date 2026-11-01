using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>"Choose your server": the Android app's ServersViewModelTest.</summary>
public class ServersModelTests
{
    const string Nas = "http://192.168.1.20:7539";
    const string Office = "http://192.168.1.30:7539";

    static string Pley(string name) => $"{{\"name\":\"{name}\",\"version\":\"1.0\"}}";

    /// <summary>The list's source over a fake network: real checks against <see cref="Server"/>.</summary>
    sealed class FakeSource : IServersSource
    {
        public FakeServer Server { get; } = new FakeServer();
        public List<SavedServer> SavedList { get; } = new List<SavedServer>();
        public List<FoundServer> SearchFinds { get; } = new List<FoundServer>();
        public List<FoundServer> SweepFinds { get; } = new List<FoundServer>();
        public List<(string Url, string Name, string? Id)> Used { get; } = new List<(string, string, string?)>();
        public List<string> Forgotten { get; } = new List<string>();
        public HashSet<string> Tokens { get; } = new HashSet<string>();
        public int Household { get; set; } = 1;

        public IReadOnlyList<SavedServer> Saved() => SavedList.OrderByDescending(s => s.LastUsed).ToList();

        public Task Search(Action<FoundServer> onFound, CancellationToken stop)
        {
            foreach (var f in SearchFinds) onFound(f);
            return Task.CompletedTask;
        }

        public Task Sweep(Action<FoundServer> onFound, CancellationToken stop)
        {
            foreach (var f in SweepFinds) onFound(f);
            return Task.CompletedTask;
        }

        public string? Subnet() => "192.168.1.";

        public Task<ServerInfo> Check(string url) => Servers.Check(Server.Transport, url);

        public Task<ServerInfo> Connect(string text, string? goAheadAt) => Servers.Connect(Server.Transport, text, goAheadAt);

        public void Use(string url, string name, string? id)
        {
            Used.Add((url, name, id));
            SavedList.RemoveAll(s => s.Url == url);
            SavedList.Add(new SavedServer(url, name, 100 + Used.Count, id));
        }

        public Task Forget(string url)
        {
            Forgotten.Add(url);
            SavedList.RemoveAll(s => s.Url == url);
            return Task.CompletedTask;
        }

        public bool HasTokens(string url) => Tokens.Contains(url);

        public int HouseholdSize() => Household;

        public FakeSource Answer(string url, string name = "Living room NAS")
        {
            Server.On("GET", url + "/api/server", 200, Pley(name));
            return this;
        }
    }

    static ServersModel Started(FakeSource source)
    {
        var model = new ServersModel(source, new FakeScheduler());
        model.Start();
        return model;
    }

    [Fact]
    public void Saved_servers_come_first_then_the_ones_found_with_their_ports()
    {
        var source = new FakeSource().Answer(Nas);
        source.SavedList.Add(new SavedServer(Nas, "Living room NAS", 5, "a"));
        source.SearchFinds.AddRange(new[]
        {
            new FoundServer(Office, "Office", "b", "1.0"),
            new FoundServer("http://192.168.1.99:7539", "Living Room NAS", "c", "1.0"),
            new FoundServer(Nas, "Living room NAS", "a", "1.0"),
        });
        var state = Started(source).State.Value;
        Assert.Equal(new[] { Nas, Office, "http://192.168.1.99:7539" }, state.Rows.Select(r => r.Url));
        Assert.Equal("192.168.1.20:7539", state.Rows[0].Address);
        Assert.True(state.Rows[0].Saved && state.Rows[0].Found);
        // Named like the saved server, and it isn't: tagged.
        Assert.True(state.Rows[2].Lookalike);
        Assert.False(state.Rows[1].Lookalike);
        Assert.True(state.Searched);
        Assert.False(state.Searching);
        Assert.Equal("192.168.1.1-254", state.SweepRange);
        Assert.Equal("192.168.1.", state.Subnet);
    }

    [Fact]
    public void A_saved_server_that_doesnt_answer_says_so()
    {
        var source = new FakeSource();
        source.SavedList.Add(new SavedServer(Nas, "Living room NAS", 1));
        Assert.True(Started(source).State.Value.Rows.Single().Unreachable);
    }

    [Fact]
    public async Task An_unencrypted_address_on_the_internet_is_warned_about_once()
    {
        var source = new FakeSource().Answer("http://pley.example.com:7539", "Remote");
        var model = Started(source);
        ChosenServer? chosen = null;
        model.Chosen += c => chosen = c;
        await model.Connect("http://pley.example.com:7539");
        Assert.IsType<ConnectProblem.Unencrypted>(model.State.Value.AddressProblem);
        Assert.Equal("pley.example.com:7539", ((ConnectProblem.Unencrypted)model.State.Value.AddressProblem!).Address);
        Assert.Empty(source.Used);
        // Connect again: it goes ahead.
        await model.Connect("http://pley.example.com:7539");
        Assert.Null(model.State.Value.AddressProblem);
        Assert.Equal("http://pley.example.com:7539", source.Used.Single().Url);
        Assert.True(model.State.Value.Rows.Single().Unencrypted);
        // Typed in: listed, but not "on this network".
        Assert.False(model.State.Value.Rows.Single().Found);
        Assert.NotNull(chosen);
    }

    [Fact]
    public async Task What_isnt_an_address_or_isnt_Pley_says_so()
    {
        var source = new FakeSource();
        source.Server.On("GET", "http://router:7539/api/server", 404);
        var model = Started(source);
        await model.Connect("not an address");
        Assert.Equal(new ConnectProblem.BadAddress(), model.State.Value.AddressProblem);
        await model.Connect("router");
        Assert.Equal(new ConnectProblem.NotPley("router:7539"), model.State.Value.AddressProblem);
        await model.Connect("192.168.1.77");
        Assert.Equal(new ConnectProblem.Unreachable("192.168.1.77:7539", "nothing answers there"), model.State.Value.AddressProblem);
        Assert.False(model.State.Value.CheckingAddress);
        // Typing again clears it.
        model.AddressEdited();
        Assert.Null(model.State.Value.AddressProblem);
    }

    [Fact]
    public async Task Choosing_saves_the_server_and_says_where_to_go()
    {
        var source = new FakeSource().Answer(Nas);
        source.SearchFinds.Add(new FoundServer(Nas, "Living room NAS", "a", "1.0"));
        var model = Started(source);
        var chosen = new List<ChosenServer>();
        model.Chosen += chosen.Add;
        await model.Choose(Nas);
        Assert.Equal((Nas, "Living room NAS", (string?)"a"), source.Used.Single());
        Assert.Equal(new ChosenServer(Nas, "Living room NAS", SignedIn: false, Profiles: 1), chosen[0]);
        Assert.Equal(new PageKey.Link(Nas), chosen[0].Next);
        Assert.Null(model.State.Value.Busy);
        // Signed in there already, with a household of four.
        source.Tokens.Add(Nas);
        source.Household = 4;
        await model.Choose(Nas);
        Assert.Equal(new ChosenServer(Nas, "Living room NAS", SignedIn: true, Profiles: 4), chosen[1]);
        Assert.Equal(new PageKey.Profiles(), chosen[1].Next);
        source.Household = 1;
        await model.Choose(Nas);
        Assert.Equal(new PageKey.Home(), chosen[2].Next);
    }

    [Fact]
    public async Task A_row_that_doesnt_answer_says_why_and_is_marked()
    {
        var source = new FakeSource();
        source.SearchFinds.Add(new FoundServer(Office, "Office", null, "1.0"));
        var model = Started(source);
        await model.Choose(Office);
        Assert.Equal(new ConnectProblem.Unreachable("192.168.1.30:7539", "nothing answers there"), model.State.Value.Problem);
        Assert.Equal("Couldn't reach 192.168.1.30:7539: nothing answers there", model.State.Value.Problem!.Text);
        Assert.Null(model.State.Value.Busy);
    }

    [Fact]
    public async Task Forgetting_a_saved_server_ends_its_sign_in()
    {
        var source = new FakeSource().Answer(Nas);
        source.SavedList.Add(new SavedServer(Nas, "Living room NAS", 1));
        var model = Started(source);
        await model.Forget(Nas);
        Assert.Equal(new[] { Nas }, source.Forgotten);
        Assert.Empty(model.State.Value.Rows);
    }

    [Fact]
    public async Task Search_this_network_adds_what_answers()
    {
        var source = new FakeSource();
        source.SweepFinds.Add(new FoundServer("http://192.168.1.50:7539", "Docker", null, "1.0"));
        var model = Started(source);
        await model.SearchNetwork();
        Assert.False(model.State.Value.Sweeping);
        Assert.Equal("Docker", model.State.Value.Rows.Single().Name);
        Assert.True(model.State.Value.Rows.Single().Found);
    }

    [Fact]
    public void Answers_after_the_screen_went_are_dropped()
    {
        var source = new FakeSource();
        var model = new ServersModel(source, new FakeScheduler());
        model.Dispose();
        source.SearchFinds.Add(new FoundServer(Office, "Office", null, "1.0"));
        model.Start();
        Assert.Empty(model.State.Value.Rows);
    }
}
