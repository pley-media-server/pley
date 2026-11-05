package app.pley.android.player.session

import android.content.Context
import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import app.pley.android.player.CastFrom
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.player.R
import java.util.Locale

/** What the session says to people, from strings_session.xml, in the language shown. */
interface PlayerTexts {
    /** A quality by the menu's name: "Original", "8 Mbps 1080p". */
    fun quality(preset: QualityPreset): String

    /**
     * A quality as offered for a file of [fileLines]: one taller than the
     * file is named by the file's lines ("8 Mbps 720p" for a 720p file).
     */
    fun quality(preset: QualityPreset, fileLines: Int?): String

    /** The menu's row while Automatic plays [playing]: "Automatic · Original", "Automatic · 8 Mbps 1080p". */
    fun automatic(playing: String): String

    /** Automatic's hint in the menu. */
    val automaticHint: String

    /** Automatic's hint while it plays [playing], below the original: "Now 8 Mbps 1080p". */
    fun automaticNow(playing: String): String

    /** Said once when Automatic goes down to the quality named [label]. */
    fun loweredNotice(label: String): String

    /** Why Automatic plays lower, for the details. */
    fun lowered(reason: LoweredFor): String
    val skipIntro: String
    val skipCredits: String

    /** "Up next · S1 E2", or "Up next". */
    fun upNext(label: String?): String
    val lastEpisode: String
    val lastInQueue: String
    val subtitlesOff: String
    val subtitlesFailed: String

    /** "Direct play", "Direct stream", "Transcoding", "Direct stream · Audio transcoding". */
    fun method(info: PlaybackInfo): String

    /** The method of a file played from its download: "Downloaded". */
    val downloaded: String

    /** A download that doesn't play here as it is, with the server out of reach to convert it. */
    val downloadNeedsServer: String

    /** A file on a drive that doesn't play here as it is: there is no server to convert it. */
    val driveUnplayable: String

    /** A file on a drive whose picture never started. */
    val driveStuck: String

    /** A file on a drive that can't be read. */
    val driveUnreadable: String

    /** The speed can't change: a file on a drive's sound goes to a receiver as it is, and nothing here decodes it. */
    val driveSpeedUndecodable: String

    /** The server out of reach, for [reason] ("no answer in time"): what to check. */
    fun unreachable(reason: UiText): String

    fun fallback(reason: FallbackReason): String
    val slowServer: String
    fun failure(failure: Failure, detail: String?): String
    val noFile: String

    /** The server stopped the playback: who did, and their [message] when given. */
    fun stopped(message: String?, byAdmin: Boolean): String

    /** A remote sent this here: "Playing from Living room TV". */
    fun playingFrom(from: CastFrom): String

    /** The speed can't change: a download's sound goes to a receiver as it is, and nothing here decodes it. */
    val speedUndecodable: String

    /** The speed can't change: a stream's sound goes to a receiver as it is, nothing here decodes it, and the server couldn't convert it. */
    val speedUnconverted: String

    /** "Find subtitles…" when the server has no subtitle search set up. */
    val searchNotSetUp: String

    /** The server has no subtitle search at all (an older one). */
    val searchUnsupported: String

    /** Nothing found in the profile's language. */
    val searchNothing: String

    /** The search or the adding failed, for [reason]. */
    fun searchFailed(reason: UiText): String

    /** A result's line under its name: its language, whether it was made for this very file, and how often it was downloaded. */
    fun foundHint(language: String?, exact: Boolean, hearingImpaired: Boolean, downloads: Long): String

    /** The track added and shown. */
    val subtitleAdded: String

    // The details' rows.
    val rowVideo: String
    val rowAudio: String
    val rowBitrate: String
    val rowEncoder: String
    val rowHdr: String
    val rowSubtitles: String
    val rowSpeed: String
    fun toneMapped(tonemap: String): String
    val burnedIn: String

    /** A transcode's [speed] against real time, and how far ahead of the playhead it is: "2.5× real time · 40 s ready". */
    fun speed(speed: Double, readySeconds: Int): String

    /** Where a transcode runs, by the server's name for it ("gpu", "hybrid"): "GPU", "GPU and CPU". */
    fun pipeline(pipeline: String): String

    // The engine's own lines for the details, "Label: value".
    fun engineVideo(format: String, width: Int, height: Int, frameRate: Float?): String
    fun engineVideoDecoder(name: String): String
    fun engineAudio(format: String, channels: Int): String
    fun engineAudioDecoder(name: String): String
    fun engineDroppedFrames(dropped: Long, total: Long): String

    /** [text] in the language shown: an error's words ([app.pley.android.common.errorText]). */
    fun words(text: UiText): String
}

/** [PlayerTexts] from the app's resources, in the language shown. */
class ResourcePlayerTexts(context: Context) : PlayerTexts {
    private val app = context.applicationContext
    private val res get() = AppLanguage.resources(app)

    private fun s(id: Int) = res.getString(id)

    override fun quality(preset: QualityPreset): String = s(
        when (preset.id) {
            QualityPreset.AUTOMATIC_ID -> R.string.session_quality_automatic
            "30m" -> R.string.session_quality_30m
            "15m" -> R.string.session_quality_15m
            "20m" -> R.string.session_quality_20m
            "12m" -> R.string.session_quality_12m
            "8m" -> R.string.session_quality_8m
            "4m" -> R.string.session_quality_4m
            "2m" -> R.string.session_quality_2m
            else -> R.string.session_quality_original
        },
    )

    override fun quality(preset: QualityPreset, fileLines: Int?): String {
        val most = preset.maxHeight
        val kbps = preset.maxBitrateKbps
        if (most == null || kbps == null || fileLines == null || fileLines >= most) return quality(preset)
        return res.getString(R.string.session_quality_named, kbps / 1000, linesLabel(fileLines))
    }

    override fun automatic(playing: String): String = res.getString(R.string.session_quality_automatic_playing, playing)

    override val automaticHint: String get() = s(R.string.session_quality_automatic_hint)

    override fun automaticNow(playing: String): String = res.getString(R.string.session_quality_automatic_now, playing)

    override fun loweredNotice(label: String): String = res.getString(R.string.session_quality_lowered_notice, label)

    override fun lowered(reason: LoweredFor): String = s(
        when (reason) {
            LoweredFor.Network -> R.string.session_quality_lowered_network
            LoweredFor.Device -> R.string.session_quality_lowered_device
        },
    )
    override val skipIntro: String get() = s(R.string.session_skip_intro)
    override val skipCredits: String get() = s(R.string.session_skip_credits)

    override fun upNext(label: String?): String =
        if (label.isNullOrEmpty()) s(R.string.session_up_next) else res.getString(R.string.session_up_next_episode, label)

    override val lastEpisode: String get() = s(R.string.session_last_episode)
    override val lastInQueue: String get() = s(R.string.session_last_in_queue)
    override val subtitlesOff: String get() = s(R.string.session_subtitles_off)
    override val subtitlesFailed: String get() = s(R.string.session_subtitles_failed)

    override fun method(info: PlaybackInfo): String = when (info.method) {
        PlaybackMethod.Remux -> if (info.audioConverted) s(R.string.session_method_remux_audio) else s(R.string.session_method_remux)
        PlaybackMethod.Transcode -> s(R.string.session_method_transcode)
        else -> s(R.string.session_method_direct)
    }

    override fun fallback(reason: FallbackReason): String = s(
        when (reason) {
            FallbackReason.Unplayable -> R.string.session_fallback_unplayable
            FallbackReason.NotSmooth -> R.string.session_fallback_not_smooth
        },
    )

    override val slowServer: String get() = s(R.string.session_slow_server)

    override fun failure(failure: Failure, detail: String?): String = when (failure) {
        Failure.Unplayable -> s(R.string.session_error_unplayable)
        Failure.Stuck -> s(R.string.session_error_stuck)
        Failure.Short -> s(R.string.session_error_short)
        Failure.Stopped -> s(R.string.session_error_stopped)
        Failure.Network -> s(R.string.session_error_network)
        Failure.Engine -> res.getString(R.string.session_error_engine, detail?.substringBefore(':')?.takeIf { it.isNotBlank() } ?: "?")
    }

    override val noFile: String get() = s(R.string.session_error_no_file)
    override val downloaded: String get() = s(R.string.session_method_downloaded)
    override val downloadNeedsServer: String get() = s(R.string.session_error_download_needs_server)
    override val driveUnplayable: String get() = s(R.string.session_drive_unplayable)
    override val driveStuck: String get() = s(R.string.session_drive_stuck)
    override val driveUnreadable: String get() = s(R.string.session_drive_unreadable)
    override val driveSpeedUndecodable: String get() = s(R.string.session_drive_speed_undecodable)

    override fun unreachable(reason: UiText): String {
        val res = res
        return res.getString(R.string.session_error_unreachable, UiText.Sentence(reason).resolve(res))
    }

    override fun playingFrom(from: CastFrom): String = from.notice(res)

    override fun stopped(message: String?, byAdmin: Boolean): String {
        val who = s(if (byAdmin) R.string.session_stopped_by_admin else R.string.session_stopped_elsewhere)
        return if (message.isNullOrBlank()) who else res.getString(R.string.session_stopped_message, who, message.trim())
    }
    override val speedUndecodable: String get() = s(R.string.session_speed_undecodable)
    override val speedUnconverted: String get() = s(R.string.session_speed_unconverted)
    override val searchNotSetUp: String get() = s(R.string.session_search_not_set_up)
    override val searchUnsupported: String get() = s(R.string.session_search_unsupported)
    override val searchNothing: String get() = s(R.string.session_search_nothing)

    override fun searchFailed(reason: UiText): String {
        val res = res
        return res.getString(R.string.session_search_failed, reason.resolve(res))
    }

    override fun foundHint(language: String?, exact: Boolean, hearingImpaired: Boolean, downloads: Long): String {
        val res = res
        return listOfNotNull(
            language,
            if (exact) res.getString(R.string.session_search_exact) else null,
            if (hearingImpaired) res.getString(R.string.session_search_sdh) else null,
            if (downloads > 0) res.getQuantityString(R.plurals.session_search_downloads, downloads.coerceAtMost(Int.MAX_VALUE.toLong()).toInt(), downloads) else null,
        ).joinToString(" · ")
    }

    override val subtitleAdded: String get() = s(R.string.session_search_added)
    override val rowVideo: String get() = s(R.string.session_row_video)
    override val rowAudio: String get() = s(R.string.session_row_audio)
    override val rowBitrate: String get() = s(R.string.session_row_bitrate)
    override val rowEncoder: String get() = s(R.string.session_row_encoder)
    override val rowHdr: String get() = s(R.string.session_row_hdr)
    override val rowSubtitles: String get() = s(R.string.session_row_subtitles)
    override val rowSpeed: String get() = s(R.string.session_row_speed)

    override fun toneMapped(tonemap: String): String = res.getString(R.string.session_tone_mapped, tonemap)

    override val burnedIn: String get() = s(R.string.session_burned_in)

    override fun speed(speed: Double, readySeconds: Int): String {
        val res = res
        // As the web writes it: one decimal, in the language's own way ("2,5" in French).
        val times = String.format(res.configuration.locales[0] ?: Locale.ROOT, "%.1f", speed)
        return if (readySeconds > 0) res.getString(R.string.session_speed_ready, times, readySeconds) else res.getString(R.string.session_speed, times)
    }

    override fun pipeline(pipeline: String): String = when (pipeline) {
        "gpu" -> "GPU"
        "cpu" -> "CPU"
        "hybrid" -> s(R.string.session_pipeline_hybrid)
        "copy" -> s(R.string.session_pipeline_copy)
        else -> pipeline
    }

    override fun engineVideo(format: String, width: Int, height: Int, frameRate: Float?): String =
        if (frameRate != null && frameRate > 0) {
            // A technical readout: the rate as the decoder gives it, the same in every language.
            res.getString(R.string.session_engine_video_rate, format, width, height, String.format(Locale.ROOT, "%.3f", frameRate))
        } else {
            res.getString(R.string.session_engine_video, format, width, height)
        }

    override fun engineVideoDecoder(name: String): String = res.getString(R.string.session_engine_video_decoder, name)

    override fun engineAudio(format: String, channels: Int): String = res.getString(R.string.session_engine_audio, format, channels)

    override fun engineAudioDecoder(name: String): String = res.getString(R.string.session_engine_audio_decoder, name)

    override fun engineDroppedFrames(dropped: Long, total: Long): String = res.getString(R.string.session_engine_dropped, dropped, total)

    override fun words(text: UiText): String = text.resolve(res)
}

/** Lines as qualities name them: "4K", "1080p", "720p". */
internal fun linesLabel(lines: Int): String = if (lines >= 2160) "4K" else "${lines}p"
