using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Net;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Finding a server: typed, searched for, or heard by broadcast.</summary>
public class ServersTests
{
    const string Pley = @"{""name"":""Home"",""version"":""0.1.0""}";

    static Dictionary<string, string> Redirect(string to) => new Dictionary<string, string> { ["Location"] = to };

    [Fact]
    public void Names_servers_by_host_and_port()
    {
        Assert.Equal("router:7539", Servers.HostPort("http://router:7539"));
        Assert.Equal("pley.example.com", Servers.HostPort("https://pley.example.com"));
    }

    [Fact]
    public async Task Tries_https_first_for_a_name_on_the_internet_then_pleys_port()
    {
        var server = new FakeServer().On("GET", "http://pley.example.com:7539/api/server", 200, Pley);
        var info = await Servers.Connect(server.Transport, "pley.example.com", goAheadAt: "http://pley.example.com:7539");
        Assert.Equal(new ServerInfo("http://pley.example.com:7539", "Home", "0.1.0"), info);
        Assert.Equal(new[] { "https://pley.example.com/api/server", "http://pley.example.com:7539/api/server" }, server.Requests.Select(r => r.Url));
        // Home addresses get 3 s to accept a connection, names on the internet 10 s; neither follows a redirect.
        Assert.All(server.Requests, r => Assert.Equal((10_000, 10_000, false), (r.ConnectTimeoutMs!.Value, r.TimeoutMs, r.FollowRedirects)));
        await Servers.Check(new FakeServer().On("GET", "http://nas:7539/api/server", 200, Pley).Transport, "http://nas:7539");
    }

    [Fact]
    public async Task Says_what_answered_when_it_isnt_pley_and_why_nothing_did()
    {
        var notPley = await Assert.ThrowsAsync<ConnectException>(() => Servers.Connect(new FakeServer().On("GET", "http://router:7539/api/server", 404).Transport, "router"));
        Assert.Equal(new ConnectProblem.NotPley("router:7539"), notPley.Problem);
        var nothing = await Assert.ThrowsAsync<ConnectException>(() => Servers.Connect(new FakeServer().Transport, "192.168.1.77"));
        Assert.Equal(new ConnectProblem.Unreachable("192.168.1.77:7539", "nothing answers there"), nothing.Problem);
        Assert.Equal("Couldn't reach 192.168.1.77:7539: nothing answers there", nothing.Message);
        var slow = await Assert.ThrowsAsync<ConnectException>(() => Servers.Connect(new FakeServer().On("GET", "http://nas:7539/api/server", 0).Transport, "nas"));
        Assert.Equal(new ConnectProblem.Unreachable("nas:7539", "no answer in time"), slow.Problem);
        var bad = await Assert.ThrowsAsync<ConnectException>(() => Servers.Connect(new FakeServer().Transport, "not an address"));
        Assert.IsType<ConnectProblem.BadAddress>(bad.Problem);
        Assert.Equal("Enter an address such as nas, 192.168.1.20 or pley.example.com.", bad.Message);
    }

    [Theory]
    [InlineData(@"{""version"":""0.1.0""}")]
    [InlineData(@"{""name"":""Home"",""version"":""  ""}")]
    [InlineData(@"{""name"":""Home""}")]
    [InlineData("<html>")]
    public async Task Is_pley_only_with_a_name_and_a_version(string body)
    {
        var e = await Assert.ThrowsAsync<ConnectException>(() => Servers.Check(new FakeServer().On("GET", "http://nas:7539/api/server", 200, body).Transport, "http://nas:7539"));
        Assert.IsType<ConnectProblem.NotPley>(e.Problem);
    }

    [Theory]
    [InlineData(@"{""name"":""Home"",""version"":""0.1.0"",""two_step_required"":true}", true)]
    [InlineData(@"{""name"":""Home"",""version"":""0.1.0"",""two_step_required"":false}", false)]
    // A server from before it says nothing of it; an odd value is off, never a server that isn't Pley.
    [InlineData(@"{""name"":""Home"",""version"":""0.1.0""}", false)]
    [InlineData(@"{""name"":""Home"",""version"":""0.1.0"",""two_step_required"":""yes""}", false)]
    [InlineData(@"{""name"":""Home"",""version"":""0.1.0"",""two_step_required"":1}", false)]
    public async Task Reads_whether_the_server_requires_two_step_sign_in(string body, bool required)
    {
        var server = new FakeServer().On("GET", "http://nas:7539/api/server", 200, body);
        Assert.Equal(required, (await Servers.Probe(server.Transport, "http://nas:7539"))!.TwoStepRequired);
    }

    [Fact]
    public async Task Takes_any_success_and_names_a_server_without_a_name_by_its_address()
    {
        var server = new FakeServer().On("GET", "http://nas:7539/api/server", 203, @"{""name"":""  "",""version"":""0.1.0""}");
        Assert.Equal("nas", (await Servers.Check(server.Transport, "http://nas:7539")).Name);
    }

    [Fact]
    public async Task Follows_a_redirect_to_https_on_the_same_host_only()
    {
        var upgraded = new FakeServer()
            .On("GET", "http://media.example.org:7539/api/server", 301, null, Redirect("https://media.example.org/api/server"))
            .On("GET", "https://media.example.org/api/server", 200, Pley);
        Assert.Equal("https://media.example.org", (await Servers.Connect(upgraded.Transport, "http://media.example.org:7539")).Url);

        var elsewhere = new FakeServer().On("GET", "http://nas:7539/api/server", 302, null, Redirect("https://login.example.com/"));
        var e = await Assert.ThrowsAsync<ConnectException>(() => Servers.Connect(elsewhere.Transport, "nas"));
        Assert.Equal(new ConnectProblem.NotPley("nas:7539"), e.Problem);
    }

    [Fact]
    public async Task Warns_once_about_plain_http_beyond_the_home_network()
    {
        var server = new FakeServer().On("GET", "http://pley.example.com:8080/api/server", 200, Pley);
        var e = await Assert.ThrowsAsync<ConnectException>(() => Servers.Connect(server.Transport, "pley.example.com:8080"));
        var warning = Assert.IsType<ConnectProblem.Unencrypted>(e.Problem);
        Assert.Equal("pley.example.com:8080 isn't encrypted: what you watch could be read on the way. Use its https address if it has one, or press Connect again to go ahead anyway.", e.Message);
        // Connect again: it goes ahead.
        Assert.Equal("Home", (await Servers.Connect(server.Transport, "pley.example.com:8080", warning.Url)).Name);
        // At home, plain http is how Pley is reached.
        Assert.Equal("Home", (await Servers.Connect(new FakeServer().On("GET", "http://nas:7539/api/server", 200, Pley).Transport, "nas")).Name);
    }

    [Fact]
    public void Searches_the_TVs_slash_24_or_its_smaller_network_but_not_the_TV()
    {
        var home = new LanAddress(IPAddress.Parse("192.168.1.20"), 24);
        var neighbours = home.Neighbours().Select(a => a.ToString()).ToList();
        Assert.Equal(253, neighbours.Count);
        Assert.Equal(("192.168.1.1", "192.168.1.254"), (neighbours.First(), neighbours.Last()));
        Assert.DoesNotContain("192.168.1.20", neighbours);
        Assert.Equal(IPAddress.Parse("192.168.1.255"), home.Broadcast);
        // A bigger network by its /24 only; a smaller one within itself.
        Assert.Equal(253, new LanAddress(IPAddress.Parse("10.1.2.3"), 16).Neighbours().Count);
        Assert.Equal(IPAddress.Parse("10.1.255.255"), new LanAddress(IPAddress.Parse("10.1.2.3"), 16).Broadcast);
        var small = new LanAddress(IPAddress.Parse("192.168.1.70"), 26).Neighbours().Select(a => a.ToString()).ToList();
        Assert.Equal(61, small.Count);
        Assert.Equal(("192.168.1.65", "192.168.1.126"), (small.First(), small.Last()));
    }

    [Fact]
    public async Task Sweeps_the_TVs_network_at_pleys_port_and_reports_what_answers()
    {
        var server = new FakeServer()
            .On("GET", "http://192.168.1.20:7539/api/server", 200, Pley)
            .On("GET", "http://192.168.1.30:7539/api/server", 200, "<html>")
            .On("GET", "http://192.168.1.40:7539/api/server", 302, null, Redirect("https://192.168.1.40/"));
        var found = new ConcurrentBag<FoundServer>();
        await Servers.Sweep(server.Transport, new[] { new LanAddress(IPAddress.Parse("192.168.1.9"), 24) }, found.Add, CancellationToken.None);
        Assert.Equal(new[] { new FoundServer("http://192.168.1.20:7539", "Home", null, "0.1.0") }, found);
        Assert.Equal(253, server.Requests.Count);
        Assert.DoesNotContain(server.Requests, r => r.Url == "http://192.168.1.9:7539/api/server");
        Assert.All(server.Requests, r => Assert.Equal((Servers.SweepConnectMs, Servers.SweepReadMs), (r.ConnectTimeoutMs!.Value, r.TimeoutMs)));
    }

    [Fact]
    public void Reads_a_broadcasts_answer()
    {
        var from = new IPEndPoint(IPAddress.Parse("192.168.1.20"), 7539);
        byte[] Bytes(string s) => Encoding.UTF8.GetBytes(s);
        Assert.Equal(new FoundServer("http://192.168.1.20:7539", "Home", "abc", "0.1.0"),
            Discovery.ReadAnswer(Bytes(@"{""pley"":1,""port"":7539,""name"":""Home"",""id"":""abc"",""version"":""0.1.0""}"), from));
        Assert.Equal(new FoundServer("http://[fe80::1]:8080", "[fe80::1]", null, ""),
            Discovery.ReadAnswer(Bytes(@"{""pley"":1,""port"":8080}"), new IPEndPoint(IPAddress.Parse("fe80::1%3"), 7539)));
        Assert.Null(Discovery.ReadAnswer(Bytes(@"{""pley"":2,""port"":7539}"), from));
        Assert.Null(Discovery.ReadAnswer(Bytes(@"{""pley"":1,""port"":0}"), from));
        Assert.Null(Discovery.ReadAnswer(Bytes("pley?"), from));
    }

    /// <summary>A socket that hears what a test says, once the request has gone out.</summary>
    sealed class FakePort : IDatagramPort
    {
        readonly Queue<(string Text, string From)> answers;

        public FakePort(params (string Text, string From)[] answers) => this.answers = new Queue<(string, string)>(answers);

        public ConcurrentQueue<IPEndPoint> Sent { get; } = new ConcurrentQueue<IPEndPoint>();

        /// <summary>Hears how many requests have gone out, after each.</summary>
        public Action<int>? OnSent { get; set; }

        public void Send(byte[] payload, IPEndPoint to)
        {
            Assert.Equal("pley?", Encoding.ASCII.GetString(payload));
            Sent.Enqueue(to);
            OnSent?.Invoke(Sent.Count);
        }

        public async Task<(byte[] Bytes, IPEndPoint From)> Receive(CancellationToken cancel)
        {
            if (answers.Count == 0) await Task.Delay(Timeout.Infinite, cancel);
            var (text, from) = answers.Dequeue();
            return (Encoding.UTF8.GetBytes(text), new IPEndPoint(IPAddress.Parse(from), 7539));
        }

        public void Dispose()
        {
        }
    }

    [Fact]
    public async Task Broadcasts_three_times_and_hears_each_server_once()
    {
        var port = new FakePort(
            (@"{""pley"":1,""port"":7539,""name"":""Zed"",""id"":""a"",""version"":""1""}", "192.168.1.20"),
            (@"{""pley"":1,""port"":7539,""name"":""Zed"",""id"":""a"",""version"":""1""}", "10.0.0.20"),
            (@"{""pley"":1,""port"":7539,""name"":""alpha"",""version"":""1""}", "192.168.1.30"));
        var heard = new ConcurrentQueue<FoundServer>();
        // The window ends once the third round has gone out, not on the clock: a busy machine
        // running the tests may take longer than the app's 1.5 s to send all three.
        using var done = new CancellationTokenSource();
        port.OnSent = n =>
        {
            if (n == 6) done.Cancel();
        };
        var found = await Discovery.Search(heard.Enqueue, done.Token, new[] { new LanAddress(IPAddress.Parse("192.168.1.9"), 24) }, () => port, windowMs: 60_000);
        Assert.Equal(new[] { "alpha", "Zed" }, found.Select(f => f.Name));
        Assert.Equal(2, heard.Count);
        Assert.Equal(new[] { "192.168.1.255", "255.255.255.255" }, port.Sent.Select(s => s.Address.ToString()).Distinct());
        Assert.Equal(6, port.Sent.Count);
        Assert.All(port.Sent, s => Assert.Equal(7539, s.Port));
    }

    [Fact]
    public void Lists_saved_servers_first_then_the_ones_found_by_id_or_address()
    {
        var saved = new[]
        {
            new SavedServer("http://192.168.1.20:7539", "Home", 2, "a"),
            new SavedServer("https://pley.example.com", "Office", 5),
        };
        var found = new[]
        {
            new FoundServer("http://192.168.1.21:7539", "Home", "a", "1"),
            new FoundServer("http://192.168.1.30:7539", "home", "b", "1"),
            new FoundServer("http://192.168.1.40:7539", "Garage", null, "1"),
        };
        Assert.Equal(new[]
        {
            new ServerEntry("https://pley.example.com", "Office", null, Saved: true, Found: false),
            new ServerEntry("http://192.168.1.20:7539", "Home", "a", Saved: true, Found: true),
            new ServerEntry("http://192.168.1.30:7539", "home", "b", Saved: false, Found: true, Lookalike: true),
            new ServerEntry("http://192.168.1.40:7539", "Garage", null, Saved: false, Found: true),
        }, Servers.Merge(saved, found));
    }
}
