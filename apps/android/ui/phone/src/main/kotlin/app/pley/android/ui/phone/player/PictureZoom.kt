package app.pley.android.ui.phone.player

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.IntRect
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt

/** How the picture meets the screen: all of it, covering the screen, or past a film's black bars. */
enum class FitMode {
    /** The whole picture, with black bars where its shape isn't the screen's. */
    Fit,

    /** Covering the screen: a 16:9 or 2.39:1 film on a 20:9 phone loses a little at the edges. */
    Fill,

    /** One step past Fill, for wide films stored with black bars inside a 16:9 picture. */
    Zoom,

    /** Pinched to a size of its own. */
    Free,
}

/**
 * The arithmetic of fitting a video to a screen (the hybrid player's, which
 * people liked). Aspects are width over height; a scale of 1 is Fit.
 */
object PictureGeometry {
    /** The furthest a pinch zooms. */
    const val MAX_SCALE = 3f

    /** The widest common film shape (CinemaScope). */
    const val SCOPE = 2.39f

    /** How much the fitted picture must grow to cover the screen. */
    fun coverScale(video: Float, screen: Float): Float =
        if (video <= 0f || screen <= 0f) 1f else max(video / screen, screen / video)

    /**
     * How much more than Fill it takes to hide 2.39:1 bars stored in a 16:9
     * picture. Only pictures narrower than the screen can hide bars this way.
     */
    fun barsScale(video: Float, screen: Float): Float =
        if (video <= 0f || screen <= 0f || video >= 2f || screen <= video) 1f else max(1f, SCOPE / screen)

    /** The scale of [mode] ([free] for a pinched one). */
    fun scaleOf(mode: FitMode, video: Float, screen: Float, free: Float): Float = when (mode) {
        FitMode.Fit -> 1f
        FitMode.Fill -> coverScale(video, screen)
        FitMode.Zoom -> coverScale(video, screen) * barsScale(video, screen)
        FitMode.Free -> free.coerceIn(1f, MAX_SCALE)
    }

    /** Whether Zoom shows more than Fill on this screen. */
    fun zoomHelps(video: Float, screen: Float): Boolean = barsScale(video, screen) > 1.02f

    /**
     * Where the picture goes on a [width] by [height] screen: fitted, grown
     * by [scale] and centered. Subtitles burned into the picture sit at its
     * bottom, so with [keepBottom] the bottom stays on screen and only the
     * top is cropped. [video] of 0 (not known yet) fills the screen.
     */
    fun place(width: Int, height: Int, video: Float, scale: Float, keepBottom: Boolean): IntRect {
        if (width <= 0 || height <= 0) return IntRect(0, 0, width.coerceAtLeast(0), height.coerceAtLeast(0))
        val screen = width.toFloat() / height
        val aspect = if (video > 0f) video else screen
        val fitWidth = if (aspect > screen) width.toFloat() else height * aspect
        val fitHeight = if (aspect > screen) width / aspect else height.toFloat()
        val w = (fitWidth * scale).roundToInt()
        val h = (fitHeight * scale).roundToInt()
        val left = (width - w) / 2
        val top = if (keepBottom && h > height) height - h else (height - h) / 2
        return IntRect(left, top, left + w, top + h)
    }

    /** The part of the placed picture that is on screen. */
    fun visible(width: Int, height: Int, video: Float, scale: Float, keepBottom: Boolean): IntRect =
        place(width, height, video, scale, keepBottom).intersect(IntRect(0, 0, width, height))
}

/**
 * The picture's zoom: Fit, Fill, Zoom or a pinched scale. Fill and Zoom
 * follow the screen and the video, so turning the phone keeps the screen
 * covered. Picture in picture always shows all of it. Read only while
 * measuring, so zooming lays out again without recomposing.
 */
@Stable
class PictureState(initial: FitMode = FitMode.Fit) {
    var mode by mutableStateOf(initial)
        private set

    /** The pinched scale, while [mode] is [FitMode.Free]. */
    private var free by mutableFloatStateOf(1f)

    /** The video's shape (width over height); 0 until its first frame is known. */
    var videoAspect by mutableFloatStateOf(0f)

    /** The selected subtitles are drawn into the picture by the server: keep its bottom on screen. */
    var burnedSubtitles by mutableStateOf(false)

    /** In picture in picture: the whole picture, always. */
    var inPip by mutableStateOf(false)

    /** Called when the mode changes by a person's hand, to remember whether they fill. */
    var onChosen: (FitMode) -> Unit = {}

    /** The scale on a screen of [screen]'s shape. */
    fun scale(screen: Float): Float = if (inPip) 1f else PictureGeometry.scaleOf(mode, videoAspect, screen, free)

    fun place(width: Int, height: Int): IntRect =
        PictureGeometry.place(width, height, videoAspect, scale(width.toFloat() / height.coerceAtLeast(1)), burnedSubtitles)

    fun visible(width: Int, height: Int): IntRect =
        PictureGeometry.visible(width, height, videoAspect, scale(width.toFloat() / height.coerceAtLeast(1)), burnedSubtitles)

    /** Fill or Fit, as remembered, before anything plays. */
    fun restore(fill: Boolean) {
        mode = if (fill) FitMode.Fill else FitMode.Fit
    }

    /**
     * What the mode button does next on a screen of [screen]'s shape: Fit
     * goes to Fill, Fill to Zoom where Zoom shows more, anything else to Fit.
     */
    fun nextMode(screen: Float): FitMode {
        val now = scale(screen)
        val cover = PictureGeometry.coverScale(videoAspect, screen)
        val zoom = cover * PictureGeometry.barsScale(videoAspect, screen)
        return when {
            now <= 1.001f -> FitMode.Fill
            now < zoom - 0.01f && PictureGeometry.zoomHelps(videoAspect, screen) -> FitMode.Zoom
            else -> FitMode.Fit
        }
    }

    /** The mode button: steps to [nextMode]. */
    fun step(screen: Float): FitMode {
        val next = nextMode(screen)
        choose(next)
        return next
    }

    fun choose(next: FitMode) {
        mode = next
        onChosen(next)
    }

    /** A pinch began: it scales from where the picture is now. */
    fun pinchStart(screen: Float) {
        free = scale(screen)
        mode = FitMode.Free
    }

    /** A pinch went on, by [zoom] since its last step. */
    fun pinchBy(zoom: Float) {
        free = (free * zoom).coerceIn(1f, PictureGeometry.MAX_SCALE)
    }

    /** The pinch ended: close to Fit, Fill or Zoom counts as that. */
    fun pinchEnd(screen: Float): FitMode {
        val cover = PictureGeometry.coverScale(videoAspect, screen)
        val zoom = cover * PictureGeometry.barsScale(videoAspect, screen)
        val ended = when {
            free <= 1.02f -> FitMode.Fit
            abs(free - cover) < SNAP -> FitMode.Fill
            PictureGeometry.zoomHelps(videoAspect, screen) && abs(free - zoom) < SNAP -> FitMode.Zoom
            else -> FitMode.Free
        }
        choose(ended)
        return ended
    }

    private companion object {
        const val SNAP = 0.04f
    }
}
