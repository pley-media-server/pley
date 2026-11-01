using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>"Who's watching?" and the PIN pad: the Android app's ProfilesViewModelTest, as a TV.</summary>
public class ProfilesModelTests
{
    const string Server = "http://192.168.1.20:7539";

    sealed class FakeSource : IProfilesSource
    {
        public string? Server { get; set; } = ProfilesModelTests.Server;
        public User? Me { get; set; } = new User { Id = 1, Name = "Anna" };
        public List<HouseholdProfile> SavedList { get; } = new List<HouseholdProfile>();
        public List<HouseholdProfile> Household { get; set; } = HouseholdOf(currentId: 1);
        public bool Switchable { get; set; } = true;
        public List<(long Id, string? Pin)> Switches { get; } = new List<(long, string?)>();
        public List<(long Id, string Pin)> PinChecks { get; } = new List<(long, string)>();
        public Func<long, string?, Exception?> OnSwitch { get; set; } = (_, _) => null;
        public Func<long, string, Task<PinCheck>> OnPinCheck { get; set; } = (_, _) => Task.FromResult(PinCheck.Confirmed);

        public IReadOnlyList<HouseholdProfile> Saved() => SavedList;

        public bool CanSwitch() => Switchable;

        public Task<IReadOnlyList<HouseholdProfile>> Load()
        {
            SavedList.Clear();
            SavedList.AddRange(Household);
            return Task.FromResult<IReadOnlyList<HouseholdProfile>>(Household);
        }

        public Task SwitchTo(long id, string? pin)
        {
            Switches.Add((id, pin));
            var e = OnSwitch(id, pin);
            return e == null ? Task.CompletedTask : Task.FromException(e);
        }

        public Task<PinCheck> ConfirmPin(long id, string pin)
        {
            PinChecks.Add((id, pin));
            return OnPinCheck(id, pin);
        }
    }

    /// <summary>Anna (no PIN), Sam (a PIN), Robin (a password, a picture), Kids (locked).</summary>
    static List<HouseholdProfile> HouseholdOf(long currentId) => new List<HouseholdProfile>
    {
        new HouseholdProfile { Id = 1, Name = "Anna", Needs = "none", Current = currentId == 1 },
        new HouseholdProfile { Id = 2, Name = "Sam", Needs = "pin", Current = currentId == 2 },
        new HouseholdProfile { Id = 3, Name = "Robin", Needs = "password", AvatarUrl = "/api/users/3/avatar?v=1", Current = currentId == 3 },
        new HouseholdProfile { Id = 4, Name = "Kids", Needs = "locked", Current = currentId == 4 },
    };

    static TokenException Refusal(string reason, string description) =>
        new TokenException(400, new OAuthError("invalid_grant", description, reason));

    static async Task<(ProfilesModel Model, FakeSource Source)> Profiles(bool gate = false, Action<FakeSource>? setUp = null)
    {
        var source = new FakeSource();
        setUp?.Invoke(source);
        var model = new ProfilesModel(source, gate);
        await model.Load();
        return (model, source);
    }

    static async Task Type(ProfilesModel model, string digits)
    {
        foreach (var d in digits) model.Digit(d - '0');
        await Task.CompletedTask;
    }

    static ProfilesStep.Pin Pad(ProfilesModel model) => Assert.IsType<ProfilesStep.Pin>(model.State.Value.Step);

    [Fact]
    public async Task The_saved_household_shows_first_then_the_servers_with_focus_on_the_one_in_use()
    {
        var shown = new List<int>();
        var source = new FakeSource();
        source.SavedList.Add(new HouseholdProfile { Id = 1, Name = "Anna", Needs = "none", Current = true });
        var model = new ProfilesModel(source, gate: false);
        model.State.Changed += s => shown.Add(s.Profiles.Count);
        await model.Load();
        Assert.Contains(1, shown);
        var state = model.State.Value;
        Assert.Equal(new[] { "Anna", "Sam", "Robin", "Kids" }, state.Profiles.Select(p => p.Name));
        Assert.Equal(1L, state.Focus);
        Assert.True(state.Profiles[0].Current);
        Assert.True(state.Profiles[1].Locked);
        Assert.Equal(Server + "/api/users/3/avatar?v=1", state.Profiles[2].AvatarUrl);
        Assert.Equal(4, source.SavedList.Count);
    }

    [Fact]
    public async Task A_profile_without_a_PIN_switches_at_once()
    {
        var (model, source) = await Profiles(setUp: s =>
        {
            s.Household = HouseholdOf(currentId: 2);
            s.Me = new User { Id = 2, Name = "Sam", HasPin = true };
        });
        var done = false;
        model.Done += () => done = true;
        await model.Pick(1);
        Assert.Equal(new[] { (1L, (string?)null) }, source.Switches);
        Assert.True(done);
    }

    [Fact]
    public async Task A_PIN_profile_takes_four_to_eight_digits_then_OK()
    {
        var (model, source) = await Profiles();
        var done = false;
        model.Done += () => done = true;
        await model.Pick(2);
        Assert.Equal("Sam", Pad(model).Profile.Name);
        await Type(model, "123");
        await model.Submit();
        // Three digits: OK does nothing.
        Assert.Empty(source.Switches);
        await Type(model, "456789");
        Assert.Equal(8, Pad(model).Digits);
        for (var i = 0; i < 4; i++) model.Delete();
        Assert.Equal(4, Pad(model).Digits);
        await model.Submit();
        Assert.Equal(new[] { (2L, (string?)"1234") }, source.Switches);
        Assert.True(done);
    }

    [Fact]
    public async Task A_wrong_PIN_clears_the_dots_shows_the_sentence_and_is_never_retried()
    {
        var (model, source) = await Profiles(setUp: s => s.OnSwitch = (_, _) => Refusal("wrong_pin", "wrong PIN"));
        await model.Pick(2);
        await Type(model, "9999");
        await model.Submit();
        var pad = Pad(model);
        Assert.Equal("Wrong PIN.", pad.Error);
        Assert.Equal(0, pad.Digits);
        Assert.Equal(1, pad.Shakes);
        Assert.False(pad.LockedOut);
        Assert.Single(source.Switches);
        // The next OK sends once more.
        await Type(model, "1234");
        await model.Submit();
        Assert.Equal(2, source.Switches.Count);
        Assert.Equal(2, Pad(model).Shakes);
    }

    [Fact]
    public async Task Throttled_shows_the_servers_wait()
    {
        var (model, _) = await Profiles(setUp: s => s.OnSwitch = (_, _) => Refusal("throttled", "too many tries: try again in 5 minutes"));
        await model.Pick(2);
        await Type(model, "1234");
        await model.Submit();
        Assert.Equal("Too many tries: try again in 5 minutes.", Pad(model).Error);
    }

    [Fact]
    public async Task A_locked_PIN_dims_the_pad()
    {
        var (model, _) = await Profiles(setUp: s => s.OnSwitch = (_, _) => Refusal("pin_locked", "locked"));
        await model.Pick(2);
        await Type(model, "1234");
        await model.Submit();
        var pad = Pad(model);
        Assert.True(pad.LockedOut);
        Assert.StartsWith("This profile's PIN is locked", pad.Error);
        // The keys do nothing now.
        await Type(model, "1234");
        Assert.Equal(0, Pad(model).Digits);
        model.BackToPick();
        Assert.IsType<ProfilesStep.Pick>(model.State.Value.Step);
    }

    [Fact]
    public async Task A_password_profile_is_approved_on_a_phone()
    {
        var (model, source) = await Profiles();
        await model.Pick(3);
        Assert.Equal("Robin", Assert.IsType<ProfilesStep.Approve>(model.State.Value.Step).Profile.Name);
        Assert.Empty(source.Switches);
    }

    [Fact]
    public async Task The_server_sending_to_the_browser_means_the_password_way()
    {
        var (model, _) = await Profiles(setUp: s =>
        {
            s.Household = HouseholdOf(1).Select(p => p.Id == 3 ? p with { Needs = "none" } : p).ToList();
            s.OnSwitch = (_, _) => Refusal("browser", "this profile needs its password: sign in again");
        });
        await model.Pick(3);
        Assert.IsType<ProfilesStep.Approve>(model.State.Value.Step);
    }

    [Fact]
    public async Task A_locked_profile_says_why()
    {
        var (model, _) = await Profiles();
        await model.Pick(4);
        var pad = Pad(model);
        Assert.True(pad.LockedOut);
        Assert.Equal("This profile can't be used from the app.", pad.Error);
    }

    [Fact]
    public async Task A_TV_starting_as_a_PIN_profile_asks_its_PIN()
    {
        var (model, source) = await Profiles(gate: true, setUp: s => s.Me = new User { Id = 1, Name = "Anna", HasPin = true });
        var done = false;
        model.Done += () => done = true;
        await model.Pick(1);
        Assert.True(Pad(model).Confirm);
        source.OnPinCheck = (_, _) => Task.FromException<PinCheck>(Refusal("wrong_pin", "wrong PIN"));
        await Type(model, "0000");
        await model.Submit();
        Assert.Equal("Wrong PIN.", Pad(model).Error);
        source.OnPinCheck = (_, _) => Task.FromResult(PinCheck.Confirmed);
        await Type(model, "1234");
        await model.Submit();
        Assert.Equal(new[] { (1L, "0000"), (1L, "1234") }, source.PinChecks);
        Assert.True(done);
        Assert.Empty(source.Switches);
    }

    [Fact]
    public async Task A_server_out_of_reach_is_no_wrong_PIN()
    {
        var (model, source) = await Profiles(gate: true, setUp: s => s.Me = new User { Id = 1, Name = "Anna", HasPin = true });
        await model.Pick(1);
        source.OnPinCheck = (_, _) => Task.FromException<PinCheck>(new NetworkException(NetworkFailure.NoAnswerInTime));
        await Type(model, "1234");
        await model.Submit();
        var pad = Pad(model);
        Assert.Equal("Can't reach your server: no answer in time.", pad.Error);
        Assert.Equal(0, pad.Shakes);
        Assert.False(pad.LockedOut);
    }

    [Fact]
    public async Task Servers_that_cant_check_the_PIN_let_it_through()
    {
        var (model, _) = await Profiles(gate: true, setUp: s =>
        {
            s.Me = new User { Id = 1, Name = "Anna", HasPin = true };
            s.OnPinCheck = (_, _) => Task.FromResult(PinCheck.Unsupported);
        });
        var done = false;
        model.Done += () => done = true;
        await model.Pick(1);
        await Type(model, "1234");
        await model.Submit();
        Assert.True(done);
    }

    [Fact]
    public async Task The_profile_in_use_continues_without_a_PIN_outside_the_cold_start()
    {
        var (model, source) = await Profiles(gate: false, setUp: s => s.Me = new User { Id = 1, Name = "Anna", HasPin = true });
        var done = false;
        model.Done += () => done = true;
        await model.Pick(1);
        Assert.True(done);
        Assert.Empty(source.PinChecks);
    }

    [Fact]
    public async Task Without_the_profiles_scope_it_offers_signing_in_as_another()
    {
        var (model, _) = await Profiles(setUp: s => s.Switchable = false);
        Assert.False(model.State.Value.CanSwitch);
        model.SignInAgain();
        Assert.Equal(0, Assert.IsType<ProfilesStep.Approve>(model.State.Value.Step).Profile.Id);
    }

    [Fact]
    public void PIN_entry_keeps_four_to_eight_digits()
    {
        var entry = new PinEntry();
        for (var i = 0; i < 3; i++) Assert.True(entry.Add(i));
        Assert.False(entry.Complete);
        for (var i = 0; i < 5; i++) Assert.True(entry.Add(i));
        Assert.False(entry.Add(9));
        Assert.False(entry.Add(10));
        Assert.Equal("01201234", entry.Take());
        Assert.Equal(0, entry.Length);
        Assert.Equal("PinEntry(0 digits)", entry.ToString());
    }

    [Fact]
    public void Profile_colours_are_the_webs()
    {
        Assert.Equal(0xE07A5Fu, ProfileColors.Of("Anna", "#e07a5f"));
        Assert.Equal(0xAABBCCu, ProfileColors.Of("Anna", "#abc"));
        // Picked from the name when the profile has none, or one that isn't a colour.
        Assert.Equal(ProfileColors.Of("Anna", null), ProfileColors.Of("Anna", "red"));
        Assert.Contains(ProfileColors.Of("Sam", null), new uint[] { 0x38BDF8, 0xA78BFA, 0xF472B6, 0xFB923C, 0xFACC15, 0x4ADE80, 0x2DD4BF, 0xF87171 });
        Assert.True(ProfileColors.DarkTextOn(0xFACC15));
        Assert.False(ProfileColors.DarkTextOn(0x0284C7));
    }
}
