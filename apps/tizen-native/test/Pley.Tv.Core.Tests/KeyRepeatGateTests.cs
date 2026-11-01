using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>D-pad rule 5: a press moves once; held 180 ms, focus moves on the app's clock at a flat pace; focus rests after 400 ms.</summary>
public class KeyRepeatGateTests
{
    [Fact]
    public void A_press_moves_once_and_held_180_ms_focus_moves_by_itself()
    {
        long now = 1000;
        var gate = new KeyRepeatGate(() => now);
        Assert.True(gate.Admit("Right", true));
        Assert.True(gate.Down);
        Assert.False(gate.Holding);
        now += 150;
        Assert.Null(gate.Tick());
        // Before the TV's repeats (400 ms), the hold begins, and its first move is due at once.
        now += 30;
        Assert.Equal("Right", gate.Tick());
        Assert.True(gate.Holding);
        now += 20;
        Assert.Null(gate.Tick());
        // The TV's repeats only say the key is still down.
        Assert.False(gate.Admit("Right", true));
        now += 40;
        Assert.Equal("Right", gate.Tick());
        Assert.True(gate.Admit("Right", false));
        Assert.False(gate.Down);
        Assert.Null(gate.Tick());
        Assert.True(gate.Admit("Return", true));
    }

    [Fact]
    public void A_quick_press_never_starts_a_hold()
    {
        long now = 1000;
        var gate = new KeyRepeatGate(() => now);
        gate.Admit("Left", true);
        now += 120;
        gate.Admit("Left", false);
        now += 500;
        Assert.Null(gate.Tick());
        Assert.False(gate.Holding);
    }

    [Fact]
    public void A_hold_says_when_it_ends_and_a_press_does_not()
    {
        long now = 1000;
        var gate = new KeyRepeatGate(() => now);
        var ended = 0;
        gate.HoldEnded += () => ended++;
        gate.Admit("Right", true);
        gate.Admit("Right", false);
        Assert.Equal(0, ended);
        gate.Admit("Right", true);
        now += 300;
        gate.Tick();
        gate.Admit("Right", false);
        Assert.Equal(1, ended);
        // A hold whose key-up was lost ends too.
        gate.Admit("Left", true);
        now += 300;
        gate.Tick();
        now += 600;
        gate.Tick();
        Assert.Equal(2, ended);
    }

    [Fact]
    public void Moves_keep_their_pace_whenever_the_frames_fall()
    {
        long now = 1000;
        var gate = new KeyRepeatGate(() => now);
        gate.Admit("Down", true);
        var moves = 0;
        // Two seconds of frames 17 ms apart, the TV repeating every 50 ms from 400 ms.
        for (var t = 0; t < 2000; t += 17)
        {
            now = 1000 + t;
            if (t >= 400 && t % 50 < 17) gate.Admit("Down", true);
            if (gate.Tick() != null) moves++;
        }
        // Nothing for 180 ms, then one every 45 ms: about 41.
        Assert.InRange(moves, 39, 43);
        Assert.True(gate.GlideMs > gate.Interval);
    }

    [Fact]
    public void Rests_400_ms_after_the_last_move_and_ends_a_hold_whose_key_up_was_lost()
    {
        long now = 1000;
        var gate = new KeyRepeatGate(() => now);
        gate.Admit("Down", true);
        now += 50;
        gate.Admit("Down", true);
        Assert.True(gate.Holding);
        Assert.False(gate.Rested());
        now += 600;
        // No repeat for 500 ms: the key-up was lost, and focus has rested since the last key.
        Assert.True(gate.Rested());
        Assert.False(gate.Holding);
        Assert.False(gate.Down);
        Assert.Null(gate.Tick());
        Assert.Equal(0, gate.UntilRest());
        gate.Admit("Down", true);
        Assert.False(gate.Holding);
    }
}
