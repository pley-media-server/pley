package app.pley.android.designsystem.image

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/**
 * What a placeholder's title is laid out for: the title, its style, the
 * box inside the placeholder's padding, and the screen's.
 */
internal data class TitleKey(
    val title: String,
    val style: TextStyle,
    val width: Int,
    val height: Int,
    val density: Float,
    val fontScale: Float,
    val direction: LayoutDirection,
)

/** A placeholder's wish for its title, which it drops when it goes or wants another. */
internal class TitleRequest {
    @Volatile
    var dropped = false
}

/**
 * Placeholders' titles, laid out on a thread of their own: a tile a held
 * key brings into a grid is measured in the frame that shows it, and
 * laying its title out there took a Google TV Streamer's main thread
 * 0.65 ms a tile, a dozen tiles for each step. The gradient shows at once
 * and the title as soon as it's ready, within a frame or two, as the
 * native Samsung app's titles do (its renderer lays out text away from its
 * main thread); a tile the grid makes ahead of the screen has it before it
 * shows. Titles laid out before are kept, and come back in the same frame.
 */
internal object TitleLayouts {
    private val worker = Executors.newSingleThreadExecutor { task ->
        Thread(task, "pley-titles").apply { isDaemon = true }
    }
    /** Titles laid out, by what for: the main thread's alone. */
    private val done = object : LinkedHashMap<TitleKey, TextLayoutResult>(64, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<TitleKey, TextLayoutResult>?): Boolean = size > KEPT
    }

    /** Titles asked for and not yet handed back. */
    private val pending = AtomicInteger()

    /** The worker's measurer, made on it: Compose's text isn't shared between threads here. */
    private var measurer: TextMeasurer? = null

    /** Whether no title is being laid out: for tests, which wait for them. */
    val idle: Boolean get() = pending.get() == 0

    /** The title laid out for [key] before, if it's kept. On the main thread. */
    fun kept(key: TitleKey): TextLayoutResult? = done[key]

    /**
     * Lays out [key]'s title on the worker and hands it to [ready] on the
     * main thread, unless [request] is dropped meanwhile. On the main thread.
     */
    fun layOut(key: TitleKey, context: Context, request: TitleRequest, ready: (TextLayoutResult) -> Unit) {
        val app = context.applicationContext
        // The main thread's looper as it is now, not as it was first: tests have one each.
        val main = Handler(Looper.getMainLooper())
        pending.incrementAndGet()
        worker.execute {
            val text = if (request.dropped) null else runCatching { measure(key, app) }.getOrNull()
            if (text != null) {
                main.post {
                    done[key] = text
                    if (!request.dropped) ready(text)
                }
            }
            // Done once handed on: what's posted runs as the main thread next idles.
            pending.decrementAndGet()
        }
    }

    /** [key]'s title centred in its box, at most four lines; a word too wide for it makes the title smaller rather than break. */
    private fun measure(key: TitleKey, context: Context): TextLayoutResult {
        val measurer = measurer ?: TextMeasurer(createFontFamilyResolver(context), Density(key.density, key.fontScale), key.direction, cacheSize = 0)
            .also { measurer = it }
        val density = Density(key.density, key.fontScale)
        val centred = key.style.copy(textAlign = TextAlign.Center)
        val constraints = Constraints(minWidth = key.width, maxWidth = key.width, maxHeight = key.height)
        fun lay(style: TextStyle) = measurer.measure(
            key.title,
            style,
            TextOverflow.Ellipsis,
            maxLines = 4,
            constraints = constraints,
            layoutDirection = key.direction,
            density = density,
        )
        val text = lay(centred)
        // A word wider than the box broke across lines: smaller, so that it fits whole.
        if (text.lineCount > 1 && brokeAWord(key.title, (0 until text.lineCount - 1).map { text.getLineEnd(it) })) {
            val scale = wholeWordScale(key.title, key.width) { word ->
                measurer.measure(word, centred, maxLines = 1, softWrap = false, layoutDirection = key.direction, density = density).size.width
            }
            if (scale < 1f) return lay(centred.scaled(scale))
        }
        return text
    }

    /** Titles kept: a screen of tiles and the rows either side, scrolled back and forth. */
    private const val KEPT = 128
}
