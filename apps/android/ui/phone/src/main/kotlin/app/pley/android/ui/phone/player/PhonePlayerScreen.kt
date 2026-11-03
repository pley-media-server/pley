package app.pley.android.ui.phone.player

import androidx.activity.compose.BackHandler
import androidx.annotation.OptIn
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.ui.compose.PlayerSurface
import androidx.media3.ui.compose.SURFACE_TYPE_SURFACE_VIEW
import androidx.media3.ui.compose.state.rememberPresentationState
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerPhase
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.player.subtitles.ScreenCaptions
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.ToastHost
import app.pley.android.ui.phone.kit.ToastHostState
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.filterNotNull
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId

/** What the player screen asks of its window: leaving, picture in picture, the volume and brightness. */
interface PlayerHost : LevelControls {
    /** Leaves the player. */
    fun back()

    /** This device has picture in picture, and the person allows it for pley. */
    val pipSupported: Boolean

    fun enterPip()
}

/** Counts compositions in tests: each part reports its name. `null` outside tests. */
internal object PlayerProbe {
    @Volatile
    var composed: ((String) -> Unit)? = null
}

/**
 * The phone and tablet player (design spec §3.5): the video on a
 * SurfaceView, subtitles over the screen, gestures, the controls, Skip,
 * Up Next, the sheet, errors and notices. Nothing recomposes while it
 * plays with the controls hidden: parts read only their own fields of the
 * controller's state, and the playhead is read where it is drawn.
 */
@Composable
fun PhonePlayerScreen(
    controller: PlaybackController,
    host: PlayerHost,
    picture: PictureState,
    captions: CaptionLook,
    style: SubtitleChoice,
    onStyle: (SubtitleChoice) -> Unit,
    inPip: Boolean,
    modifier: Modifier = Modifier,
    /** Settings → Playback details: how it plays, over the video while the controls are hidden. */
    showDetails: Boolean = false,
) {
    PlayerProbe.composed?.invoke("screen")
    val ui = controller.state.collectAsStateWithLifecycle()
    val state = remember(controller) { PlayerScreenState(controller, ui) }
    val scope = rememberCoroutineScope()
    val toasts = remember { ToastHostState() }
    var heightPx by remember { mutableIntStateOf(0) }
    var widthPx by remember { mutableIntStateOf(0) }

    PlayerEffects(state, picture, toasts, inPip)
    // The server stopped the playback: the error says why, then the player leaves.
    val currentHost by rememberUpdatedState(host)
    LaunchedEffect(state) {
        snapshotFlow { state.stoppedByServer }.filter { it }.collect {
            delay(STOPPED_LEAVE_MS)
            currentHost.back()
        }
    }

    BackHandler(enabled = state.sheet != null) { state.back() }

    Box(
        modifier
            .fillMaxSize()
            // A window of its own: UI Automator (the benchmarks' journeys) finds its tags as resource ids.
            .semantics { testTagsAsResourceId = true }
            .background(Color.Black)
            .onSizeChanged {
                widthPx = it.width
                heightPx = it.height
            }
            .testTag(TAG_PLAYER),
    ) {
        val engine = rememberEngine(state, ui)
        engine?.let { VideoPicture(it, picture) { state.handoff } }
        // Under the subtitles, which it mustn't dim.
        if (!inPip) ControlScrims(state)
        Captions(state, engine, picture, captions, inPip)
        if (inPip) {
            Spinner(rememberSpinning(state))
            return@Box
        }
        Box(
            Modifier
                .fillMaxSize()
                .playerGestures(state, picture, host, scope),
        )
        Controls(state, picture, host) { if (heightPx > 0) widthPx.toFloat() / heightPx else 0f }
        DetailsReadout(state, showDetails)
        SeekRunBadge(state)
        LevelIndicator(state)
        ModeLabel(state)
        SkipButton(state)
        UpNextCard(state)
        UnlockButton(state)
        Spinner(rememberSpinning(state), reconnecting = state.reconnecting)
        SleepNoticeOf(state)
        PlayerSheet(state, style, onStyle)
        (state.phase as? PlayerPhase.Error)?.let {
            ErrorOverlay(
                it,
                onRetry = state.controller::retry,
                onBack = host::back,
                title = stringResource(
                    when {
                        state.stoppedByServer -> R.string.player_stopped_title
                        it.offline -> R.string.player_offline_title
                        else -> R.string.player_error_title
                    },
                ),
            )
        }
        ToastHost(
            toasts,
            Modifier
                .align(Alignment.TopCenter)
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(top = 64.dp),
        )
    }
}

/** The sleep timer's notice, read here so the rest of the screen doesn't recompose with its count. */
@Composable
private fun BoxScope.SleepNoticeOf(state: PlayerScreenState) {
    SleepNotice(state.sleep.secondsLeft.takeIf { state.phase !is PlayerPhase.Error }, music = false, onKeep = state.controller::keepWatching)
}

/** The tag of the player's root, for tests. */
const val TAG_PLAYER = "phone-player"

/**
 * The engine, once the controller has one: it is made while the server
 * answers, before the state's first change, so reading it again whenever
 * the state changes finds it.
 */
@Composable
private fun rememberEngine(state: PlayerScreenState, ui: State<*>): Player? {
    val engine by remember(state) {
        androidx.compose.runtime.derivedStateOf {
            ui.value
            state.controller.engine?.player
        }
    }
    return engine
}

/** The scrim behind the controls, read in its own scope so showing them recomposes only it. */
@Composable
private fun ControlScrims(state: PlayerScreenState) = Scrims(state.controlsVisible && !state.locked)

/** The controls' groups. */
@Composable
private fun BoxScope.Controls(state: PlayerScreenState, picture: PictureState, host: PlayerHost, screenAspect: () -> Float) {
    val visible = state.controlsVisible && !state.locked
    val spinning = rememberSpinning(state)
    ControlsGroup(visible, Modifier.align(Alignment.TopCenter)) { TopBar(state, host) }
    ControlsGroup(visible, Modifier.align(Alignment.Center)) { CentreControls(state, spinning) }
    ControlsGroup(visible, Modifier.align(Alignment.BottomCenter)) { BottomBar(state, picture, screenAspect) }
}

/** Timers and side effects: hiding the controls, the playhead's clock, notices, the picture's burned subtitles. */
@Composable
private fun PlayerEffects(state: PlayerScreenState, picture: PictureState, toasts: ToastHostState, inPip: Boolean) {
    // The controls hide after 3 s untouched, unless something keeps them.
    LaunchedEffect(state) {
        snapshotFlow { Triple(state.controlsVisible, state.keepControls, state.touches) }.collectLatest { (visible, keep, _) ->
            if (visible && !keep) {
                delay(PlayerScreenState.HIDE_AFTER_MS)
                state.controlsVisible = false
            }
        }
    }
    // The playhead, only while something shows it.
    LaunchedEffect(state, inPip) {
        snapshotFlow { (state.controlsVisible || state.sheet == SheetPage.Chapters) && !inPip }.collectLatest { shown ->
            while (shown) {
                state.playhead.longValue = state.livePosition()
                delay(PLAYHEAD_MS)
            }
        }
    }
    LaunchedEffect(state, picture) {
        snapshotFlow { state.burnedSubtitles }.collect { picture.burnedSubtitles = it }
    }
    // Paused from anywhere (a headset, picture in picture, audio focus): the controls show, and stay.
    LaunchedEffect(state) {
        snapshotFlow { state.phase == PlayerPhase.Paused }.collect { paused -> if (paused && !state.locked) state.controlsVisible = true }
    }
    // Each item after the first (the next episode, a queue's next) shows the controls as it begins to play,
    // its title at the top as the first's was: the TV and the native Samsung app show the title then.
    LaunchedEffect(state) {
        snapshotFlow { state.itemId.takeIf { state.phase == PlayerPhase.Playing } }
            .filterNotNull()
            .distinctUntilChanged()
            .drop(1)
            .collect { if (!state.locked) state.showControls() }
    }
    // The session's notices, as toasts.
    LaunchedEffect(state, toasts) {
        snapshotFlow { state.notice }.collect { notice ->
            if (notice != null) {
                toasts.show(notice)
                state.controller.dismissNotice()
            }
        }
    }
    val original = stringResource(R.string.player_play_original)
    val lower = stringResource(R.string.player_lower_quality)
    val currentOriginal by rememberUpdatedState(original)
    val currentLower by rememberUpdatedState(lower)
    // The player switched to a transcode by itself: offer the original again.
    LaunchedEffect(state, toasts) {
        snapshotFlow { state.canPlayOriginal }.filter { it }.collect {
            val why = state.controller.state.value.note ?: return@collect
            toasts.show(why, currentOriginal, { state.controller.playOriginal() })
        }
    }
    // The server can't convert fast enough: offer a lower quality.
    LaunchedEffect(state, toasts) {
        snapshotFlow { state.suggestLowerQuality }.filter { it }.collect {
            val why = state.controller.state.value.note ?: currentLower
            toasts.show(why, currentLower, { state.open(SheetPage.Quality) }, error = true)
        }
    }
}

/** Between text subtitles and the bottom of the screen. */
private val CAPTION_MARGIN = 20.dp

/** How often the playhead is read while the controls show. */
private const val PLAYHEAD_MS = 250L

/**
 * Text subtitles over the whole screen, gliding up above the controls while
 * they show, with them (their fade's time and easing), not a jump: only
 * their drawing moves, so their lines stay as they are. Picture subtitles
 * (PGS) over the part of the picture on screen, so they keep their place
 * in Fit and stay in view when zoomed.
 */
@Composable
private fun Captions(state: PlayerScreenState, player: Player?, picture: PictureState, look: CaptionLook, inPip: Boolean) {
    PlayerProbe.composed?.invoke("captions")
    val cues = state.controller.cues.collectAsStateWithLifecycle()
    val lifted = state.controlsVisible && !state.locked && !inPip
    // A little room under them (less in picture in picture's small window),
    // and the controls' height more while they show, read in the drawing only.
    val lift = animateFloatAsState(if (lifted) 1f else 0f, PleyMotion.overlay(reducedMotion()), label = "captions")
    ScreenCaptions(
        player = player,
        serverCues = cues,
        look = look,
        pictures = false,
        bottomPadding = if (inPip) 4.dp else CAPTION_MARGIN,
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer { translationY = -ControlsLift.toPx() * lift.value },
    )
    ScreenCaptions(
        player = player,
        serverCues = cues,
        look = look,
        pictures = true,
        bottomPadding = 0.dp,
        modifier = Modifier.fillMaxSize().layout { measurable, constraints ->
            val area = picture.visible(constraints.maxWidth, constraints.maxHeight)
            val placeable = measurable.measure(Constraints.fixed(area.width.coerceAtLeast(0), area.height.coerceAtLeast(0)))
            layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(area.left, area.top) }
        },
    )
}

/**
 * The video on a SurfaceView (frames straight to the display: HDR and Dolby
 * Vision pass through), placed by [picture]: fitted, filled or zoomed, its
 * bottom kept on screen when subtitles are burned in. Black until the
 * first frame, which then fades in over 200 ms as on the native Samsung
 * app (at once with reduced motion), except while [holding]: a stream made
 * ready beside the one that played takes over, and the last picture stays
 * until its first. Zooming lays it out again; nothing recomposes, and the
 * fade only draws.
 */
@OptIn(UnstableApi::class)
@Composable
private fun VideoPicture(player: Player, picture: PictureState, holding: () -> Boolean) {
    PlayerProbe.composed?.invoke("video")
    val presentation = rememberPresentationState(player)
    LaunchedEffect(presentation, picture) {
        snapshotFlow { presentation.videoSizeDp }.collect { size ->
            picture.videoAspect = if (size != null && size.isSpecified && size.height > 0f) size.width / size.height else 0f
        }
    }
    Box(Modifier.fillMaxSize().clipToBounds()) {
        PlayerSurface(
            player = player,
            modifier = Modifier
                .layout { measurable, constraints ->
                    val area = picture.place(constraints.maxWidth, constraints.maxHeight)
                    val placeable = measurable.measure(Constraints.fixed(area.width.coerceAtLeast(0), area.height.coerceAtLeast(0)))
                    layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(area.left, area.top) }
                },
            surfaceType = SURFACE_TYPE_SURFACE_VIEW,
        )
        val covered = presentation.coverSurface && !holding()
        val cover = animateFloatAsState(if (covered) 1f else 0f, if (covered) snap() else PleyMotion.overlay(reducedMotion()), label = "cover")
        Box(Modifier.fillMaxSize().drawBehind { if (cover.value > 0f) drawRect(Color.Black, alpha = cover.value) })
    }
}
