package app.pley.android.player.session

import app.pley.android.caps.CapsDecision
import app.pley.android.caps.CapsSnapshot
import app.pley.android.caps.PlaybackCapsPolicy
import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemDetail
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.StreamKind
import app.pley.android.model.caps.ClientCaps
import app.pley.android.player.PlayRequest

/**
 * How a playback is asked for: which file, from where, with which tracks,
 * and what goes in the body of POST /api/playback. Pure: it reads the item,
 * the device's capabilities and the choices, nothing else.
 *
 * The audio is the server's to choose unless the person picked a track: it
 * knows the profile's language and, from the capabilities, which of that
 * language's tracks this device plays best, and says which one plays
 * ([PlaybackInfo.audioIndex]). Track indexes go to the server only when the
 * server has to act on them (PleyApi.kt:116-120 of the old player sent them
 * always, which forced remuxes and burn-ins of files that play as they are):
 * - `audio_index` for a track picked, when it can't be played here
 *   (neither decoded nor passed through), the session streams HLS (one
 *   audio track), or it isn't the track the item's details give; the
 *   server's own choice is asked for again with each session, so a stream
 *   gets the track it copies;
 * - `subtitle_index` only to burn subtitles shown as pictures into HLS,
 *   and never when this player side-loads them (the server's .mks).
 * In direct play both are switched here, by the engine.
 */
object PlaybackPlan {
    /**
     * [caps] as the server is told of one video: without the screen's Dolby
     * Vision when the engine says the display mode it plays in doesn't show
     * it ([dolbyVisionShown] `false`), and without passthrough when the
     * sound must be [decoded] here: the server converts what isn't.
     */
    fun capsFor(caps: CapsSnapshot?, dolbyVisionShown: Boolean?, decoded: Boolean = false): CapsSnapshot? {
        val shown = if (dolbyVisionShown == false) caps?.withoutDolbyVisionScreen() else caps
        return if (decoded) shown?.withoutPassthrough() else shown
    }

    /** The file to play: the one asked for, else the one that plays best here, else the first. */
    fun file(item: ItemDetail, requested: Long?, caps: CapsSnapshot?, policy: PlaybackCapsPolicy): MediaFile? {
        requested?.let { id -> item.files.firstOrNull { it.id == id }?.let { return it } }
        val best = caps?.let { runCatching { policy.bestFile(item, it) }.getOrNull() }
        return item.files.firstOrNull { it.id == best } ?: item.files.firstOrNull()
    }

    /** Where to start: as asked, else from the start when asked or watched, else the resume point. */
    fun startMs(item: ItemDetail, request: PlayRequest): Long = when {
        request.startMs != null -> request.startMs.coerceAtLeast(0)
        request.fromStart || item.userState.watched -> 0
        else -> item.userState.positionMs.coerceAtLeast(0)
    }

    fun audioStreams(file: MediaFile?): List<MediaStream> = file?.streams.orEmpty().filter { it.kind == StreamKind.Audio }

    fun subtitleStreams(file: MediaFile?): List<MediaStream> = file?.streams.orEmpty().filter { it.kind == StreamKind.Subtitle }

    /**
     * The subtitle tracks the server's conversion of [file] holds, in their
     * order: those Matroska takes (MP4's text made SRT), not teletext and
     * the like (apps/transcoder plan.rs, `MATROSKA_SUBTITLES`).
     */
    fun convertedSubtitleStreams(file: MediaFile?): List<MediaStream> =
        subtitleStreams(file).filter { !it.external && it.codec.lowercase() in CONVERTED_SUBTITLES }

    private val CONVERTED_SUBTITLES =
        setOf("subrip", "ass", "ssa", "webvtt", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle", "mov_text")

    fun videoStream(file: MediaFile?): MediaStream? {
        val videos = file?.streams.orEmpty().filter { it.kind == StreamKind.Video }
        return videos.firstOrNull { it.default } ?: videos.firstOrNull()
    }

    /**
     * The audio to start with: the track asked for, else `null`, automatic:
     * the server picks the track this device plays best, in the profile's
     * language, and its answer says which ([PlaybackInfo.audioIndex]).
     */
    fun initialAudio(file: MediaFile?, requested: Long?): MediaStream? =
        requested?.let { r -> audioStreams(file).firstOrNull { it.index.toLong() == r } }

    /**
     * The audio [file] plays with track [index], as a session's answer
     * names it ([PlaybackInfo.audioIndex]): that track, else the file's
     * default, else the first.
     */
    fun playedAudio(file: MediaFile?, index: Int?): MediaStream? {
        val audio = audioStreams(file)
        return audio.firstOrNull { it.index == index }
            ?: audio.firstOrNull { it.default }
            ?: audio.firstOrNull()
    }

    /**
     * The audio a download starts with when none was asked for, chosen here
     * as no server is asked: [detailsAudio]. When that one doesn't play here
     * (TrueHD or DTS on a TV whose output takes neither) and another track
     * in its language does, that one: the file then plays as it is (a
     * remux's E-AC-3 keeps its Atmos). Never a commentary.
     */
    fun localAudio(file: MediaFile?, caps: CapsSnapshot?): MediaStream? {
        val chosen = detailsAudio(file) ?: return null
        if (caps?.report == null || audioPlaysHere(chosen, caps)) return chosen
        return audioStreams(file)
            .filter { it !== chosen && it.language == chosen.language && !isCommentary(it) && audioPlaysHere(it, caps) }
            .maxWithOrNull(compareBy<MediaStream>({ it.channels ?: 0 }, { it.default }, { -it.index }))
            ?: chosen
    }

    /**
     * The audio the item's details give: the profile's choice
     * (`selected_audio`, which its defaults fill in), else the file's
     * default, else the first. The details don't know the device: a request
     * that names no track gets that one, or a track of its language this
     * device plays better.
     */
    fun detailsAudio(file: MediaFile?): MediaStream? {
        val audio = audioStreams(file)
        return audio.firstOrNull { it.index == file?.selectedAudio }
            ?: audio.firstOrNull { it.default }
            ?: audio.firstOrNull()
    }

    private fun isCommentary(stream: MediaStream): Boolean = stream.title?.contains("commentary", ignoreCase = true) == true

    /**
     * The subtitles to start with: as asked, else the profile's choice (the
     * server applies its defaults to `selected_subtitle`); `null` is off,
     * whatever the file flags as default.
     */
    fun initialSubtitle(file: MediaFile?, requested: Long?): MediaStream? {
        val subs = subtitleStreams(file)
        return requested?.let { r -> subs.firstOrNull { it.index.toLong() == r } }
            ?: subs.firstOrNull { it.index == file?.selectedSubtitle }
    }

    /** Subtitles shown as pictures (PGS, VobSub): a stream burns them into the video. */
    fun isImage(stream: MediaStream?): Boolean = stream != null && stream.textBased == false

    /**
     * Whether pictures chosen for a stream are side-loaded (the .mks drawn
     * by the player) rather than burned in: the report says so
     * (`picture_subtitles_sideload`) and the player can read them.
     */
    fun sideloadsPictures(caps: CapsSnapshot?, canRead: Boolean = true): Boolean =
        canRead && caps?.report?.pictureSubtitlesSideload == true

    /** Where the server's .mks of picture subtitles [index] of [fileId] is. */
    fun pictureSubtitlesPath(fileId: Long, index: Int): String = "/api/files/$fileId/subtitles/$index.mks"

    /**
     * [url] asked for from where the player is ([atMs] of the file): a track
     * the server hasn't copied out of its file yet comes from there first.
     */
    fun pictureUrlAt(url: String, atMs: Long): String =
        if (atMs > 0) "$url${if ('?' in url) '&' else '?'}at=$atMs" else url

    /** Whether this device decodes [stream]'s audio or sends it as it is to the receiver. */
    fun audioPlaysHere(stream: MediaStream?, caps: CapsSnapshot?): Boolean {
        if (stream == null) return true
        val report = caps?.report ?: return true
        val codec = stream.codec.lowercase()
        val known = report.audioCodecs + report.audioDecode.orEmpty() + report.audioPassthrough.orEmpty()
        return known.any { it.equals(codec, ignoreCase = true) }
    }

    /** Whether this device opens [file]'s container as it is. */
    fun containerPlaysHere(file: MediaFile?, caps: CapsSnapshot?): Boolean {
        val container = file?.container?.lowercase() ?: return true
        val containers = caps?.report?.containers?.map { it.lowercase() } ?: return true
        return container in containers ||
            (container == "mkv" && "matroska" in containers) ||
            (container == "matroska" && "mkv" in containers)
    }

    /**
     * Whether the session will surely be a stream (HLS), before asking:
     * another mode than auto, a video this device doesn't play as it is, a
     * container it doesn't open, or audio it can't play. Automatic audio
     * (`null`) doesn't count: the server picks a track that plays here when
     * it can.
     */
    fun streams(mode: PlaybackMode, decision: CapsDecision?, file: MediaFile?, audio: MediaStream?, caps: CapsSnapshot?): Boolean =
        mode == PlaybackMode.Remux || mode == PlaybackMode.Transcode ||
            (decision != null && decision != CapsDecision.Direct) ||
            !containerPlaysHere(file, caps) ||
            !audioPlaysHere(audio, caps)

    /** What a request carries besides the item: the choices of the moment. */
    data class Choices(
        val file: MediaFile,
        val startMs: Long,
        /** `null`: automatic, the server picks the track. */
        val audio: MediaStream?,
        val subtitle: MediaStream?,
        val quality: QualityPreset,
        val mode: PlaybackMode,
        /** The session will stream (HLS): the server needs the track indexes. */
        val streaming: Boolean,
        /** Pictures are side-loaded by the player: none to burn in. */
        val sideload: Boolean = false,
    )

    /**
     * The body of POST /api/playback for [choices]. Automatic audio names no
     * track, even for a stream: the server picks it. [keepPrevious] when it
     * is made ready while the session before plays on: the server leaves
     * that one to the player to end.
     */
    fun request(itemId: Long, choices: Choices, clientId: String, caps: CapsSnapshot?, keepPrevious: Boolean = false): PlaybackRequest {
        val audioHere = audioPlaysHere(choices.audio, caps)
        // Named when the server must convert it, or when it isn't the one the
        // details give (without a track named, the server plays that one or
        // one of its language, and judges a file played as it is by it).
        val notTheDetails = choices.audio != null && choices.audio.index != detailsAudio(choices.file)?.index
        return PlaybackRequest(
            itemId = itemId,
            fileId = choices.file.id,
            audioIndex = choices.audio?.index?.takeIf { choices.streaming || !audioHere || notTheDetails },
            subtitleIndex = choices.subtitle?.index?.takeIf { choices.streaming && isImage(choices.subtitle) && !choices.sideload },
            maxBitrate = choices.quality.maxBitrateKbps,
            maxHeight = choices.quality.maxHeight,
            startMs = choices.startMs,
            mode = choices.mode,
            clientId = clientId,
            keepPrevious = true.takeIf { keepPrevious },
            caps = caps?.report ?: ClientCaps(),
        )
    }

    /**
     * Whether [info]'s session must be asked again with the track indexes:
     * it streams, though it wasn't expected to, and its audio isn't the one
     * chosen, or the subtitles chosen are pictures to burn in. Automatic
     * audio is whatever the server chose: never asked again for it.
     */
    fun needsIndexes(info: PlaybackInfo, sent: PlaybackRequest, choices: Choices): Boolean {
        if (!info.hls) return false
        val audio = choices.audio
        val wrongAudio = audio != null && sent.audioIndex == null && info.audioIndex != null && info.audioIndex != audio.index
        val missingBurn = isImage(choices.subtitle) && sent.subtitleIndex == null && !choices.sideload
        return wrongAudio || missingBurn
    }

    /** The container's MIME type, for engines that can't tell from the address. */
    fun mimeTypeOf(container: String?): String? = when (container?.lowercase()) {
        "mkv", "matroska", "mka" -> "video/x-matroska"
        "mp4", "m4v", "mov" -> "video/mp4"
        "webm" -> "video/webm"
        "avi" -> "video/x-msvideo"
        "ts", "m2ts", "mpegts" -> "video/mp2t"
        else -> null
    }

    /** The video's dynamic range, as the policy reads it. */
    fun rangeOf(video: MediaStream?): DynamicRange = when {
        video == null -> DynamicRange.Sdr
        video.dynamicRange != null && video.dynamicRange != DynamicRange.Unknown -> video.dynamicRange!!
        video.dvProfile != null -> DynamicRange.DolbyVision
        else -> DynamicRange.Sdr
    }
}

/**
 * The Dolby Vision guards: a session that would show wrong colours is
 * asked for again another way.
 *
 * - A remux of profile 5 that doesn't name its Dolby Vision (an older
 *   server, or none in its master playlist): TS segments lose the
 *   signalling, and profile 5 has no base layer to fall back to, so the
 *   picture comes out green and purple. It is transcoded instead
 *   (tone-mapped). One whose answer says it stays Dolby Vision
 *   ([PlaybackInfo.videoRange], "Dolby Vision P5") plays as it comes.
 * - Direct play of profile 7 without a profile 7 decoder (or a quirk that
 *   adds one): Media3 has no fallback for its dual layer. A remux sends the
 *   HEVC base layer, HDR10, when the screen shows HDR10; else a transcode.
 */
object DvGuards {
    /** The mode to ask [info]'s session again with; `null` when it plays right. */
    fun retryMode(info: PlaybackInfo, file: MediaFile?, caps: CapsSnapshot?): PlaybackMode? {
        val video = PlaybackPlan.videoStream(file) ?: return null
        if (PlaybackPlan.rangeOf(video) != DynamicRange.DolbyVision) return null
        return when (video.dvProfile) {
            5 -> if (info.method == PlaybackMethod.Remux && !keepsDolbyVision(info)) PlaybackMode.Transcode else null
            7 -> {
                val report = caps?.report
                // Quirks (the Shield's) are in the snapshot's profiles already;
                // they're listed on any screen, and decoded only on a Dolby Vision one.
                val decodesP7 = DynamicRange.DolbyVision in report?.hdrTypes.orEmpty() &&
                    report?.dolbyVisionProfiles.orEmpty().any { it.profile == 7 }
                when {
                    info.method != PlaybackMethod.Direct || decodesP7 -> null
                    DynamicRange.Hdr10 in report?.hdrTypes.orEmpty() || report?.hdr == true -> PlaybackMode.Remux
                    else -> PlaybackMode.Transcode
                }
            }
            else -> null
        }
    }

    /** The session's stream names its Dolby Vision, so the screen gets it ("Dolby Vision P5", not "… base layer"). */
    private fun keepsDolbyVision(info: PlaybackInfo): Boolean =
        info.videoRange?.let { it.startsWith("Dolby Vision") && "not decoded" !in it } == true
}
