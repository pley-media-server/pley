using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>
/// How a playback is asked for (the Android app's PlaybackPlan): which file, from where, with which
/// tracks, and what goes in the body of POST /api/playback. Pure: it reads the item, the TV's
/// capabilities and the choices, nothing else.
///
/// The audio is the server's to choose unless the person picked a track: it knows the profile's
/// language and, from the capabilities, which of that language's tracks this TV plays best, and
/// says which one plays (<see cref="PlaybackInfo.AudioIndex"/>). Track indexes go to the server
/// only when it has to act on them: <c>audio_index</c> for a track picked, when it can't be
/// played here, the session streams HLS (one audio track), or it isn't the track the item's
/// details give; <c>subtitle_index</c> only to burn subtitles shown as pictures into HLS, never
/// when this player draws them (the server's .mks).
/// </summary>
public static class PlaybackPlan
{
    /// <summary>The subtitle codecs Matroska takes (apps/transcoder plan.rs, <c>MATROSKA_SUBTITLES</c>): what a conversion holds.</summary>
    static readonly HashSet<string> ConvertedSubtitles = new HashSet<string>
    {
        "subrip", "ass", "ssa", "webvtt", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle", "mov_text",
    };

    /// <summary>The file to play: the one asked for, else the one that plays best here, else the first.</summary>
    public static MediaFile? File(ItemDetail item, long? requested, ClientCaps? caps)
    {
        if (requested is long id && item.Files.FirstOrDefault(f => f.Id == id) is { } asked) return asked;
        MediaFile? best = null;
        if (caps != null)
        {
            try
            {
                best = CapsPolicy.BestFile(item, caps);
            }
            catch (Exception)
            {
                // A report the policy can't read: the first file, as without one.
            }
        }
        return best ?? item.Files.FirstOrDefault();
    }

    /// <summary>Where to start: as asked, else from the start when asked or watched, else the resume point.</summary>
    public static long StartMs(ItemDetail item, long? startMs, bool fromStart)
    {
        if (startMs is long at) return Math.Max(0, at);
        if (fromStart || item.UserState.Watched) return 0;
        return Math.Max(0, item.UserState.PositionMs);
    }

    public static IReadOnlyList<MediaStream> AudioStreams(MediaFile? file) => (file?.Streams ?? new MediaStream[0]).Where(s => s.Kind == "audio").ToList();

    /// <summary>
    /// <paramref name="chosen"/>'s place among the <paramref name="listed"/> audio tracks the TV's
    /// player lists; null when it leaves the track out. The player lists a file's tracks in the
    /// file's order, without those it can't play (<see cref="TizenCaps.PlayerLeavesOut"/>); a
    /// player that lists none (not ready, or an engine that doesn't say) goes by the file's order.
    /// </summary>
    public static int? EngineAudioOrdinal(IReadOnlyList<MediaStream> streams, MediaStream chosen, int listed)
    {
        var at = OfferedAudio(streams, listed).IndexOf(chosen);
        return at >= 0 ? at : null;
    }

    /// <summary>The file's audio stream at the TV's track <paramref name="ordinal"/> of <paramref name="listed"/>.</summary>
    public static MediaStream? EngineAudioStream(IReadOnlyList<MediaStream> streams, int? ordinal, int listed)
    {
        var offered = OfferedAudio(streams, listed);
        return ordinal is int i && i >= 0 && i < offered.Count ? offered[i] : null;
    }

    static List<MediaStream> OfferedAudio(IReadOnlyList<MediaStream> streams, int listed)
    {
        if (listed <= 0 || listed == streams.Count) return streams.ToList();
        var kept = streams.Where(s => !TizenCaps.PlayerLeavesOut.Contains(s.Codec.ToLowerInvariant())).ToList();
        return kept.Count == listed ? kept : streams.ToList();
    }

    public static IReadOnlyList<MediaStream> SubtitleStreams(MediaFile? file) => (file?.Streams ?? new MediaStream[0]).Where(s => s.Kind == "subtitle").ToList();

    /// <summary>The subtitle tracks the server's conversion of <paramref name="file"/> holds, in their order.</summary>
    public static IReadOnlyList<MediaStream> ConvertedSubtitleStreams(MediaFile? file) =>
        SubtitleStreams(file).Where(s => s.External != true && ConvertedSubtitles.Contains(s.Codec.ToLowerInvariant())).ToList();

    public static MediaStream? VideoStream(MediaFile? file) => CapsPolicy.VideoOf(file);

    /// <summary>The audio to start with: the track asked for, else null, automatic: the server picks it, and its answer says which.</summary>
    public static MediaStream? InitialAudio(MediaFile? file, int? requested) =>
        requested is int r ? AudioStreams(file).FirstOrDefault(s => s.Index == r) : null;

    /// <summary>The audio <paramref name="file"/> plays with track <paramref name="index"/>, as a session's answer names it: that track, else the default, else the first.</summary>
    public static MediaStream? PlayedAudio(MediaFile? file, int? index)
    {
        var audio = AudioStreams(file);
        return audio.FirstOrDefault(s => s.Index == index) ?? audio.FirstOrDefault(s => s.Default) ?? audio.FirstOrDefault();
    }

    /// <summary>
    /// The audio the item's details give: the profile's choice (<c>selected_audio</c>, which its
    /// defaults fill in), else the file's default, else the first.
    /// </summary>
    public static MediaStream? DetailsAudio(MediaFile? file)
    {
        var audio = AudioStreams(file);
        return audio.FirstOrDefault(s => s.Index == file?.SelectedAudio) ?? audio.FirstOrDefault(s => s.Default) ?? audio.FirstOrDefault();
    }

    /// <summary>The subtitles to start with: as asked, else the profile's choice; null is off, whatever the file flags as default.</summary>
    public static MediaStream? InitialSubtitle(MediaFile? file, int? requested)
    {
        var subs = SubtitleStreams(file);
        return (requested is int r ? subs.FirstOrDefault(s => s.Index == r) : null) ?? subs.FirstOrDefault(s => s.Index == file?.SelectedSubtitle);
    }

    /// <summary>Subtitles shown as pictures (PGS, VobSub, DVB): a stream burns them into the video unless the player draws them.</summary>
    public static bool IsImage(MediaStream? stream) => stream != null && stream.TextBased == false;

    /// <summary>Whether pictures are drawn by the player from the server's .mks rather than burned in: the report says so and the player can read them.</summary>
    public static bool SideloadsPictures(ClientCaps? caps, bool canRead) => canRead && caps?.PictureSubtitlesSideload == true;

    /// <summary>Where the server's .mks of picture subtitles <paramref name="index"/> of <paramref name="fileId"/> is.</summary>
    public static string PictureSubtitlesPath(long fileId, int index) => $"/api/files/{fileId}/subtitles/{index}.mks";

    /// <summary>
    /// <paramref name="url"/> asked for from where the player is (<paramref name="atMs"/> of the file), 0 at the
    /// start too: a track the server hasn't copied out of its file yet comes from there first, and is answered with
    /// the pictures around there until it is whole.
    /// </summary>
    public static string PictureUrlAt(string url, long atMs) => $"{url}{(url.Contains('?') ? "&" : "?")}at={Math.Max(0, atMs)}";

    /// <summary>Where the server's WebVTT of a text track is, when the session doesn't name it.</summary>
    public static string TextSubtitlesPath(long fileId, int index) => $"/api/files/{fileId}/subtitles/{index}.vtt";

    /// <summary>Whether this TV decodes <paramref name="stream"/>'s audio or sends it as it is to the receiver.</summary>
    public static bool AudioPlaysHere(MediaStream? stream, ClientCaps? caps)
    {
        if (stream == null || caps == null) return true;
        var codec = stream.Codec.ToLowerInvariant();
        return caps.AudioCodecs.Concat(caps.AudioDecode).Concat(caps.AudioPassthrough).Any(c => string.Equals(c, codec, StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>Whether this TV opens <paramref name="file"/>'s container as it is.</summary>
    public static bool ContainerPlaysHere(MediaFile? file, ClientCaps? caps)
    {
        var container = file?.Container?.ToLowerInvariant();
        if (container == null || caps == null) return true;
        var containers = caps.Containers.Select(c => c.ToLowerInvariant()).ToList();
        return containers.Contains(container)
            || (container == "mkv" && containers.Contains("matroska"))
            || (container == "matroska" && containers.Contains("mkv"));
    }

    /// <summary>
    /// Whether the session will surely be a stream (HLS), before asking: another mode than auto, a
    /// video this TV doesn't play as it is, a container it doesn't open, or audio it can't play.
    /// Automatic audio (null) doesn't count: the server picks a track that plays here when it can.
    /// </summary>
    public static bool Streams(string mode, CapsDecision? decision, MediaFile? file, MediaStream? audio, ClientCaps? caps) =>
        mode is "remux" or "transcode"
        || (decision != null && decision.Kind != CapsDecisionKind.Direct)
        || !ContainerPlaysHere(file, caps)
        || !AudioPlaysHere(audio, caps);

    /// <summary>What a request carries besides the item: the choices of the moment.</summary>
    /// <param name="Audio">Null: automatic, the server picks the track.</param>
    /// <param name="Streaming">The session will stream (HLS): the server needs the track indexes.</param>
    /// <param name="Sideload">Pictures are drawn by the player: none to burn in.</param>
    public sealed record Choices(MediaFile File, long StartMs, MediaStream? Audio, MediaStream? Subtitle, QualityPreset Quality, string Mode, bool Streaming, bool Sideload = false);

    /// <summary>The audio index a request names for <paramref name="c"/>; null leaves the choice to the server.</summary>
    public static int? AudioIndexFor(Choices c, ClientCaps? caps)
    {
        if (c.Audio == null) return null;
        // Named when the server must convert it, or when it isn't the one the details give (without
        // a track named, the server plays that one or one of its language, and judges a file played
        // as it is by it).
        var notTheDetails = c.Audio.Index != DetailsAudio(c.File)?.Index;
        return c.Streaming || !AudioPlaysHere(c.Audio, caps) || notTheDetails ? c.Audio.Index : null;
    }

    /// <summary>The subtitle index a request names for <paramref name="c"/>: only pictures to burn into a stream.</summary>
    public static int? SubtitleIndexFor(Choices c) => c.Subtitle != null && c.Streaming && IsImage(c.Subtitle) && !c.Sideload ? c.Subtitle.Index : null;

    /// <summary>
    /// Whether <paramref name="info"/>'s session must be asked again with the track indexes: it
    /// streams, though it wasn't expected to, and its audio isn't the one chosen, or the subtitles
    /// chosen are pictures to burn in. Automatic audio is whatever the server chose: never asked again for it.
    /// </summary>
    public static bool NeedsIndexes(PlaybackInfo info, IReadOnlyDictionary<string, object?> sent, Choices c)
    {
        if (!info.Hls) return false;
        var audio = c.Audio;
        var wrongAudio = audio != null && !sent.ContainsKey("audio_index") && info.AudioIndex != null && info.AudioIndex != audio.Index;
        var missingBurn = IsImage(c.Subtitle) && !sent.ContainsKey("subtitle_index") && !c.Sideload;
        return wrongAudio || missingBurn;
    }

    /// <summary>The container's MIME type, for engines that can't tell from the address.</summary>
    public static string? MimeTypeOf(string? container) => container?.ToLowerInvariant() switch
    {
        "mkv" or "matroska" or "mka" => "video/x-matroska",
        "mp4" or "m4v" or "mov" => "video/mp4",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "ts" or "m2ts" or "mpegts" => "video/mp2t",
        _ => null,
    };
}

/// <summary>
/// The Dolby Vision guards (the Android app's DvGuards): a session that would show wrong colours is
/// asked for again another way.
/// <list type="bullet">
/// <item>A remux of profile 5 that doesn't name its Dolby Vision: TS segments lose the signalling,
/// and profile 5 has no base layer to fall back to, so the picture comes out green and purple. It
/// is transcoded instead (tone-mapped). One whose answer says it stays Dolby Vision plays as it comes.</item>
/// <item>Direct play of profile 7 without a profile 7 decoder: a remux sends the HEVC base layer,
/// HDR10, when the screen shows HDR10; else a transcode.</item>
/// </list>
/// </summary>
public static class DvGuards
{
    /// <summary>The mode to ask <paramref name="info"/>'s session again with; null when it plays right.</summary>
    public static string? RetryMode(PlaybackInfo info, MediaFile? file, ClientCaps? caps)
    {
        var video = PlaybackPlan.VideoStream(file);
        if (video == null || CapsPolicy.RangeOf(video) != "dolby_vision") return null;
        switch (video.DvProfile)
        {
            case 5:
                return info.Method == "remux" && !KeepsDolbyVision(info) ? "transcode" : null;
            case 7:
                var decodesP7 = caps?.DolbyVisionProfiles.Any(p => p.Profile == 7) == true;
                if (info.Method != "direct" || decodesP7) return null;
                return caps?.HdrTypes.Contains("hdr10") == true || caps?.Hdr == true ? "remux" : "transcode";
            default:
                return null;
        }
    }

    /// <summary>The session's stream names its Dolby Vision, so the screen gets it ("Dolby Vision P5", not "… not decoded").</summary>
    static bool KeepsDolbyVision(PlaybackInfo info) =>
        info.VideoRange is { } range && range.StartsWith("Dolby Vision", StringComparison.Ordinal) && range.IndexOf("not decoded", StringComparison.Ordinal) < 0;
}
