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
    /// Whether this TV plays a channel's stream.ts as the provider sends it: its picture sent as it
    /// is (or none), its sound sent as it is or in a format every Samsung TV decodes itself (AC3,
    /// MP2…), which the plan converts for players that can't: as HLS, such a channel starts later.
    /// Android plays such sound where its device's probe says it decodes it or passes it to the
    /// receiver: its devices decode different things.
    /// </summary>
    public static bool PlaysAsSent(LivePlan plan) =>
        plan.Video is "copy" or "none"
        && (plan.Audio is "copy" or "none" || (plan.AudioCodec != null && Player.TizenCaps.DecodedAudio.Contains(plan.AudioCodec)));

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

    /// <summary>The programmes of <paramref name="guides"/> by channel, each list by start, one programme per start.</summary>
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

    /// <summary>The programmes that show between <paramref name="from"/> and <paramref name="to"/>.</summary>
    public static IReadOnlyList<LiveProgram> Between(IReadOnlyList<LiveProgram>? programs, long from, long to) =>
        (programs ?? Array.Empty<LiveProgram>()).Where(p => p.Stop > from && p.Start < to).ToList();

    /// <summary>The programme on at <paramref name="time"/> of <paramref name="programs"/>.</summary>
    public static LiveProgram? At(IReadOnlyList<LiveProgram>? programs, long time) =>
        programs?.FirstOrDefault(p => p.Start <= time && p.Stop > time);

    /// <summary>A programme's rest at <paramref name="now"/>, as hours and minutes, a minute at least.</summary>
    public static (int Hours, int Minutes) Left(LiveProgram program, long now)
    {
        var minutes = Math.Max(1, (int)Math.Floor((program.Stop - now) / 60.0 + 0.5));
        return (minutes / 60, minutes % 60);
    }
}
