package app.pley.android.model

import kotlinx.serialization.Serializable

/**
 * A media file of an item (apps/server/src/catalog.rs, `MediaFileDto`).
 * Sizes are bytes, bitrates bits per second, durations ms.
 */
@Serializable
data class MediaFile(
    val id: Long,
    /** An app's token gets the file name only, never where the server keeps it. */
    val path: String,
    val size: Long = 0,
    /** "mkv", "mp4", "webm", "avi", "mp3", "flac"… */
    val container: String? = null,
    val durationMs: Long? = null,
    val bitrate: Long? = null,
    val width: Int? = null,
    val height: Int? = null,
    /** ffprobe's name: "h264", "hevc", "av1"… */
    val videoCodec: String? = null,
    /** The default audio stream's codec. */
    val audioCodec: String? = null,
    val streams: List<MediaStream> = emptyList(),
    /** From `{edition-Final Cut}` in the file's or its folder's name. */
    val edition: String? = null,
    /** The audio track ([MediaStream.index]) to start with for this profile: its choice, else its defaults. `null`: the file's default. */
    val selectedAudio: Int? = null,
    /**
     * The profile picked [selectedAudio] for the movie or show. `false`:
     * nothing was picked, so each device plays its best track of that
     * language (the server picks it at play time), and a title's page says
     * automatic with the language rather than naming this track.
     */
    val audioPicked: Boolean = false,
    /** The subtitle track to start with; `null`: none. */
    val selectedSubtitle: Int? = null,
    /** From the container, by start time. */
    val chapters: List<Chapter> = emptyList(),
)

/** A stream's kind (packages/transcoder-api, `StreamKind`). */
@Serializable(with = StreamKind.Serializer::class)
enum class StreamKind(override val wire: String) : WireEnum {
    Video("video"),
    Audio("audio"),
    Subtitle("subtitle"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<StreamKind>("StreamKind", StreamKind.entries, StreamKind.Unknown)
}

/**
 * One stream of a file, as ffprobe found it (packages/transcoder-api,
 * `MediaStream`). Fields that don't apply to its kind, or weren't found,
 * are left out.
 */
@Serializable
data class MediaStream(
    /** ffprobe's stream index; subtitle files beside the media count from 1000. */
    val index: Int,
    val kind: StreamKind,
    val codec: String,
    /** ISO 639-2 as found in the file ("eng"). */
    val language: String? = null,
    val title: String? = null,
    val default: Boolean = false,
    val forced: Boolean = false,
    /** Bits per second. */
    val bitrate: Long? = null,
    /** "Main 10", "High", "LC"… */
    val profile: String? = null,
    // Video.
    val width: Int? = null,
    val height: Int? = null,
    val frameRate: Double? = null,
    val bitDepth: Int? = null,
    /** "yuv420p10le"… */
    val pixFmt: String? = null,
    /** The codec level as ffprobe numbers it: 153 for HEVC 5.1, 41 for H.264 4.1. */
    val level: Int? = null,
    val interlaced: Boolean? = null,
    val hdr: Boolean? = null,
    val dynamicRange: DynamicRange? = null,
    /** Dolby Vision profile: 5, 7, 8… */
    val dvProfile: Int? = null,
    val dvLevel: Int? = null,
    /** Dolby Vision base-layer compatibility: 0 none, 1 HDR10, 2 SDR, 4 HLG, 6 Blu-ray HDR10 (profile 7). */
    val dvBlCompat: Int? = null,
    /** Profile 7's enhancement layer: "fel" (full) or "mel" (minimal); `null` when not known. */
    val dvEl: String? = null,
    /**
     * HDR10+ metadata in a Dolby Vision or PQ stream's first frames: `true`
     * on a Dolby Vision stream is Dolby Vision + HDR10+; `null` when not
     * looked for (other streams, or an older server).
     */
    val hdr10plus: Boolean? = null,
    /** "smpte2084", "arib-std-b67", "bt709"… */
    val colorTransfer: String? = null,
    val colorPrimaries: String? = null,
    val colorSpace: String? = null,
    // Audio.
    val channels: Int? = null,
    /** "5.1(side)", "stereo"… */
    val channelLayout: String? = null,
    /** Hz. */
    val sampleRate: Int? = null,
    // Subtitles.
    /** srt, ass, WebVTT, mov_text (true); PGS, VobSub (false: they're burned in). */
    val textBased: Boolean? = null,
    /** A subtitle file beside the media, or one added on the server ([added]). */
    val external: Boolean = false,
    /** A subtitle added on the server (found online, or a file uploaded). */
    val added: Boolean = false,
)

/** A chapter of a file, from its container. */
@Serializable
data class Chapter(
    val startMs: Long,
    val endMs: Long,
    val title: String? = null,
    /** "/api/files/{id}/chapters/{n}.jpg?v=…", once the file has seek previews. */
    val thumbUrl: String? = null,
)

/**
 * The chapters a book's part is gone through by (Previous and Next, its
 * page's list): those with a length, by start; none with fewer than two.
 */
fun playableChapters(chapters: List<Chapter>): List<Chapter> =
    chapters.filter { it.endMs > it.startMs }.sortedBy { it.startMs }.takeIf { it.size >= 2 }.orEmpty()

/** What a marker marks. */
@Serializable(with = MarkerKind.Serializer::class)
enum class MarkerKind(override val wire: String) : WireEnum {
    Intro("intro"),
    Credits("credits"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<MarkerKind>("MarkerKind", MarkerKind.entries, MarkerKind.Unknown)
}

/** How a marker was found (docs/API.md, "Intro and credits detection"). */
@Serializable(with = MarkerSource.Serializer::class)
enum class MarkerSource(override val wire: String) : WireEnum {
    /** A chapter titled like an intro or credits. */
    Chapter("chapter"),

    /** Audio the season's episodes share. */
    Fingerprint("fingerprint"),

    /** Text on black near the end. */
    Frames("frames"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<MarkerSource>("MarkerSource", MarkerSource.entries, MarkerSource.Unknown)
}

/**
 * An intro or the credits of a file (apps/server/src/markers/mod.rs,
 * `Marker`): markers belong to a file, since versions can be different
 * cuts. Credits that run to the end of the file end at its duration.
 */
@Serializable
data class Marker(
    val fileId: Long,
    val kind: MarkerKind,
    val startMs: Long,
    val endMs: Long,
    val source: MarkerSource = MarkerSource.Unknown,
)
