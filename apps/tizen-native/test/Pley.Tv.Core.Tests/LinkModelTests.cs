using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Signing in with a code: the Android app's LinkViewModelTest.</summary>
public class LinkModelTests
{
    const string Server = "http://192.168.1.20:7539";

    sealed class FakeSource : ILinkSource
    {
        readonly FakeScheduler scheduler;

        public FakeSource(FakeScheduler scheduler) => this.scheduler = scheduler;

        /// <summary>What each Start does, in turn: a code, or an exception to throw.</summary>
        public Queue<object> Starts { get; } = new Queue<object>();
        /// <summary>What each Poll says, in turn; the last of each is its answer.</summary>
        public Queue<DevicePoll[]> Polls { get; } = new Queue<DevicePoll[]>();
        public int Started { get; private set; }
        public int Polled { get; private set; }
        public string? Name { get; set; } = "Anna";

        public DeviceCode Code(string user = "BCDF-GHJK", long life = 600_000) => new DeviceCode(Server, Server, "device-secret", user,
            Server + "/link", Server + "/link?code=" + user, scheduler.Now + life, 5, Server + "/oauth/token", Server + "/oauth/revoke", "library");

        public Task<DeviceCode> Start(string server)
        {
            Started++;
            var next = Starts.Count > 0 ? Starts.Dequeue() : Code();
            return next is Exception e ? Task.FromException<DeviceCode>(e) : Task.FromResult((DeviceCode)next);
        }

        public Task<DevicePoll> Poll(DeviceCode code, Action<DevicePoll> onUpdate, CancellationToken stop)
        {
            Polled++;
            var said = Polls.Count > 0 ? Polls.Dequeue() : new DevicePoll[] { new DevicePoll.Waiting() };
            foreach (var p in said)
            {
                if (stop.IsCancellationRequested) break;
                onUpdate(p);
            }
            return Task.FromResult(said[said.Length - 1]);
        }

        public Task<string?> SignedIn(string server) => Task.FromResult(Name);

        /// <summary>What the server says of two-step sign-in, once the test answers.</summary>
        public TaskCompletionSource<bool> TwoStep { get; } = new TaskCompletionSource<bool>();
        public List<string> AskedTwoStep { get; } = new List<string>();

        public Task<bool> TwoStepRequired(string server)
        {
            AskedTwoStep.Add(server);
            return TwoStep.Task;
        }
    }

    static (LinkModel Model, FakeSource Source, FakeScheduler Clock) Link()
    {
        var clock = new FakeScheduler();
        var source = new FakeSource(clock);
        return (new LinkModel(source, clock, Server), source, clock);
    }

    [Fact]
    public void The_code_shows_in_two_halves_with_the_address_to_enter_it_at()
    {
        var (model, _, _) = Link();
        model.Started();
        var state = model.State.Value;
        Assert.Equal(new[] { "BCDF", "GHJK" }, state.Code!.Halves);
        Assert.Equal("192.168.1.20:7539/link", state.Code.Address);
        Assert.Equal(Server + "/link?code=BCDF-GHJK", state.Code.Qr);
        Assert.IsType<LinkStatus.Waiting>(state.Shown);
        Assert.Equal(new[] { "ABCDEFGHJ" }, new LinkCode("ABCDEFGHJ", "", "", 0).Halves);
    }

    [Fact]
    public void A_server_out_of_reach_is_asked_again_after_longer_and_longer()
    {
        var (model, source, clock) = Link();
        source.Starts.Enqueue(new UnreachableException());
        source.Starts.Enqueue(new UnreachableException());
        model.Started();
        Assert.Equal(new LinkStatus.Unreachable(clock.Now + 2_000), model.State.Value.Status);
        clock.Advance(2_000);
        Assert.Equal(new LinkStatus.Unreachable(clock.Now + 4_000), model.State.Value.Status);
        clock.Advance(4_000);
        Assert.IsType<LinkStatus.Waiting>(model.State.Value.Status);
        Assert.Equal(3, source.Started);
    }

    [Fact]
    public void A_refusal_says_why_and_stops()
    {
        var (model, source, _) = Link();
        source.Starts.Enqueue(new AuthException("this server is too old to sign in with a code"));
        model.Started();
        Assert.Equal(new LinkStatus.Failed("This server is too old to sign in with a code."), model.State.Value.Status);
        model.Stopped();
        model.Started();
        Assert.Equal(1, source.Started);
    }

    [Fact]
    public void A_decline_offers_a_new_code()
    {
        var (model, source, _) = Link();
        source.Polls.Enqueue(new DevicePoll[] { new DevicePoll.Waiting(), new DevicePoll.Denied() });
        model.Started();
        Assert.IsType<LinkStatus.Declined>(model.State.Value.Status);
        Assert.True(model.State.Value.NeedsNewCode);
        source.Starts.Enqueue(source.Code("MNPQ-RSTV"));
        model.NewCode();
        Assert.Equal("MNPQ-RSTV", model.State.Value.Code!.UserCode);
        Assert.IsType<LinkStatus.Waiting>(model.State.Value.Status);
    }

    [Fact]
    public void An_expired_code_is_renewed_three_times_then_waits_for_OK()
    {
        var (model, source, _) = Link();
        for (var i = 0; i < 4; i++) source.Polls.Enqueue(new DevicePoll[] { new DevicePoll.Waiting(), new DevicePoll.Expired() });
        model.Started();
        Assert.Equal(4, source.Started);
        Assert.IsType<LinkStatus.Expired>(model.State.Value.Status);
        Assert.True(model.State.Value.NeedsNewCode);
    }

    [Fact]
    public void Approved_it_says_who_for_a_moment_then_goes_in()
    {
        var (model, source, clock) = Link();
        var done = false;
        model.Done += () => done = true;
        source.Polls.Enqueue(new DevicePoll[] { new DevicePoll.Waiting(), new DevicePoll.SignedIn(Server) });
        model.Started();
        Assert.Equal(new LinkStatus.SignedIn("Anna"), model.State.Value.Status);
        Assert.False(done);
        clock.Advance(LinkModel.SignedInMs);
        Assert.True(done);
    }

    [Fact]
    public void Says_when_the_server_requires_two_step_sign_in()
    {
        var (model, source, _) = Link();
        Assert.False(model.State.Value.TwoStepRequired);
        model.Started();
        // The code didn't wait for the answer.
        Assert.Equal("BCDF-GHJK", model.State.Value.Code!.UserCode);
        Assert.IsType<LinkStatus.Waiting>(model.State.Value.Status);
        source.TwoStep.SetResult(true);
        Assert.True(model.State.Value.TwoStepRequired);
        Assert.IsType<LinkStatus.Waiting>(model.State.Value.Status);
        Assert.Equal(new[] { Server }, source.AskedTwoStep);
        Assert.Equal("This server requires two-step sign-in: your phone sets it up if you haven't yet.", LinkText.TwoStepRequired);
    }

    [Fact]
    public void Says_nothing_of_two_step_sign_in_otherwise()
    {
        // Not required, or a server from before it.
        var (notRequired, source, _) = Link();
        source.TwoStep.SetResult(false);
        Assert.False(notRequired.State.Value.TwoStepRequired);
        // No answer.
        var (unreachable, other, _) = Link();
        other.TwoStep.SetException(new UnreachableException());
        Assert.False(unreachable.State.Value.TwoStepRequired);
        Assert.Single(other.AskedTwoStep);
    }

    static AppState App(FakeServer server) =>
        new AppState(new AppPlatform("tizen", "client", new MemoryKeyValue()), server.Transport, (_, _, _, _, _) => Task.FromResult(new EventEnd(0, false)), new FakeScheduler());

    [Fact]
    public async Task The_app_asks_the_server_and_a_profiles_approval_doesnt()
    {
        const string Home = "https://pley.example.com";
        var server = new FakeServer().On("GET", Home + "/api/server", 200, @"{""name"":""Home"",""version"":""0.1.0"",""two_step_required"":true}");
        var app = App(server);
        Assert.True(await new AppLinkSource(app).TwoStepRequired(Home));
        Assert.Single(server.Requests);
        // A profile's approval is about the household, not the server's sign-in.
        Assert.False(await new AppLinkSource(app, 3).TwoStepRequired(Home));
        Assert.False(await new AppLinkSource(app, 0).TwoStepRequired(Home));
        Assert.Single(server.Requests);
        // A server out of reach says nothing of it.
        Assert.False(await new AppLinkSource(App(new FakeServer())).TwoStepRequired(Home));
    }

    [Fact]
    public void Back_on_screen_the_code_is_asked_about_again_or_replaced_once_it_ran_out()
    {
        var (model, source, clock) = Link();
        model.Started();
        model.Stopped();
        model.Started();
        Assert.Equal(1, source.Started);
        Assert.Equal(2, source.Polled);
        model.Stopped();
        clock.Advance(600_000);
        model.Started();
        Assert.Equal(2, source.Started);
    }
}
