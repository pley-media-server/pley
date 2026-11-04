using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Threading;
using Tizen.Network.WiFi;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A profiling build's trace of the network while a video plays, once a second, off the UI thread:
/// what each interface received, TCP's segments and retransmissions, the Wi-Fi link (signal, rate,
/// band, and the driver's retries and discards), the player's buffering and the bytes waiting
/// unread in the TV's sockets (a full queue is a player that reads slowly, an empty one a network
/// that brings slowly). A release build has none of it.
/// </summary>
static class NetTrace
{
#if PROFILE
    const int EveryMs = 1000;

    static readonly object gate = new object();
    static Timer? timer;
    static long lastAt;
    static Dictionary<string, long> lastRx = new Dictionary<string, long>();
    static long[]? lastTcp;
    static long[]? lastWireless;
    static int wifiTick;
    static string wifi = "";
    static volatile int buffered = -1;

    static volatile string? source;
    static Func<long>? position;
    static int speeding;

    /// <summary>The player's last buffering percent, as it said it.</summary>
    public static void Buffered(int percent) => buffered = percent;

    /// <summary>What plays, for <see cref="Speed"/>: its address (never written out: it holds a key) and where it is.</summary>
    public static void Playing(string url, Func<long> at)
    {
        source = url;
        position = at;
    }

    /// <summary>
    /// The listener's "speed": with the film paused, the TV fetches its stream's segments itself from
    /// where it is, one at a time as the player does, then the same segment again (ready on the
    /// server, so the network's own speed), then four at once, and says how long each took to start
    /// coming and how fast it came.
    /// </summary>
    public static void Speed()
    {
        if (Interlocked.CompareExchange(ref speeding, 1, 0) != 0) return;
        _ = System.Threading.Tasks.Task.Run(async () =>
        {
            try
            {
                await SpeedTest();
            }
            catch (Exception e)
            {
                Profile.Mark($"speed: {e.GetType().Name} {e.Message}");
            }
            finally
            {
                Volatile.Write(ref speeding, 0);
            }
        });
    }

    static async System.Threading.Tasks.Task SpeedTest()
    {
        var url = source;
        if (url == null)
        {
            Profile.Mark("speed: nothing has played");
            return;
        }
        using var http = new System.Net.Http.HttpClient { Timeout = TimeSpan.FromSeconds(30) };
        if (!url.Contains(".m3u8"))
        {
            // A file: its bytes from where it is, in 8 MB pieces.
            Profile.Mark("speed: a file, read in 8 MB ranges");
            var from = 0L;
            for (var i = 0; i < 6; i++, from += 8 << 20) await Fetch(http, new Uri(url), $"range {i}", from, 8 << 20);
            return;
        }
        var playlistUri = new Uri(url);
        var playlist = await http.GetStringAsync(playlistUri);
        if (playlist.Contains("#EXT-X-STREAM-INF"))
        {
            var variant = Lines(playlist).First(l => !l.StartsWith("#", StringComparison.Ordinal));
            playlistUri = new Uri(playlistUri, variant);
            playlist = await http.GetStringAsync(playlistUri);
        }
        var segments = new List<(Uri Uri, double Seconds)>();
        double next = 0;
        foreach (var line in Lines(playlist))
        {
            if (line.StartsWith("#EXTINF:", StringComparison.Ordinal))
                double.TryParse(line.Substring(8).Split(',')[0], NumberStyles.Float, CultureInfo.InvariantCulture, out next);
            else if (!line.StartsWith("#", StringComparison.Ordinal)) segments.Add((new Uri(playlistUri, line), next));
        }
        if (segments.Count == 0)
        {
            Profile.Mark("speed: the playlist lists no segments");
            return;
        }
        // The segment playing now, and on from there.
        var at = (position?.Invoke() ?? 0) / 1000.0;
        var start = 0;
        for (double sum = 0; start < segments.Count - 1 && sum + segments[start].Seconds <= at; start++) sum += segments[start].Seconds;
        Profile.Mark($"speed: {segments.Count} segments of about {segments[start].Seconds:0.#} s, from #{start} ({at:0} s)");

        // One at a time, as the player fetches: past what the player already has.
        var first = Math.Min(start + 3, segments.Count - 1);
        long bytes = 0;
        double ms = 0, waited = 0;
        var n = 0;
        for (var i = first; i < Math.Min(first + 8, segments.Count); i++, n++)
        {
            var r = await Fetch(http, segments[i].Uri, $"#{i} one at a time", 0, 0, segments[i].Seconds);
            bytes += r.Bytes;
            ms += r.Ms;
            waited += r.FirstByteMs;
        }
        if (n > 0) Profile.Mark($"speed: one at a time {bytes * 8 / ms / 1000:0.0} Mbps over {n} segments, {waited / n:0} ms each before the first byte");

        // The last again: the server has it, so this is the network (and the server's disk) alone.
        var again = Math.Min(first + n - 1, segments.Count - 1);
        for (var k = 0; k < 3; k++) await Fetch(http, segments[again].Uri, $"#{again} again", 0, 0, segments[again].Seconds);

        // Four at once, from those fetched: the network with more than one connection.
        var sw = System.Diagnostics.Stopwatch.StartNew();
        var four = await System.Threading.Tasks.Task.WhenAll(Enumerable.Range(first, Math.Min(4, n)).Select(i => Fetch(http, segments[i].Uri, $"#{i} of four at once", 0, 0, segments[i].Seconds, quiet: true)));
        Profile.Mark($"speed: four at once {four.Sum(f => f.Bytes) * 8 / sw.Elapsed.TotalMilliseconds / 1000:0.0} Mbps together");
    }

    /// <summary>
    /// The listener's "wifi": the access point the TV is joined to (its BSSID: which mesh node), and
    /// every access point of the same network in the TV's last scan, strongest first, with signal,
    /// band and rate. Other networks are left out: they are the neighbours'. With <paramref name="scan"/>,
    /// the TV scans first (the debug build has network.set for it), as its last scan may hold only its own.
    /// </summary>
    public static void WiFi(bool scan)
    {
        _ = System.Threading.Tasks.Task.Run(async () =>
        {
            try
            {
                // A scan by BSSID: the ordinary one lists a network once, however many access points it has.
                if (scan)
                {
                    var sw = System.Diagnostics.Stopwatch.StartNew();
                    await WiFiManager.BssidScanAsync();
                    Profile.Mark($"wifi: scanned in {sw.ElapsedMilliseconds} ms");
                }
                using var ap = WiFiManager.GetConnectedAP();
                if (ap == null)
                {
                    Profile.Mark("wifi: not joined to an access point");
                    return;
                }
                ap.Refresh();
                var me = ap.NetworkInformation;
                Profile.Mark($"wifi: joined to {me.Bssid} rssi {me.Rssi} dBm ({me.RssiLevel}) {me.Frequency} MHz rate {me.MaxSpeed} Mbps, network \"{me.Essid}\"");
                var found = (scan ? WiFiManager.GetFoundBssids() : WiFiManager.GetFoundAPs()).ToList();
                if (scan)
                {
                    // Each way of scanning, as TVs differ in which gives every access point.
                    Profile.Mark($"wifi: by BSSID {found.Count}, the ordinary scan {WiFiManager.GetFoundAPs().Count()}");
                    await WiFiManager.ScanSpecificAPAsync(me.Essid);
                    var named = WiFiManager.GetFoundSpecificAPs().ToList();
                    Profile.Mark($"wifi: a scan for \"{me.Essid}\" heard {named.Count}");
                    if (named.Count > found.Count(f => f.NetworkInformation.Essid == me.Essid)) found = named;
                }
                var ours = found.Where(f => f.NetworkInformation.Essid == me.Essid).OrderByDescending(f => f.NetworkInformation.Rssi).ToList();
                Profile.Mark($"wifi: the last scan heard {found.Count} access points, {ours.Count} of this network");
                foreach (var f in ours)
                {
                    var n = f.NetworkInformation;
                    Profile.Mark($"wifi:   {n.Bssid} rssi {n.Rssi} dBm {n.Frequency} MHz rate {n.MaxSpeed} Mbps{(n.Bssid == me.Bssid ? "  <- joined" : "")}");
                }
                // Access points the scan heard without a name (hidden, or a mesh's own links), by signal: no neighbour's name in them.
                foreach (var f in found.Where(f => string.IsNullOrEmpty(f.NetworkInformation.Essid)).OrderByDescending(f => f.NetworkInformation.Rssi).Take(8))
                {
                    var n = f.NetworkInformation;
                    Profile.Mark($"wifi:   unnamed {n.Bssid} rssi {n.Rssi} dBm {n.Frequency} MHz");
                }
                foreach (var f in found) f.Dispose();
            }
            catch (Exception e)
            {
                Profile.Mark($"wifi: {e.GetType().Name} {e.Message}");
            }
        });
    }

    static IEnumerable<string> Lines(string text) => text.Split('\n').Select(l => l.Trim()).Where(l => l.Length > 0);

    /// <summary>One request: how long until its first byte, how many came and how fast; a range when <paramref name="length"/> isn't 0.</summary>
    static async System.Threading.Tasks.Task<(long Bytes, double Ms, double FirstByteMs)> Fetch(System.Net.Http.HttpClient http, Uri uri, string what, long from, long length, double seconds = 0, bool quiet = false)
    {
        var sw = System.Diagnostics.Stopwatch.StartNew();
        using var request = new System.Net.Http.HttpRequestMessage(System.Net.Http.HttpMethod.Get, uri);
        if (length > 0) request.Headers.Range = new System.Net.Http.Headers.RangeHeaderValue(from, from + length - 1);
        using var response = await http.SendAsync(request, System.Net.Http.HttpCompletionOption.ResponseHeadersRead);
        var firstByte = sw.Elapsed.TotalMilliseconds;
        using var body = await response.Content.ReadAsStreamAsync();
        var buffer = new byte[256 * 1024];
        long bytes = 0;
        int read;
        while ((read = await body.ReadAsync(buffer, 0, buffer.Length)) > 0) bytes += read;
        var ms = sw.Elapsed.TotalMilliseconds;
        var mbps = bytes * 8 / Math.Max(1, ms - firstByte) / 1000;
        var needs = seconds > 0 ? $", the film needs {bytes * 8 / seconds / 1e6:0} Mbps here" : "";
        if (!quiet) Profile.Mark($"speed: {what}: {(int)response.StatusCode} {bytes / 1e6:0.0} MB, first byte {firstByte:0} ms, then {mbps:0.0} Mbps ({ms:0} ms in all){needs}");
        return (bytes, ms, firstByte);
    }

    public static void Start()
    {
        lock (gate)
        {
            if (timer != null) return;
            lastAt = 0;
            lastRx = new Dictionary<string, long>();
            lastTcp = null;
            lastWireless = null;
            wifiTick = 0;
            buffered = -1;
            timer = new Timer(_ => Sample(), null, 0, EveryMs);
        }
        Profile.Mark("net: tracing while the video plays");
    }

    public static void Stop()
    {
        lock (gate)
        {
            if (timer == null) return;
            timer.Dispose();
            timer = null;
        }
        Profile.Mark("net: tracing stopped");
    }

    static void Sample()
    {
        try
        {
            var now = Environment.TickCount64;
            var seconds = lastAt == 0 ? 0 : (now - lastAt) / 1000.0;
            lastAt = now;
            var parts = new List<string>();

            // Each interface's received bytes, as Mbps.
            var rx = Rx();
            var rates = rx.Where(kv => kv.Key != "lo")
                .Select(kv => seconds > 0 && lastRx.TryGetValue(kv.Key, out var before) ? $"{kv.Key} {(kv.Value - before) * 8 / seconds / 1e6:0.0}" : null)
                .Where(s => s != null).ToList();
            lastRx = rx;
            if (rates.Count > 0) parts.Add("rx Mbps " + string.Join(", ", rates));

            // TCP: segments in, retransmitted (ours: acks rarely are, so these are mostly what the TV sent again), errors in.
            if (Tcp() is { } tcp)
            {
                if (lastTcp != null && seconds > 0)
                    parts.Add($"tcp in {(tcp[0] - lastTcp[0]) / seconds:0}/s retrans {(tcp[1] - lastTcp[1]) / seconds:0}/s errs {tcp[2] - lastTcp[2]}");
                lastTcp = tcp;
            }

            // The Wi-Fi driver's view: link quality, signal and noise in dBm, and its discards and missed beacons.
            if (Wireless() is { } w)
            {
                var delta = lastWireless == null ? "" : $" (+{w[3] - lastWireless[3]} nwid +{w[4] - lastWireless[4]} crypt +{w[5] - lastWireless[5]} frag +{w[6] - lastWireless[6]} retry +{w[7] - lastWireless[7]} misc +{w[8] - lastWireless[8]} beacon)";
                parts.Add($"wireless link {w[0]} level {w[1]} noise {w[2]}{delta}");
                lastWireless = w;
            }

            // The access point as Tizen reports it, every few seconds: asking is slow.
            if (wifiTick++ % 3 == 0) wifi = WiFiNow();
            if (wifi.Length > 0) parts.Add(wifi);

            if (Queued() is { } q) parts.Add(q);
            parts.Add($"buffering {(buffered < 0 ? "?" : buffered + " %")}");
            Profile.Mark("net: " + string.Join(" | ", parts));
        }
        catch (Exception e)
        {
            Profile.Mark($"net: {e.GetType().Name} {e.Message}");
        }
    }

    static Dictionary<string, long> Rx()
    {
        var found = new Dictionary<string, long>();
        foreach (var line in File.ReadAllLines("/proc/net/dev").Skip(2))
        {
            var colon = line.IndexOf(':');
            if (colon < 0) continue;
            var name = line.Substring(0, colon).Trim();
            var fields = line.Substring(colon + 1).Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
            if (fields.Length > 0 && long.TryParse(fields[0], NumberStyles.Integer, CultureInfo.InvariantCulture, out var bytes)) found[name] = bytes;
        }
        return found;
    }

    /// <summary>InSegs, RetransSegs and InErrs from /proc/net/snmp's Tcp lines.</summary>
    static long[]? Tcp()
    {
        var lines = File.ReadAllLines("/proc/net/snmp").Where(l => l.StartsWith("Tcp:", StringComparison.Ordinal)).ToList();
        if (lines.Count < 2) return null;
        var names = lines[0].Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
        var values = lines[1].Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
        long Of(string name)
        {
            var i = Array.IndexOf(names, name);
            return i > 0 && i < values.Length && long.TryParse(values[i], NumberStyles.Integer, CultureInfo.InvariantCulture, out var v) ? v : 0;
        }
        return new[] { Of("InSegs"), Of("RetransSegs"), Of("InErrs") };
    }

    /// <summary>/proc/net/wireless's line: link, level, noise, then discarded nwid, crypt, frag, retry, misc and missed beacons.</summary>
    static long[]? Wireless()
    {
        if (!File.Exists("/proc/net/wireless")) return null;
        foreach (var line in File.ReadAllLines("/proc/net/wireless").Skip(2))
        {
            var colon = line.IndexOf(':');
            if (colon < 0) continue;
            var fields = line.Substring(colon + 1).Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
            if (fields.Length < 10) continue;
            long Num(string s) => long.TryParse(s.TrimEnd('.'), NumberStyles.Integer, CultureInfo.InvariantCulture, out var v) ? v : 0;
            return new[] { Num(fields[1]), Num(fields[2]), Num(fields[3]), Num(fields[4]), Num(fields[5]), Num(fields[6]), Num(fields[7]), Num(fields[8]), Num(fields[9]) };
        }
        return null;
    }

    static string WiFiNow()
    {
        try
        {
            using var ap = WiFiManager.GetConnectedAP();
            if (ap == null) return "";
            ap.Refresh();
            var n = ap.NetworkInformation;
            return $"wifi rssi {n.Rssi} dBm rate {n.MaxSpeed} Mbps {n.Frequency} MHz";
        }
        catch (Exception e)
        {
            return $"wifi: {e.GetType().Name}";
        }
    }

    /// <summary>The established TCP connections' unread bytes (rx_queue), largest first: the player's sockets among them.</summary>
    static string? Queued()
    {
        var queues = new List<long>();
        foreach (var file in new[] { "/proc/net/tcp", "/proc/net/tcp6" })
        {
            if (!File.Exists(file)) continue;
            foreach (var line in File.ReadAllLines(file).Skip(1))
            {
                var f = line.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
                // sl local remote st tx_queue:rx_queue ...; 01 is established.
                if (f.Length < 5 || f[3] != "01") continue;
                var q = f[4].Split(':');
                if (q.Length == 2 && long.TryParse(q[1], NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var rxq)) queues.Add(rxq);
            }
        }
        if (queues.Count == 0) return null;
        var top = queues.OrderByDescending(v => v).Take(3).Select(v => (v / 1024).ToString(CultureInfo.InvariantCulture) + " kB");
        return $"tcp conns {queues.Count} unread {string.Join(" ", top)}";
    }
#else
    public static void Playing(string url, Func<long> at) { }
    public static void Speed() { }
    public static void WiFi(bool scan) { }
    public static void Buffered(int percent) { }
    public static void Start() { }
    public static void Stop() { }
#endif
}
