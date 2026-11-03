using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// The audio track chosen, on the TV's player: switched to once the player is ready, found among
/// the tracks it lists (a 2025 S95F leaves TrueHD out), and Info saying what plays.
/// </summary>
public class AudioTrackTests
{
    readonly PlayerHarness h = new PlayerHarness();

    static readonly MediaStream TrueHd = new MediaStream { Index = 1, Kind = "audio", Codec = "truehd", Language = "eng", Default = true, Channels = 8 };
    static readonly MediaStream Eac3 = new MediaStream { Index = 2, Kind = "audio", Codec = "eac3", Language = "eng", Channels = 8 };
    static readonly MediaStream Italian = new MediaStream { Index = 3, Kind = "audio", Codec = "eac3", Language = "ita", Channels = 6 };

    static ItemDetail Film(params MediaStream[] streams) => PlayerHarness.Movie() with
    {
        Files = new[] { PlayerHarness.FileOf(10) with { Streams = streams } },
    };

    [Fact]
    public void The_track_is_switched_to_once_the_player_is_ready_not_before()
    {
        h.Add(PlayerHarness.Movie());
        h.Start(new PlayRequest(1, 0, true), show: false);
        // Asked while it prepared, the TV's player said no and the file played its first track.
        Assert.Empty(h.Engine.AudioSwitches);
        h.Engine.Show();
        Assert.Equal(new[] { (0, (string?)"eng") }, h.Engine.AudioSwitches);
    }

    [Fact]
    public void A_track_after_one_the_player_leaves_out_is_found_at_its_place_in_the_players_list()
    {
        var streams = new[] { TrueHd, Eac3, Italian };
        Assert.Equal(1, PlaybackPlan.EngineAudioOrdinal(streams, Italian, 2));
        Assert.Equal(0, PlaybackPlan.EngineAudioOrdinal(streams, Eac3, 2));
        Assert.Null(PlaybackPlan.EngineAudioOrdinal(streams, TrueHd, 2));
        Assert.Same(Eac3, PlaybackPlan.EngineAudioStream(streams, 0, 2));
    }

    [Fact]
    public void A_player_listing_every_track_or_none_goes_by_the_files_order()
    {
        var streams = new[] { TrueHd, Eac3, Italian };
        Assert.Equal(0, PlaybackPlan.EngineAudioOrdinal(streams, TrueHd, 3));
        Assert.Equal(2, PlaybackPlan.EngineAudioOrdinal(streams, Italian, 0));
        // Fewer tracks for another reason than one it leaves out: the file's order, as before.
        Assert.Equal(2, PlaybackPlan.EngineAudioOrdinal(new[] { Eac3, Italian, TrueHd with { Codec = "dts" } }, TrueHd with { Codec = "dts" }, 2));
    }

    [Fact]
    public void Choosing_the_second_of_two_tracks_after_trueHd_switches_the_players_second()
    {
        h.Add(Film(PlayerHarness.Video, TrueHd, Eac3, Italian));
        h.Engine.Tracks.AddRange(new[] { new EngineTrack("eng", null, null), new EngineTrack("ita", null, null) });
        h.Start(new PlayRequest(1, 0, true));
        h.Model.SelectAudio(Italian.Index);
        Assert.Equal((1, "ita"), h.Engine.AudioSwitches.Last());
    }

    [Fact]
    public void Info_says_what_plays_when_the_player_leaves_the_chosen_track_out()
    {
        h.Add(Film(PlayerHarness.Video, TrueHd, Eac3));
        h.Engine.Tracks.Add(new EngineTrack("eng", null, null));
        h.Engine.PlayingTrack = 0;
        h.Start(new PlayRequest(1, 0, true));
        // TrueHD isn't among the player's tracks: nothing to switch to.
        Assert.Empty(h.Engine.AudioSwitches);
        h.Model.SetDetailsOpen(true);
        Assert.Equal(SessionText.AudioNotOffered(Details.AudioLabel(TrueHd), Details.AudioLabel(Eac3)), h.State.Details!.Why);
        Assert.Contains(h.State.Details.Rows, r => r.Value == Details.AudioLabel(Eac3));
    }
}
