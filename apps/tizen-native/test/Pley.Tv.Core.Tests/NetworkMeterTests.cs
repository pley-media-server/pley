using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The TV's network speed from the bytes its interfaces received (TizenMediaEngine's, for the Info panel).</summary>
public class NetworkMeterTests
{
    const string Dev = "Inter-|   Receive                            |  Transmit\n"
        + " face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets\n"
        + "    lo: 5000000    1000    0    0    0     0          0         0  5000000    1000\n"
        + " wlan0: 125000000  90000    0    0    0     0          0         0  2000000    9000\n"
        + "  eth0: 0          0        0    0    0     0          0         0  0          0\n";

    [Fact]
    public void Every_interface_but_loopback_counts()
    {
        Assert.Equal(125_000_000, NetworkMeter.Received(Dev));
        Assert.Null(NetworkMeter.Received("Inter-|   Receive\n face |bytes\n    lo: 5000 10 0 0\n"));
        Assert.Null(NetworkMeter.Received(""));
    }

    [Fact]
    public void The_fastest_second_of_the_last_ten_is_told()
    {
        var m = new NetworkMeter();
        m.Add(0, 0);
        Assert.Null(m.Bps);
        // 21 MB in a second while the player fills its buffer: 168 Mbit/s.
        m.Add(1_000, 21_000_000);
        // Then only the video's own 12 MB a second (96 Mbit/s) once it's full.
        for (var s = 2; s <= 10; s++) m.Add(s * 1_000, 21_000_000 + (s - 1) * 12_000_000L);
        Assert.Equal(168_000_000, m.Bps);
        // Ten seconds on, the fast second is gone.
        m.Add(11_500, 21_000_000 + 9 * 12_000_000L + 18_000_000);
        Assert.Equal(96_000_000, m.Bps);
    }

    [Fact]
    public void Readings_too_close_and_a_counter_that_went_back_are_left_out()
    {
        var m = new NetworkMeter();
        m.Add(0, 1_000_000);
        // A reading 0.2 s on is noise: left out, and the next second measured from the first.
        m.Add(200, 1_500_000);
        m.Add(1_000, 2_000_000);
        Assert.Equal(8_000_000, m.Bps);
        // The interface came again from 0: no rate from it, and the next one counts from there.
        m.Add(2_000, 100);
        Assert.Equal(8_000_000, m.Bps);
        m.Add(3_000, 4_000_100);
        Assert.Equal(32_000_000, m.Bps);
    }

    [Fact]
    public void Nothing_received_is_not_known()
    {
        var m = new NetworkMeter();
        m.Add(0, 500);
        m.Add(1_000, 500);
        Assert.Null(m.Bps);
    }
}
