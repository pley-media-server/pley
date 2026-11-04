package app.pley.android.common.perf

import java.util.Locale

/**
 * One window's frames, as the frame-rate counter ([FrameRate]) tells them:
 * [frames] drawn, as [fps]; the longest, in milliseconds from the frame's
 * vsync to its drawing done; and how many [missed] their deadline (the
 * display's next refresh).
 */
data class FrameReading(val frames: Int, val fps: Int, val longestMs: Double, val missed: Int) {
    /** The log's line: "fps=58 longest=21.4 missed=2". */
    val line: String
        get() = "fps=$fps longest=${String.format(Locale.ROOT, "%.1f", longestMs)} missed=$missed"

    /** The box's text: "58 fps  21.4 ms  2 late". */
    val label: String
        get() = "$fps fps  ${String.format(Locale.ROOT, "%.1f", longestMs)} ms  $missed late"

    /** None late, a few (less than a frame in four), or many: the box's colour. */
    val grade: Grade
        get() = when {
            missed == 0 -> Grade.Smooth
            missed * 4 < frames -> Grade.Some
            else -> Grade.Many
        }

    enum class Grade { Smooth, Some, Many }
}

/**
 * Counts the frames of a window of [windowMs], from its first frame: each
 * frame [add]ed with its duration and its deadline; [close] gives the
 * window's reading and starts the next. A window of two frames or fewer
 * gives none, as Samsung's counter does: a screen at rest draws now and
 * then (a cursor's blink, the box's own new text), and that's no frame
 * rate. One thread adds and closes.
 */
class FrameTally(val windowMs: Long = WINDOW_MS) {
    private var frames = 0
    private var longest = 0L
    private var missed = 0

    /** Whether a window is open: a frame has come since the last [close]. */
    val open: Boolean
        get() = frames > 0

    /** A frame that took [durationNanos] and had [deadlineNanos] to be shown on time. */
    fun add(durationNanos: Long, deadlineNanos: Long) {
        frames++
        if (durationNanos > longest) longest = durationNanos
        if (durationNanos > deadlineNanos) missed++
    }

    /** Frames drawn whose times weren't reported (the system dropped them while the counter was busy): counted, not timed. */
    fun lost(count: Int) {
        if (count > 0) frames += count
    }

    /** The window's reading, or `null` for a window of [MIN_FRAMES] or fewer; the next window starts empty. */
    fun close(): FrameReading? {
        val reading = if (frames > MIN_FRAMES) {
            FrameReading(frames, fps = (frames * 1000L / windowMs).toInt(), longestMs = longest / 1_000_000.0, missed = missed)
        } else {
            null
        }
        frames = 0
        longest = 0
        missed = 0
        return reading
    }

    companion object {
        /** Samsung's counter's window too: a reading every half second while something moves. */
        const val WINDOW_MS = 500L

        /** At most this many frames in a window is a screen at rest. */
        const val MIN_FRAMES = 2

        /** A frame's time at [refreshRate] Hz, its deadline where the system doesn't say (before Android 12). */
        fun intervalNanos(refreshRate: Float): Long =
            if (refreshRate >= 1f) (1_000_000_000.0 / refreshRate).toLong() else 1_000_000_000L / 60
    }
}
