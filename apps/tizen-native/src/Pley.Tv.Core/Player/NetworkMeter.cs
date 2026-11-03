using System;
using System.Collections.Generic;
using System.Globalization;

namespace Pley.Tv.Core.Player;

/// <summary>
/// How fast the TV's network brings what it plays, where its player doesn't say (Tizen's): from
/// the bytes its network interfaces have received (Linux's /proc/net/dev), read once a second. A
/// player with a full buffer takes only the video's own rate, so the fastest second of the last
/// ten is told: one in which the player was filling its buffer, as fast as the network let it.
/// What else the TV downloads at the time counts too, as it takes from the same network.
/// </summary>
public sealed class NetworkMeter
{
    /// <summary>The fastest second of this long is told.</summary>
    public const long WindowMs = 10_000;
    /// <summary>Readings closer together than this are left out: a second's rate from less is noise.</summary>
    public const long EveryMs = 900;

    readonly Queue<(long AtMs, long Bps)> rates = new Queue<(long AtMs, long Bps)>();
    long lastAtMs = -1;
    long lastBytes;

    /// <summary>The bytes received so far, <paramref name="receivedBytes"/>, read at <paramref name="atMs"/>.</summary>
    public void Add(long atMs, long receivedBytes)
    {
        if (lastAtMs >= 0 && atMs - lastAtMs < EveryMs) return;
        // A counter that went back (an interface come again) starts over from here.
        if (lastAtMs >= 0 && receivedBytes >= lastBytes)
            rates.Enqueue((atMs, (long)((receivedBytes - lastBytes) * 8_000.0 / (atMs - lastAtMs))));
        lastAtMs = atMs;
        lastBytes = receivedBytes;
        while (rates.Count > 0 && rates.Peek().AtMs < atMs - WindowMs) rates.Dequeue();
    }

    /// <summary>Bits per second, the fastest second of the last <see cref="WindowMs"/>; null before two readings, or when nothing came.</summary>
    public long? Bps
    {
        get
        {
            long best = 0;
            foreach (var r in rates) best = Math.Max(best, r.Bps);
            return best > 0 ? best : null;
        }
    }

    /// <summary>The bytes every interface but loopback has received, from /proc/net/dev's text; null when it has none.</summary>
    public static long? Received(string procNetDev)
    {
        long total = 0;
        var any = false;
        foreach (var line in procNetDev.Split('\n'))
        {
            // "  wlan0: 123456 789 0 0 ...": the name, then received bytes first.
            var colon = line.IndexOf(':');
            if (colon < 0) continue;
            var name = line.Substring(0, colon).Trim();
            if (name.Length == 0 || name == "lo") continue;
            var fields = line.Substring(colon + 1).Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries);
            if (fields.Length == 0 || !long.TryParse(fields[0], NumberStyles.None, CultureInfo.InvariantCulture, out var bytes)) continue;
            total += bytes;
            any = true;
        }
        return any ? total : null;
    }
}
