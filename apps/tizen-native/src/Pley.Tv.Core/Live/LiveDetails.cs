using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Live;

/// <summary>
/// How a channel plays, for the live player's Playback details: the film player's Info panel
/// (<see cref="Readout"/>), with a channel's own lines. The way first, in a film's words: Direct
/// play from its stream.ts; from its HLS, Direct stream when the server sends the picture as it
/// is ("Direct stream · Audio transcoding" when it converts the sound), else Transcoding. Then
/// why it isn't Direct play, when the plan and the TV's report say; what the provider sends and
/// what the server makes of it; and This TV, as a film's. A channel has no file and no tracks to
/// list: its Video and Audio lines say what a film's sections of those names do.
/// </summary>
public static class LiveDetails
{
    /// <summary>
    /// The panel, as the film player's: "Asking the server…" while the channel plays from its HLS
    /// and the server hasn't said what it makes of it.
    /// </summary>
    /// <param name="direct">What plays is the channel's stream.ts, as the provider sends it.</param>
    /// <param name="plan">What the server last said of the channel; null until it says.</param>
    /// <param name="caps">The TV's report, which says why a channel isn't played as it is sent.</param>
    /// <param name="tv">What the TV's player says of what it plays, with the channel's own counts (<see cref="LivePlayerModel.Readout"/>).</param>
    public static PlayerPanel Panel(bool direct, LivePlan? plan, ClientCaps? caps, ReadoutInput tv) =>
        PlayerPanels.Info(Sections(direct, plan, caps, tv), waiting: plan == null && !direct);

    /// <summary>The panel's sections: how it plays, then this TV.</summary>
    public static IReadOnlyList<MediaFactBlock> Sections(bool direct, LivePlan? plan, ClientCaps? caps, ReadoutInput tv)
    {
        var sections = new List<MediaFactBlock>();
        var facts = Facts(direct, plan, caps).ToList();
        if (facts.Count > 0)
        {
            // Beside the provider's bitrate, as a film's: a channel that stalls on a network
            // bringing less than the provider sends is one the network can't carry.
            if (tv.Stats?.Bandwidth is long bandwidth) facts.Add(new MediaFact(SessionText.RowNetwork, ItemFormat.Bitrate(bandwidth)));
            sections.Add(new MediaFactBlock(Readout.HowItPlays, facts));
        }
        var device = Readout.Device(tv.Stats, tv);
        if (device.Count > 0) sections.Add(new MediaFactBlock(Readout.ThisTv, device));
        return sections;
    }

    /// <summary>
    /// The way a channel plays, in a film's words (<see cref="SessionText.Method"/>), for the
    /// panel's Playback line and the badge in the player's bar: Direct play from its stream.ts,
    /// whatever the plan would make for other players; from its HLS, what the plan says, a remux
    /// with the sound converted saying so too. Null for the HLS without a plan, whose way only
    /// the plan says.
    /// </summary>
    public static string? Method(bool direct, LivePlan? plan)
    {
        if (direct) return SessionText.MethodDirect;
        if (plan == null) return null;
        if (plan.Video is not ("copy" or "none")) return SessionText.MethodTranscode;
        return plan.Audio == "aac" ? SessionText.MethodRemuxAudio : SessionText.MethodRemux;
    }

    /// <summary>
    /// How it plays, a line each, without those whose value isn't known: Playback, Why, Video,
    /// Audio, Bitrate and Encoder. Without a plan, Playback alone, and nothing at all for the HLS,
    /// whose way the plan says.
    /// </summary>
    public static IReadOnlyList<MediaFact> Facts(bool direct, LivePlan? plan, ClientCaps? caps)
    {
        var facts = new List<MediaFact>();
        if (Method(direct, plan) is not { } method) return facts;
        facts.Add(new MediaFact(Strings.T("player.readout.playback"), method));
        if (plan == null) return facts;
        // From its stream.ts the TV plays what the provider sends, whatever the plan would make for other players.
        var transcoded = !direct && plan.Video is not ("copy" or "none");
        var converted = !direct && plan.Audio == "aac";
        if (!direct && Why(plan, caps) is { } why) facts.Add(new MediaFact(Strings.T("player.readout.why"), why));
        if (Video(plan, transcoded) is { Length: > 0 } video) facts.Add(new MediaFact(SessionText.RowVideo, video));
        if (Audio(plan, converted) is { Length: > 0 } audio) facts.Add(new MediaFact(SessionText.RowAudio, audio));
        if (Bitrate(plan, transcoded) is { Length: > 0 } bitrate) facts.Add(new MediaFact(SessionText.RowBitrate, bitrate));
        if (transcoded) facts.Add(new MediaFact(SessionText.RowEncoder, Details.EncoderName(plan.Video)));
        return facts;
    }

    /// <summary>
    /// Why a channel plays from its HLS and not as the provider sends it, in a sentence; null when
    /// neither its plan nor the TV's report says (it plays as sent but its stream.ts failed, say).
    /// </summary>
    public static string? Why(LivePlan plan, ClientCaps? caps) => LiveRules.WhyNotAsSent(plan, caps) switch
    {
        NotAsSent.Interlaced => LiveText.WhyInterlaced,
        NotAsSent.Video => LiveText.WhyVideo(ItemFormat.CodecLabel(plan.VideoCodec)),
        NotAsSent.Audio => LiveText.WhyAudio(ItemFormat.CodecLabel(plan.AudioCodec)),
        _ => null,
    };

    /// <summary>
    /// The provider's picture, "HEVC 10-bit · 3840×2160 · 50 fps" (" · interlaced"), then what a
    /// transcode makes of it: " → H.264 1080p". Empty without a picture.
    /// </summary>
    static string Video(LivePlan plan, bool transcoded)
    {
        if (plan.Video == "none") return "";
        var sent = ItemFormat.Join(" · ",
            ItemFormat.Join(" ", ItemFormat.CodecLabel(plan.VideoCodec), plan.BitDepth is int depth && depth > 0 ? ItemFormat.BitDepth(depth) : null),
            plan.Width > 0 && plan.Height > 0 ? $"{plan.Width}×{plan.Height}" : null,
            ItemFormat.FrameRateLabel(plan.FrameRate),
            plan.Interlaced ? LiveText.Interlaced : null);
        if (!transcoded) return sent;
        // A transcode's lines; a server that doesn't say them keeps the provider's.
        var made = ItemFormat.Join(" ", ItemFormat.CodecLabel(EncodedCodec(plan.Video)), (plan.OutHeight ?? plan.Height) is int lines && lines > 0 ? $"{lines}p" : null);
        return sent.Length > 0 ? $"{sent} → {made}" : made;
    }

    /// <summary>The codec an encoder makes, in the server's names: HEVC for "hevc_nvenc" and "libx265", else H.264, which is what the server makes of live video.</summary>
    static string EncodedCodec(string encoder) => encoder.StartsWith("hevc", System.StringComparison.Ordinal) || encoder == "libx265" ? "hevc" : "h264";

    /// <summary>The provider's sound, "EAC3 5.1", then " → AAC Stereo" when the server converts it. Empty without sound.</summary>
    static string Audio(LivePlan plan, bool converted)
    {
        if (plan.Audio == "none") return "";
        var sent = ItemFormat.Join(" ", ItemFormat.CodecLabel(plan.AudioCodec), ItemFormat.ChannelsLabel(plan.AudioChannels));
        if (!converted) return sent;
        // The server makes AAC stereo of sound it converts (docs/API.md, "The stream").
        var made = $"{ItemFormat.CodecLabel("aac")} {ItemFormat.ChannelsLabel(2)}";
        return sent.Length > 0 ? $"{sent} → {made}" : made;
    }

    /// <summary>What the provider sends now, "12 Mbps", then a transcode's video bitrate: " → 8.0 Mbps". Empty until the server has measured it.</summary>
    static string Bitrate(LivePlan plan, bool transcoded)
    {
        var sent = ItemFormat.Bitrate(plan.Kbps * 1000);
        if (sent.Length == 0 || !transcoded || plan.VideoKbps is not int made || made <= 0) return sent;
        return $"{sent} → {ItemFormat.Bitrate(made * 1000L)}";
    }
}
