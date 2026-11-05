using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// Casts from the remote, as the fixture packages/tv's tracks.test.ts reads too has them: which
/// casts this TV plays, with the banner they show, and what a title of each kind plays.
/// </summary>
public class CastRuleTests
{
    public static IEnumerable<object[]> CastCases => Indexes("casts");
    public static IEnumerable<object[]> ItemCases => Indexes("items");

    static IEnumerable<object[]> Indexes(string rule) =>
        Enumerable.Range(0, Fixtures.Load("tracks").GetProperty(rule).GetArrayLength()).Select(i => new object[] { i });

    static JsonElement Case(string rule, int i) => Fixtures.Load("tracks").GetProperty(rule)[i];

    static long? Long(JsonElement e) => e.ValueKind == JsonValueKind.Null ? null : e.GetInt64();

    /// <summary><paramref name="name"/> of <paramref name="e"/>; null when it is null or left out.</summary>
    static int? Int(JsonElement e, string name) => e.TryGetProperty(name, out var v) && v.ValueKind != JsonValueKind.Null ? v.GetInt32() : null;

    [Theory]
    [MemberData(nameof(CastCases))]
    public void A_cast_does_what_the_fixture_says(int i)
    {
        var c = Case("casts", i);
        var input = c.GetProperty("in");
        var e = input.GetProperty("cast");
        var cast = new ServerEvent
        {
            Type = "cast",
            ReceiverId = e.GetProperty("receiver_id").GetString(),
            ItemId = Long(e.GetProperty("item_id")),
            PlaylistId = Long(e.GetProperty("playlist_id")),
            ChannelId = Long(e.GetProperty("channel_id")),
            StartMs = Long(e.GetProperty("start_ms")),
            Shuffle = e.GetProperty("shuffle").GetBoolean(),
            AudioIndex = Int(e, "audio_index"),
            SubtitleIndex = Int(e, "subtitle_index"),
            From = e.GetProperty("from").GetString(),
            ProfileId = e.TryGetProperty("profile_id", out var p) ? Long(p) : null,
            SwitchTicket = e.TryGetProperty("switch_ticket", out var t) ? t.GetString() : null,
        };
        // The profile the TV acts as: none known when the case leaves it out.
        var profile = input.TryGetProperty("profile", out var on) ? Long(on) : null;
        var action = Casts.Action(input.GetProperty("receiver_id").GetString()!, profile, input.GetProperty("shown").GetBoolean(), cast);
        var banner = Casts.Banner(cast.From);
        var expected = c.GetProperty("out");
        var written = action.Act switch
        {
            CastAct.Item => new Dictionary<string, object?> { ["act"] = "item", ["item_id"] = action.Id, ["start_ms"] = action.StartMs, ["shuffle"] = action.Shuffle, ["banner"] = banner },
            CastAct.Playlist => new Dictionary<string, object?> { ["act"] = "playlist", ["playlist_id"] = action.Id, ["shuffle"] = action.Shuffle, ["banner"] = banner },
            CastAct.Channel => new Dictionary<string, object?> { ["act"] = "channel", ["channel_id"] = action.Id, ["banner"] = banner },
            _ => new Dictionary<string, object?> { ["act"] = "none" },
        };
        // The tracks a title's cast plays with, in the cases that say them (casts made before they were sent don't).
        if (action.Act == CastAct.Item && expected.TryGetProperty("audio_index", out _)) written["audio_index"] = action.AudioIndex;
        if (action.Act == CastAct.Item && expected.TryGetProperty("subtitle_index", out _)) written["subtitle_index"] = action.SubtitleIndex;
        // The switch to the remote's profile first, when it makes one.
        if (action.SwitchTo is { } to) written["switch"] = new Dictionary<string, object?> { ["profile"] = to.Profile, ["ticket"] = to.Ticket };
        Fixtures.SameFields(input, written, expected);
    }

    [Theory]
    [MemberData(nameof(ItemCases))]
    public void A_title_cast_plays_what_the_fixture_says(int i)
    {
        var c = Case("items", i);
        var input = c.GetProperty("in");
        var plan = Casts.Plays(input.GetProperty("kind").GetString()!, Long(input.GetProperty("start_ms")), input.GetProperty("shuffle").GetBoolean());
        var written = plan.Plays switch
        {
            ItemPlays.Video => new Dictionary<string, object?> { ["plays"] = "video", ["start_ms"] = plan.StartMs },
            ItemPlays.Track => new Dictionary<string, object?> { ["plays"] = "track", ["start_ms"] = plan.StartMs },
            ItemPlays.Album => new Dictionary<string, object?> { ["plays"] = "album", ["shuffle"] = plan.Shuffle },
            ItemPlays.Artist => new Dictionary<string, object?> { ["plays"] = "artist", ["shuffle"] = plan.Shuffle },
            ItemPlays.NextEpisode => new Dictionary<string, object?> { ["plays"] = "next_episode" },
            _ => new Dictionary<string, object?> { ["plays"] = "nothing" },
        };
        Fixtures.SameFields(input, written, c.GetProperty("out"));
    }

    [Fact]
    public void A_cast_is_read_off_the_event_stream_and_needs_its_receiver()
    {
        var events = new SseReader().Feed(
            "data: {\"type\":\"cast\",\"receiver_id\":\"tizen-1\",\"item_id\":5,\"playlist_id\":null,\"channel_id\":null,\"start_ms\":60000,\"shuffle\":false,\"from\":\"Living room phone\"}\n\n" +
            "data: {\"type\":\"cast\",\"receiver_id\":\"tizen-1\",\"item_id\":null,\"playlist_id\":3,\"channel_id\":null,\"start_ms\":null,\"shuffle\":true,\"from\":null}\n\n" +
            "data: {\"type\":\"cast\",\"item_id\":5}\n\n" +
            "data: {\"type\":\"player_command\",\"session_id\":\"s1\",\"command\":\"subtitles\",\"position_ms\":null,\"offset_ms\":null,\"stream_index\":-1}\n\n");
        Assert.Equal(new (string, string?, long?, long?, long?, bool?, string?)[]
        {
            ("cast", "tizen-1", 5, null, 60_000, false, "Living room phone"),
            ("cast", "tizen-1", null, 3, null, true, null),
            ("player_command", null, null, null, null, null, null),
        }, events.Select(e => (e.Type, e.ReceiverId, e.ItemId, e.PlaylistId, e.StartMs, e.Shuffle, e.From)));
        Assert.Equal(-1, events.Last().StreamIndex);
    }

    [Fact]
    public void A_title_cast_carries_the_tracks_picked_on_the_remotes_page()
    {
        var e = new SseReader().Feed(
            "data: {\"type\":\"cast\",\"receiver_id\":\"tizen-1\",\"item_id\":5,\"playlist_id\":null,\"channel_id\":null,\"start_ms\":null,\"shuffle\":false,\"audio_index\":2,\"subtitle_index\":-1,\"from\":null}\n\n").Single();
        Assert.Equal((2, -1), (e.AudioIndex, e.SubtitleIndex));
        Assert.Equal(new CastAction(CastAct.Item, 5, AudioIndex: 2, SubtitleIndex: -1), Casts.Action("tizen-1", 2, true, e));
    }

    [Fact]
    public void A_cast_from_another_profile_carries_its_profile_and_ticket()
    {
        var e = new SseReader().Feed(
            "data: {\"type\":\"cast\",\"receiver_id\":\"tizen-1\",\"item_id\":5,\"playlist_id\":null,\"channel_id\":null,\"start_ms\":null,\"shuffle\":false,\"from\":null,\"profile_id\":4,\"switch_ticket\":\"t1\"}\n\n").Single();
        Assert.Equal(((long?)4, (string?)"t1"), (e.ProfileId, e.SwitchTicket));
        Assert.Equal(new CastSwitch(4, "t1"), Casts.Action("tizen-1", 2, true, e).SwitchTo);
    }
}

/// <summary>A cast to this TV through its receiver, for each kind of thing sent (packages/tv's cast.test.ts).</summary>
public class CastReceiverTests
{
    readonly Server server = new Server();
    readonly Target target = new Target();
    readonly CastReceiver receiver;

    public CastReceiverTests() => receiver = new CastReceiver(server, target, new FakeScheduler(), new Random(3));

    /// <summary>The server's side: items, children, next episodes and playlists the test gives, answered at once unless held.</summary>
    sealed class Server : ICastServer
    {
        public readonly Dictionary<long, ItemDetail> Items = new Dictionary<long, ItemDetail>();
        public readonly Dictionary<long, IReadOnlyList<ItemSummary>> Children_ = new Dictionary<long, IReadOnlyList<ItemSummary>>();
        public readonly Dictionary<long, ItemSummary?> Next = new Dictionary<long, ItemSummary?>();
        public readonly Dictionary<long, PlaylistDetail> Playlists = new Dictionary<long, PlaylistDetail>();
        /// <summary>When set, items are answered only once it is.</summary>
        public TaskCompletionSource<bool>? Held;

        public async Task<ItemDetail> Item(long id)
        {
            if (Held != null) await Held.Task;
            return Items.TryGetValue(id, out var item) ? item : throw new ApiException(404, "not_found", "item not found");
        }

        public Task<ItemSummary?> OnDeck(long id) => Task.FromResult(Next.TryGetValue(id, out var next) ? next : null);

        public Task<IReadOnlyList<ItemSummary>> Children(long id) => Task.FromResult(Children_.TryGetValue(id, out var list) ? list : new ItemSummary[0]);

        public Task<PlaylistDetail> Playlist(long id) =>
            Playlists.TryGetValue(id, out var p) ? Task.FromResult(p) : Task.FromException<PlaylistDetail>(new ApiException(404, "not_found", "playlist not found"));
    }

    /// <summary>The app's side: what it was told to play, the switches it made, and what it said.</summary>
    sealed class Target : ICastTarget
    {
        public readonly List<(PlayRequest Request, string Banner)> Videos = new List<(PlayRequest, string)>();
        public readonly List<(long[] Tracks, int At, bool Shuffle, long FromMs, string Banner)> Queues = new List<(long[], int, bool, long, string)>();
        public readonly List<(long Channel, string Banner)> Channels = new List<(long, string)>();
        public readonly List<string> Said = new List<string>();
        public readonly List<(long Profile, string Ticket)> Switches = new List<(long, string)>();
        /// <summary>When set, a switch is answered only once it is.</summary>
        public TaskCompletionSource<bool>? SwitchHeld;
        /// <summary>When set, a switch fails with it.</summary>
        public Exception? Refuse;

        public string ReceiverId => "tizen-1";
        public bool Shown { get; set; } = true;
        public long? Profile { get; set; } = 2;

        public async Task SwitchTo(long profile, string ticket)
        {
            Switches.Add((profile, ticket));
            if (SwitchHeld != null) await SwitchHeld.Task;
            if (Refuse != null) throw Refuse;
            Profile = profile;
        }

        public void Video(PlayRequest request, string banner) => Videos.Add((request, banner));
        public void Music(IReadOnlyList<ItemSummary> tracks, int at, bool shuffle, long fromMs, string banner) => Queues.Add((tracks.Select(t => t.Id).ToArray(), at, shuffle, fromMs, banner));
        public void Channel(long channelId, string banner) => Channels.Add((channelId, banner));
        public void Say(string text) => Said.Add(text);
        public void Failed(Exception error) => Said.Add("failed: " + error.Message);
    }

    static ItemSummary Summary(long id, string kind, long positionMs = 0, bool watched = false) =>
        new ItemSummary { Id = id, LibraryId = 1, Kind = kind, Title = $"{kind} {id}", UserState = new UserState { PositionMs = positionMs, Watched = watched } };

    static ItemDetail Detail(long id, string kind, long? parent = null, long positionMs = 0, bool watched = false) =>
        new ItemDetail { Id = id, LibraryId = 1, Kind = kind, Title = $"{kind} {id}", ParentId = parent, UserState = new UserState { PositionMs = positionMs, Watched = watched } };

    static ServerEvent Cast(long? item = null, long? playlist = null, long? channel = null, long? startMs = null, bool shuffle = false, string? from = "Living room phone", string receiver = "tizen-1",
        int? audio = null, int? subtitle = null, long? profile = null, string? ticket = null) =>
        new ServerEvent
        {
            Type = "cast", ReceiverId = receiver, ItemId = item, PlaylistId = playlist, ChannelId = channel, StartMs = startMs, Shuffle = shuffle, From = from, AudioIndex = audio,
            SubtitleIndex = subtitle, ProfileId = profile, SwitchTicket = ticket,
        };

    [Fact]
    public void A_cast_from_another_profiles_remote_switches_to_it_with_its_ticket_then_plays_once()
    {
        server.Items[5] = Detail(5, "movie");
        receiver.Take(Cast(item: 5, startMs: 60_000, profile: 4, ticket: "t1"));
        Assert.Equal(new[] { (4L, "t1") }, target.Switches);
        Assert.Equal(new[] { (new PlayRequest(5, 60_000, false), "Playing from Living room phone") }, target.Videos);
        Assert.Equal(4, target.Profile);
    }

    [Fact]
    public void A_cast_plays_at_once_on_its_profile_already_and_does_nothing_from_another_without_a_ticket()
    {
        server.Items[5] = Detail(5, "movie");
        receiver.Take(Cast(item: 5, profile: 3));
        receiver.Take(Cast(item: 5, profile: 2, ticket: "t1"));
        Assert.Empty(target.Switches);
        Assert.Single(target.Videos);
    }

    [Fact]
    public void A_failed_switch_says_so_and_plays_nothing_nor_says_anything_once_signed_out()
    {
        server.Items[5] = Detail(5, "movie");
        target.Refuse = new TokenException(400, null);
        receiver.Take(Cast(item: 5, profile: 4, ticket: "t1"));
        Assert.Empty(target.Videos);
        Assert.Equal(new[] { Casts.SwitchFailed }, target.Said);
        Assert.Equal(2, target.Profile);
        target.SwitchHeld = new TaskCompletionSource<bool>();
        receiver.Take(Cast(item: 5, profile: 4, ticket: "t2"));
        target.Shown = false;
        target.SwitchHeld.SetResult(true);
        Assert.Equal(new[] { Casts.SwitchFailed }, target.Said);
    }

    [Fact]
    public void A_cast_heard_during_the_switch_is_weighed_as_the_profile_switched_to()
    {
        server.Items[5] = Detail(5, "movie");
        server.Items[6] = Detail(6, "movie");
        target.SwitchHeld = new TaskCompletionSource<bool>();
        receiver.Take(Cast(item: 5, profile: 4, ticket: "t1"));
        // One for the profile the TV leaves, and the same remote's next, with a ticket of its own.
        receiver.Take(Cast(item: 5, from: "Kitchen tablet"));
        receiver.Take(Cast(item: 6, profile: 4, ticket: "t2"));
        target.SwitchHeld.SetResult(true);
        Assert.Equal(new[] { (4L, "t1") }, target.Switches);
        Assert.Equal(new[] { (new PlayRequest(6, 0, false), "Playing from Living room phone") }, target.Videos);
    }

    [Fact]
    public void A_movie_or_an_episode_plays_with_the_tracks_picked_on_the_remotes_page()
    {
        server.Items[5] = Detail(5, "movie", positionMs: 600_000);
        receiver.Take(Cast(item: 5, startMs: 60_000, audio: 2, subtitle: -1));
        receiver.Take(Cast(item: 5, subtitle: 3));
        Assert.Equal(new[]
        {
            new PlayRequest(5, 60_000, false, AudioIndex: 2, SubtitleIndex: -1),
            new PlayRequest(5, 600_000, false, SubtitleIndex: 3),
        }, target.Videos.Select(v => v.Request));
    }

    [Fact]
    public void A_movie_or_an_episode_plays_from_where_it_was_sent_else_where_Play_resumes_saying_where_it_came_from()
    {
        server.Items[5] = Detail(5, "movie", positionMs: 600_000);
        server.Items[6] = Detail(6, "episode", positionMs: 600_000, watched: true);
        receiver.Take(Cast(item: 5, startMs: 60_000));
        receiver.Take(Cast(item: 5));
        receiver.Take(Cast(item: 6, from: null));
        Assert.Equal(new[]
        {
            (new PlayRequest(5, 60_000, false), "Playing from Living room phone"),
            (new PlayRequest(5, 600_000, false), "Playing from Living room phone"),
            (new PlayRequest(6, 0, false), "Playing from another device"),
        }, target.Videos);
        Assert.Equal(4_000, Casts.BannerMs);
    }

    [Fact]
    public void A_show_or_a_season_plays_its_next_episode_or_says_there_is_nothing_to_play()
    {
        server.Items[20] = Detail(20, "show");
        server.Items[21] = Detail(21, "season");
        server.Next[20] = Summary(23, "episode", positionMs: 90_000);
        receiver.Take(Cast(item: 20));
        receiver.Take(Cast(item: 21));
        Assert.Equal(new[] { (new PlayRequest(23, 90_000, false), "Playing from Living room phone") }, target.Videos);
        Assert.Equal(new[] { Casts.NothingToPlay }, target.Said);
    }

    [Fact]
    public void An_album_plays_its_tracks_in_order_or_shuffled_from_a_random_one()
    {
        server.Items[30] = Detail(30, "album");
        server.Children_[30] = new[] { Summary(31, "track"), Summary(32, "track"), Summary(33, "track") };
        receiver.Take(Cast(item: 30));
        receiver.Take(Cast(item: 30, shuffle: true));
        Assert.Equal(2, target.Queues.Count);
        Assert.All(target.Queues, q => Assert.Equal(new long[] { 31, 32, 33 }, q.Tracks));
        Assert.Equal((0, false, 0L), (target.Queues[0].At, target.Queues[0].Shuffle, target.Queues[0].FromMs));
        Assert.True(target.Queues[1].Shuffle);
        Assert.InRange(target.Queues[1].At, 0, 2);
    }

    [Fact]
    public void An_artist_plays_their_tracks_album_by_album()
    {
        server.Items[40] = Detail(40, "artist");
        server.Children_[40] = new[] { Summary(41, "album"), Summary(42, "album") };
        server.Children_[41] = new[] { Summary(43, "track"), Summary(44, "track") };
        server.Children_[42] = new[] { Summary(45, "track") };
        receiver.Take(Cast(item: 40));
        Assert.Equal(new long[] { 43, 44, 45 }, target.Queues.Single().Tracks);
        Assert.Equal((0, false), (target.Queues.Single().At, target.Queues.Single().Shuffle));
    }

    [Fact]
    public void A_track_plays_in_its_album_from_it_at_the_place_sent_and_one_without_an_album_alone()
    {
        server.Items[52] = Detail(52, "track", parent: 50);
        server.Items[60] = Detail(60, "track");
        server.Children_[50] = new[] { Summary(51, "track"), Summary(52, "track"), Summary(53, "track") };
        receiver.Take(Cast(item: 52, startMs: 30_000, shuffle: true));
        receiver.Take(Cast(item: 60));
        Assert.Equal(new[] { (new long[] { 51, 52, 53 }, 1, false, 30_000L), (new long[] { 60 }, 0, false, 0L) },
            target.Queues.Select(q => (q.Tracks, q.At, q.Shuffle, q.FromMs)));
    }

    [Fact]
    public void A_playlist_plays_music_as_the_queue_and_video_in_the_player_through_the_playlist()
    {
        server.Playlists[3] = new PlaylistDetail { Id = 3, Kind = "audio", Entries = new[] { 71L, 72, 73 }.Select(id => new PlaylistEntry { EntryId = id, Item = Summary(id, "track") }).ToList() };
        server.Playlists[4] = new PlaylistDetail { Id = 4, Kind = "video", Entries = new[] { new PlaylistEntry { EntryId = 1, Item = Summary(81, "movie", positionMs: 120_000) } } };
        server.Playlists[5] = new PlaylistDetail { Id = 5, Kind = "video" };
        receiver.Take(Cast(playlist: 3));
        receiver.Take(Cast(playlist: 4, shuffle: true));
        receiver.Take(Cast(playlist: 5));
        Assert.Equal(new long[] { 71, 72, 73 }, target.Queues.Single().Tracks);
        Assert.Equal((0, false), (target.Queues.Single().At, target.Queues.Single().Shuffle));
        Assert.Equal(new PlayRequest(81, 120_000, false, Queue: new PlayQueue.Playlist(4)), target.Videos.Single().Request);
        Assert.Equal(new[] { Casts.NothingToPlay }, target.Said);
    }

    [Fact]
    public void A_channel_is_tuned_at_once()
    {
        receiver.Take(Cast(channel: 7, from: "Firefox on Windows"));
        Assert.Equal(new[] { (7L, "Playing from Firefox on Windows") }, target.Channels);
    }

    [Fact]
    public void Another_TVs_cast_or_one_while_the_app_doesnt_show_does_nothing()
    {
        server.Items[5] = Detail(5, "movie");
        receiver.Take(Cast(item: 5, receiver: "tizen-2"));
        target.Shown = false;
        receiver.Take(Cast(item: 5));
        Assert.Empty(target.Videos);
        Assert.Empty(target.Said);
    }

    [Fact]
    public void A_cast_overtaken_by_a_newer_one_or_hidden_away_while_asked_for_plays_nothing()
    {
        server.Items[5] = Detail(5, "movie");
        server.Items[6] = Detail(6, "movie");
        server.Held = new TaskCompletionSource<bool>();
        receiver.Take(Cast(item: 5));
        var first = server.Held;
        server.Held = null;
        receiver.Take(Cast(item: 6));
        first.SetResult(true);
        Assert.Equal(new long[] { 6 }, target.Videos.Select(v => v.Request.ItemId));
        server.Held = new TaskCompletionSource<bool>();
        receiver.Take(Cast(item: 5));
        target.Shown = false;
        server.Held.SetResult(true);
        Assert.Single(target.Videos);
        Assert.Empty(target.Said);
    }

    [Fact]
    public void Says_why_when_what_it_was_sent_cant_be_found()
    {
        receiver.Take(Cast(item: 9));
        Assert.Equal(new[] { "failed: item not found" }, target.Said);
    }
}

/// <summary>Music a cast starts: the track it sent from where it says.</summary>
public class MusicCastTests
{
    [Fact]
    public void The_track_a_cast_sent_starts_where_it_says()
    {
        var server = new FakeMusicServer();
        var player = new MusicPlayer(server, () => new FakeEngine(), new FakeScheduler());
        var tracks = Enumerable.Range(1, 2).Select(i => new ItemSummary { Id = 10 + i, Kind = "track", Title = $"Track {i}", ParentId = 9, DurationMs = 200_000 }).ToList();
        player.Play(tracks, 1, false, 30_000);
        Assert.Equal(new[] { "12 at 30000" }, server.Asked);
    }
}

/// <summary>This TV as a receiver of the profile's remotes' casts: the event stream says so.</summary>
public class ReceiverStreamTests
{
    const string Home = "https://pley.example.com";

    [Fact]
    public void Names_itself_by_its_install_id_a_TV_taking_titles_and_channels_where_Live_TV_shows()
    {
        Assert.Equal("receiver=tizen-0a1b&device_kind=tv&casts=item,switch", Casts.ReceiverQuery("tizen-0a1b", false));
        Assert.Equal("receiver=tizen-0a1b&device_kind=tv&casts=item,live,switch", Casts.ReceiverQuery("tizen-0a1b", true));
    }

    [Fact]
    public async Task Opens_the_event_stream_as_one_and_again_when_Live_TV_comes_or_goes()
    {
        var store = new MemoryKeyValue();
        store.Set("pley.installId", "tizen-0a1b");
        var scheduler = new FakeScheduler();
        var server = new FakeServer();
        var urls = new List<string>();
        var state = new AppState(new AppPlatform("tizen", "client", store), server.Transport, (url, headers, onOpen, onText, cancel) =>
        {
            urls.Add(url);
            onOpen();
            return new TaskCompletionSource<EventEnd>().Task;
        }, scheduler);
        state.Use(Home);
        await state.TokenStore!.Install(new StoredTokens(Home, "pley_at_0", "pley_rt_0", scheduler.Now + 3_600_000, "library play", Home + "/t", Home + "/r"), "library play");
        state.Listen(true);
        await Task.Delay(50);
        // What it said already: nothing opens again.
        state.TakesLive(false);
        state.TakesLive(true);
        await Task.Delay(50);
        state.TakesLive(true);
        await Task.Delay(50);
        Assert.Equal(new[]
        {
            Home + "/api/events?receiver=tizen-0a1b&device_kind=tv&casts=item,switch",
            Home + "/api/events?receiver=tizen-0a1b&device_kind=tv&casts=item,live,switch",
        }, urls);
        state.Listen(false);
    }
}

/// <summary>A title cast with the tracks picked on the remote's page plays with them, for that play.</summary>
public class CastTracksTests
{
    readonly PlayerHarness h = new PlayerHarness();

    [Fact]
    public void The_tracks_sent_play_in_place_of_the_profiles_choices_without_being_saved()
    {
        h.Add(PlayerHarness.Movie() with { Files = new[] { PlayerHarness.FileOf(10) with { SelectedSubtitle = 3 } } });
        h.Start(new PlayRequest(1, 0, false, AudioIndex: 2, SubtitleIndex: -1));
        h.Play(4_000);
        h.Model.Pause();
        Assert.Equal(2, h.LastBody["audio_index"]);
        Assert.Equal(new PlayingTracks(2, -1), h.Source.ReportTracks.Single());
        Assert.True(h.State.Subtitles.Single(s => s.Index == null).Selected);
        Assert.Empty(h.Source.Streams);
    }

    [Fact]
    public void Tracks_the_file_doesnt_have_leave_it_to_the_profiles_choices()
    {
        // Another version's tracks: the audio is the server's pick, the subtitles the profile's.
        h.Add(PlayerHarness.Movie() with { Files = new[] { PlayerHarness.FileOf(10) with { SelectedSubtitle = 3 } } });
        h.Start(new PlayRequest(1, 0, false, AudioIndex: 9, SubtitleIndex: 8));
        h.Play(4_000);
        h.Model.Pause();
        Assert.False(h.LastBody.ContainsKey("audio_index"));
        Assert.Equal(new PlayingTracks(1, 3), h.Source.ReportTracks.Single());
    }
}
