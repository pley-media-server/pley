package app.pley.android.player

import app.pley.android.common.AppLanguage
import app.pley.android.model.Marker
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.TranscodeInfo
import app.pley.android.player.sleep.SleepState
import java.text.DecimalFormatSymbols
import java.util.Locale

/** Where a playback stands. */
sealed interface PlayerPhase {
    /** Asking the server and loading. */
    data object Preparing : PlayerPhase

    /** The TV is changing its display mode to the video's frame rate; the picture may go dark. */
    data object Switching : PlayerPhase

    data object Buffering : PlayerPhase

    data object Playing : PlayerPhase

    data object Paused : PlayerPhase

    /** The end, with nothing next: the screen goes back to where playing started. */
    data object Ended : PlayerPhase

    /**
     * It stopped: [message] for people, and whether trying again may help.
     * [offline]: the server couldn't be reached ("Can't reach your server").
     */
    data class Error(val message: String, val canRetry: Boolean, val offline: Boolean = false) : PlayerPhase
}

/** An audio or subtitle track as the player's menus show it. */
data class TrackChoice(
    /** The server's stream index; `null` for "Off". */
    val index: Long?,
    val label: String,
    val selected: Boolean,
    /** Subtitles shown as pictures, which a stream draws onto the video ("burn-in"). */
    val burnIn: Boolean = false,
)

/** A quality in the player's menu (apps/web/src/player/SettingsPopover.tsx). */
data class QualityChoice(
    /** "original", "8m"…: what [PlaybackController.selectQuality] takes. */
    val id: String,
    /** "Original", "8 Mbps 1080p". */
    val label: String,
    /** The original's "1080p · 8.2 Mbps", Automatic's "Now 8 Mbps 1080p". */
    val hint: String?,
    val selected: Boolean,
)

/** A chapter for the chapters menu and the seek bar's ticks. */
data class ChapterMark(
    val title: String?,
    val startMs: Long,
    val endMs: Long,
    /** Absolute address of its picture, once the file has seek previews. */
    val imageUrl: String?,
) {
    /**
     * What the chapter is called wherever chapters show: its title, unless
     * it has none or the title only says where it starts ("00:06:19.212",
     * as mkvmerge names chapters, or "6:19"), which tells a viewer nothing.
     * `null` then, and the screens number it: "Chapter 3". A time inside a
     * real name ("Act 2: 12:00 at Noon") keeps it.
     */
    val name: String? = title?.trim()?.takeIf { it.isNotEmpty() && !TIME_ONLY.matches(it) }

    private companion object {
        /** A title that is only a time: "6:19", "00:06:19", "00:06:19.212", "01:02:03,500". */
        val TIME_ONLY = Regex("""\d{1,2}(:\d{2}){1,2}([.,]\d+)?""")
    }
}

/** One line of the details: "Video", "HEVC 4K HDR10 → H.264 1080p SDR". */
data class DetailRow(val label: String, val value: String)

/** How the video plays, while the details are open (apps/web/src/player/PlaybackDetails.tsx). */
data class PlaybackDetailsUi(
    /** "Direct play", "Direct stream", "Transcoding", "Direct stream · Audio transcoding". */
    val method: String,
    /** Why, in a sentence or two; `null` for a direct play that needs no reason. */
    val why: String?,
    /** Where a transcode runs ("GPU", "CPU"…), for its badge; `null` otherwise. */
    val pipeline: String?,
    val rows: List<DetailRow>,
    /** The engine's own lines: decoders, picture, dropped frames. */
    val engine: List<String>,
    /** The way the server sends it; `null` in details made by hand. */
    val playbackMethod: PlaybackMethod? = null,
    /** The file playing as the server described it, for a full technical readout. */
    val file: MediaFile? = null,
    /** The streams in play: the file's video, the audio and the subtitles chosen. */
    val video: MediaStream? = null,
    val audio: MediaStream? = null,
    val subtitle: MediaStream? = null,
    /** How [subtitle] reaches the screen; `null` without subtitles. */
    val subtitleDelivery: SubtitleDelivery? = null,
    /** The transcode's live numbers (remux and transcode); `null` in direct play or before they come. */
    val transcode: TranscodeInfo? = null,
)

/** How subtitles reach the screen. */
enum class SubtitleDelivery {
    /** Read from the file played as it is, drawn by the player. */
    File,

    /** The server's WebVTT (streams, and subtitle files beside the media), drawn by the player. */
    Server,

    /** Picture subtitles beside a stream: the server's .mks of the track, drawn by the player. */
    ServerPictures,

    /** Drawn into the picture by the server's transcode. */
    BurnedIn,
}

/** What plays next, during its card. */
data class UpNextUi(
    val itemId: Long,
    val title: String,
    /** "Up next · S1 E2", or "Up next". */
    val heading: String,
    /** Absolute address of its still (16:9), for the card; preloaded. */
    val imageUrl: String?,
    /** Seconds before it plays by itself; `null` when it won't (autoplay off, or cancelled). */
    val secondsLeft: Int?,
)

/** The speeds offered: 0.5× to 2×, 1× normal. */
object PlaybackSpeeds {
    val ALL: List<Float> = listOf(0.5f, 0.75f, 1f, 1.25f, 1.5f, 2f)

    /** "1×", "1.25×", "0.5×", with [locale]'s decimal separator ("1,25×" in French). */
    fun label(speed: Float, locale: Locale = AppLanguage.locale): String {
        val text = if (speed == speed.toInt().toFloat()) speed.toInt().toString() else speed.toString().trimEnd('0')
        return decimals(text, locale) + "×"
    }
}

/** [number], written with a point, with [locale]'s decimal separator instead. */
internal fun decimals(number: String, locale: Locale): String = number.replace('.', DecimalFormatSymbols.getInstance(locale).decimalSeparator)

/** The audio delay's range and step. */
object AudioDelays {
    const val MAX_MS = 5_000L
    const val STEP_MS = 50L

    /** [ms] within ±5 s, on a 50 ms step. */
    fun clamp(ms: Long): Long = (Math.round(ms.toDouble() / STEP_MS) * STEP_MS).coerceIn(-MAX_MS, MAX_MS)

    /** "+0.25 s", "−1.5 s", "0 s": the sound later (+) or earlier (−) than the picture, with [locale]'s decimal separator. */
    fun label(ms: Long, locale: Locale = AppLanguage.locale): String {
        if (ms == 0L) return "0 s"
        val seconds = java.math.BigDecimal(kotlin.math.abs(ms)).movePointLeft(3).stripTrailingZeros().toPlainString()
        return (if (ms > 0) "+" else "−") + decimals(seconds, locale) + " s"
    }
}

/** A subtitle found online, as the search's list shows it. */
data class FoundSubtitle(
    /** What [PlaybackController.addFoundSubtitle] takes. */
    val id: String,
    /** The release it was made for. */
    val label: String,
    /** "English · Made for this file · 1,234 downloads". */
    val hint: String?,
)

/** "Find subtitles…": where the search stands. */
sealed interface SubtitleSearchUi {
    data object Searching : SubtitleSearchUi

    /** The results, best first; none found when empty ([message] says so). */
    data class Found(val results: List<FoundSubtitle>, val message: String?) : SubtitleSearchUi

    /** Adding [id]: its track comes next. */
    data class Adding(val id: String, val results: List<FoundSubtitle>) : SubtitleSearchUi

    /** It can't search: not set up on the server, or it failed ([message] says which). */
    data class Unavailable(val message: String) : SubtitleSearchUi
}

/**
 * What a player screen shows, in one immutable value. Positions change
 * often: screens read them only where they draw, so the rest of the
 * screen doesn't recompose with them.
 */
data class PlayerUiState(
    val phase: PlayerPhase = PlayerPhase.Preparing,
    /** The item playing; 0 until known. */
    val itemId: Long = 0,
    /** A movie's "Title (Year)", an episode's show. */
    val title: String = "",
    /** An episode's "S1 E2 — Title". */
    val subtitle: String? = null,
    val positionMs: Long = 0,
    val durationMs: Long = 0,
    val bufferedMs: Long = 0,
    /** The server is out of reach: playback waits where it stood and keeps trying ("Reconnecting…"). */
    val reconnecting: Boolean = false,
    val audio: List<TrackChoice> = emptyList(),
    val subtitles: List<TrackChoice> = emptyList(),
    /** The quality chosen, as the menu's row says it: "Original", "8 Mbps 1080p", "Automatic · Original". */
    val quality: String = "Original",
    val qualities: List<QualityChoice> = emptyList(),
    /** "Direct play", "Direct stream", "Transcoding"; `null` before the server answered. */
    val method: String? = null,
    /** The way [method] names, for its badge's colour: a download plays as it is ([PlaybackMethod.Direct]). */
    val playbackMethod: PlaybackMethod? = null,
    /** Why it plays this way when the player chose it: "The original didn't play smoothly on this device." */
    val note: String? = null,
    /** The player switched to a transcode by itself: offer "Play original". */
    val canPlayOriginal: Boolean = false,
    /** The server can't convert fast enough: say so, and offer a lower quality. */
    val suggestLowerQuality: Boolean = false,
    /** "Skip Intro" or "Skip Credits" while one is showing. */
    val skipLabel: String? = null,
    /** The playing file's intro and credits, for the seek bar. */
    val markers: List<Marker> = emptyList(),
    val chapters: List<ChapterMark> = emptyList(),
    /** Seek previews are ready: [PlaybackController.thumbnail] gives them. */
    val hasThumbnails: Boolean = false,
    /** The title of what plays next, during its card. */
    val upNext: String? = null,
    val upNextSecondsLeft: Int? = null,
    /** Everything the Up Next card shows. */
    val upNextCard: UpNextUi? = null,
    /** The details, while open. */
    val details: PlaybackDetailsUi? = null,
    /** A short message for a toast, until [PlaybackController.dismissNotice]: "This is the last episode". */
    val notice: String? = null,
    /** How far the subtitles are moved: positive shows them later ([PlaybackController.setSubtitleOffset]). */
    val subtitleOffsetMs: Long = 0,
    /** The subtitles chosen can be moved: text, or pictures drawn by the player from the server's .mks. */
    val subtitleSync: Boolean = false,
    /** The subtitles chosen are text, which the subtitle style (size, background, font, position) is for: pictures keep their own look. */
    val subtitleStyled: Boolean = false,
    /** The sleep timer. */
    val sleep: SleepState = SleepState(),
    /** The playback speed, 1 normal ([PlaybackController.setSpeed]). */
    val speed: Float = 1f,
    /** Why the speed can't change now (a download's sound that nothing here decodes); `null` when it can. */
    val speedNote: String? = null,
    /** How much later than the picture the sound plays, ms ([PlaybackController.setAudioDelay]). */
    val audioDelayMs: Long = 0,
    /** "Find subtitles…" is offered: the file streams from the server. */
    val canFindSubtitles: Boolean = false,
    /** The search for subtitles online, while its list is open. */
    val subtitleSearch: SubtitleSearchUi? = null,
    /** The server stopped this playback (an administrator, or another device): the error says so, and the screen leaves. */
    val stoppedByServer: Boolean = false,
    /**
     * The player moves to a stream made ready beside the one that played (a
     * quality, track or Automatic change): the last picture stays on screen
     * until the new one's first, and a wait shows the spinner only after
     * [HANDOFF_SPINNER_DELAY_MS].
     */
    val handoff: Boolean = false,
) {
    companion object {
        /** How long a wait lasts before the spinner shows while the last picture stands in ([handoff]). */
        const val HANDOFF_SPINNER_DELAY_MS = 2_500L
    }
}
