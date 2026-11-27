using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>
/// Play now through Up Next's countdown, as the Android TV app's is (TvPlayerScreenTest: "Play now
/// holds Up Next's countdown", "with reduced motion Play now counts down as Next in 7"): its words,
/// the width it keeps, and how far its sweep reaches. Its words in other languages are
/// PlayerWordsTests'.
/// </summary>
public sealed class PlayNowCountdownTests
{
    [Fact]
    public void Play_now_holds_the_countdown_and_keeps_its_width_as_the_count_shortens()
    {
        var play = new PlayNowCountdown();
        play.Set(10, reducedMotion: false, nowMs: 0);
        Assert.Equal(("Play now (10)", "Play now (10)"), (play.Label, play.Widest));
        play.Set(9, reducedMotion: false, nowMs: 1_000);
        Assert.Equal(("Play now (9)", "Play now (10)"), (play.Label, play.Widest));
        // The label is never narrower than its widest words: 9's are narrower than 10's, and take 10's width.
        Assert.Equal(96f, PlayNowCountdown.LabelWidth(words: 88, widest: 96));
        Assert.Equal(96f, PlayNowCountdown.LabelWidth(words: 96, widest: 96));
        // No countdown (autoplay off): the words alone, at their own width.
        play.Set(null, reducedMotion: false, nowMs: 2_000);
        Assert.Equal(("Play now", "Play now"), (play.Label, play.Widest));
    }

    [Fact]
    public void With_reduced_motion_play_now_counts_down_as_next_in_7()
    {
        var play = new PlayNowCountdown();
        play.Set(7, reducedMotion: true, nowMs: 0);
        Assert.Equal(("Next in 7", "Next in 10"), (play.Label, play.Widest));
        // Its sweep steps once a second, to where that second starts.
        Assert.Equal(60f, play.SweepWidth(200, 0), 3);
        Assert.False(play.Moving(0));
        Assert.Equal(60f, play.SweepWidth(200, 900), 3);
        play.Set(6, reducedMotion: true, nowMs: 1_000);
        Assert.Equal(80f, play.SweepWidth(200, 1_000), 3);
        Assert.False(play.Moving(1_000));
    }

    [Fact]
    public void The_sweep_reaches_as_far_across_the_button_as_the_countdown_has_gone()
    {
        var play = new PlayNowCountdown();
        Assert.Equal((0f, false), (play.SweepWidth(200, 0), play.Moving(0)));
        play.Set(10, reducedMotion: false, nowMs: 0);
        Assert.Equal(0f, play.SweepWidth(200, 0), 3);
        Assert.Equal(10f, play.SweepWidth(200, 500), 3);
        Assert.True(play.Moving(500));
        Assert.Equal(20f, play.SweepWidth(200, 1_000), 3);
        play.Set(9, reducedMotion: false, nowMs: 1_000);
        Assert.Equal(30f, play.SweepWidth(200, 1_500), 3);
        // A wider button (its focus growing it is the view's scale, not this width): the same part of it.
        Assert.Equal(45f, play.SweepWidth(300, 1_500), 3);
        // The last second fills it.
        play.Set(1, reducedMotion: false, nowMs: 9_000);
        Assert.Equal(200f, play.SweepWidth(200, 10_000), 3);
    }

    [Fact]
    public void The_card_said_again_for_the_same_second_leaves_the_sweep_gliding_as_it_was()
    {
        var play = new PlayNowCountdown();
        play.Set(10, reducedMotion: false, nowMs: 0);
        // The player's state changed for something else, midway through the second.
        play.Set(10, reducedMotion: false, nowMs: 600);
        Assert.Equal(16f, play.SweepWidth(200, 800), 3);
        Assert.Equal(20f, play.SweepWidth(200, 1_000), 3);
        Assert.False(play.Moving(1_000));
    }

    [Fact]
    public void A_countdown_that_stops_empties_the_sweep()
    {
        var play = new PlayNowCountdown();
        play.Set(4, reducedMotion: false, nowMs: 0);
        Assert.True(play.SweepWidth(200, 500) > 0);
        play.Set(null, reducedMotion: false, nowMs: 500);
        Assert.Equal((0f, false, "Play now"), (play.SweepWidth(200, 600), play.Moving(600), play.Label));
    }

    [Fact]
    public void A_card_that_goes_and_comes_back_at_the_same_second_sweeps_from_that_seconds_start()
    {
        var play = new PlayNowCountdown();
        play.Set(10, reducedMotion: false, nowMs: 0);
        // Play now pressed at once: the card goes within its first second, and fades as it was.
        play.Gone();
        Assert.Equal("Play now (10)", play.Label);
        Assert.Equal(6f, play.SweepWidth(200, 300), 3);
        // The next episode's card, at 10 too: its sweep glides from empty, not resting where the first card's would have stopped.
        play.Set(10, reducedMotion: false, nowMs: 60_000);
        Assert.Equal(0f, play.SweepWidth(200, 60_000), 3);
        Assert.True(play.Moving(60_000));
        Assert.Equal(10f, play.SweepWidth(200, 60_500), 3);
        // A card without a countdown after one that went: the words alone, and no sweep.
        play.Gone();
        play.Set(null, reducedMotion: false, nowMs: 61_000);
        Assert.Equal((0f, false, "Play now"), (play.SweepWidth(200, 61_000), play.Moving(61_000), play.Label));
    }
}
