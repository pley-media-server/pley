package app.pley.android.ui.tv.player

import android.view.KeyEvent as AndroidKeyEvent
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.media3.common.Player
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.engine.EngineState
import app.pley.android.player.engine.VideoSurface
import app.pley.android.player.sleep.SleepTimer
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.player.subtitles.ScreenCaptions
import app.pley.android.ui.tv.R
import app.pley.android.player.R as PlayerR
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * The TV player (design spec §2.7, TV addendum §5.2) over a
 * [PlaybackController]: the video on its SurfaceView, subtitles, and the
 * remote's keys.
 *
 * - **Controls hidden.** Only the video and subtitles show; the title for
 *   a few seconds as each item begins ([TvTitleBanner]), Skip, Up Next and
 *   the sleep timer's notice are all that appear unasked. Centre shows the
 *   controls on the scrubber, as Up does (a showing Skip button or Keep
 *   watching takes it), Down on Play/Pause. Left and Right show them on the
 *   scrubber too, with the preview, and seek from there. Centre on the
 *   scrubber plays or pauses.
 * - **Seeking** ([TvSeeker]), as YouTube's TV app does: Left and Right
 *   move where it goes, 10 s a press (held: scrub), and it goes there on
 *   OK, or by itself 5 s after the last key; fast forward and rewind go
 *   400 ms after the last press. Centre on the scrubber
 *   during or just after a seek confirms it and plays on
 *   ([TvSeeker.confirm]), as Centre with the controls hidden does while
 *   one is on its way. Play/Pause, the button or the media key, only
 *   plays or pauses: a seek waiting still goes. Another seek (Previous,
 *   Next, Skip) takes the place of one waiting.
 * - **Controls shown.** Focus moves as usual; they hide after 5 s without
 *   a key unless paused, a seek waits, or in a panel or the chapters'
 *   cards. Up on the scrubber while a seek waits, or the Chapters button,
 *   opens the chapters as cards over it ([TvChapterStrip]).
 * - **Back** walks out: panel, then Up Next, then the chapters' cards, then
 *   a seek waiting (the controls stay, and it plays on from where it was),
 *   then the controls, then the player ([onExit]).
 * - **Sleep timer.** Its notice takes focus 30 s before it pauses (Keep
 *   watching); paused by it for 10 minutes, the player leaves. A playback
 *   the server stopped says why and leaves after 20 s.
 * - **Media keys** and CAPTIONS work whatever shows, but an error: then
 *   only Stop does, and it leaves.
 *
 * [onExit] leaves the player for where playing started; [now] is the wall
 * clock, for the clock and "Ends at". [captionLook] is Settings →
 * Subtitles, which the Subtitles panel changes with [onSubtitleStyle]; [showDetails] (Settings → Playback details) keeps the compact
 * readout over the video while nothing else shows, in the title's place:
 * the title doesn't show as items begin.
 */
@Composable
internal fun TvPlayerScreen(
    controller: PlaybackController,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
    captionLook: CaptionLook = CaptionLook(),
    onSubtitleStyle: ((CaptionLook) -> Unit)? = null,
    showDetails: Boolean = false,
    now: () -> Long = System::currentTimeMillis,
) {
    val state = controller.state.collectAsState()
    val engineState = rememberEngineState(controller)
    val ui = remember(controller) { TvPlayerUi() }
    val seeker = remember(controller) { TvSeeker({ state.value.positionMs }, { state.value.durationMs }, controller::seekTo) }
    val resources = LocalResources.current
    val exit by rememberUpdatedState(onExit)

    val phase by remember(state) { derivedStateOf { state.value.phase } }
    val error = phase as? PlayerPhase.Error
    val switching by remember(engineState) { derivedStateOf { engineState.value?.switchingDisplayMode == true } }
    val reconnecting by remember(state) { derivedStateOf { state.value.reconnecting } }
    val handoff by remember(state) { derivedStateOf { state.value.handoff } }
    val card by remember(state) { derivedStateOf { state.value.upNextCard } }
    val skipLabel by remember(state) { derivedStateOf { state.value.skipLabel } }
    val playing by remember(state) { derivedStateOf { state.value.phase.wantsToPlay() } }
    val options by remember(state) {
        derivedStateOf {
            val s = state.value
            RowOptions(
                previousNext = s.subtitle != null || s.chapters.size > 1,
                subtitles = s.subtitles.isNotEmpty() || s.canFindSubtitles,
                // One track still has its delay.
                audio = s.audio.isNotEmpty(),
                quality = s.qualities.isNotEmpty(),
                chapters = s.chapters.size > 1,
            )
        }
    }
    val previewing by remember(seeker) { derivedStateOf { seeker.target != null } }
    val chaptersOpen by remember(ui) { derivedStateOf { ui.chapter != null } }
    // Skip shows while its marker runs, unless a seek preview, the chapters' cards, Up Next or an error is on screen.
    val skipShown = skipLabel != null && card == null && error == null && !switching && !(ui.controls && (previewing || chaptersOpen))
    val sleepNotice by remember(state) { derivedStateOf { state.value.sleep.secondsLeft != null } }
    val sleepFired by remember(state) { derivedStateOf { state.value.sleep.fired } }
    val stopped by remember(state) { derivedStateOf { state.value.stoppedByServer } }
    val noticeShown = sleepNotice && error == null

    // ---------------------------------------------------------------- what the keys do

    /**
     * Play/Pause, the button and the media key: plays or pauses, and a
     * seek waiting stays on its way. Nothing while the playback starts or
     * the TV changes its refresh rate.
     */
    fun playPause() {
        if (state.value.phase == PlayerPhase.Preparing || switching) return
        if (state.value.phase.wantsToPlay()) {
            controller.pause()
            if (!ui.controls) ui.showControls(PlayerFocus.Scrubber)
        } else {
            controller.play()
            // Playing again, the controls go after a while.
            if (ui.controls) ui.touch()
        }
    }

    /**
     * OK on the scrubber, or with the controls hidden: during or just after
     * a seek it goes there, playing (paused, it plays from there); else, on
     * the scrubber, it plays or pauses, and with the controls hidden they
     * show on the scrubber. Never a pause out of sight of the controls.
     */
    fun ok() {
        if (!ui.controls && seeker.target == null) {
            ui.showControls(PlayerFocus.Scrubber)
            return
        }
        if (seeker.confirm()) {
            if (!state.value.phase.wantsToPlay()) controller.play()
            return
        }
        playPause()
    }

    fun focusFor(focus: ControlFocus): PlayerFocus = when (focus) {
        ControlFocus.Scrubber -> PlayerFocus.Scrubber
        ControlFocus.PlayPause -> PlayerFocus.PlayPause
        ControlFocus.Default -> if (skipShown) PlayerFocus.Skip else PlayerFocus.Scrubber
    }

    /**
     * A seek of its own (Previous, Next, a chapter): one waiting for OK gives
     * way to it, and so do the chapters' cards, whose scrubber would show the
     * card focused rather than where the video went.
     */
    fun seekTo(ms: Long) {
        seeker.cancel()
        ui.closeChapters()
        controller.seekTo(ms)
    }

    /**
     * The chapters as cards over the scrubber ([TvChapterStrip]), from
     * [from] (the scrubber, or the Chapters button): on the chapter holding
     * where a seek waiting was going, else the playhead. The seek gives way
     * to them: a chapter chosen goes to its start, and closed, it plays on
     * from where it is.
     */
    fun openChapters(from: PlayerFocus) {
        val s = state.value
        if (s.chapters.isEmpty()) return
        val at = seeker.target ?: s.positionMs
        seeker.cancel()
        ui.openChapters(TvPlayerText.chapterNear(s.chapters, at), from)
    }

    /** A chapter chosen in the cards: it goes to its start, and the cards close. */
    fun goToChapter(index: Int) {
        state.value.chapters.getOrNull(index)?.let { seekTo(it.startMs) }
        ui.closeChapters()
    }

    fun next() {
        val s = state.value
        val pos = seeker.target ?: s.positionMs
        val chapter = s.chapters.firstOrNull { it.startMs > pos + CHAPTER_SLACK_MS }
        when {
            s.subtitle != null -> {
                seeker.cancel()
                ui.closeChapters()
                controller.playNext()
            }
            chapter != null -> seekTo(chapter.startMs)
        }
    }

    fun previous() {
        val s = state.value
        val pos = seeker.target ?: s.positionMs
        val current = TvPlayerText.chapterAt(s.chapters, pos)
        val to = when {
            current == null -> 0L
            pos - s.chapters[current].startMs > RESTART_CHAPTER_MS || current == 0 -> s.chapters[current].startMs
            else -> s.chapters[current - 1].startMs
        }
        seekTo(to)
    }

    fun toggleCaptions() {
        val subs = state.value.subtitles
        if (subs.isEmpty()) return
        val on = subs.firstOrNull { it.selected && it.index != null }
        if (on != null) {
            ui.rememberedSubtitle = on.index
            controller.selectSubtitle(null)
            ui.toast(resources.getString(R.string.tv_player_subtitles_off_toast), TvPlayerUi.CAPTIONS_TOAST_MS)
        } else {
            val pick = subs.firstOrNull { it.index != null && it.index == ui.rememberedSubtitle }
                ?: subs.firstOrNull { it.index != null && !it.burnIn }
                ?: subs.firstOrNull { it.index != null }
                ?: return
            controller.selectSubtitle(pick.index)
            ui.toast(resources.getString(R.string.tv_player_subtitles_toast, pick.label), TvPlayerUi.CAPTIONS_TOAST_MS)
        }
    }

    /**
     * The remote's audio key opens the Audio panel over the controls, the
     * track playing ticked: the person chooses there, as with the Audio
     * button (stepping to the next track could land on a commentary and
     * keep it for the title).
     */
    fun openAudio() {
        val s = state.value
        if (s.audio.size < 2 || s.phase is PlayerPhase.Error || s.upNextCard != null || ui.panel == PlayerPanel.Audio) return
        ui.showControls(PlayerFocus.Audio)
        ui.openPanel(PlayerPanel.Audio)
    }

    fun act(key: PlayerKey) {
        when (key) {
            PlayerKey.PlayPause -> playPause()
            PlayerKey.Ok -> ok()
            PlayerKey.Play -> controller.play()
            PlayerKey.Pause -> {
                controller.pause()
                if (!ui.controls && card == null && error == null) ui.showControls(PlayerFocus.Scrubber)
            }
            is PlayerKey.Jump -> {
                // Fast forward and rewind seek from the scrubber, with its preview: the cards give way.
                ui.closeChapters()
                seeker.press(if (key.deltaMs < 0) -1 else 1, kotlin.math.abs(key.deltaMs), TvSeeker.SETTLE_MS)
            }
            PlayerKey.Next -> next()
            PlayerKey.Previous -> previous()
            PlayerKey.Captions -> toggleCaptions()
            PlayerKey.AudioTrack -> openAudio()
            PlayerKey.Stop -> exit()
            is PlayerKey.Show -> ui.showControls(focusFor(key.focus))
            is PlayerKey.Step -> seeker.press(key.direction)
            PlayerKey.Chapters -> openChapters(PlayerFocus.Scrubber)
        }
    }

    /** Left, Right, Centre and the keys that show the controls, as [TvPlayerKeys] sorted them. */
    fun handle(key: PlayerKey, native: AndroidKeyEvent, busy: Boolean): Boolean {
        val down = native.action == AndroidKeyEvent.ACTION_DOWN
        val first = down && native.repeatCount == 0
        when (key) {
            is PlayerKey.Step -> when {
                busy -> Unit
                down -> {
                    // The first press shows the controls on the scrubber, the preview with them.
                    if (!ui.controls) ui.showControls(PlayerFocus.Scrubber)
                    if (first) seeker.press(key.direction) else seeker.repeat(key.direction, native.eventTime)
                }
                // Let go, a scrub waits for OK where it got to.
                else -> seeker.release()
            }
            else -> if (first) act(key)
        }
        return true
    }

    fun onKey(event: KeyEvent): Boolean {
        val native = event.nativeKeyEvent
        val code = native.keyCode
        TvPlayerKeys.media(code)?.let { key ->
            // An error on screen: only Stop, which leaves. The others would seek, change a track or
            // play the next episode under it.
            val acts = key == PlayerKey.Stop || state.value.phase !is PlayerPhase.Error
            if (acts && native.action == AndroidKeyEvent.ACTION_DOWN && native.repeatCount == 0) act(key)
            return true
        }
        if (state.value.phase is PlayerPhase.Error) return false
        if (state.value.upNextCard != null) {
            return code == AndroidKeyEvent.KEYCODE_DPAD_UP || code == AndroidKeyEvent.KEYCODE_DPAD_DOWN
        }
        if (ui.panel != null) {
            ui.touch()
            return false
        }
        val busy = state.value.phase == PlayerPhase.Preparing || switching
        if (!ui.controls) {
            val buttonFocused = (skipShown && ui.focused == PlayerFocus.Skip) || (noticeShown && ui.focused == PlayerFocus.SleepNotice)
            val key = TvPlayerKeys.hidden(code, skipShowing = buttonFocused) ?: return false
            return handle(key, native, busy)
        }
        ui.touch()
        if (ui.focused == PlayerFocus.Scrubber) {
            val chaptersOnUp = seeker.target != null && state.value.chapters.isNotEmpty()
            val key = TvPlayerKeys.scrubber(code, chaptersOnUp) ?: return false
            return handle(key, native, busy)
        }
        // The chapters' cards, a side panel's rows and the row's buttons have their own keys.
        return false
    }

    BackHandler {
        when {
            ui.panel == PlayerPanel.FindSubtitles -> {
                controller.closeSubtitleSearch()
                ui.openPanel(PlayerPanel.Subtitles)
            }
            ui.panel != null -> ui.closePanel()
            state.value.upNextCard != null -> controller.cancelUpNext()
            ui.chapter != null -> ui.closeChapters()
            // A seek waiting for OK: Back lets it go, and it plays on from where it was.
            ui.controls && seeker.target != null -> {
                seeker.cancel()
                ui.touch()
            }
            ui.controls -> ui.hideControls(if (skipShown) PlayerFocus.Skip else PlayerFocus.Video)
            else -> exit()
        }
    }

    // ---------------------------------------------------------------- what follows the playback

    // Focus goes where it was asked for, once that is laid out.
    LaunchedEffect(ui.focusAsk) {
        val requester = ui.requester(ui.focusAsk.target)
        repeat(FOCUS_TRIES) {
            withFrameNanos { }
            if (runCatching { requester.requestFocus() }.getOrDefault(false)) return@LaunchedEffect
        }
    }
    // Skip takes focus while its marker runs; when it goes, focus goes back to the video or the scrubber.
    LaunchedEffect(skipShown) {
        when {
            skipShown && !ui.controls && ui.panel == null -> ui.ask(PlayerFocus.Skip)
            !skipShown && ui.focused == PlayerFocus.Skip -> ui.ask(if (ui.controls) PlayerFocus.Scrubber else PlayerFocus.Video)
        }
    }
    // The sleep timer's notice takes focus as Skip does (Skip first); when it goes, focus goes back.
    LaunchedEffect(noticeShown) {
        when {
            noticeShown && !skipShown && !ui.controls && ui.panel == null && card == null -> ui.ask(PlayerFocus.SleepNotice)
            !noticeShown && ui.focused == PlayerFocus.SleepNotice -> ui.ask(if (ui.controls) PlayerFocus.Scrubber else PlayerFocus.Video)
        }
    }
    // Paused by the sleep timer for 10 minutes: the player leaves.
    LaunchedEffect(sleepFired, phase) {
        if (sleepFired && phase == PlayerPhase.Paused) {
            delay(SleepTimer.LEAVE_AFTER_MS)
            exit()
        }
    }
    // The server stopped the playback: the error says why, then the player leaves.
    LaunchedEffect(stopped) {
        if (stopped) {
            delay(STOPPED_LEAVE_MS)
            exit()
        }
    }
    // Up Next takes the screen's attention: the controls and panels make way, Play now has focus.
    LaunchedEffect(card != null) {
        if (card != null) {
            seeker.cancel()
            ui.closePanel(refocus = false)
            ui.hideControls(PlayerFocus.UpNext)
            ui.ask(PlayerFocus.UpNext)
        } else if (ui.focused == PlayerFocus.UpNext) {
            ui.ask(PlayerFocus.Video)
        }
    }
    // An error takes the screen, with Try again focused.
    LaunchedEffect(error != null) {
        if (error != null) {
            seeker.cancel()
            ui.closePanel(refocus = false)
            ui.hideControls(PlayerFocus.Error)
            ui.ask(PlayerFocus.Error)
        } else if (ui.focused == PlayerFocus.Error) {
            ui.ask(PlayerFocus.Video)
        }
    }
    // The controls hide after 5 s without a key, unless paused, a seek waits, or in a panel or the chapters' cards.
    // A seek waiting keeps them until it lands, 5 s after the last key too: they go with it, not 5 s later.
    LaunchedEffect(ui.controls, ui.panel, ui.input, playing, chaptersOpen) {
        if (ui.controls && ui.panel == null && playing && !chaptersOpen) {
            delay(TvPlayerUi.HIDE_AFTER_MS)
            snapshotFlow { seeker.target == null }.first { it }
            ui.hideControls(if (skipShown) PlayerFocus.Skip else PlayerFocus.Video)
        }
    }
    // Presses in a row seek once they stop, OK sooner: Left and Right after 5 s, the jumps after 400 ms.
    // The badge goes after its own time.
    LaunchedEffect(seeker.presses) {
        val press = seeker.presses
        launch {
            delay(TvSeeker.BADGE_MS)
            seeker.hideBadge(press)
        }
        delay(seeker.waitMs)
        seeker.settle(press)
    }
    // A while after a seek, OK plays and pauses again.
    LaunchedEffect(seeker.seeks) {
        val seek = seeker.seeks
        delay(TvSeeker.CONFIRM_MS)
        seeker.settled(seek)
    }
    // The title as each item first plays (the next episode by itself too), for a few seconds.
    val itemId by remember(state) { derivedStateOf { state.value.itemId } }
    LaunchedEffect(itemId, phase == PlayerPhase.Playing) {
        if (phase == PlayerPhase.Playing) ui.playing(itemId)
    }
    LaunchedEffect(ui.banner) {
        val shown = ui.banner ?: return@LaunchedEffect
        delay(TvPlayerUi.BANNER_MS)
        ui.hideBanner(shown)
    }
    // The playback's messages, as toasts.
    val notice by remember(state) { derivedStateOf { state.value.notice } }
    LaunchedEffect(notice) {
        notice?.let {
            ui.toast(it)
            controller.dismissNotice()
        }
    }
    // As the phone's player does: a switch to a transcode the player made by itself says why, and
    // a server that can't convert fast enough says so; both point to Quality, which has the way back.
    val switchedToTranscode by remember(state) { derivedStateOf { state.value.canPlayOriginal } }
    LaunchedEffect(switchedToTranscode) {
        val why = state.value.note
        if (switchedToTranscode && why != null) ui.toast(resources.getString(R.string.tv_player_transcode_toast, why), TvPlayerUi.ERROR_TOAST_MS)
    }
    val serverTooSlow by remember(state) { derivedStateOf { state.value.suggestLowerQuality } }
    LaunchedEffect(serverTooSlow) {
        if (serverTooSlow) {
            val why = state.value.note ?: resources.getString(PlayerR.string.session_slow_server)
            ui.toast(resources.getString(R.string.tv_player_slow_server_toast, why), TvPlayerUi.ERROR_TOAST_MS, error = true)
        }
    }
    // The playback's details are asked for while the Info panel is open or Playback details show.
    val detailsShown = showDetails && !ui.controls && ui.panel == null && card == null && error == null
    val infoOpen = ui.panel == PlayerPanel.Info || detailsShown
    val details = remember(controller) { DetailsWatch(controller) }
    LaunchedEffect(infoOpen) { details.open(infoOpen) }
    DisposableEffect(details) { onDispose { details.open(false) } }

    // ---------------------------------------------------------------- the screen

    val reduced = reducedMotion()
    val controlsAlpha = animateFloatAsState(if (ui.controls) 1f else 0f, PleyMotion.overlay(reduced), label = "controls")
    val controlsPresent by remember { derivedStateOf { ui.controls || controlsAlpha.value > 0f } }
    val player by remember(controller) { derivedStateOf { engineState.value.let { controller.engine?.player } } }
    val videoLabel = stringResource(R.string.tv_player_play)

    Box(
        modifier
            .fillMaxSize()
            // A window of its own: UI Automator (the benchmarks' journeys) finds its tags as resource ids.
            .semantics { testTagsAsResourceId = true }
            .background(Color.Black)
            .onPreviewKeyEvent(::onKey)
            .testTag(TAG_PLAYER),
    ) {
        // The first picture fades in from black as the controls do; a stream taking over keeps the last one.
        VideoSurface(player, Modifier.fillMaxSize(), reveal = PleyMotion.overlay(reduced)) { handoff }
        // Under the subtitles, which the scrim mustn't dim.
        Box(Modifier.fillMaxSize().controlScrims(controlsAlpha))
        // Playback details have the top left while they show over the video: the title gives way to them.
        if (!showDetails) TvTitleBanner(state, ui)
        PlayerSubtitles(controller, player, captionLook, lifted = ui.controls)
        // Holds focus while the controls are hidden: the keys arrive through it.
        Box(Modifier.fillMaxSize().videoFocus(ui, enabled = !ui.controls, label = videoLabel))
        if (controlsPresent) {
            TvPlayerTopBar(state, controlsAlpha, now)
            // The chapters' cards take the preview's place.
            if (ui.controls && !chaptersOpen) TvSeekPreview(state, seeker, controller)
            TvChapterStrip(state, ui, controller, onGo = ::goToChapter)
            TvScrubber(state, seeker, ui, controller, controlsAlpha, skipShown)
            TvControlRow(
                playing = playing,
                options = options,
                ui = ui,
                alpha = controlsAlpha,
                onPlayPause = ::playPause,
                onPrevious = ::previous,
                onNext = ::next,
                onChapters = { openChapters(PlayerFocus.Chapters) },
            )
        }
        if (detailsShown) TvPlaybackDetails(state, engineState)
        TvSkipButton(state, skipShown, ui.controls, ui) {
            seeker.cancel()
            controller.skip()
        }
        TvSleepNotice(state, noticeShown, ui.controls, ui, onKeep = controller::keepWatching)
        TvUpNextCard(card, ui, onPlay = controller::playNext, onCancel = controller::cancelUpNext)
        if (!ui.controls) TvSeekBadge(seeker.badgeMs)
        if (switching || phase == PlayerPhase.Switching) TvSwitchingCover()
        TvBufferingRing(
            waiting = phase == PlayerPhase.Buffering || phase == PlayerPhase.Preparing || switching,
            reconnecting = reconnecting,
            handoff = handoff,
        )
        TvPlayerPanels(ui, state, engineState, controller, captionLook, onSubtitleStyle)
        if (error != null) {
            TvPlayerError(
                error.message,
                error.canRetry,
                ui,
                onRetry = controller::retry,
                onBack = exit,
                title = stringResource(
                    when {
                        stopped -> R.string.tv_player_stopped_title
                        error.offline -> R.string.tv_player_offline_title
                        else -> R.string.tv_player_error_title
                    },
                ),
            )
        }
        TvPlayerToast(ui.toast, ui.controls, ui::clearToast)
    }
}

/**
 * Subtitles over the video: the server's and the file's own through the
 * player. Text stays inside the TV's safe area, its last line
 * [SUBTITLES_BOTTOM] above the bottom edge, and glides up above the
 * controls while they show, with them (their fade's time and easing), not
 * a jump; it keeps its size and its lines either way, as only its drawing
 * moves. Pictures (PGS,
 * VobSub) keep the places their frames give them on the whole screen.
 * The cues are collected inside, so they recompose nothing else.
 */
@Composable
private fun PlayerSubtitles(controller: PlaybackController, player: Player?, look: CaptionLook, lifted: Boolean) {
    val cues = controller.cues.collectAsState()
    // Read in the drawing only: the lift redraws, nothing composes or lays out again.
    val lift = animateFloatAsState(if (lifted) 1f else 0f, PleyMotion.overlay(reducedMotion()), label = "subtitles")
    ScreenCaptions(
        player = player,
        serverCues = cues,
        look = look,
        pictures = false,
        bottomPadding = SUBTITLES_BOTTOM,
        sidePadding = TvSafeArea.Horizontal,
        topPadding = TvSafeArea.Vertical,
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer { translationY = -(SUBTITLES_LIFTED - SUBTITLES_BOTTOM).toPx() * lift.value }
            .testTag(TAG_SUBTITLES_TEXT),
    )
    ScreenCaptions(player = player, serverCues = cues, look = look, pictures = true, bottomPadding = 0.dp, modifier = Modifier.fillMaxSize())
}

/** Asks for the playback's details while the Info panel is open, and stops asking once it closes. */
private class DetailsWatch(private val controller: PlaybackController) {
    private var asked = false

    fun open(open: Boolean) {
        if (open == asked) return
        asked = open
        controller.setDetailsOpen(open)
    }
}

/** The engine's state, once the playback has an engine; `null` before (and in tests without one). */
@Composable
private fun rememberEngineState(controller: PlaybackController): State<EngineState?> = produceState<EngineState?>(null, controller) {
    var engine = controller.engine
    while (engine == null) {
        delay(ENGINE_POLL_MS)
        engine = controller.engine
    }
    engine.state.collect { value = it }
}

private const val FOCUS_TRIES = 8
private const val ENGINE_POLL_MS = 250L

/** Next goes to a chapter starting at least this far ahead. */
private const val CHAPTER_SLACK_MS = 1_000L

/** Previous restarts the chapter playing when it is this far in, else goes to the one before. */
private const val RESTART_CHAPTER_MS = 3_000L

/**
 * Text subtitles' last line stays this far above the bottom edge: the
 * safe area's 27 dp and a little more, so descenders and outlines clear
 * an overscanned edge.
 */
internal val SUBTITLES_BOTTOM = TvSafeArea.Vertical + 13.dp

/** While the controls show, above them: their times start at [TIMES_TOP] on the 540 dp canvas. */
internal val SUBTITLES_LIFTED = PleyTv.CanvasHeight - TIMES_TOP + 12.dp

internal const val TAG_PLAYER = "pley.tv.player"

/** The text subtitles' layer, which glides up above the controls. */
internal const val TAG_SUBTITLES_TEXT = "pley.tv.player.subtitle-text"
