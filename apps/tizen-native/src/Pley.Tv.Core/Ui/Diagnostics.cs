using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core.Player;

namespace Pley.Tv.Core.Ui;

/// <summary>A labelled fact of Diagnostics: "Model", "QE55S95D".</summary>
public sealed record Fact(string Label, string Value);

/// <summary>Diagnostics' state.</summary>
/// <param name="Device">The app and the TV: "App", then the TV's facts.</param>
/// <param name="Lines">What it plays, one line each; empty until checked.</param>
/// <param name="Error">The check failed: "The check of this TV failed.", and Check again.</param>
public sealed record DiagnosticsState(IReadOnlyList<Fact> Device, IReadOnlyList<string> Lines, bool Probing, Exception? Error = null);

/// <summary>What Diagnostics says (the Android app's tv_diagnostics strings), in the language shown (Locales/*/diagnostics.json).</summary>
public static class DiagnosticsText
{
    public static string Title => Strings.T("diagnostics.title");
    public static string Device => Strings.T("diagnostics.device");
    public static string App => Strings.T("diagnostics.app");
    public static string Plays => Strings.T("diagnostics.plays");
    public static string Probing => Strings.T("diagnostics.probing");
    public static string ProbeFailed => Strings.T("diagnostics.probeFailed");
    public static string ProbeAgain => Strings.T("diagnostics.probeAgain");

    /// <summary>"Pley 0.1.0".</summary>
    public static string AppValue(string version) => $"Pley {version}";

    /// <summary>
    /// What Diagnostics says of the TV, label and value: its model, its Tizen and model year, its
    /// panel, its sound output and its tier (the Android app's deviceFacts, with what a Samsung TV
    /// tells in place of Android's memory and buffers).
    /// </summary>
    public static IReadOnlyList<Fact> DeviceFacts(TizenFacts f) => new[]
    {
        new Fact(Strings.T("diagnostics.facts.model"), f.Model),
        new Fact(Strings.T("diagnostics.facts.system"), SystemValue(f)),
        new Fact(Strings.T("diagnostics.facts.panel"), Panel(f)),
        new Fact(Strings.T("diagnostics.facts.soundOutput"), f.OutputMode ?? "—"),
        new Fact(Strings.T("diagnostics.facts.tier"), Tier(TizenCaps.Tier(f))),
    };

    static string Panel(TizenFacts f) => f.Uhd8k ? "8K" : f.Uhd ? "4K" : "HD";

    /// <summary>"Tizen 8.0 (2024 models)".</summary>
    static string SystemValue(TizenFacts f) => Strings.T("diagnostics.facts.systemValue",
        ("version", f.Tizen.ToString("0.0", CultureInfo.InvariantCulture)), ("year", f.Year.ToString(CultureInfo.InvariantCulture)));

    /// <summary>The tier the TV is counted in ("low", "mid", "high"), in words.</summary>
    static string Tier(string tier) => tier switch
    {
        "low" => Strings.T("diagnostics.tiers.low"),
        "mid" => Strings.T("diagnostics.tiers.mid"),
        "high" => Strings.T("diagnostics.tiers.high"),
        _ => tier,
    };

    /// <summary>
    /// What the TV told the server it plays, one line each (the web TV app's caps lines for the same
    /// TV): HDR, the decoders and their limits, the sound it decodes and sends to a receiver, the
    /// streams it reads.
    /// </summary>
    public static IReadOnlyList<string> Lines(TizenFacts f, IReadOnlyDictionary<string, object?>? caps = null)
    {
        var c = caps ?? TizenCaps.Of(f);
        var hdr = ((IEnumerable<string>)c["hdr_types"]!).Select(t => t switch
        {
            "hdr10" => "HDR10",
            "hdr10plus" => "HDR10+",
            "hlg" => "HLG",
            _ => "Dolby Vision",
        }).ToList();
        var video = ((IEnumerable<Dictionary<string, object?>>)c["video"]!).Select(v => $"{v["codec"]} {v["max_width"]}×{v["max_height"]}@{v["max_fps"]}");
        var decode = (IEnumerable<string>)c["audio_decode"]!;
        var passthrough = ((IEnumerable<string>)c["audio_passthrough"]!).ToList();
        var segments = (IEnumerable<string>)((Dictionary<string, object?>)c["hls"]!)["segments"]!;
        var sent = passthrough.Count > 0 ? string.Join(", ", passthrough) : Strings.T("diagnostics.lines.nothing");
        return new[]
        {
            Strings.T("diagnostics.lines.tv", ("model", f.Model)),
            SystemValue(f),
            Strings.T("diagnostics.lines.panel", ("panel", Panel(f))),
            Strings.T("diagnostics.lines.hdr", ("formats", hdr.Count > 0 ? string.Join(", ", hdr) : Strings.T("diagnostics.lines.none"))),
            Strings.T("diagnostics.lines.video", ("decoders", string.Join(", ", video))),
            Strings.T("diagnostics.lines.audioDecoded", ("formats", string.Join(", ", decode))),
            f.OutputMode != null
                ? Strings.T("diagnostics.lines.sentWithOutput", ("formats", sent), ("output", f.OutputMode))
                : Strings.T("diagnostics.lines.sent", ("formats", sent)),
            Strings.T("diagnostics.lines.streams", ("segments", string.Join(", ", segments))),
        };
    }
}

/// <summary>
/// Diagnostics (the Android app's DiagnosticsViewModel): what this TV decodes and shows, and why,
/// for when something doesn't play. The TV is asked what it is (<paramref name="probe"/>) as the
/// page opens, and again with Check again after a check that failed.
///
/// The TV page (TvDiagnosticsScreen): "Diagnostics", then "This TV" with a line per fact (label
/// 180 dp wide, then the value), then "What it plays" with a line each; every line takes focus
/// (the accent fill, 36 dp at least, 16 dp in), so the remote scrolls through them; nothing acts
/// but Check again.
/// </summary>
public sealed class DiagnosticsModel
{
    readonly string version;
    readonly Func<Task<TizenFacts>> probe;
    readonly IScheduler scheduler;
    readonly Func<TizenFacts, IReadOnlyDictionary<string, object?>> caps;

    /// <param name="caps">The report the TV sends with its play requests; TizenCaps' own without.</param>
    public DiagnosticsModel(string version, Func<Task<TizenFacts>> probe, IScheduler scheduler, Func<TizenFacts, IReadOnlyDictionary<string, object?>>? caps = null)
    {
        this.version = version;
        this.probe = probe;
        this.scheduler = scheduler;
        this.caps = caps ?? (f => TizenCaps.Of(f));
        State = new Signal<DiagnosticsState>(new DiagnosticsState(new[] { AppFact }, Array.Empty<string>(), true));
        Probe();
    }

    public Signal<DiagnosticsState> State { get; }

    Fact AppFact => new Fact(DiagnosticsText.App, DiagnosticsText.AppValue(version));

    /// <summary>Asks the TV again (Check again).</summary>
    public void Probe()
    {
        State.Value = State.Value with { Probing = true, Error = null };
        Plays.When(scheduler, probe, facts =>
        {
            var device = new[] { AppFact }.Concat(DiagnosticsText.DeviceFacts(facts)).ToList();
            State.Value = new DiagnosticsState(device, DiagnosticsText.Lines(facts, caps(facts)), false);
        }, e => State.Value = State.Value with { Probing = false, Error = e });
    }
}
