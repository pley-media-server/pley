package app.pley.android.ui.phone.player

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

/** What the player's gestures change outside the screen: the media volume and the window's brightness, from 0 to 1. */
interface LevelControls {
    fun volume(): Float

    fun setVolume(level: Float)

    fun brightness(): Float

    fun setBrightness(level: Float)

    /** Whether dragging changes them: not on a car's screen, whose volume is the car's and whose brightness isn't the window's. */
    val levelsDraggable: Boolean get() = true
}

/**
 * The player's touch gestures over the video (design spec §3.5):
 * - a tap shows or hides the controls; on the left or right third it waits
 *   for a second tap, and two taps there seek 10 s back or forward (more
 *   taps in a row add 10 s each);
 * - a vertical drag on the right half sets the volume, on the left half the
 *   brightness ([LevelControls.levelsDraggable]);
 * - two fingers pinch the picture between Fit and Fill, and freely beyond;
 * - while [PlayerScreenState.locked], a tap only shows the unlock button.
 *
 * The controls above take their own touches; everything else lands here.
 */
internal fun Modifier.playerGestures(
    state: PlayerScreenState,
    picture: PictureState,
    levels: LevelControls,
    scope: CoroutineScope,
): Modifier = pointerInput(state, picture, levels) {
    val slop = viewConfiguration.touchSlop
    val doubleTapMs = viewConfiguration.doubleTapTimeoutMillis
    val edge = 32.dp.toPx()
    var pendingTap: Job? = null
    var lastSideTap = Pair(0, 0L)

    fun tap(at: Offset, time: Long) {
        if (state.locked) {
            state.unlockShown = !state.unlockShown
            return
        }
        val third = size.width / 3f
        val side = when {
            at.x < third -> -1
            at.x > size.width - third -> 1
            else -> 0
        }
        val run = state.seekRun
        when {
            side == 0 -> {
                pendingTap?.cancel()
                state.toggleControls()
            }
            // A run of seeks goes on with every tap on its side.
            run != null && run.side == side && time - run.at < SEEK_RUN_MS -> state.doubleTapSeek(side, time)
            lastSideTap.first == side && time - lastSideTap.second < doubleTapMs -> {
                pendingTap?.cancel()
                lastSideTap = Pair(0, 0L)
                state.doubleTapSeek(side, time)
            }
            else -> {
                lastSideTap = Pair(side, time)
                pendingTap?.cancel()
                pendingTap = scope.launch {
                    delay(doubleTapMs)
                    state.toggleControls()
                }
            }
        }
    }

    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = true)
        val start = down.position
        var mode = Gesture.Undecided
        var level = 0f
        val screen = if (size.height > 0) size.width.toFloat() / size.height else 0f
        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Main)
            val pressed = event.changes.filter { it.pressed }
            if (pressed.isEmpty()) break
            if (pressed.size >= 2 && !state.locked && (mode == Gesture.Undecided || mode == Gesture.Pinch)) {
                if (mode != Gesture.Pinch) {
                    mode = Gesture.Pinch
                    picture.pinchStart(screen)
                }
                val zoom = event.calculateZoom()
                if (zoom != 1f) picture.pinchBy(zoom)
                event.changes.forEach { it.consume() }
                continue
            }
            val change = pressed.first()
            if (mode == Gesture.Undecided) {
                val moved = change.position - start
                val nearEdge = start.y < edge || start.y > size.height - edge
                if (abs(moved.y) > slop && abs(moved.y) > abs(moved.x) && !state.locked && !nearEdge && state.sheet == null && levels.levelsDraggable) {
                    val volume = start.x > size.width / 2f
                    mode = if (volume) Gesture.Volume else Gesture.Brightness
                    level = if (volume) levels.volume() else levels.brightness()
                } else if (moved.getDistance() > slop) {
                    mode = Gesture.Ignored
                }
            }
            if (mode == Gesture.Volume || mode == Gesture.Brightness) {
                val dy = change.positionChange().y
                level = (level - dy / (size.height * DRAG_SPAN)).coerceIn(0f, 1f)
                if (mode == Gesture.Volume) levels.setVolume(level) else levels.setBrightness(level)
                state.levelDrag = LevelDrag(volume = mode == Gesture.Volume, level = level)
                change.consume()
            }
        }
        when (mode) {
            Gesture.Undecided -> tap(start, down.uptimeMillis)
            Gesture.Pinch -> {
                state.modeShown = picture.pinchEnd(screen)
                state.touched()
            }
            Gesture.Volume, Gesture.Brightness, Gesture.Ignored -> Unit
        }
        if (mode == Gesture.Volume || mode == Gesture.Brightness) {
            scope.launch {
                val shown = state.levelDrag
                delay(LEVEL_LINGER_MS)
                if (state.levelDrag === shown) state.levelDrag = null
            }
        }
    }
}

private enum class Gesture { Undecided, Pinch, Volume, Brightness, Ignored }

/** A drag over this share of the screen's height goes from 0 to 1. */
private const val DRAG_SPAN = 0.6f

/** Taps on a side keep seeking for this long after the last one. */
internal const val SEEK_RUN_MS = 700L

/** The level shows this long after the drag ends. */
private const val LEVEL_LINGER_MS = 600L
