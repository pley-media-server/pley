using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Ui;

/// <summary>The code on screen and where to enter it.</summary>
/// <param name="UserCode">"BCDF-GHJK".</param>
/// <param name="Address">Where to enter it, as people type it: "192.168.1.20:7539/link" (no scheme, with the port).</param>
/// <param name="Qr">What the QR code holds: the address with the code in it.</param>
/// <param name="ExpiresAt">When the code stops working, in ms since the epoch.</param>
public sealed record LinkCode(string UserCode, string Address, string Qr, long ExpiresAt)
{
    /// <summary>The code's two halves, "BCDF" and "GHJK", for the two fields (one when it isn't 4 + 4).</summary>
    public IReadOnlyList<string> Halves
    {
        get
        {
            var parts = UserCode.Split('-');
            return parts.Length == 2 && parts[0].Length > 0 && parts[1].Length > 0 ? parts : new[] { UserCode };
        }
    }

    /// <summary>
    /// The address to show for <paramref name="uri"/>: without its scheme, with its port
    /// ("192.168.1.20:7539/link"). Not <see cref="Addresses.DisplayAddress"/>, which drops Pley's port.
    /// </summary>
    public static string AddressOf(string uri)
    {
        if (!Uri.TryCreate(uri, UriKind.Absolute, out var parsed) || string.IsNullOrEmpty(parsed.Host)) return uri;
        var port = parsed.IsDefaultPort ? "" : $":{parsed.Port}";
        return parsed.Host + port + parsed.AbsolutePath.TrimEnd('/');
    }
}

/// <summary>Where a code stands.</summary>
public abstract record LinkStatus
{
    /// <summary>Asking the server for a code.</summary>
    public sealed record Starting : LinkStatus;
    /// <summary>"Waiting for approval…".</summary>
    public sealed record Waiting : LinkStatus;
    /// <summary>The server can't be reached: asking again at <paramref name="RetryAt"/> (the scheduler's clock).</summary>
    public sealed record Unreachable(long RetryAt) : LinkStatus;
    /// <summary>The approver said no: "Get a new code".</summary>
    public sealed record Declined : LinkStatus;
    /// <summary>Renewed three times without an answer: "Code expired: press OK for a new one".</summary>
    public sealed record Expired : LinkStatus;
    /// <summary>"Signed in as Anna", for a moment, then in.</summary>
    public sealed record SignedIn(string? Name) : LinkStatus;
    /// <summary>It can't go on: an older server, or a refusal, in the sign-in's words.</summary>
    public sealed record Failed(string Message) : LinkStatus;
}

public sealed record LinkState(string Server, LinkCode? Code = null, LinkStatus? Status = null)
{
    public LinkStatus Shown => Status ?? new LinkStatus.Starting();

    /// <summary>A decline, or a code expired for good: "Get a new code" takes focus.</summary>
    public bool NeedsNewCode => Shown is LinkStatus.Declined or LinkStatus.Expired;
}

/// <summary>What the link screen asks: the server's sign-in, and the app once signed in.</summary>
public interface ILinkSource
{
    Task<DeviceCode> Start(string server);

    Task<DevicePoll> Poll(DeviceCode code, Action<DevicePoll> onUpdate, CancellationToken stop);

    /// <summary>Signed in: who the app is now, by name, once known (null after a few seconds without).</summary>
    Task<string?> SignedIn(string server);
}

/// <summary>The link screen's source in the app: <see cref="AppState"/>'s sign-in, then its account.</summary>
public sealed class AppLinkSource : ILinkSource
{
    readonly AppState app;
    readonly DeviceSignIn signIn;
    readonly long? profile;

    /// <param name="profile">The profile a phone approves this code for (one that needs its password); 0: any; null: signing in.</param>
    public AppLinkSource(AppState app, long? profile = null)
    {
        this.app = app;
        this.profile = profile;
        signIn = app.DeviceSignIn();
    }

    public Task<DeviceCode> Start(string server) => signIn.Start(server);

    public Task<DevicePoll> Poll(DeviceCode code, Action<DevicePoll> onUpdate, CancellationToken stop) => signIn.Poll(code, onUpdate, stop);

    public async Task<string?> SignedIn(string server)
    {
        // The approver picked the profile on /link: Who's watching? isn't asked again.
        app.ProfileGatePassed = true;
        app.Use(server);
        // New tokens may replace others for this server (another profile): who the app is follows.
        var account = profile != null ? app.Switched(profile > 0 ? profile : null) : app.LoadAccount();
        await Task.WhenAny(account, Task.Delay(LinkModel.NameWaitMs));
        return app.Me.Value?.Name;
    }
}

/// <summary>
/// Signing in with a code shown on screen (RFC 8628; the Android app's LinkViewModel): the TV's way
/// in, approved on a phone or computer at the server's /link.
///
/// The code is asked about while the screen shows only (<see cref="Started"/>,
/// <see cref="Stopped"/>): a decline shows "Request declined" with "Get a new code"; an expired code
/// is renewed silently up to three times, then waits for OK; network trouble counts down to the
/// next try (2, 4, 8, 16, then 30 s). A code that expired while the screen was away is replaced at
/// once. Once approved, "Signed in as &lt;name&gt;" shows for half a second, then Home.
/// </summary>
public sealed class LinkModel
{
    /// <summary>Silent renewals before "Code expired" (30 minutes of 10-minute codes).</summary>
    public const int Renewals = 3;
    /// <summary>"Signed in as Anna" stays this long.</summary>
    public const long SignedInMs = 500;
    /// <summary>How long the name is waited for once signed in.</summary>
    public const int NameWaitMs = 3_000;

    readonly ILinkSource source;
    readonly IScheduler scheduler;
    DeviceCode? device;
    CancellationTokenSource? job;
    IDisposable? timer;
    bool visible;
    int renewals;
    int startFailures;

    public LinkModel(ILinkSource source, IScheduler scheduler, string server)
    {
        this.source = source;
        this.scheduler = scheduler;
        State = new Signal<LinkState>(new LinkState(server));
    }

    public Signal<LinkState> State { get; }

    /// <summary>Signed in, and the moment of saying so is over: Home.</summary>
    public event Action? Done;

    /// <summary>The screen shows: a code is asked about (a new one if the old one ran out).</summary>
    public void Started()
    {
        visible = true;
        if (State.Value.Status is LinkStatus.SignedIn or LinkStatus.Failed or LinkStatus.Declined or LinkStatus.Expired) return;
        if (device == null || scheduler.Now >= device.ExpiresAt)
        {
            renewals = 0;
            Fetch();
        }
        else
        {
            Follow(device);
        }
    }

    /// <summary>The screen went: nothing is asked until it shows again.</summary>
    public void Stopped()
    {
        visible = false;
        Cancel();
    }

    /// <summary>"Get a new code" (after a decline, or once the code expired for good).</summary>
    public void NewCode()
    {
        if (State.Value.Status is LinkStatus.SignedIn) return;
        renewals = 0;
        startFailures = 0;
        Fetch();
    }

    void Cancel()
    {
        job?.Cancel();
        job = null;
        timer?.Dispose();
        timer = null;
    }

    /// <summary>Asks for a new code; <paramref name="silent"/> (a renewal) keeps the status line as it is meanwhile.</summary>
    async void Fetch(bool silent = false)
    {
        Cancel();
        if (!visible) return;
        if (!silent) Update(s => s with { Status = new LinkStatus.Starting() });
        var mine = job = new CancellationTokenSource();
        DeviceCode code;
        try
        {
            code = await source.Start(State.Value.Server);
        }
        catch (Exception e) when (e is UnreachableException or NetworkException)
        {
            if (mine.IsCancellationRequested) return;
            // Can't reach the server: try again after a while, as the polling does.
            var wait = DeviceSignIn.BackoffMs[Math.Min(startFailures, DeviceSignIn.BackoffMs.Length - 1)];
            startFailures++;
            Update(s => s with { Status = new LinkStatus.Unreachable(scheduler.Now + wait) });
            timer = scheduler.After(wait, () => Fetch(silent: true));
            return;
        }
        catch (AuthException e)
        {
            if (!mine.IsCancellationRequested) Update(s => s with { Status = new LinkStatus.Failed(Errors.AsSentence(e.Message)) });
            return;
        }
        if (mine.IsCancellationRequested) return;
        startFailures = 0;
        device = code;
        Update(s => s with { Code = Shown(code), Status = new LinkStatus.Waiting() });
        Follow(code);
    }

    async void Follow(DeviceCode code)
    {
        job?.Cancel();
        var mine = job = new CancellationTokenSource();
        try
        {
            await source.Poll(code, poll => scheduler.Post(() =>
            {
                if (!mine.IsCancellationRequested) OnPoll(poll);
            }), mine.Token);
        }
        catch (Exception e)
        {
            if (!mine.IsCancellationRequested) Update(s => s with { Status = new LinkStatus.Failed(Errors.Message(e)) });
        }
    }

    void OnPoll(DevicePoll poll)
    {
        switch (poll)
        {
            case DevicePoll.Waiting:
                Update(s => s with { Status = new LinkStatus.Waiting() });
                break;
            case DevicePoll.Unreachable u:
                Update(s => s with { Status = new LinkStatus.Unreachable(scheduler.Now + u.RetryInMs) });
                break;
            case DevicePoll.Denied:
                Update(s => s with { Status = new LinkStatus.Declined() });
                break;
            case DevicePoll.Expired:
                Expired();
                break;
            case DevicePoll.SignedIn:
                SignedIn();
                break;
            case DevicePoll.Failed f:
                Update(s => s with { Status = new LinkStatus.Failed(Errors.AsSentence(f.Message)) });
                break;
        }
    }

    /// <summary>The code ran out: a new one, silently, up to <see cref="Renewals"/> times while the screen shows.</summary>
    void Expired()
    {
        if (renewals < Renewals && visible)
        {
            renewals++;
            // Posted: this runs inside the poll that Fetch cancels.
            scheduler.Post(() => Fetch(silent: true));
        }
        else
        {
            Update(s => s with { Status = new LinkStatus.Expired() });
        }
    }

    async void SignedIn()
    {
        job = null;
        string? name = null;
        try
        {
            name = await source.SignedIn(State.Value.Server);
        }
        catch (Exception)
        {
            // Signed in all the same: without the name.
        }
        Update(s => s with { Status = new LinkStatus.SignedIn(name) });
        timer = scheduler.After(SignedInMs, () => Done?.Invoke());
    }

    static LinkCode Shown(DeviceCode code) =>
        new LinkCode(code.UserCode, LinkCode.AddressOf(code.VerificationUri), code.VerificationUriComplete ?? code.VerificationUri, code.ExpiresAt);

    void Update(Func<LinkState, LinkState> change) => State.Value = change(State.Value);
}
