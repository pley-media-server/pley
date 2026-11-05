using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Ui;

/// <summary>One server in the list: saved, found on the network, or both.</summary>
/// <param name="Address">Its host and port, as people type it ("192.168.1.20:7539"), never the scheme.</param>
/// <param name="Lookalike">Found, not saved, and named like a saved server it isn't.</param>
/// <param name="Unencrypted">Plain http beyond the home network.</param>
/// <param name="Unreachable">A saved server that didn't answer.</param>
public sealed record ServerRow(
    string Url,
    string Name,
    string Address,
    bool Saved,
    bool Found,
    bool Lookalike = false,
    bool Unencrypted = false,
    bool Unreachable = false,
    string? Id = null);

/// <summary>What "Choose your server" shows (the Android app's ServersUiState).</summary>
public sealed record ServersState
{
    /// <summary>Saved servers first, most recently used first, then the ones found.</summary>
    public IReadOnlyList<ServerRow> Rows { get; init; } = Array.Empty<ServerRow>();
    /// <summary>Asking the network who's there (1.5 s).</summary>
    public bool Searching { get; init; }
    /// <summary>A search ended at least once.</summary>
    public bool Searched { get; init; }
    /// <summary>"Search this network" is running: every address of the TV's network is asked.</summary>
    public bool Sweeping { get; init; }
    /// <summary>The addresses a sweep asks: "192.168.1.1-254"; null when the network isn't known.</summary>
    public string? SweepRange { get; init; }
    /// <summary>The server being opened.</summary>
    public string? Busy { get; init; }
    public ConnectProblem? Problem { get; init; }
    /// <summary>About the typed address.</summary>
    public ConnectProblem? AddressProblem { get; init; }
    public bool CheckingAddress { get; init; }
}

/// <summary>A server was chosen and saved as the one in use.</summary>
/// <param name="SignedIn">The TV holds tokens for it already.</param>
/// <param name="Profiles">Its household's size as last seen; 1 while unknown.</param>
public sealed record ChosenServer(string Url, string Name, bool SignedIn, int Profiles)
{
    /// <summary>Where the app goes next: signing in, Who's watching?, or Home.</summary>
    public PageKey Next => !SignedIn ? new PageKey.Link(Url) : Profiles > 1 ? new PageKey.Profiles() : new PageKey.Home();
}

/// <summary>What the server list reads and changes: the app's state and the network, or a fake in tests.</summary>
public interface IServersSource
{
    /// <summary>The servers this TV chose, most recently used first.</summary>
    IReadOnlyList<SavedServer> Saved();

    /// <summary>Asks the network who's there (UDP); <paramref name="onFound"/> hears each, on any thread.</summary>
    Task Search(Action<FoundServer> onFound, CancellationToken stop);

    /// <summary>Asks every address of the TV's network; <paramref name="onFound"/> hears each, on any thread.</summary>
    Task Sweep(Action<FoundServer> onFound, CancellationToken stop);

    /// <summary>The TV's network's first three numbers and a dot ("192.168.1."); null when unknown.</summary>
    string? Subnet();

    /// <summary>A known server answers as Pley (<see cref="Servers.Check"/>); throws <see cref="ConnectException"/>.</summary>
    Task<ServerInfo> Check(string url);

    /// <summary>What was typed is a Pley server (<see cref="Servers.Connect"/>); throws <see cref="ConnectException"/>.</summary>
    Task<ServerInfo> Connect(string text, string? goAheadAt);

    /// <summary>Saves the server as the one in use.</summary>
    void Use(string url, string name, string? id);

    /// <summary>Forgets a saved server: its sign-in ends, and what was saved from it goes.</summary>
    Task Forget(string url);

    bool HasTokens(string url);

    /// <summary>The household of the server in use, as last seen; 1 while unknown.</summary>
    int HouseholdSize();
}

/// <summary>The server list's source in the app: <see cref="AppState"/>, discovery and the sweep.</summary>
public sealed class AppServersSource : IServersSource
{
    readonly AppState app;

    public AppServersSource(AppState app) => this.app = app;

    public IReadOnlyList<SavedServer> Saved() => app.SavedServers();

    public Task Search(Action<FoundServer> onFound, CancellationToken stop) => Discovery.Search(onFound, stop);

    public Task Sweep(Action<FoundServer> onFound, CancellationToken stop) => Servers.Sweep(app.Http, onFound, stop);

    public string? Subnet()
    {
        var lan = Lans.Current().FirstOrDefault();
        if (lan == null) return null;
        var b = lan.Address.GetAddressBytes();
        return $"{b[0]}.{b[1]}.{b[2]}.";
    }

    public Task<ServerInfo> Check(string url) => Heard(Servers.Check(app.Http, url));

    public Task<ServerInfo> Connect(string text, string? goAheadAt) => Heard(Servers.Connect(app.Http, text, goAheadAt));

    /// <summary>The server's answer, its language heard by the app: a server chosen signs in in it.</summary>
    async Task<ServerInfo> Heard(Task<ServerInfo> asked)
    {
        var info = await asked;
        app.HeardServer(info.Url, info.Language);
        return info;
    }

    public void Use(string url, string name, string? id)
    {
        app.RememberServer(url, name, id);
        app.Use(url);
    }

    public Task Forget(string url) => app.Forget(url);

    public bool HasTokens(string url) => app.HasTokens(url);

    public int HouseholdSize() => app.HouseholdSize();
}

/// <summary>
/// "Choose your server" (the Android app's ServersViewModel): saved servers first, then the ones
/// that answer on the network as they come; an address to type (a name on the internet is tried
/// with https first); "Search this network"; forgetting a saved server, which ends its sign-in.
/// Used on the UI thread: what the network says comes back there.
/// </summary>
public sealed class ServersModel : IDisposable
{
    readonly IServersSource source;
    readonly IScheduler scheduler;
    readonly CancellationTokenSource gone = new CancellationTokenSource();
    IReadOnlyList<SavedServer> saved = Array.Empty<SavedServer>();
    readonly Dictionary<string, FoundServer> found = new Dictionary<string, FoundServer>();
    readonly List<string> foundOrder = new List<string>();
    readonly HashSet<string> unreachable = new HashSet<string>();
    /// <summary>Servers typed in: listed, but not "on this network".</summary>
    readonly HashSet<string> typed = new HashSet<string>();
    /// <summary>Saved servers being asked directly.</summary>
    readonly HashSet<string> checking = new HashSet<string>();
    /// <summary>The unencrypted address the person was warned about: connecting again goes ahead.</summary>
    string? warnedAbout;

    public ServersModel(IServersSource source, IScheduler scheduler)
    {
        this.source = source;
        this.scheduler = scheduler;
    }

    public Signal<ServersState> State { get; } = new Signal<ServersState>(new ServersState());

    /// <summary>A server was chosen: the screen goes on to sign in, or in.</summary>
    public event Action<ChosenServer>? Chosen;

    /// <summary>Shows the saved servers, asks them and the network who's there.</summary>
    public void Start()
    {
        string? subnet = null;
        try
        {
            subnet = source.Subnet();
        }
        catch (Exception)
        {
            // The network's list can't be read: checking the network shows no range.
        }
        Update(s => s with { SweepRange = subnet == null ? null : subnet + "1-254" });
        SavedChanged();
        _ = Search();
    }

    /// <summary>Asks the network again who's there.</summary>
    public async Task Search()
    {
        if (State.Value.Searching) return;
        Update(s => s with { Searching = true });
        try
        {
            await source.Search(server => scheduler.Post(() => Found(server)), gone.Token);
        }
        catch (Exception)
        {
            // No network: nothing found.
        }
        if (!gone.IsCancellationRequested) Update(s => s with { Searching = false, Searched = true });
    }

    /// <summary>"Search this network": every address of the TV's network, for servers that never hear the broadcast.</summary>
    public async Task SearchNetwork()
    {
        if (State.Value.Sweeping) return;
        Update(s => s with { Sweeping = true });
        try
        {
            await source.Sweep(server => scheduler.Post(() => Found(server)), gone.Token);
        }
        catch (Exception)
        {
            // No network: nothing found.
        }
        if (!gone.IsCancellationRequested) Update(s => s with { Sweeping = false, Searched = true });
    }

    /// <summary>Opens a row's server: checks it answers as Pley, saves it as the one in use, and says so (<see cref="Chosen"/>).</summary>
    public async Task Choose(string url)
    {
        if (State.Value.Busy != null) return;
        Update(s => s with { Busy = url, Problem = null });
        try
        {
            var info = await source.Check(url);
            if (gone.IsCancellationRequested) return;
            Keep(info.Url, info.Name, IdOf(url));
        }
        catch (ConnectException e)
        {
            if (gone.IsCancellationRequested) return;
            if (e.Problem is ConnectProblem.Unreachable)
            {
                unreachable.Add(url);
                Publish();
            }
            Update(s => s with { Problem = e.Problem });
        }
        finally
        {
            if (!gone.IsCancellationRequested) Update(s => s with { Busy = null });
        }
    }

    /// <summary>
    /// Connects to what was typed: <c>nas</c>, <c>192.168.1.20</c>, <c>pley.example.com</c> (https
    /// first) or a full address, and opens it. Plain http beyond the home network is warned about
    /// once; connecting again goes ahead.
    /// </summary>
    public async Task Connect(string text)
    {
        if (State.Value.CheckingAddress) return;
        if (Addresses.Candidates(text).Count == 0)
        {
            Update(s => s with { AddressProblem = new ConnectProblem.BadAddress() });
            return;
        }
        Update(s => s with { CheckingAddress = true, AddressProblem = null });
        try
        {
            var info = await source.Connect(text, warnedAbout);
            if (gone.IsCancellationRequested) return;
            Remember(new FoundServer(info.Url, info.Name, null, info.Version));
            typed.Add(info.Url);
            Publish();
            Keep(info.Url, info.Name, IdOf(info.Url));
        }
        catch (ConnectException e)
        {
            if (gone.IsCancellationRequested) return;
            if (e.Problem is ConnectProblem.Unencrypted u) warnedAbout = u.Url;
            Update(s => s with { AddressProblem = e.Problem });
        }
        finally
        {
            if (!gone.IsCancellationRequested) Update(s => s with { CheckingAddress = false });
        }
    }

    /// <summary>The typed address changed: its problem goes.</summary>
    public void AddressEdited()
    {
        if (State.Value.AddressProblem != null) Update(s => s with { AddressProblem = null });
    }

    /// <summary>Forgets the saved server <paramref name="url"/>: its sign-in is ended there, and what was saved from it deleted.</summary>
    public async Task Forget(string url)
    {
        try
        {
            await source.Forget(url);
        }
        catch (Exception)
        {
            // The tokens are gone whatever the server answered.
        }
        if (!gone.IsCancellationRequested) SavedChanged();
    }

    /// <summary>The screen went: answers still coming are dropped.</summary>
    public void Dispose() => gone.Cancel();

    void Found(FoundServer server)
    {
        if (gone.IsCancellationRequested) return;
        Remember(server);
        typed.Remove(server.Url);
        unreachable.Remove(server.Url);
        Publish();
    }

    void Remember(FoundServer server)
    {
        if (!found.ContainsKey(server.Url)) foundOrder.Add(server.Url);
        found[server.Url] = server;
        unreachable.Remove(server.Url);
    }

    void Keep(string url, string name, string? id)
    {
        source.Use(url, name, id);
        SavedChanged();
        Chosen?.Invoke(new ChosenServer(url, name, source.HasTokens(url), Math.Max(1, source.HouseholdSize())));
    }

    void SavedChanged()
    {
        saved = source.Saved();
        Publish();
        CheckSaved();
    }

    /// <summary>Saved servers that didn't answer the search are asked directly.</summary>
    void CheckSaved()
    {
        foreach (var server in saved)
        {
            if (found.ContainsKey(server.Url) || !checking.Add(server.Url)) continue;
            _ = Ask(server.Url);
        }
    }

    async Task Ask(string url)
    {
        bool answers;
        try
        {
            await source.Check(url);
            answers = true;
        }
        catch (Exception)
        {
            answers = false;
        }
        if (gone.IsCancellationRequested) return;
        checking.Remove(url);
        if (answers) unreachable.Remove(url);
        else unreachable.Add(url);
        Publish();
    }

    string? IdOf(string url) => (found.TryGetValue(url, out var f) ? f.Id : null) ?? saved.FirstOrDefault(s => s.Url == url)?.Id;

    void Publish()
    {
        var rows = Servers.Merge(saved, foundOrder.Select(u => found[u])).Select(e => new ServerRow(
            e.Url,
            e.Name,
            Servers.HostPort(e.Url),
            e.Saved,
            e.Found && !typed.Contains(e.Url),
            e.Lookalike,
            Addresses.IsUnencrypted(e.Url),
            !e.Found && unreachable.Contains(e.Url),
            e.Id)).ToList();
        Update(s => s with { Rows = rows });
    }

    void Update(Func<ServersState, ServersState> change) => State.Value = change(State.Value);
}
