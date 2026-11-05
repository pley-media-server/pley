package app.pley.android.testing.player.session

import android.graphics.Bitmap
import androidx.media3.common.text.Cue
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind
import app.pley.android.model.MarkerSource
import app.pley.android.player.ChapterMark
import app.pley.android.player.FoundSubtitle
import app.pley.android.player.SubtitleSearchUi
import app.pley.android.player.DetailRow
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.PlaybackServices
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.QualityChoice
import app.pley.android.player.TrackChoice
import app.pley.android.player.UpNextUi
import app.pley.android.player.session.QualityPreset
import app.pley.android.player.session.SavedPlaybackState
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * A playback for the player screens' tests: the screen draws whatever
 * [state] is set to (every field of [PlayerUiState] can be driven, see
 * [FakePlaybackController.Companion]), and every command it gives is
 * recorded in [calls].
 */
class FakePlaybackController(initial: PlayerUiState = PlayerUiState()) : PlaybackController {
    /** A command: its name and arguments. */
    data class Call(val name: String, val args: List<Any?> = emptyList())

    private val ui = MutableStateFlow(initial)
    override val state: StateFlow<PlayerUiState> = ui.asStateFlow()

    private val cueState = MutableStateFlow<List<Cue>>(emptyList())
    override val cues: StateFlow<List<Cue>> = cueState.asStateFlow()

    override var engine: PlayerEngine? = null

    /** What [thumbnail] answers. */
    var thumbnails: (Long) -> Bitmap? = { null }

    private val recorded = mutableListOf<Call>()

    /** The commands so far, oldest first. */
    val calls: List<Call> get() = recorded.toList()

    /** The commands named [name]. */
    fun calls(name: String): List<Call> = recorded.filter { it.name == name }

    /** The state from now on. */
    fun set(state: PlayerUiState) {
        ui.value = state
    }

    /** Changes the state. */
    fun update(change: (PlayerUiState) -> PlayerUiState) = ui.update(change)

    /** The server's subtitles showing now. */
    fun showCues(cues: List<Cue>) {
        cueState.value = cues
    }

    private fun record(name: String, vararg args: Any?) {
        recorded += Call(name, args.toList())
    }

    override fun play() = record("play")

    override fun pause() = record("pause")

    override fun setAway(away: Boolean) = record("setAway", away)

    override fun seekTo(positionMs: Long) = record("seekTo", positionMs)

    override fun selectAudio(index: Long) = record("selectAudio", index)

    override fun selectSubtitle(index: Long?) = record("selectSubtitle", index)

    override fun selectQuality(name: String) = record("selectQuality", name)

    override fun setSubtitleOffset(offsetMs: Long) = record("setSubtitleOffset", offsetMs)

    override fun setSleep(choice: SleepChoice) = record("setSleep", choice)

    override fun setSpeed(speed: Float) = record("setSpeed", speed)

    override fun setAudioDelay(delayMs: Long) = record("setAudioDelay", delayMs)

    override fun searchSubtitles() = record("searchSubtitles")

    override fun addFoundSubtitle(resultId: String) = record("addFoundSubtitle", resultId)

    override fun closeSubtitleSearch() = record("closeSubtitleSearch")

    override fun keepWatching() = record("keepWatching")

    override fun skip() = record("skip")

    override fun playNext() = record("playNext")

    override fun cancelUpNext() = record("cancelUpNext")

    override fun playOriginal() = record("playOriginal")

    override fun setDetailsOpen(open: Boolean) = record("setDetailsOpen", open)

    override fun dismissNotice() {
        record("dismissNotice")
        ui.update { it.copy(notice = null) }
    }

    override fun dismissRemoteNotice() {
        record("dismissRemoteNotice")
        ui.update { it.copy(remoteNotice = null) }
    }

    override suspend fun thumbnail(positionMs: Long): Bitmap? = thumbnails(positionMs)

    override fun saveState(): SavedPlaybackState = ui.value.let {
        SavedPlaybackState(
            itemId = it.itemId,
            fileId = null,
            positionMs = it.positionMs,
            audioIndex = it.audio.firstOrNull { a -> a.selected }?.index,
            subtitleIndex = it.subtitles.firstOrNull { s -> s.selected }?.index,
            quality = it.qualities.firstOrNull { q -> q.selected }?.id ?: QualityPreset.ORIGINAL_ID,
            queue = null,
        )
    }

    override fun retry() = record("retry")

    override fun close() = record("close")

    companion object {
        /**
         * An episode playing with every field of [PlayerUiState] filled: a
         * stream at 8 Mbps with a fallback note, two audio tracks, subtitles
         * with one to burn in, the intro showing, chapters, previews, the
         * details open, Up Next counting down and a notice; the subtitles
         * moved, a sleep timer running, 1.5×, the sound 0.2 s later, subtitles
         * found online, and (only to fill them) the server out of reach, a
         * stop by the server and the notes of a sound that can't follow.
         */
        fun everything(): PlayerUiState = PlayerUiState(
            phase = PlayerPhase.Playing,
            itemId = 20_001,
            title = "Fixture Show",
            subtitle = "S1 E1 — Pilot",
            positionMs = 3_000,
            durationMs = 20_000,
            bufferedMs = 12_000,
            reconnecting = true,
            audio = listOf(TrackChoice(1, "English (AAC Stereo)", true), TrackChoice(2, "French (EAC3 5.1)", false)),
            subtitles = listOf(
                TrackChoice(null, "Off", false),
                TrackChoice(3, "English (SRT)", true),
                TrackChoice(4, "French (PGS)", false, burnIn = true),
            ),
            quality = "4 Mbps 720p",
            qualities = listOf(
                QualityChoice("original", "Original", "4K · 62 Mbps", false),
                QualityChoice("4m", "4 Mbps 720p", null, true),
            ),
            method = "Transcoding",
            note = "The original didn't play smoothly on this device.",
            canPlayOriginal = true,
            suggestLowerQuality = true,
            skipLabel = "Skip Intro",
            markers = listOf(
                Marker(1, MarkerKind.Intro, 2_000, 8_000, MarkerSource.Fingerprint),
                Marker(1, MarkerKind.Credits, 16_000, 20_000, MarkerSource.Frames),
            ),
            chapters = listOf(ChapterMark("Intro", 0, 8_000, null), ChapterMark("Part 1", 8_000, 20_000, null)),
            hasThumbnails = true,
            upNext = "Second",
            upNextSecondsLeft = 7,
            upNextCard = UpNextUi(20_002, "Second", "Up next · S1 E2", null, 7),
            details = PlaybackDetailsUi(
                method = "Transcoding",
                why = "The original didn't play smoothly on this device.",
                pipeline = "CPU",
                rows = listOf(DetailRow("Video", "H.264 1080p → H.264 1080p")),
                engine = listOf("Video decoder: fake"),
            ),
            notice = "This is the last episode",
            remoteNotice = "Paused",
            pausedByRemote = true,
            subtitleOffsetMs = 500,
            subtitleSync = true,
            subtitleStyled = true,
            sleep = SleepState(SleepChoice.Minutes(30), minutesLeft = 12),
            stoppedByServer = true,
            speed = 1.5f,
            speedNote = "Not for this download: its sound goes to your receiver as it is, and nothing here decodes it",
            audioDelayMs = 200,
            canFindSubtitles = true,
            subtitleSearch = SubtitleSearchUi.Found(
                listOf(FoundSubtitle("opensubtitles:1", "Fixture.Show.S01E01.1080p.WEB", "English · Made for this file · 1,234 downloads")),
                message = null,
            ),
            handoff = true,
        )
    }
}

/** [PlaybackServices] handing out [FakePlaybackController]s, recording the requests. */
class FakePlaybackServices(private val make: (PlayRequest) -> PlaybackController = { FakePlaybackController() }) : PlaybackServices {
    val requests = mutableListOf<PlayRequest>()
    private val controllers = mutableMapOf<String, PlaybackController>()

    override fun prepare(request: PlayRequest): String {
        requests += request
        val key = "fake-${requests.size}"
        controllers[key] = make(request)
        return key
    }

    override fun controller(key: String): PlaybackController? = controllers[key]
}
