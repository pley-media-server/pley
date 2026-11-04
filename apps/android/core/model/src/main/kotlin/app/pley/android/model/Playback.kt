package app.pley.android.model

import app.pley.android.model.caps.ClientCaps
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.KeepGeneratedSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonTransformingSerializer
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** How the app asks for an item to be delivered (apps/server/src/playback.rs, `PlaybackMode`). */
@Serializable(with = PlaybackMode.Serializer::class)
enum class PlaybackMode(override val wire: String) : WireEnum {
    /** The cheapest the caps allow: direct play, then remux, then transcode. */
    Auto("auto"),

    /** The original file, even when the caps don't claim it plays. */
    Direct("direct"),

    /** HLS, the video copied when it can be. */
    Remux("remux"),

    /** HLS, the video always converted. */
    Transcode("transcode"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<PlaybackMode>("PlaybackMode", PlaybackMode.entries, PlaybackMode.Unknown)
}

/** How the server delivers it (apps/server/src/playback.rs, `Method`). */
@Serializable(with = PlaybackMethod.Serializer::class)
enum class PlaybackMethod(override val wire: String) : WireEnum {
    /** The file as it is, from GET /api/files/{id}/stream. */
    Direct("direct"),

    /** HLS with the video copied. */
    Remux("remux"),

    /** HLS with the video converted to H.264. */
    Transcode("transcode"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<PlaybackMethod>("PlaybackMethod", PlaybackMethod.entries, PlaybackMethod.Unknown)
}

/**
 * The body of POST /api/playback (apps/server/src/playback.rs,
 * `PlaybackRequest`). Fields left `null` are left out, and the server
 * picks: the item's first file, the audio this player plays best in the
 * profile's language, no subtitles burned in, the original quality, the
 * start.
 */
@Serializable
data class PlaybackRequest(
    val itemId: Long,
    val fileId: Long? = null,
    /** [MediaStream.index] of the audio; `null`: the server's choice, which [PlaybackInfo.audioIndex] names. */
    val audioIndex: Int? = null,
    /** Only for subtitles shown as pictures (PGS, VobSub), which are burned in by a transcode. */
    val subtitleIndex: Int? = null,
    /** kbps; `null`: the original quality. */
    val maxBitrate: Int? = null,
    /** Lines: 1080, 720, 480. */
    val maxHeight: Int? = null,
    /** Where to start, in ms; a transcode starts producing there. */
    val startMs: Long? = null,
    val mode: PlaybackMode = PlaybackMode.Auto,
    /** The player: starting a session ends this profile's earlier ones on the same client. */
    val clientId: String? = null,
    /**
     * Started while the client's last session plays on (a quality or track
     * change made ready before the player moves to it): that one isn't
     * ended, the player ends it as it moves over. `null` leaves it out.
     */
    val keepPrevious: Boolean? = null,
    /** What this device decodes. */
    val caps: ClientCaps = ClientCaps(),
    /** The remote's commands this player obeys (Remote control); `null` leaves them out: none but Stop. */
    val controls: List<Control>? = null,
    /** What remotes call this device ("Living room TV"); `null`: the server names it from the User-Agent. */
    val deviceName: String? = null,
    /** What kind of device it plays on; `null`: the server tells from the app, which for this one is [DeviceKind.Other]. */
    val deviceKind: DeviceKind? = null,
)

/**
 * What kind of device a session plays on (docs/API.md, "Remote control"):
 * what its player says in its play request, else what the server makes of
 * the app.
 */
@Serializable(with = DeviceKind.Serializer::class)
enum class DeviceKind(override val wire: String) : WireEnum {
    Tv("tv"),
    Phone("phone"),
    Tablet("tablet"),
    Computer("computer"),
    Browser("browser"),
    Other("other"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<DeviceKind>("DeviceKind", DeviceKind.entries, DeviceKind.Unknown)
}

/**
 * A command of the remote, from another of the profile's devices
 * (docs/API.md, "Remote control"): what a player says it obeys in its play
 * request, and what a remote sends it. Stop is always obeyed, and isn't one.
 */
@Serializable(with = Control.Serializer::class)
enum class Control(override val wire: String) : WireEnum {
    Play("play"),
    Pause("pause"),

    /** To a place: `position_ms`. */
    Seek("seek"),

    /** Back or forward from where it is: `offset_ms`. */
    Skip("skip"),

    /** The next episode or track; on Live TV, the next channel. */
    Next("next"),

    /** The track before, or the start of this one; on Live TV, the channel before. */
    Previous("previous"),

    /** Another audio track of the title: `stream_index`. */
    Audio("audio"),

    /** Other subtitles of the title, or none: `stream_index`, `-1` for none. */
    Subtitles("subtitles"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<Control>("Control", Control.entries, Control.Unknown)
}

/** What a session of GET /api/sessions plays. */
@Serializable(with = PlayingKind.Serializer::class)
enum class PlayingKind(override val wire: String) : WireEnum {
    /** A movie, an episode or a track: [PlayingSession.item]. */
    Item("item"),

    /** A Live TV channel: [PlayingSession.channel]. */
    Live("live"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<PlayingKind>("PlayingKind", PlayingKind.entries, PlayingKind.Unknown)
}

/** The channel a live session of GET /api/sessions watches; [now] is the programme on. */
@Serializable
data class PlayingChannel(
    val id: Long,
    val name: String = "",
    val number: String? = null,
    val logo: String? = null,
    val now: String? = null,
)

/**
 * One of the profile's sessions whose player is there (GET /api/sessions),
 * on this device or another. While it plays, it is at [positionMs] plus
 * [ageMs] plus the time since the answer came, by this device's clock: the
 * server says how long ago its player reported rather than when, so the
 * two clocks are never compared.
 */
@Serializable
data class PlayingSession(
    val sessionId: String,
    val kind: PlayingKind = PlayingKind.Unknown,
    /** What the player calls its device; `null` when the server can't tell. */
    val device: String? = null,
    /**
     * Which device that is, the same for everything one sign-in plays
     * ("app-12", "web-9f2c…"); empty from a server before it said.
     */
    val deviceId: String = "",
    /** And what kind; [DeviceKind.Unknown] from a server before it said. */
    val deviceKind: DeviceKind = DeviceKind.Unknown,
    val item: ItemSummary? = null,
    val channel: PlayingChannel? = null,
    /** Playing, paused or buffering; live is always playing. */
    val state: ProgressState = ProgressState.Unknown,
    /** Live: 0. */
    val positionMs: Long = 0,
    /** `null` when the length isn't known, and on live. */
    val durationMs: Long? = null,
    /** Its player's last report: unix seconds, by the server's clock. */
    val updatedAt: Long = 0,
    /** How long before the answer that report was, by the server's clock. */
    val ageMs: Long = 0,
    /** What its player obeys; Stop always works. */
    val controls: List<Control> = emptyList(),
    /** The audio track it plays, by stream index; `null` when not known, and on live. */
    val audioIndex: Int? = null,
    /** The subtitles it shows, by stream index, [SUBTITLES_OFF] for none; `null` when not known, and on live. */
    val subtitleIndex: Int? = null,
    /** Its file's tracks, which [Control.Audio] and [Control.Subtitles] pick from; none on live. */
    val tracks: SessionTracks = SessionTracks(),
) {
    companion object {
        /** The subtitle index of none, in sessions, commands and progress reports. */
        const val SUBTITLES_OFF = -1
    }
}

/** The tracks of a session's file (GET /api/sessions): the audio and the subtitles a remote picks from. */
@Serializable
data class SessionTracks(
    val audio: List<TrackOption> = emptyList(),
    val subtitles: List<TrackOption> = emptyList(),
)

/** One of [SessionTracks]: [index] is the stream index commands and reports name. */
@Serializable
data class TrackOption(
    val index: Int,
    /** "eng". */
    val language: String? = null,
    val title: String? = null,
    /** "eac3", "subrip". */
    val codec: String? = null,
    /** Audio only. */
    val channels: Int? = null,
    val default: Boolean = false,
    val forced: Boolean = false,
)

/**
 * The body of POST /api/sessions/{id}/command: [positionMs] for
 * [Control.Seek] (0 to the length), [offsetMs] for [Control.Skip] (up to
 * 10 minutes either way), [streamIndex] for [Control.Audio] and
 * [Control.Subtitles] (one of the session's tracks; subtitles
 * [PlayingSession.SUBTITLES_OFF] for none); the other commands take none.
 */
@Serializable
data class PlayerCommandRequest(
    val command: Control,
    val positionMs: Long? = null,
    val offsetMs: Long? = null,
    val streamIndex: Int? = null,
)

/** What a device plays when a remote casts to it (docs/API.md, "Casting"). */
@Serializable(with = CastKind.Serializer::class)
enum class CastKind(override val wire: String) : WireEnum {
    /** Titles, music and playlists. */
    Item("item"),

    /** Live TV channels. */
    Live("live"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<CastKind>("CastKind", CastKind.entries, CastKind.Unknown)
}

/**
 * One of the profile's devices that takes casts (GET /api/remote/devices):
 * one of its apps with the event stream open as a receiver, the one that
 * connected last for its device.
 */
@Serializable
data class RemoteDevice(
    /** The app's own id for the device: where casts go. */
    val receiverId: String,
    /** As [PlayingSession.deviceId]: what it plays has the same. */
    val deviceId: String = "",
    /** As [PlayingSession.device]; `null` when the server can't tell. */
    val name: String? = null,
    val kind: DeviceKind = DeviceKind.Unknown,
    /** What it plays when cast to. */
    val casts: List<CastKind> = emptyList(),
)

/**
 * The body of POST /api/remote/devices/{id}/cast: exactly one of [itemId]
 * (a movie, an episode, a show, a season, a track, an album or an artist),
 * [playlistId] and [channelId]; [startMs] where a title starts (0 to a
 * day), [shuffle] for a playlist, an album or an artist.
 */
@Serializable
data class CastRequest(
    val itemId: Long? = null,
    val playlistId: Long? = null,
    val channelId: Long? = null,
    val startMs: Long? = null,
    val shuffle: Boolean? = null,
)

/** Where a download stands on the server (apps/server/src/api/playback.rs, `DownloadState`). */
@Serializable(with = ConversionState.Serializer::class)
enum class ConversionState(override val wire: String) : WireEnum {
    /** The original plays on this device as it is: download it. */
    Direct("direct"),
    Queued("queued"),
    Running("running"),

    /** The server's file for this device is whole. */
    Done("done"),
    Failed("failed"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<ConversionState>("ConversionState", ConversionState.entries, ConversionState.Unknown)
}

/**
 * POST /api/conversions and GET /api/conversions/{id}: what a download
 * takes, decided as a playback is. The original ([ConversionState.Direct]),
 * or a file the server makes for this device; [url] is where it comes
 * from, with ranges.
 */
@Serializable
data class ConversionInfo(
    /** The conversion; `null` for the original. */
    val id: String? = null,
    val state: ConversionState = ConversionState.Unknown,
    /** How far the server has come, 0 to 1. */
    val progress: Double = 0.0,
    /** Bytes: the original's, or the converted file's once done. */
    val size: Long? = null,
    val error: String? = null,
    val url: String = "",
)

/** A subtitle track of the file being played. */
@Serializable
data class SubtitleTrack(
    val index: Int,
    val codec: String,
    val language: String? = null,
    val title: String? = null,
    val default: Boolean = false,
    val forced: Boolean = false,
    val external: Boolean = false,
    val textBased: Boolean = false,
    /** WebVTT: "/api/files/7/subtitles/3.vtt"; `null` for subtitles shown as pictures. */
    val url: String? = null,
    /** Added on the server (found online, or a file uploaded), not beside the media. */
    val added: Boolean = false,
)

/**
 * The answer of POST /api/playback (apps/server/src/playback.rs,
 * `PlaybackInfo`): how the item plays and from where.
 */
@Serializable
data class PlaybackInfo(
    /** For progress reports, the session's status and DELETE /api/playback/{session} when done. */
    val sessionId: String,
    val method: PlaybackMethod,
    /** Direct: "/api/files/{id}/stream"; otherwise the HLS playlist, "/api/playback/{session}/index.m3u8". */
    val url: String,
    val hls: Boolean = false,
    val fileId: Long,
    val durationMs: Long = 0,
    /** The audio track that plays: the one asked for, else the server's choice for this player. */
    val audioIndex: Int? = null,
    /** The subtitle burned in, only when [burnSubtitles]. */
    val subtitleIndex: Int? = null,
    val burnSubtitles: Boolean = false,
    /** The audio is converted, also in a remux, where the video is copied. */
    val audioConverted: Boolean = false,
    /** Every subtitle track of the file. */
    val subtitles: List<SubtitleTrack> = emptyList(),
    /** Why it isn't played as it is, one short sentence each. */
    val reasons: List<String> = emptyList(),
    /** The same in one line. */
    val reason: String? = null,
    /**
     * When the original is converted: the closest quality players offer
     * ("30m" = 30 Mbps 4K, "20m", "15m" = 15 Mbps 4K, "12m", "8m", "4m",
     * "2m"); `null` when a quality was asked for.
     */
    val autoQuality: String? = null,
    /**
     * What the screen gets of HDR video, by name: "Dolby Vision P8.1",
     * "HDR10 (Dolby Vision base layer)", "Dolby Vision P8.1 (converted from
     * P7 FEL)", "SDR (tone-mapped)"… `null` for SDR video and older servers.
     */
    val videoRange: String? = null,
    /**
     * The stream comes in several sizes Media3 moves between by itself, as
     * the connection allows: Automatic leaves the session be.
     */
    val adaptive: Boolean = false,
)

/** A transcode job's state (packages/transcoder-api, `JobState`). */
@Serializable(with = JobState.Serializer::class)
enum class JobState(override val wire: String) : WireEnum {
    Starting("starting"),

    /** An ffmpeg process is making segments. */
    Running("running"),

    /** Nothing running: the player paused, or everything asked for is ready. */
    Idle("idle"),

    /** The whole file is made. */
    Finished("finished"),

    /** The last process failed: see [TranscodeInfo.lastError]. */
    Failed("failed"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<JobState>("JobState", JobState.entries, JobState.Unknown)
}

/** What a transcode does with a stream. */
@Serializable(with = StreamAction.Serializer::class)
enum class StreamAction(override val wire: String) : WireEnum {
    Copy("copy"),
    Transcode("transcode"),

    /** The file has no such stream, or it isn't played. */
    None("none"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<StreamAction>("StreamAction", StreamAction.entries, StreamAction.Unknown)
}

/**
 * The live numbers of a session's transcode, for the player's details
 * (apps/server/src/playback.rs, `TranscodeInfo`).
 */
@Serializable
data class TranscodeInfo(
    val jobId: String,
    val state: JobState = JobState.Unknown,
    val video: StreamAction = StreamAction.Unknown,
    val audio: StreamAction = StreamAction.Unknown,
    /** "HEVC 3840×2160 Dolby Vision P8.1". */
    val videoIn: String? = null,
    /** "H.264 1920×1080". */
    val videoOut: String? = null,
    /** "EAC3 5.1". */
    val audioIn: String? = null,
    /** "AAC 2.0". */
    val audioOut: String? = null,
    /** "h264_nvenc", "libx264"… */
    val encoder: String? = null,
    /** The software encoder's preset now ("veryfast"…); the automatic one changes. */
    val preset: String? = null,
    /** "cuda", "vulkan", "cpu"… */
    val decoder: String? = null,
    /** "gpu" (decode, filters and encode on one GPU), "hybrid", "cpu" or "copy". */
    val pipeline: String? = null,
    /** "libplacebo", "opencl" or "zscale" when HDR or Dolby Vision is tone-mapped to SDR. */
    val tonemap: String? = null,
    val subtitleBurn: Boolean = false,
    /** ffmpeg's speed: 2.5 is two and a half times real time. */
    val speed: Double? = null,
    val fps: Double? = null,
    /** How far ffmpeg got, in ms of the media. */
    val transcodedMs: Long = 0,
    val segmentsReady: Int = 0,
    val segmentsTotal: Int = 0,
    /** What the transcode aims for, video and audio together, in bits per second. */
    val bitrateOut: Long? = null,
    val lastError: String? = null,
)

/**
 * GET /api/playback/{session}: how the session plays now, with its
 * transcode's live numbers (apps/server/src/playback.rs, `SessionStatus`).
 */
@Serializable
data class SessionStatus(
    val sessionId: String,
    val method: PlaybackMethod,
    /** `null` for direct play, or when the transcoder can't say. */
    val transcode: TranscodeInfo? = null,
)

/** The state a progress report gives. */
@Serializable(with = ProgressState.Serializer::class)
enum class ProgressState(override val wire: String) : WireEnum {
    Playing("playing"),
    Paused("paused"),

    /** Waiting for data. The server takes it (apps/server/src/api/items.rs) though docs/API.md doesn't list it. */
    Buffering("buffering"),

    /** The player closed. */
    Stopped("stopped"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<ProgressState>("ProgressState", ProgressState.entries, ProgressState.Unknown)
}

/**
 * The body of POST /api/items/{id}/progress (apps/server/src/api/items.rs,
 * `Progress`): every ~10 s while playing, on pause, and [ProgressState.Stopped]
 * on closing. At 90 % the item is watched and its resume point cleared.
 */
@Serializable
data class ProgressReport(
    val positionMs: Long,
    /** The player's own duration; `null`: the item's. */
    val durationMs: Long? = null,
    val state: ProgressState = ProgressState.Playing,
    /** Keeps the session, and its transcode, alive. */
    val sessionId: String? = null,
    /**
     * How long ago the report was made, for one sent late (a download played
     * while the server was out of reach): the server keeps whatever happened
     * to the item since.
     */
    val ageMs: Long? = null,
    /** The audio track the player plays, by stream index; `null` leaves it out, and the server keeps what it knew. */
    val audioIndex: Int? = null,
    /** The subtitles it shows, by stream index, [PlayingSession.SUBTITLES_OFF] for none; `null` leaves it out. */
    val subtitleIndex: Int? = null,
)

/**
 * The body of PUT /api/items/{id}/streams: the tracks picked, remembered
 * for the whole show (episodes) or the movie, by language, title and
 * forced, so every episode starts with its own matching tracks.
 */
@OptIn(ExperimentalSerializationApi::class)
@KeepGeneratedSerializer
@Serializable(with = StreamChoice.Serializer::class)
data class StreamChoice(
    /** The file the indexes are of; `null`: the item's first. */
    val fileId: Long? = null,
    /**
     * The audio track the person picked. `null` is left out of the body
     * ([PleyJson] writes no nulls), which keeps the audio chosen before:
     * subtitles alone were picked. Without a pick, the server chooses the
     * audio for each device.
     */
    val audioIndex: Int? = null,
    /** `null`: subtitles off (left out, which the server reads the same). */
    val subtitleIndex: Int? = null,
    /**
     * The person chose Automatic: the audio picked before is forgotten
     * (`audio_index` is sent as `null`), and each device plays its best
     * track of the language again. [audioIndex] is not sent then.
     */
    val automaticAudio: Boolean = false,
) {
    /** [automaticAudio] travels as an `audio_index` of `null`, the one `null` a body carries. */
    object Serializer : JsonTransformingSerializer<StreamChoice>(generatedSerializer()) {
        override fun transformSerialize(element: JsonElement): JsonElement {
            val fields = element.jsonObject
            val automatic = fields[AUTOMATIC]?.jsonPrimitive?.booleanOrNull == true
            return JsonObject(
                buildMap {
                    putAll(fields - AUTOMATIC)
                    if (automatic) put(AUDIO, JsonNull)
                },
            )
        }

        override fun transformDeserialize(element: JsonElement): JsonElement {
            val fields = element.jsonObject
            val automatic = fields.containsKey(AUDIO) && fields[AUDIO] is JsonNull
            return if (automatic) JsonObject(fields - AUDIO + (AUTOMATIC to JsonPrimitive(true))) else element
        }

        private const val AUDIO = "audio_index"
        private const val AUTOMATIC = "automatic_audio"
    }
}

/** The body of PUT /api/items/{id}/rating: 0 to 10, rounded to half stars; `null` clears it. */
@Serializable
data class RatingRequest(
    val rating: Double? = null,
)

/**
 * What the server remembers of this client's network, for Automatic's start
 * (GET and PUT /api/playback/network, docs/QUALITY.md "Automatic").
 */
@Serializable
data class PlaybackNetwork(
    /** The server's own network: never a speed test. */
    val local: Boolean = false,
    /** The most Automatic starts at here, kbps; `null`: no limit known. */
    val ceilingKbps: Int? = null,
    /** "playback" (a step down) or "probe" (a speed test); `null` with no ceiling. */
    val source: String? = null,
    /** Unix ms of what set it. */
    val since: Long? = null,
    /** Run a speed test before Automatic starts. */
    val probe: Boolean = false,
)

/** A body of PUT /api/playback/network: a speed test's result, or Automatic's ceiling (`null` forgets it). */
sealed interface NetworkReport {
    val clientId: String

    /** [kbps] measured; [complete]: the whole test arrived within the time it had. */
    data class Probe(override val clientId: String, val kbps: Int, val complete: Boolean) : NetworkReport

    /** Automatic's ceiling changed: a step down or up to a quality; `null` for the original, or Automatic picked again. */
    data class Ceiling(override val clientId: String, val kbps: Int?) : NetworkReport
}

/** What a speed test (GET /api/playback/speedtest) got: [bytes] in [ms] from the first byte; [complete] when all came. */
data class SpeedSample(val bytes: Long, val ms: Long, val complete: Boolean) {
    /** kbps: bytes × 8 / seconds / 1000. */
    val kbps: Int get() = if (ms <= 0) 0 else (bytes * 8.0 / ms).toInt()
}

/** One subtitle found online for a file (GET /api/items/{id}/subtitles/search). */
@Serializable
data class SubtitleResult(
    /** Opaque: what POST /api/items/{id}/subtitles takes back. */
    val id: String,
    val provider: String = "",
    val language: String? = null,
    /** The release it was made for: "Movie.2019.1080p.BluRay.x264-GRP". */
    val name: String = "",
    val format: String? = null,
    val hearingImpaired: Boolean = false,
    val forced: Boolean = false,
    val downloads: Long = 0,
    /** "hash": made for this very file; "title": for the title. */
    val matched: String? = null,
)

/**
 * The answer of GET /api/items/{id}/subtitles/search. [setup] is false when
 * the server has no subtitle search set up (its 503 with `"setup": false`).
 */
@Serializable
data class SubtitleResults(val results: List<SubtitleResult> = emptyList(), val setup: Boolean = true)

/** The body of POST /api/items/{id}/subtitles that adds a subtitle found by search. */
@Serializable
data class AddSubtitle(val result: String, val fileId: Long? = null)
