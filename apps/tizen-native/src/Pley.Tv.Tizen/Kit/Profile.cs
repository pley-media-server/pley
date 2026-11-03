using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Reflection;
using System.Threading;
using System.Threading.Tasks;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Path = System.IO.Path;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A profiling build's marks (docs/TV_PLATFORMS.md, "Profiling"): what happened when, how long
/// the UI thread was held and by what, the frame rate, memory, and the marks before a crash,
/// each a line to the address the build was made with. A build made without one has none of
/// this: every call here is left out when it's compiled, its arguments with it, and so is all
/// the code behind them.
/// </summary>
static class Profile
{
    /// <summary>A point in time for <see cref="Took"/>; 0 in a build without profiling, which never reads the clock for it.</summary>
#if PROFILE
    public static long Start() => Stopwatch.GetTimestamp();
#else
    public static long Start() => 0;
#endif

    /// <summary>Marks how long it has been since <paramref name="start"/> (<see cref="Start"/>).</summary>
    [Conditional("PROFILE")]
    public static void Took(string what, long start) => Run.Mark($"{what} took {Run.Ms(start):0.0} ms");

    [Conditional("PROFILE")]
    public static void Mark(string what) => Run.Mark(what);


    /// <summary>
    /// The UI thread goes into <paramref name="what"/>, a call into the TV (its player, its window):
    /// a hang there is named by the watchdog. <see cref="Leave"/> as it comes back.
    /// </summary>
    [Conditional("PROFILE")]
    public static void Enter(string what) => Run.Enter(what);

    [Conditional("PROFILE")]
    public static void Leave() => Run.Leave();

    /// <summary>Marks when <paramref name="task"/> ends, from now.</summary>
    [Conditional("PROFILE")]
    public static void Ends(string what, Task task)
    {
        var start = Start();
        task.ContinueWith(_ => Run.Mark($"{what} came in {Run.Ms(start):0} ms"), TaskScheduler.FromCurrentSynchronizationContext());
    }

    /// <summary>A picture fetched and decoded from <paramref name="start"/>: the first few marked, then a count every 20.</summary>
    [Conditional("PROFILE")]
    public static void Picture(long start) => Run.Picture(Run.Ms(start));

    /// <summary>
    /// Starts watching: the UI thread's stalls, memory, the frame rate (over everything, top
    /// right), and crashes; the marks of a run that ended without <see cref="End"/> are sent first.
    /// </summary>
    [Conditional("PROFILE")]
    public static void Begin(Window window, string dataDir) => Run.Begin(window, dataDir);

    /// <summary>A clean end: the run's marks, kept in case of a crash, go.</summary>
    [Conditional("PROFILE")]
    public static void End() => Run.End();

    /// <summary>Keys the listener sends go to <paramref name="onKey"/>, as the remote's: the app is driven from the computer.</summary>
    [Conditional("PROFILE")]
    public static void Keys(Action<string, bool> onKey) => Run.Keys(onKey);

    /// <summary>While the listener records (its "record"), <paramref name="animation"/> plays as many times slower as it asked, so its frames can be caught.</summary>
    [Conditional("PROFILE")]
    public static void Slowed(Animation animation) => Run.Slowed(animation);

#if PROFILE
    /// <summary>All the profiling's state, only ever touched in a profiling build.</summary>
    static class Run
    {
        static readonly Stopwatch Since = Stopwatch.StartNew();
        /// <summary>Milliseconds from the process's start to <see cref="Since"/> starting.</summary>
        static readonly double Before = StartOffset();
        static readonly HttpClient Http = new HttpClient { Timeout = TimeSpan.FromSeconds(2) };
        /// <summary>For asking for a command: the listener holds the ask until it has one, so a command comes at once.</summary>
        static readonly HttpClient Held = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };
        static readonly string? Address = typeof(Profile).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .FirstOrDefault(a => a.Key == "PleyMetrics")?.Value;
        static string last = "start";
        static string? trail;
        static Timer? beat, memory;
        static long beatAt;
        /// <summary>The call the UI thread is in, and since when (ms); null between calls.</summary>
        static volatile string? busy;
        static long busyAt;
        static Thread? watchdog;
        static int pictures;
        static double picturesMs;
        static Counter? counter;

        public static double Ms(long start) => (Stopwatch.GetTimestamp() - start) * 1000.0 / Stopwatch.Frequency;

        static double StartOffset()
        {
            try
            {
                return (DateTime.Now - Process.GetCurrentProcess().StartTime).TotalMilliseconds;
            }
            catch (Exception)
            {
                return 0;
            }
        }

        static double Now => Before + Since.Elapsed.TotalMilliseconds;

        /// <summary>
        /// Lines on their way, to the trail on the TV (Mark's) and the listener: written by a thread
        /// of their own, so a mark costs its caller a queue's add. Marked from the TV player's own
        /// threads (its buffering), a file written and a post started there held the player's
        /// callbacks, twenty at once after a seek, and the app died moments later.
        /// </summary>
        static readonly System.Collections.Concurrent.BlockingCollection<(string Line, bool Keep)> Outgoing =
            new System.Collections.Concurrent.BlockingCollection<(string, bool)>(new System.Collections.Concurrent.ConcurrentQueue<(string, bool)>(), 4096);
        static Thread? writer;
        static int memoryTicks;

        public static void Mark(string what)
        {
            last = what;
            Outgoing.TryAdd(($"profile {Now,8:0} ms  {what}", true));
        }

        public static void Send(string line) => Outgoing.TryAdd((line, false));

        /// <summary>The thread that writes the trail and posts the lines, in order.</summary>
        static void Write()
        {
            writer = new Thread(() =>
            {
                foreach (var (line, keep) in Outgoing.GetConsumingEnumerable())
                {
                    if (Address != null) _ = Post(line);
                    // On the TV too, at once: a native crash takes the posts on their way with the process.
                    if (!keep || trail == null) continue;
                    try
                    {
                        File.AppendAllText(trail, $"{DateTime.Now:HH:mm:ss.fff} {line}\n");
                    }
                    catch (Exception)
                    {
                        // Nowhere to write.
                    }
                }
            }) { IsBackground = true, Name = "pley-marks" };
            writer.Start();
        }

        static async Task Post(string line)
        {
            try
            {
                await Http.PostAsync(Address, new StringContent(line));
            }
            catch (Exception)
            {
                // No listener: nothing to tell.
            }
        }

        public static void Picture(double ms)
        {
            var n = Interlocked.Increment(ref pictures);
            picturesMs += ms;
            if (n <= 3 || n % 20 == 0) Mark($"picture #{n} {ms:0} ms (average {picturesMs / n:0} ms)");
        }

        public static void Begin(Window window, string dataDir)
        {
            Trail(dataDir);
            CatchCrashes(dataDir);
            Sampler(dataDir);
            Write();
            // Core's marks too (the relay's, Live TV's), from whatever thread they come on.
            Core.Marks.Listener = Mark;
            WatchStalls();
            WatchMemory();
            Watchdog();
            // Held here: NUI calls it from native code, which doesn't keep it from being collected.
            counter = new Counter(window);
            window.AddFrameUpdateCallback(counter);
            Commands(window, dataDir);
        }

        static Timer? asking, pressing;
        static Capture? capture;
        static bool asked;
        static Action<string, bool>? press;
        static readonly Queue<string> Pressed = new Queue<string>();
        static readonly string Tv = TvModel();

        static string TvModel() =>
            global::Tizen.System.Information.TryGetValue("http://tizen.org/system/model_name", out string model) ? model : "";

        public static void Keys(Action<string, bool> onKey) => press = onKey;

        /// <summary>
        /// What the listener asks for (scripts/metrics.mjs): the app asks, and the listener holds the
        /// ask until it has a command. "shot" has it capture its window (its views; the video is on a
        /// plane of its own and comes out black) and post the PNG to the listener's /shot; "keys Down
        /// Down OK" presses those keys; "record" catches frames (<see cref="Record"/>).
        /// </summary>
        static void Commands(Window window, string dataDir)
        {
            if (Address == null) return;
            var root = Address.TrimEnd('/');
            asking = new Timer(250);
            asking.Tick += (_, _) =>
            {
                if (!asked) _ = Ask(window, dataDir, root);
                return true;
            };
            asking.Start();
        }

        static async Task Ask(Window window, string dataDir, string root)
        {
            asked = true;
            try
            {
                // Back on the UI thread after the await: NUI's synchronization context.
                var wanted = (await Held.GetStringAsync($"{root}/command?tv={Uri.EscapeDataString(Tv)}&hold=1")).Trim();
                if (wanted == "shot") Shoot(window, dataDir, root);
                else if (wanted.StartsWith("keys ", StringComparison.Ordinal)) Press(wanted.Substring(5).Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries));
                else if (wanted.StartsWith("record ", StringComparison.Ordinal)) Record(window, dataDir, root, wanted.Substring(7).Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries));
            }
            catch (Exception)
            {
                // No listener: nothing asked.
            }
            finally
            {
                asked = false;
            }
        }

        /// <summary>The remote's keys by their names (Up, Down, Left, Right, OK, Back, Menu, or a key's own name), 300 ms apart: a page a key opens has a moment to come in before the next.</summary>
        static void Press(IEnumerable<string> keys)
        {
            foreach (var key in keys)
                Pressed.Enqueue(key switch { "OK" => "Return", "Back" => "XF86Back", "Menu" => "XF86Menu", _ => key });
            if (pressing == null)
            {
                pressing = new Timer(300);
                pressing.Tick += (_, _) =>
                {
                    if (Pressed.Count == 0) return false;
                    var key = Pressed.Dequeue();
                    Mark($"key: {key}");
                    press?.Invoke(key, true);
                    press?.Invoke(key, false);
                    return Pressed.Count > 0;
                };
            }
            if (!pressing.IsRunning()) pressing.Start();
        }

        static void Shoot(Window window, string dataDir, string root)
        {
            var file = Path.Combine(dataDir, "shot.png");
            // The last one is let go now that it's done: disposed in its own Finished, NUI was still in it.
            capture?.Dispose();
            capture = new Capture();
            capture.Finished += (_, e) =>
            {
                var ok = e.Success;
                Task.Run(async () =>
                {
                    try
                    {
                        if (!ok) throw new IOException("the capture failed");
                        using var body = new ByteArrayContent(File.ReadAllBytes(file));
                        using var shot = new HttpClient { Timeout = TimeSpan.FromSeconds(20) };
                        await shot.PostAsync($"{root}/shot?tv={Uri.EscapeDataString(Tv)}", body);
                    }
                    catch (Exception error)
                    {
                        Mark($"screenshot: {error.GetType().Name} {error.Message}");
                    }
                });
            };
            var size = window.WindowSize;
            capture.Start(window.GetDefaultLayer(), new Size(size.Width, size.Height), file, Color.Black);
            Mark($"screenshot: {size.Width}x{size.Height} asked");
        }

        static float slowBy = 1;
        static Timer? recording;

        public static void Slowed(Animation animation)
        {
            if (slowBy > 1) animation.SpeedFactor = 1 / slowBy;
        }

        /// <summary>
        /// "record 20 6000 0,0,960,1080 Right": animations slowed 20 times (those that say they can
        /// be, <see cref="Slowed"/>), the keys pressed, and the window's area (in its pixels) caught
        /// as JPEGs one after another for 6 s. A capture takes the TV a moment, so a 200 ms
        /// animation gives a frame or two at its own speed; slowed, it gives dozens. Each frame goes
        /// to the listener's /frame with when it was caught at the animation's own speed, then
        /// /recorded says how many came.
        /// </summary>
        static void Record(Window window, string dataDir, string root, string[] words)
        {
            if (recording != null || words.Length < 3) return;
            var number = System.Globalization.CultureInfo.InvariantCulture;
            var slow = Math.Max(1, float.Parse(words[0], number));
            var forMs = int.Parse(words[1], number);
            var area = words[2].Split(',').Select(v => int.Parse(v, number)).ToArray();
            var dir = Path.Combine(dataDir, "frames");
            Forget(dir);
            Directory.CreateDirectory(dir);
            var frames = new List<(string File, double At)>();
            Capture? last = null;
            var catching = false;
            var from = Now;
            slowBy = slow;
            Mark($"record: {slow}x slower for {forMs} ms, {words[2]}");
            Press(words.Skip(3));
            recording = new Timer(5);
            recording.Tick += (_, _) =>
            {
                if (catching) return true;
                // Disposed here, not in its own Finished: NUI is still in it there.
                last?.Dispose();
                last = null;
                if (Now - from >= forMs)
                {
                    slowBy = 1;
                    recording = null;
                    Mark($"record: {frames.Count} frames");
                    _ = SendFrames(root, frames, slow);
                    return false;
                }
                var file = Path.Combine(dir, $"{frames.Count:000}.jpg");
                var at = Now - from;
                var capture = new Capture();
                capture.Finished += (_, e) =>
                {
                    if (e.Success) frames.Add((file, at));
                    catching = false;
                };
                capture.SetImageQuality(92);
                catching = true;
                last = capture;
                capture.Start(window.GetDefaultLayer(), new Position(area[0], area[1]), new Size(area[2], area[3]), file, Color.Black);
                return true;
            };
            recording.Start();
        }

        static async Task SendFrames(string root, List<(string File, double At)> frames, float slow)
        {
            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(20) };
            var tv = Uri.EscapeDataString(Tv);
            for (var i = 0; i < frames.Count; i++)
            {
                try
                {
                    using var body = new ByteArrayContent(File.ReadAllBytes(frames[i].File));
                    await http.PostAsync($"{root}/frame?tv={tv}&n={i}&t={frames[i].At / slow:0.0}", body);
                }
                catch (Exception error)
                {
                    Mark($"record: frame {i}: {error.GetType().Name} {error.Message}");
                }
            }
            try
            {
                await http.PostAsync($"{root}/recorded?tv={tv}&n={frames.Count}", new StringContent(""));
            }
            catch (Exception)
            {
                // No listener: nothing to tell.
            }
        }

        public static void End()
        {
            try
            {
                if (trail != null) File.Delete(trail);
            }
            catch (Exception)
            {
                // Gone already.
            }
            trail = null;
            Forget(samples);
        }

        static string? samples;

        /// <summary>
        /// The app's threads looked at from outside it every 250 ms, by a shell of its own: each one's
        /// state (R running, S sleeping, D waiting on the kernel, T stopped) and the kernel call it
        /// waits in, written to a ring of files. The app freezes whole now and then, its watchdog and
        /// writer with it, so nothing in it can tell why; the shell is no .NET thread, so a pause of
        /// .NET's doesn't stop it, and if the TV stops the app's processes, the samples stop with
        /// them. A run that ended without <see cref="End"/> has its last samples sent at the next start.
        /// </summary>
        static void Sampler(string dataDir)
        {
            var dir = Path.Combine(dataDir, "samples");
            try
            {
                if (Directory.Exists(dir))
                {
                    var found = new System.Collections.Generic.List<(double At, string Text)>();
                    foreach (var f in Directory.GetFiles(dir, "s.*"))
                    {
                        if (f.EndsWith(".tmp", StringComparison.Ordinal)) continue;
                        var text = File.ReadAllText(f);
                        var first = text.Split('\n')[0];
                        if (first.StartsWith("at ", StringComparison.Ordinal)
                            && double.TryParse(first.Substring(3).Trim(), System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var at))
                            found.Add((at, text));
                    }
                    found.Sort((a, b) => a.At.CompareTo(b.At));
                    for (var i = Math.Max(0, found.Count - 8); i < found.Count; i++)
                    {
                        var line = "SAMPLE LAST RUN " + found[i].Text.Trim().Replace("\n", " | ");
                        Send(line.Length > 3900 ? line.Substring(0, 3900) : line);
                    }
                    Forget(dir);
                }
                Directory.CreateDirectory(dir);
                // The TV's clock since it started, which the samples count in: it lines them up with the marks.
                Mark($"uptime {File.ReadAllText("/proc/uptime").Trim()}");
                var script = Path.Combine(dataDir, "sampler.sh");
                File.WriteAllText(script, SamplerScript.Replace("\r", ""));
                var shell = Process.Start(new ProcessStartInfo("/bin/sh", $"{script} {Process.GetCurrentProcess().Id} {dir}") { UseShellExecute = false });
                Mark(shell != null ? $"sampler: started, process {shell.Id}" : "sampler: didn't start");
                samples = dir;
            }
            catch (Exception e)
            {
                Mark($"sampler: didn't start: {e.GetType().Name}: {e.Message}");
            }
        }

        static void Forget(string? dir)
        {
            try
            {
                if (dir != null && Directory.Exists(dir))
                    foreach (var f in Directory.GetFiles(dir)) File.Delete(f);
            }
            catch (Exception)
            {
                // Gone already.
            }
        }

        /// <summary>
        /// The sampler: shell built-ins in its loop (read, echo), so it forks little; a ring of 20
        /// files, each written whole then moved into place, so the last ones are always readable.
        /// </summary>
        const string SamplerScript = @"p=$1; d=$2; i=0
while [ -d /proc/$p ]; do
  f=$d/s.$i
  {
    read up idle < /proc/uptime; echo ""at $up""
    while read k v; do case $k in State:|VmRSS:|Threads:) printf '%s %s  ' ""$k"" ""$v"";; esac; done < /proc/$p/status; echo
    while read c; do printf '%s ' ""$c""; done < /proc/$p/cgroup; echo
    for t in /proc/$p/task/*; do
      s=; n=; w=
      read -r s < $t/stat; read -r n < $t/comm; read -r w < $t/wchan
      s=${s##*) }; echo ""${t##*/} $n ${s%% *} $w""
    done
  } > $f.tmp 2>/dev/null
  mv $f.tmp $f
  i=$(( (i + 1) % 20 ))
  sleep 0.25 2>/dev/null || usleep 250000
done
";

        /// <summary>The last 40 marks of a run that ended without <see cref="End"/> are sent; this run's go to the same file.</summary>
        static void Trail(string dataDir)
        {
            var file = Path.Combine(dataDir, "trail.txt");
            try
            {
                if (File.Exists(file))
                {
                    var lines = File.ReadAllLines(file);
                    for (var i = Math.Max(0, lines.Length - 40); i < lines.Length; i++) Send("TRAIL LAST RUN " + lines[i]);
                    File.Delete(file);
                }
            }
            catch (Exception)
            {
                // Unreadable.
            }
            trail = file;
        }

        /// <summary>An exception nothing caught is written down before the app goes, and sent at the next start.</summary>
        static void CatchCrashes(string dataDir)
        {
            var file = Path.Combine(dataDir, "crash.txt");
            try
            {
                if (File.Exists(file))
                {
                    Send("CRASH LAST RUN: " + File.ReadAllText(file).Replace("\n", " | "));
                    File.Delete(file);
                }
            }
            catch (Exception)
            {
                // Unreadable: nothing to send.
            }
            AppDomain.CurrentDomain.UnhandledException += (s, e) =>
            {
                try
                {
                    File.WriteAllText(file, $"{DateTime.Now:O} after: {last}\n{e.ExceptionObject}");
                }
                catch (Exception)
                {
                    // Nowhere to write.
                }
            };
            TaskScheduler.UnobservedTaskException += (s, e) => Send("UNOBSERVED TASK: " + e.Exception?.ToString().Replace("\n", " | "));
        }

        public static void Enter(string what)
        {
            Interlocked.Exchange(ref busyAt, Since.ElapsedMilliseconds);
            busy = what;
        }

        public static void Leave() => busy = null;

        /// <summary>
        /// A thread of its own watching the UI thread's beat: one missing for 1.5 s is a hang, told
        /// from this thread (the UI's own marks stop with it) with the call it's in and the mark
        /// before, again every 2 s while it lasts, and when it ends. A crash ends the marks with no
        /// hang told; a hang the TV ends by closing the app ends them after one.
        /// </summary>
        static void Watchdog()
        {
            watchdog = new Thread(() =>
            {
                long toldAt = 0;
                var hung = false;
                while (true)
                {
                    Thread.Sleep(250);
                    var now = Since.ElapsedMilliseconds;
                    var gap = now - Interlocked.Read(ref beatAt);
                    if (gap >= 1500 && (!hung || now - toldAt >= 2000))
                    {
                        hung = true;
                        toldAt = now;
                        var b = busy;
                        var line = $"profile {Now,8:0} ms  UI HUNG {gap} ms, in: {b ?? "(no call of the TV's)"}{(b != null ? $" for {now - Interlocked.Read(ref busyAt)} ms" : "")}, after: {last}, gc0={GC.CollectionCount(0)} gc2={GC.CollectionCount(2)}";
                        // To the trail too: a hang the TV ends by closing the app leaves it there.
                        Outgoing.TryAdd((line, true));
                    }
                    else if (hung && gap < 1500)
                    {
                        hung = false;
                        Send($"profile {Now,8:0} ms  UI back");
                    }
                }
            }) { IsBackground = true, Name = "pley-watchdog" };
            watchdog.Start();
        }

        /// <summary>A beat on the UI thread every 16 ms: one late by more than 50 ms is a stall, named by the mark before it.</summary>
        static void WatchStalls()
        {
            beat = new Timer(16);
            beatAt = Since.ElapsedMilliseconds;
            beat.Tick += (s, e) =>
            {
                var now = Since.ElapsedMilliseconds;
                var gap = now - Interlocked.Read(ref beatAt);
                Interlocked.Exchange(ref beatAt, now);
                if (gap > 66) Send($"profile {Now,8:0} ms  STALL {gap} ms on the UI thread, after: {last}");
                return true;
            };
            beat.Start();
        }

        /// <summary>The process's memory as the TV sees it (VmRSS, VmHWM) and the .NET heap, every 5 s.</summary>
        static void WatchMemory()
        {
            memory = new Timer(5000);
            memory.Tick += (s, e) =>
            {
                string rss = "?", peak = "?";
                try
                {
                    foreach (var line in File.ReadAllLines("/proc/self/status"))
                    {
                        if (line.StartsWith("VmRSS:")) rss = line.Substring(6).Trim();
                        else if (line.StartsWith("VmHWM:")) peak = line.Substring(6).Trim();
                    }
                }
                catch (Exception)
                {
                    // Not readable here.
                }
                // The TV's memory as a whole, and how readily it would close this app for it: a TV short
                // of memory freezes an app (every thread at once) and then closes it.
                string available = "?", free = "?", swap = "?", adj = "?";
                try
                {
                    foreach (var line in File.ReadAllLines("/proc/meminfo"))
                    {
                        if (line.StartsWith("MemAvailable:")) available = line.Substring(13).Trim();
                        else if (line.StartsWith("MemFree:")) free = line.Substring(8).Trim();
                        else if (line.StartsWith("SwapFree:")) swap = line.Substring(9).Trim();
                    }
                    adj = File.ReadAllText("/proc/self/oom_score_adj").Trim();
                }
                catch (Exception)
                {
                    // Not readable here.
                }
                Send($"profile {Now,8:0} ms  MEMORY rss={rss} peak={peak} heap={GC.GetTotalMemory(false) / 1048576.0:0.0} MB gc0={GC.CollectionCount(0)} gc2={GC.CollectionCount(2)} tv-available={available} tv-free={free} swap-free={swap} oom-adj={adj}, after: {last}");
                // To the trail too, every 30 s: the last before a close says what the TV had left.
                if (++memoryTicks % 6 == 0) Outgoing.TryAdd(($"profile {Now,8:0} ms  MEMORY rss={rss} tv-available={available} oom-adj={adj}", true));
                return true;
            };
            memory.Start();
        }

        /// <summary>
        /// The frames drawn a second and the longest frame, every half second, sent while something
        /// moves. Not shown: over the player it was in the way, and the rail says the build is a
        /// debug one. NUI draws only while something moves, so it reads 0 at rest.
        /// </summary>
        sealed class Counter : FrameUpdateCallbackInterface
        {
            const uint EveryMs = 500;

            readonly Timer timer = new Timer(EveryMs);
            int frames;
            /// <summary>The longest frame since the last reading, in microseconds (the update thread writes it).</summary>
            int longest;
            bool first;

            public Counter(Window window)
            {
                timer.Tick += (s, e) =>
                {
                    var count = Interlocked.Exchange(ref frames, 0);
                    var worst = Interlocked.Exchange(ref longest, 0);
                    if (count > 2) Send($"fps={count * 1000 / EveryMs} longest={worst / 1000f:0.0}");
                    return true;
                };
                timer.Start();
            }

            /// <summary>Each frame, on NUI's update thread; the first is marked.</summary>
            public override void OnUpdate(float elapsedSeconds)
            {
                if (!first)
                {
                    first = true;
                    Mark("first frame drawn");
                }
                Interlocked.Increment(ref frames);
                var us = (int)(elapsedSeconds * 1_000_000);
                int seen;
                while (us > (seen = Volatile.Read(ref longest)) && Interlocked.CompareExchange(ref longest, us, seen) != seen)
                {
                }
            }
        }
    }
#else
    /// <summary>
    /// A build without profiling has none of it: the calls above are left out where they're made,
    /// and these, all they name, do nothing.
    /// </summary>
    static class Run
    {
        public static double Ms(long start) => 0;
        public static void Mark(string what) { }
        public static void Picture(double ms) { }
        public static void Begin(Window window, string dataDir) { }
        public static void End() { }
        public static void Enter(string what) { }
        public static void Leave() { }
        public static void Keys(Action<string, bool> onKey) { }
        public static void Slowed(Animation animation) { }
    }
#endif
}
