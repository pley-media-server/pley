using System;
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
/// this: every call here is left out when it's compiled, its arguments with it.
/// </summary>
static class Profile
{
    /// <summary>A point in time for <see cref="Took"/>; cheap enough to stay in any build.</summary>
    public static long Start() => Stopwatch.GetTimestamp();

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

    /// <summary>All the profiling's state, only ever touched in a profiling build.</summary>
    static class Run
    {
        static readonly Stopwatch Since = Stopwatch.StartNew();
        /// <summary>Milliseconds from the process's start to <see cref="Since"/> starting.</summary>
        static readonly double Before = StartOffset();
        static readonly HttpClient Http = new HttpClient { Timeout = TimeSpan.FromSeconds(2) };
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
            Write();
            // Core's marks too (the relay's, Live TV's), from whatever thread they come on.
            Core.Marks.Listener = Mark;
            WatchStalls();
            WatchMemory();
            Watchdog();
            // Held here: NUI calls it from native code, which doesn't keep it from being collected.
            counter = new Counter(window);
            window.AddFrameUpdateCallback(counter);
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
        }

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
                Send($"profile {Now,8:0} ms  MEMORY rss={rss} peak={peak} heap={GC.GetTotalMemory(false) / 1048576.0:0.0} MB gc0={GC.CollectionCount(0)} gc2={GC.CollectionCount(2)}, after: {last}");
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
}
