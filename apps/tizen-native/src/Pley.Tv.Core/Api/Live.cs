using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Pley.Tv.Core.Api;

// Live TV's part of the API (docs/API.md, "Live TV"), as the Android app's model/LiveTv.kt has it.
// Times are unix seconds.

/// <summary>
/// GET /api/live: whether to show Live TV. <see cref="Allowed"/>: it is open to this profile;
/// <see cref="Available"/>: and it has channels. <see cref="Recent"/>: the channels watched last, newest first.
/// </summary>
public sealed record LiveSummary
{
    public bool Available { get; init; }
    public bool Allowed { get; init; }
    public int Channels { get; init; }
    /// <summary>For administrators; 0 for the others.</summary>
    public int Sources { get; init; }
    public IReadOnlyList<long> Recent { get; init; } = Array.Empty<long>();
}

/// <summary>A programme of the guide; <see cref="Image"/> is an address on the server, when the guide has a picture.</summary>
public sealed record LiveProgram
{
    public long ChannelId { get; init; }
    public long Start { get; init; }
    public long Stop { get; init; }
    public string Title { get; init; } = "";
    public string? Subtitle { get; init; }
    public string? Description { get; init; }
    public IReadOnlyList<string> Categories { get; init; } = Array.Empty<string>();
    public string? Image { get; init; }
    public int? Season { get; init; }
    public int? Episode { get; init; }
    public bool IsNew { get; init; }
    /// <summary>Shown as it happens (a match, the news).</summary>
    public bool IsLive { get; init; }
    public string? Rating { get; init; }

    /// <summary>On at <paramref name="now"/>.</summary>
    public bool AiringAt(long now) => Start <= now && Stop > now;

    /// <summary>How far it is at <paramref name="now"/>, from 0 to 1.</summary>
    public float ProgressAt(long now) => Math.Max(0f, Math.Min(1f, (float)(now - Start) / Math.Max(1, Stop - Start)));

    /// <summary>"S2 E14", or "E14", when the guide numbers the episode.</summary>
    [JsonIgnore]
    public string? EpisodeLabel => Season != null && Episode != null ? $"S{Season} E{Episode}" : Episode != null ? $"E{Episode}" : null;
}

/// <summary>A channel of the lineup, with what's on now and next; <see cref="Logo"/> is its logo's address on the server.</summary>
public sealed record LiveChannel
{
    public long Id { get; init; }
    public long SourceId { get; init; }
    public string Name { get; init; } = "";
    public string? Number { get; init; }
    public string Group { get; init; } = "";
    public string? Logo { get; init; }
    public bool Favorite { get; init; }
    /// <summary>The guide has programmes for it.</summary>
    public bool HasGuide { get; init; }
    public LiveProgram? Now { get; init; }
    public LiveProgram? Next { get; init; }

    /// <summary>What is on at <paramref name="now"/>; null once <see cref="Now"/> ended, before the list is asked again.</summary>
    public LiveProgram? OnAt(long now) =>
        Now != null && Now.AiringAt(now) ? Now : Next != null && Next.AiringAt(now) ? Next : null;

    /// <summary>What comes after <see cref="OnAt"/>: <see cref="Next"/>, or what was listed as now when it hasn't begun.</summary>
    public LiveProgram? NextAt(long now)
    {
        var listed = new[] { Now, Next }.Where(p => p != null).Select(p => p!);
        var on = OnAt(now);
        return on == null ? listed.FirstOrDefault(p => p.Start > now) : listed.FirstOrDefault(p => p.Start >= on.Stop);
    }
}

/// <summary>A group of channels as the provider names it: <see cref="Region"/> is the "UK" of "UK| Sport", <see cref="Label"/> the rest.</summary>
public sealed record LiveGroup
{
    readonly string label = "";

    public string Name { get; init; } = "";
    public string? Region { get; init; }
    /// <summary>The name without its region; the name when the server says none.</summary>
    public string Label
    {
        get => label.Length > 0 ? label : Name;
        init => label = value ?? "";
    }
    public int Count { get; init; }
}

/// <summary>GET /api/live/groups: the groups in the lineup's order, and how many channels in all, marked and watched lately.</summary>
public sealed record LiveGroups
{
    public int Total { get; init; }
    public int Favorites { get; init; }
    public int Recent { get; init; }
    public IReadOnlyList<LiveGroup> Groups { get; init; } = Array.Empty<LiveGroup>();
}

/// <summary>GET /api/live/browse: <see cref="Channels"/> from <see cref="Offset"/> of a list of <see cref="Total"/>.</summary>
public sealed record LiveChannelPage
{
    public int Total { get; init; }
    public int Offset { get; init; }
    public IReadOnlyList<LiveChannel> Channels { get; init; } = Array.Empty<LiveChannel>();
}

/// <summary>GET /api/live/guide: the programmes between <see cref="From"/> and <see cref="To"/>, by channel.</summary>
public sealed record LiveGuide
{
    public long From { get; init; }
    public long To { get; init; }
    public IReadOnlyDictionary<long, IReadOnlyList<LiveProgram>> Programs { get; init; } = new Dictionary<long, IReadOnlyList<LiveProgram>>();
}

/// <summary>
/// POST /api/live/channels/{id}/watch's answer: <see cref="Url"/> is the channel's live HLS
/// playlist (with keyed URLs, an address that carries its own key), read as
/// <see cref="SessionId"/> until it is left or goes quiet for 45 s.
/// </summary>
public sealed record LiveWatching
{
    public string SessionId { get; init; } = "";
    public string Url { get; init; } = "";
    public string Kind { get; init; } = "hls";
    public LiveChannel? Channel { get; init; }
}

/// <summary>
/// GET /api/live/sessions/{session}/plan: what the transcoder makes of the channel a session
/// watches (<see cref="Video"/>: <c>copy</c> or the encoder; <see cref="Audio"/>: <c>copy</c>,
/// <c>aac</c> or <c>none</c>). Sent as it is, the channel plays from its <c>stream.ts</c>, sooner
/// than its first HLS segments (<see cref="Direct"/>).
/// </summary>
public sealed record LivePlan
{
    public string? VideoCodec { get; init; }
    public int? Width { get; init; }
    public int? Height { get; init; }
    public double? FrameRate { get; init; }
    public bool Interlaced { get; init; }
    public string? AudioCodec { get; init; }
    public int? AudioChannels { get; init; }
    public string Video { get; init; } = "";
    public string Audio { get; init; } = "";

    /// <summary>The channel is sent as it is (or without picture or sound): <c>stream.ts</c> plays it straight away.</summary>
    public bool Direct => Video is "copy" or "none" && Audio is "copy" or "none";
}

/// <summary>
/// A connection of a channel's source in use, as a <c>409</c> from watching lists them:
/// <see cref="Watchers"/> names who watches (administrators only), <see cref="Yours"/> only the
/// asker does, and whether it may be taken over.
/// </summary>
public sealed record BusyConnection
{
    public long ChannelId { get; init; }
    public string ChannelName { get; init; } = "";
    public IReadOnlyList<string> Watchers { get; init; } = Array.Empty<string>();
    public int People { get; init; }
    public bool Yours { get; init; }
    public bool CanTakeOver { get; init; }

    /// <summary>The connections a refusal lists (a <c>409</c> from watching); none in any other.</summary>
    public static IReadOnlyList<BusyConnection> Of(Exception e)
    {
        if (e is not ApiException { Body: { } body } || Json.Field(body, "connections") is not { ValueKind: JsonValueKind.Array } list)
            return Array.Empty<BusyConnection>();
        try
        {
            return JsonSerializer.Deserialize<List<BusyConnection>>(list.GetRawText(), ApiJson.Options) ?? new List<BusyConnection>();
        }
        catch (JsonException)
        {
            return Array.Empty<BusyConnection>();
        }
    }
}

/// <summary>
/// Which channels a list has (<c>category</c> of GET /api/live/browse): every one, the
/// favourites, those watched lately, or one group's.
/// </summary>
public abstract record LiveCategory
{
    /// <summary>How it is kept and passed on: "all", "favorites", "recent" or "group:UK| Sport".</summary>
    public abstract string Key { get; }

    /// <summary><c>category</c> on the wire.</summary>
    public abstract string Wire { get; }

    public sealed record All : LiveCategory
    {
        public override string Key => "all";
        public override string Wire => "all";
    }

    public sealed record Favorites : LiveCategory
    {
        public override string Key => "favorites";
        public override string Wire => "favorites";
    }

    public sealed record Recent : LiveCategory
    {
        public override string Key => "recent";
        public override string Wire => "recent";
    }

    public sealed record Group(string Name) : LiveCategory
    {
        public override string Key => "group:" + Name;
        public override string Wire => "group";
    }

    /// <summary>The category <paramref name="key"/> names; every channel for anything else.</summary>
    public static LiveCategory Parse(string? key) => key switch
    {
        "favorites" => new Favorites(),
        "recent" => new Recent(),
        { } k when k.StartsWith("group:", StringComparison.Ordinal) => new Group(k.Substring(6)),
        _ => new All(),
    };
}
