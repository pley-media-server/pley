using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Live;

/// <summary>A region ("UK") gathering its groups ("UK| Sport", "UK| News"…), with their channels' count.</summary>
public sealed record GroupRegion(string Region, IReadOnlyList<LiveGroup> Groups, int Count);

/// <summary>
/// The groups as the categories list them: <paramref name="Regions"/> in alphabetical order, each
/// opening to its groups, then <paramref name="Others"/> (no region, or a region of one group) in
/// the provider's order. With fewer than two regions, every group is in the others.
/// </summary>
public sealed record GroupSections(IReadOnlyList<GroupRegion> Regions, IReadOnlyList<LiveGroup> Others);

/// <summary>Why a channel plays as HLS and not from its stream.ts, as far as its plan and the TV's report say.</summary>
public enum NotAsSent
{
    /// <summary>The server's Direct Play is off (or the server is older, and has none).</summary>
    Off,
    /// <summary>Its picture is interlaced, and the server makes it progressive.</summary>
    Interlaced,
    /// <summary>This TV has no decoder for its picture: the codec, or its bit depth, size or rate.</summary>
    Video,
    /// <summary>This TV neither decodes its sound nor passes it to the receiver.</summary>
    Audio,
    /// <summary>Not for a reason people are told: the plan doesn't say what the channel carries, the TV's report is missing, or its player doesn't read that codec out of MPEG-TS.</summary>
    Unknown,
}

/// <summary>Live TV's rules, shared by its pages and its player (the Android app's LiveTv object).</summary>
public static class LiveRules
{
    /// <summary>The guide's columns: half an hour each.</summary>
    public const long SlotS = 30 * 60;

    /// <summary>How far ahead the guide goes from where it starts.</summary>
    public const long GuideSpanS = 48 * 3600;

    /// <summary>The guide is fetched in chunks of six hours, a page of channels at a time.</summary>
    public const long GuideChunkS = 6 * 3600;

    /// <summary>Channels asked for at once, as the web asks.</summary>
    public const int Page = 100;

    /// <summary>At most this many groups show for a search.</summary>
    public const int FoundGroupsMax = 12;

    /// <summary>
    /// The picture the TV's player reads out of MPEG-TS, in ffprobe's names: what broadcasts
    /// carry. AV1 and VP9, which the TV decodes from files, have no place in a provider's stream
    /// the player knows of, so such a channel stays with its HLS.
    /// </summary>
    static readonly string[] StreamVideo = { "h264", "hevc", "mpeg2video" };

    /// <summary>The sound the TV's player reads out of MPEG-TS.</summary>
    static readonly string[] StreamSound = { "aac", "ac3", "eac3", "mp2", "mp3" };

    /// <summary>
    /// Whether this TV plays a channel's stream.ts as the provider sends it (Direct Play), which
    /// starts sooner than its HLS and costs the server no transcode:
    ///
    /// - Whatever the server's setting, a channel whose picture the plan sends as it is (or that
    ///   has none), with sound sent as it is or in a format every Samsung TV decodes itself (AC3,
    ///   MP2…), which the plan converts for players that can't. Android plays such sound where its
    ///   device's probe says it decodes it or passes it to the receiver: its devices decode
    ///   different things.
    /// - While the server lets players (<see cref="LivePlan.DirectPlay"/>; an older server doesn't),
    ///   a channel the plan would convert that this TV decodes by its own report
    ///   (<paramref name="caps"/>, the one a film's direct play goes by): its picture's codec at
    ///   its bit depth, size and rate, and sound it decodes or passes to the receiver, of those the
    ///   TV's player reads out of MPEG-TS. Interlaced video the plan converts stays with the
    ///   server, whose own setting says whether it is made progressive.
    ///
    /// Without the report, only the first.
    /// </summary>
    public static bool PlaysAsSent(LivePlan plan, Player.ClientCaps? caps = null) => WhyNotAsSent(plan, caps) == null;

    /// <summary>Why a channel plays as HLS, not as the provider sends it; null when it plays as sent (<see cref="PlaysAsSent"/>).</summary>
    public static NotAsSent? WhyNotAsSent(LivePlan plan, Player.ClientCaps? caps)
    {
        var videoSent = plan.Video is "copy" or "none";
        var audioSent = plan.Audio is "copy" or "none";
        var sound = plan.AudioCodec?.ToLowerInvariant();
        if (videoSent && (audioSent || (sound != null && Player.TizenCaps.DecodedAudio.Contains(sound)))) return null;
        // An older server has no such setting: nothing to say of it.
        if (plan.DirectPlay != true) return plan.DirectPlay == false ? NotAsSent.Off : NotAsSent.Unknown;
        if (caps == null) return NotAsSent.Unknown;
        if (!videoSent)
        {
            if (plan.Interlaced) return NotAsSent.Interlaced;
            var picture = plan.VideoCodec?.ToLowerInvariant();
            if (picture == null) return NotAsSent.Unknown;
            if (!Player.CapsPolicy.Decodes(caps, picture, plan.BitDepth ?? 8, plan.Width, plan.Height, plan.FrameRate)) return NotAsSent.Video;
            if (!StreamVideo.Contains(picture)) return NotAsSent.Unknown;
        }
        if (audioSent) return null;
        if (sound == null) return NotAsSent.Unknown;
        if (!caps.AudioDecode.Concat(caps.AudioPassthrough).Concat(caps.AudioCodecs).Contains(sound, StringComparer.OrdinalIgnoreCase)) return NotAsSent.Audio;
        return StreamSound.Contains(sound) ? null : NotAsSent.Unknown;
    }

    static readonly Regex LeadingBars = new Regex(@"^\|+\s*", RegexOptions.CultureInvariant);
    static readonly Regex Bracketed = new Regex(@"^\[([^\]]{1,16})]\s*(.+)$", RegexOptions.CultureInvariant);
    static readonly Regex Barred = new Regex(@"^([^|]{1,16})\|\s*(.+)$", RegexOptions.CultureInvariant);
    static readonly Regex Colon = new Regex(@"^([A-Z]{1,4}):\s*(.+)$", RegexOptions.CultureInvariant);
    static readonly Regex LeadingMarks = new Regex(@"^[|:\-\s]+", RegexOptions.CultureInvariant);

    /// <summary>Division rounding down, as Kotlin's floorDiv: times before 1970 stay in their slot.</summary>
    public static long FloorDiv(long a, long b) => a / b - (a % b != 0 && (a < 0) != (b < 0) ? 1 : 0);

    /// <summary>The regions of two or more groups, alphabetically, then the rest in the provider's order; one list with fewer than two regions.</summary>
    public static GroupSections Sections(IReadOnlyList<LiveGroup> groups)
    {
        var order = new List<string>();
        var byRegion = new Dictionary<string, List<LiveGroup>>();
        foreach (var g in groups)
        {
            if (g.Region == null) continue;
            if (!byRegion.TryGetValue(g.Region, out var list))
            {
                byRegion[g.Region] = list = new List<LiveGroup>();
                order.Add(g.Region);
            }
            list.Add(g);
        }
        var regions = order.Where(r => byRegion[r].Count > 1)
            .Select(r => new GroupRegion(r, byRegion[r], byRegion[r].Sum(g => g.Count)))
            .OrderBy(r => r.Region, StringComparer.OrdinalIgnoreCase)
            .ToList();
        if (regions.Count < 2) return new GroupSections(Array.Empty<GroupRegion>(), groups);
        var gathered = new HashSet<string>(regions.Select(r => r.Region));
        return new GroupSections(regions, groups.Where(g => g.Region == null || !gathered.Contains(g.Region)).ToList());
    }

    /// <summary>The groups a search names, as quick links beside its channels: a dozen at most.</summary>
    public static IReadOnlyList<LiveGroup> FoundGroups(IReadOnlyList<LiveGroup> groups, string query)
    {
        var q = query.Trim().ToLowerInvariant();
        if (q.Length == 0) return Array.Empty<LiveGroup>();
        return groups.Where(g => g.Name.ToLowerInvariant().Contains(q)).Take(FoundGroupsMax).ToList();
    }

    /// <summary>
    /// A name's region and the rest, when it begins with one the way providers write them ("UK|
    /// Sport", "[UK] Sport", "UK: Sport"), as the server splits group names.
    /// </summary>
    public static (string Region, string Label)? SplitRegion(string name)
    {
        var n = LeadingBars.Replace(name.Trim(), "");
        var m = Bracketed.Match(n);
        if (!m.Success) m = Barred.Match(n);
        if (!m.Success) m = Colon.Match(n);
        if (!m.Success) return null;
        var region = m.Groups[1].Value.Trim();
        var rest = LeadingMarks.Replace(m.Groups[2].Value, "").Trim();
        return region.Length > 0 && rest.Length > 0 ? (region, rest) : null;
    }

    /// <summary>
    /// A channel's name as a list shows it: in a group, without the region the group already says
    /// ("Sky Sports 1" in UK · Sport, not "UK| Sky Sports 1").
    /// </summary>
    public static string ChannelName(LiveChannel channel, LiveCategory? category)
    {
        if (category is not LiveCategory.Group) return channel.Name;
        if (SplitRegion(channel.Name) is not { } name || SplitRegion(channel.Group) is not { } group) return channel.Name;
        return string.Equals(name.Region, group.Region, StringComparison.OrdinalIgnoreCase) ? name.Label : channel.Name;
    }

    /// <summary>"UK · Sport" for a group of a region; its label otherwise, or <paramref name="name"/> without the group.</summary>
    public static string GroupName(LiveGroup? group, string name) =>
        group?.Region != null ? $"{group.Region} · {group.Label}" : group?.Label ?? name;

    /// <summary>Where the guide starts for <paramref name="now"/> (unix seconds): the start of the half hour before the one it is in.</summary>
    public static long GuideStart(long now) => FloorDiv(now, SlotS) * SlotS - SlotS;

    /// <summary>
    /// The guide's chunks that cover <paramref name="from"/> to <paramref name="to"/>, and one
    /// either side, so what comes into view next is there already; none before
    /// <paramref name="start"/> or past its span.
    /// </summary>
    public static IReadOnlyList<long> Chunks(long start, long from, long to)
    {
        const long size = GuideChunkS;
        var first = FloorDiv(Math.Max(from, start), size) * size - size;
        var last = FloorDiv(Math.Min(to, start + GuideSpanS), size) * size + size;
        var floor = FloorDiv(start, size) * size;
        var out_ = new List<long>();
        for (var c = first; c <= last; c += size)
            if (c >= floor && c < start + GuideSpanS) out_.Add(c);
        return out_;
    }

    /// <summary>The programs of <paramref name="guides"/> by channel, each list by start, one program per start.</summary>
    public static IReadOnlyDictionary<long, IReadOnlyList<LiveProgram>> Merge(IEnumerable<LiveGuide> guides)
    {
        var byChannel = new Dictionary<long, Dictionary<long, LiveProgram>>();
        foreach (var guide in guides)
        {
            foreach (var entry in guide.Programs)
            {
                if (!byChannel.TryGetValue(entry.Key, out var known)) byChannel[entry.Key] = known = new Dictionary<long, LiveProgram>();
                foreach (var p in entry.Value)
                    if (!known.ContainsKey(p.Start)) known[p.Start] = p;
            }
        }
        return byChannel.ToDictionary(e => e.Key, e => (IReadOnlyList<LiveProgram>)e.Value.Values.OrderBy(p => p.Start).ToList());
    }

    /// <summary>The programs that show between <paramref name="from"/> and <paramref name="to"/>.</summary>
    public static IReadOnlyList<LiveProgram> Between(IReadOnlyList<LiveProgram>? programs, long from, long to) =>
        (programs ?? Array.Empty<LiveProgram>()).Where(p => p.Stop > from && p.Start < to).ToList();

    /// <summary>The program on at <paramref name="time"/> of <paramref name="programs"/>.</summary>
    public static LiveProgram? At(IReadOnlyList<LiveProgram>? programs, long time) =>
        programs?.FirstOrDefault(p => p.Start <= time && p.Stop > time);

    /// <summary>A program's rest at <paramref name="now"/>, as hours and minutes, a minute at least.</summary>
    public static (int Hours, int Minutes) Left(LiveProgram program, long now)
    {
        var minutes = Math.Max(1, (int)Math.Floor((program.Stop - now) / 60.0 + 0.5));
        return (minutes / 60, minutes % 60);
    }
}
