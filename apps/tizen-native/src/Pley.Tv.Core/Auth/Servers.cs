using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Auth;

/// <summary>A Pley server that answered at <paramref name="Url"/>.</summary>
public sealed record ServerInfo(string Url, string Name, string Version);

/// <summary>Why an address isn't a server to sign in to.</summary>
public abstract record ConnectProblem
{
    public sealed record BadAddress : ConnectProblem;

    public sealed record NotPley(string Address) : ConnectProblem;

    /// <param name="Reason">Why, in a few words (<see cref="Transports.Reason"/>).</param>
    public sealed record Unreachable(string Address, string Reason) : ConnectProblem;

    /// <summary>
    /// Pley answers at <paramref name="Url"/>, over plain http beyond the home network: said once,
    /// and connecting again with <paramref name="Url"/> as <c>goAheadAt</c> goes ahead.
    /// </summary>
    public sealed record Unencrypted(string Address, string Url) : ConnectProblem;

    /// <summary>The Android app's words for each problem (TvServersScreen's problemText).</summary>
    public string Text => this switch
    {
        BadAddress => "Enter an address such as nas, 192.168.1.20 or pley.example.com.",
        NotPley p => $"Something answers at {p.Address}, but it isn't Pley.",
        Unreachable u => $"Couldn't reach {u.Address}: {u.Reason}",
        Unencrypted u => $"{u.Address} isn't encrypted: what you watch could be read on the way. Use its https address if it has one, or press Connect again to go ahead anyway.",
        _ => "",
    };
}

public class ConnectException : Exception
{
    public ConnectException(ConnectProblem problem) : base(problem.Text) => Problem = problem;

    public ConnectProblem Problem { get; }
}

/// <summary>One row of the server list: saved, found on the network, or both.</summary>
/// <param name="Lookalike">Found, not saved, and named like a saved server it isn't.</param>
public sealed record ServerEntry(string Url, string Name, string? Id, bool Saved, bool Found, bool Lookalike = false);

/// <summary>Finding and checking servers: the Android app's <c>ServerChecks</c> and <c>Servers</c>.</summary>
public static class Servers
{
    /// <summary>How many addresses a search of the network asks at once.</summary>
    public const int SweepParallel = 32;
    /// <summary>How long an address has to accept the connection in a search: a home network answers in a few ms.</summary>
    public const int SweepConnectMs = 400;
    /// <summary>How long an address may take to answer in a search, once connected.</summary>
    public const int SweepReadMs = 2_000;
    /// <summary>How long a search takes at most.</summary>
    public const int SweepTotalMs = 5_000;
    /// <summary>How long an answer may take when checking an address, once connected.</summary>
    public const int ProbeReadMs = 10_000;

    static readonly Regex HostOfUrl = new Regex(@"^(https?)://([^/]+)", RegexOptions.CultureInvariant);

    /// <summary><c>host:port</c> as the server list shows it, never the scheme ("192.168.1.20:7539", "pley.example.com").</summary>
    public static string HostPort(string url)
    {
        var m = HostOfUrl.Match(url);
        return m.Success ? m.Groups[2].Value : url;
    }

    /// <summary>What GET /api/server answered: Pley's name and version, a redirect, or neither.</summary>
    sealed record Answer(ServerInfo? Info, string? RedirectTo);

    static async Task<Answer> Ask(Transport http, string url, int connectMs, int readMs, CancellationToken cancel)
    {
        var res = await http(new HttpRequest
        {
            Url = url + "/api/server",
            Headers = new Dictionary<string, string> { ["Accept"] = "application/json" },
            ResponseType = ResponseType.Json,
            ConnectTimeoutMs = connectMs,
            TimeoutMs = readMs,
            Cancel = cancel,
        }).ConfigureAwait(false);
        if (res.Status is >= 300 and < 400) return new Answer(null, res.Location);
        if (res.Status < 200 || res.Status >= 300) return new Answer(null, null);
        if (res.Json() is not { ValueKind: JsonValueKind.Object } body) return new Answer(null, null);
        // Both are always there in Pley's answer: without either, it is something else.
        var name = Json.String(body, "name");
        var version = Json.String(body, "version");
        if (name == null || string.IsNullOrWhiteSpace(version)) return new Answer(null, null);
        var shown = Plain.Of(name);
        return new Answer(new ServerInfo(url, shown.Length > 0 ? shown : Addresses.DisplayAddress(url), version!), null);
    }

    /// <summary>A home address gets 3 s to accept a connection, one on the internet 10 s.</summary>
    static int ConnectTimeoutOf(string url) =>
        Uri.TryCreate(url, UriKind.Absolute, out var uri) && Addresses.IsLocalHost(uri.Host) ? Transports.HomeConnectTimeoutMs : Transports.ConnectTimeoutMs;

    /// <summary>
    /// Asks <paramref name="url"/> whether it is Pley (GET /api/server, no token, no redirect
    /// followed): its name and version, null when something else answered. Network trouble
    /// throws <see cref="NetworkException"/>.
    /// </summary>
    public static async Task<ServerInfo?> Probe(Transport http, string url, CancellationToken cancel = default) =>
        (await Ask(http, url, ConnectTimeoutOf(url), ProbeReadMs, cancel).ConfigureAwait(false)).Info;

    /// <summary>A redirect from http to https on the same host (a proxy that wants https): that origin, else null.</summary>
    static string? UpgradeOf(string url, string? location)
    {
        if (location == null || !Uri.TryCreate(url, UriKind.Absolute, out var from) || !Uri.TryCreate(from, location, out var to)) return null;
        if (to.Scheme != "https" || !string.Equals(to.Host, from.Host, StringComparison.OrdinalIgnoreCase)) return null;
        return Addresses.Origin(to.AbsoluteUri);
    }

    /// <summary>
    /// What someone typed is a Pley server: each of <see cref="Addresses.Candidates"/> in turn
    /// (https first for names on the internet), and a redirect from http to https on the same
    /// host tried next. The first Pley answer wins.
    /// </summary>
    static async Task<ServerInfo> Find(Transport http, string input)
    {
        var list = Addresses.Candidates(input);
        if (list.Count == 0) throw new ConnectException(new ConnectProblem.BadAddress());
        var queue = new LinkedList<string>(list);
        var tried = new HashSet<string>();
        string? notPley = null;
        NetworkException? failure = null;
        while (queue.Count > 0)
        {
            var url = queue.First!.Value;
            queue.RemoveFirst();
            if (!tried.Add(url)) continue;
            try
            {
                var answer = await Ask(http, url, ConnectTimeoutOf(url), ProbeReadMs, default).ConfigureAwait(false);
                if (answer.Info != null) return answer.Info;
                var upgrade = UpgradeOf(url, answer.RedirectTo);
                if (upgrade != null) queue.AddFirst(upgrade);
                else notPley ??= url;
            }
            catch (NetworkException e)
            {
                failure = e;
            }
        }
        if (notPley != null) throw new ConnectException(new ConnectProblem.NotPley(HostPort(notPley)));
        throw new ConnectException(new ConnectProblem.Unreachable(HostPort(list[list.Count - 1]), failure?.Message ?? Transports.Reason(NetworkFailure.NothingAnswers)));
    }

    /// <summary>
    /// The server someone typed (the address pad's Connect). Pley over plain http beyond the
    /// home network is warned about once (<see cref="ConnectProblem.Unencrypted"/>); connecting
    /// again with its address as <paramref name="goAheadAt"/> goes ahead. Throws
    /// <see cref="ConnectException"/> with why none did.
    /// </summary>
    public static async Task<ServerInfo> Connect(Transport http, string input, string? goAheadAt = null)
    {
        var info = await Find(http, input).ConfigureAwait(false);
        if (Addresses.IsUnencrypted(info.Url) && info.Url != goAheadAt)
            throw new ConnectException(new ConnectProblem.Unencrypted(HostPort(info.Url), info.Url));
        return info;
    }

    /// <summary>A server already known (a row): it answers as Pley, or why not.</summary>
    public static Task<ServerInfo> Check(Transport http, string url) => Find(http, url);

    /// <summary>
    /// "Search this network": GET /api/server on every address of the TV's /24s (or smaller
    /// networks) at Pley's port, but the TV's own, 32 at a time, for at most 5 s. For servers that
    /// never hear broadcasts (Docker on a bridge network). Servers are reported as they answer,
    /// on any thread; a redirect is nothing found.
    /// </summary>
    public static async Task Sweep(Transport http, IReadOnlyList<LanAddress> lans, Action<FoundServer> onFound, CancellationToken stop)
    {
        var targets = lans.SelectMany(l => l.Neighbours()).Select(a => $"http://{a}:{Addresses.DefaultPort}").Distinct().ToList();
        using var budget = CancellationTokenSource.CreateLinkedTokenSource(stop);
        budget.CancelAfter(SweepTotalMs);
        var next = -1;
        async Task Worker()
        {
            while (!budget.IsCancellationRequested)
            {
                var i = Interlocked.Increment(ref next);
                if (i >= targets.Count) return;
                try
                {
                    var info = (await Ask(http, targets[i], SweepConnectMs, SweepReadMs, budget.Token).ConfigureAwait(false)).Info;
                    if (info != null && !stop.IsCancellationRequested) onFound(new FoundServer(info.Url, info.Name, null, info.Version));
                }
                catch (NetworkException)
                {
                    // Nothing there.
                }
            }
        }
        await Task.WhenAll(Enumerable.Range(0, SweepParallel).Select(_ => Worker())).ConfigureAwait(false);
    }

    /// <summary>The search of <see cref="Sweep(Transport, IReadOnlyList{LanAddress}, Action{FoundServer}, CancellationToken)"/> on the networks the TV is on now.</summary>
    public static Task Sweep(Transport http, Action<FoundServer> onFound, CancellationToken stop) => Sweep(http, Lans.Current(), onFound, stop);

    /// <summary>Whether two addresses are the same server.</summary>
    public static bool SameServer(string a, string b) => Addresses.Origin(a) == Addresses.Origin(b);

    static bool Same(string aUrl, string? aId, string bUrl, string? bId) => (aId != null && aId == bId) || aUrl == bUrl;

    /// <summary>
    /// The server list: saved servers, most recently opened first, then the ones found on the
    /// network that aren't saved. A server is the same by its id, wherever it answered, or by
    /// its address.
    /// </summary>
    public static IReadOnlyList<ServerEntry> Merge(IEnumerable<SavedServer> saved, IEnumerable<FoundServer> found)
    {
        var savedList = saved.ToList();
        var foundList = found.ToList();
        var entries = savedList.OrderByDescending(s => s.LastUsed)
            .Select(s => new ServerEntry(s.Url, s.Name, s.Id, Saved: true, Found: foundList.Any(f => Same(s.Url, s.Id, f.Url, f.Id))))
            .ToList();
        foreach (var f in foundList)
        {
            if (entries.Any(e => Same(e.Url, e.Id, f.Url, f.Id))) continue;
            var lookalike = savedList.Any(s => string.Equals(s.Name.Trim(), f.Name.Trim(), StringComparison.OrdinalIgnoreCase));
            entries.Add(new ServerEntry(f.Url, f.Name, f.Id, Saved: false, Found: true, Lookalike: lookalike));
        }
        return entries;
    }
}
