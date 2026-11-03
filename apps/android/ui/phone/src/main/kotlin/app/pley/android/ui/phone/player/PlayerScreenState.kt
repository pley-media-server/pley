package app.pley.android.ui.phone.player

import androidx.compose.runtime.State
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.pley.android.player.ChapterMark
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.QualityChoice
import app.pley.android.player.SubtitleSearchUi
import app.pley.android.player.TrackChoice
import app.pley.android.player.UpNextUi
import app.pley.android.player.sleep.SleepState

/** A page of the player's sheet. */
enum class SheetPage { Settings, Quality, Audio, Subtitles, Style, Details, Chapters, Sleep, Speed, FindSubtitles }

/** A run of double-taps on one side: where it lands and how far it went. */
data class SeekRun(val side: Int, val totalMs: Long, val at: Long)

/** The volume or the brightness, while a vertical drag sets it. */
data class LevelDrag(val volume: Boolean, val level: Float)

/**
 * What the phone's player screen shows, read so that each part recomposes
 * only for what it draws: the controller's state is one value that changes
 * twice a second with the playhead, so every part reads its own field
 * through [derivedStateOf], and positions are read only where they are
 * drawn ([playhead], in the seek bar's draw phase).
 */
@Stable
class PlayerScreenState(
    val controller: PlaybackController,
    private val ui: State<PlayerUiState>,
) {
    // ---------------------------------------------------------------- the controller's state, by part

    val phase: PlayerPhase by derivedStateOf { ui.value.phase }
    val title: String by derivedStateOf { ui.value.title }
    val subtitle: String? by derivedStateOf { ui.value.subtitle }
    val method: String? by derivedStateOf { ui.value.method }
    val skipLabel: String? by derivedStateOf { ui.value.skipLabel }
    val upNext: UpNextUi? by derivedStateOf { ui.value.upNextCard }
    val chapters: List<ChapterMark> by derivedStateOf { ui.value.chapters }
    val audio: List<TrackChoice> by derivedStateOf { ui.value.audio }
    val subtitles: List<TrackChoice> by derivedStateOf { ui.value.subtitles }
    val qualities: List<QualityChoice> by derivedStateOf { ui.value.qualities }
    val quality: String by derivedStateOf { ui.value.quality }
    val hasThumbnails: Boolean by derivedStateOf { ui.value.hasThumbnails }
    val notice: String? by derivedStateOf { ui.value.notice }
    val canPlayOriginal: Boolean by derivedStateOf { ui.value.canPlayOriginal }
    val suggestLowerQuality: Boolean by derivedStateOf { ui.value.suggestLowerQuality }
    val details by derivedStateOf { ui.value.details }
    val subtitleSync: Boolean by derivedStateOf { ui.value.subtitleSync }
    val subtitleStyled: Boolean by derivedStateOf { ui.value.subtitleStyled }
    val subtitleOffsetMs: Long by derivedStateOf { ui.value.subtitleOffsetMs }
    val sleep: SleepState by derivedStateOf { ui.value.sleep }
    val stoppedByServer: Boolean by derivedStateOf { ui.value.stoppedByServer }
    val speed: Float by derivedStateOf { ui.value.speed }
    val speedNote: String? by derivedStateOf { ui.value.speedNote }
    val audioDelayMs: Long by derivedStateOf { ui.value.audioDelayMs }
    val canFindSubtitles: Boolean by derivedStateOf { ui.value.canFindSubtitles }
    val subtitleSearch: SubtitleSearchUi? by derivedStateOf { ui.value.subtitleSearch }

    /** An episode: Next episode is offered. */
    val episode: Boolean by derivedStateOf { ui.value.subtitle != null }

    /** The subtitles chosen are drawn into the picture by the server. */
    val burnedSubtitles: Boolean by derivedStateOf { ui.value.subtitles.any { it.selected && it.burnIn } }

    /** Waiting for the picture: preparing, buffering or changing the display mode. */
    val waiting: Boolean by derivedStateOf {
        val p = ui.value.phase
        p == PlayerPhase.Preparing || p == PlayerPhase.Buffering || p == PlayerPhase.Switching
    }

    /** A stream made ready beside the one that played takes over: its last picture stays, and the spinner waits longer. */
    val handoff: Boolean by derivedStateOf { ui.value.handoff }

    /** The server is out of reach and playback keeps trying. */
    val reconnecting: Boolean by derivedStateOf { ui.value.reconnecting }

    /** Meant to play: the play button shows Pause. */
    val playing: Boolean by derivedStateOf {
        val p = ui.value.phase
        p == PlayerPhase.Playing || p == PlayerPhase.Buffering || p == PlayerPhase.Preparing || p == PlayerPhase.Switching
    }

    val durationMs: Long by derivedStateOf { ui.value.durationMs }

    /** Read only while drawing. */
    val bufferedMs: Long get() = ui.value.bufferedMs

    // ---------------------------------------------------------------- the screen's own state

    /** The controls show (top bar, centre buttons, seek bar and bottom row). */
    var controlsVisible by mutableStateOf(true)

    var sheet by mutableStateOf<SheetPage?>(null)
        private set

    /** The page the sheet was opened at: Back on a sub-page returns here when it was Settings. */
    var sheetRoot by mutableStateOf<SheetPage?>(null)
        private set

    /** Where the knob is while the seek bar is dragged; `null` otherwise. */
    var scrubMs by mutableStateOf<Long?>(null)

    /** Touches only show the unlock button. */
    var locked by mutableStateOf(false)
        private set

    /** The unlock button shows, after a tap while [locked]. */
    var unlockShown by mutableStateOf(false)

    /** Remaining time on the right, or the whole length. */
    var showRemaining by mutableStateOf(true)

    /** The run of double-taps showing now. */
    var seekRun by mutableStateOf<SeekRun?>(null)

    /** The volume or brightness drag showing now. */
    var levelDrag by mutableStateOf<LevelDrag?>(null)

    /** The fit mode just chosen, shown for a moment ("Fill"). */
    var modeShown by mutableStateOf<FitMode?>(null)

    /** Bumped by every touch: the controls' hide timer starts again. */
    var touches by mutableIntStateOf(0)
        private set

    /**
     * The playhead while something shows it (the seek bar and its times):
     * updated a few times a second while the controls show, never while
     * they are hidden.
     */
    val playhead = mutableLongStateOf(0L)

    /** The controls stay: paused, waiting, a menu open, the seek bar held, an error. */
    val keepControls: Boolean by derivedStateOf {
        val p = ui.value.phase
        p == PlayerPhase.Paused || p is PlayerPhase.Error || waiting || sheet != null || scrubMs != null
    }

    fun touched() {
        touches++
    }

    fun toggleControls() {
        touched()
        controlsVisible = !controlsVisible
    }

    fun showControls() {
        touched()
        controlsVisible = true
    }

    fun open(page: SheetPage) {
        touched()
        if (sheet == null) sheetRoot = page
        sheet = page
        if (page == SheetPage.Details) controller.setDetailsOpen(true)
        if (page == SheetPage.FindSubtitles) controller.searchSubtitles()
    }

    /** The details readout over the video is showing (Settings → Playback details): its numbers stay live. */
    private var readout = false

    fun readout(shown: Boolean) {
        if (readout == shown) return
        readout = shown
        controller.setDetailsOpen(shown || sheet == SheetPage.Details)
    }

    /** Back inside the sheet: to Settings from one of its pages, else closed. */
    fun back() {
        val now = sheet ?: return
        if (now == SheetPage.Details) controller.setDetailsOpen(readout)
        if (now == SheetPage.FindSubtitles) {
            // Back from the search: to the subtitles it was opened from.
            controller.closeSubtitleSearch()
            sheet = SheetPage.Subtitles
            return
        }
        if (sheetRoot == SheetPage.Settings && now != SheetPage.Settings) sheet = SheetPage.Settings else closeSheet()
    }

    fun closeSheet() {
        if (sheet == SheetPage.Details) controller.setDetailsOpen(readout)
        if (sheet == SheetPage.FindSubtitles) controller.closeSubtitleSearch()
        sheet = null
        sheetRoot = null
        touched()
    }

    fun lock() {
        closeSheet()
        locked = true
        controlsVisible = false
        unlockShown = true
    }

    fun unlock() {
        locked = false
        unlockShown = false
        showControls()
    }

    fun togglePlay() {
        touched()
        if (playing) controller.pause() else controller.play()
    }

    /**
     * The playhead as precisely as it can be known: the engine's own while
     * the media plays (between the controller's half-second ticks), the
     * controller's while a session starts or after the end.
     */
    fun livePosition(): Long {
        val p = ui.value.phase
        val engine = controller.engine
        val moving = p == PlayerPhase.Playing || p == PlayerPhase.Paused || p == PlayerPhase.Buffering
        return if (engine != null && moving) engine.player.currentPosition.coerceAtLeast(0) else ui.value.positionMs
    }

    /** Seeks by [deltaMs] from the playhead (or from the run of seeks under way). */
    fun seekBy(deltaMs: Long, from: Long = livePosition()) {
        touched()
        val duration = durationMs
        val target = (from + deltaMs).let { if (duration > 0) it.coerceIn(0, duration) else it.coerceAtLeast(0) }
        controller.seekTo(target)
        playhead.longValue = target
    }

    /** A double-tap on [side] (-1 left, 1 right): seeks 10 s more in the run. */
    fun doubleTapSeek(side: Int, now: Long) {
        val run = seekRun
        val total = if (run != null && run.side == side) run.totalMs + SEEK_STEP_MS else SEEK_STEP_MS
        seekBy(side * SEEK_STEP_MS)
        seekRun = SeekRun(side, total, now)
    }

    companion object {
        /** Double-tap and the centre buttons seek this far. */
        const val SEEK_STEP_MS = 10_000L

        /** The controls hide after this long untouched. */
        const val HIDE_AFTER_MS = 3_000L

        /** The spinner waits this long, so short waits never flash one. */
        const val SPINNER_DELAY_MS = 400L
    }
}
