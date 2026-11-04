package app.pley.android.designsystem.image

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.isSpecified
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.layout.layout
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.LayoutModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.node.invalidateMeasurement
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.reducedMotion
import coil3.BitmapImage
import coil3.ImageLoader
import coil3.SingletonImageLoader
import coil3.compose.AsyncImage
import coil3.compose.AsyncImagePainter
import coil3.compose.LocalPlatformContext
import coil3.decode.DataSource
import coil3.request.SuccessResult
import coil3.request.transitionFactory
import coil3.transition.CrossfadeTransition
import kotlin.math.floor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The loader [Artwork] uses: Coil's singleton (the app's [PleyImageLoader][app.pley.android.designsystem.PleyImageLoader])
 * unless something provides another, as tests and the debug gallery do.
 */
val LocalArtworkImageLoader = staticCompositionLocalOf<ImageLoader?> { null }

/**
 * Whether a picture has loaded, read only while drawing: the placeholder
 * goes and the trace counters count without recomposing anything. One per
 * image: `remember(image) { ArtworkLoad() }`.
 */
@Stable
class ArtworkLoad {
    /**
     * Whether the picture is drawn in full: loaded and, when it fades in
     * (from disk or the network), faded in, so the placeholder stays under
     * it till then rather than leaving the page showing through.
     */
    var loaded: Boolean by mutableStateOf(false)
        private set

    /** Whether loading has ended: with the picture, or without one when none of its addresses loads. */
    var settled: Boolean by mutableStateOf(false)
        private set

    /** Whether none of its addresses loaded: there is no picture to wait for. */
    var failed: Boolean by mutableStateOf(false)
        private set

    /**
     * Whether the picture has loaded before ([ShownArtwork]): while it comes
     * back, its well is plain, without the title, as once it is in. Set by
     * [ArtworkBox] as it composes, before anything is drawn.
     */
    internal var known: Boolean = false

    /**
     * Whether the loaded picture has no transparency, so it hides whatever is
     * under it: a well drawn there would be a fill of the whole box that never
     * shows. Set before [loaded], which is what the drawing reads.
     */
    internal var opaque: Boolean = false
        private set

    private var fade: Job? = null

    internal fun onState(state: AsyncImagePainter.State, scope: CoroutineScope) {
        fade?.cancel()
        fade = null
        settled = state is AsyncImagePainter.State.Success || state is AsyncImagePainter.State.Error
        failed = state is AsyncImagePainter.State.Error
        val result = (state as? AsyncImagePainter.State.Success)?.result
        // A JPEG, or a PNG without an alpha channel; anything else may let the well show through.
        opaque = (result?.image as? BitmapImage)?.bitmap?.hasAlpha() == false
        val millis = result?.fadeMillis() ?: 0
        if (millis == 0) {
            loaded = result != null
            return
        }
        loaded = false
        fade = scope.launch {
            // Coil's fade starts on the next frame.
            withFrameNanos { }
            delay(millis.toLong())
            loaded = true
        }
    }
}

/** How long Coil fades this picture in: not at all from the memory cache. */
private fun SuccessResult.fadeMillis(): Int =
    if (dataSource == DataSource.MEMORY_CACHE) 0 else (request.transitionFactory as? CrossfadeTransition.Factory)?.durationMillis ?: 0

/**
 * A picture filling its box: [image]'s first address that loads, at the
 * box's width. Until then, and when none does, the title's gradient
 * ([Placeholder]) with [title] centred on it in [titleStyle] (none for
 * backdrops: leave [titleStyle] null); a picture that has loaded before
 * comes back without it, and without fading in, as none does with reduced
 * motion. Nothing here recomposes when the picture arrives: the
 * placeholder is drawn behind it, in the draw phase. [load] tells whoever
 * passes it when the picture is in.
 */
@Composable
fun Artwork(
    image: ArtworkImage?,
    title: String,
    modifier: Modifier = Modifier,
    titleStyle: TextStyle? = null,
    contentScale: ContentScale = ContentScale.Crop,
    alignment: Alignment = Alignment.Center,
    load: ArtworkLoad = remember(image) { ArtworkLoad() },
) {
    ArtworkBox(image, load, modifier.artworkPlaceholder(title, titleStyle, load), contentScale, alignment)
}

/**
 * The box and its picture; [modifier] carries the placeholder and whatever
 * is drawn over it. One node either way, the picture drawn as its content:
 * a grid brings in a row of these at each step of a held key.
 */
@Composable
internal fun ArtworkBox(
    image: ArtworkImage?,
    load: ArtworkLoad,
    modifier: Modifier,
    contentScale: ContentScale = ContentScale.Crop,
    alignment: Alignment = Alignment.Center,
) {
    if (image == null || image.isEmpty) {
        Box(modifier)
        return
    }
    val loader = LocalArtworkImageLoader.current ?: SingletonImageLoader.get(LocalPlatformContext.current)
    load.known = remember(image, loader) { loader.shownArtwork?.has(image) == true }
    // Reduced motion: no fading either, things change in place.
    val reduced = reducedMotion()
    val model = remember(image, reduced) { if (reduced && image.fade) image.copy(fade = false) else image }
    val scope = rememberCoroutineScope()
    val onState = remember(load, scope) { { state: AsyncImagePainter.State -> load.onState(state, scope) } }
    AsyncImage(
        model = rememberArtworkModel(model),
        contentDescription = null,
        imageLoader = loader,
        modifier = modifier.then(BoxSized),
        contentScale = contentScale,
        alignment = alignment,
        onState = onState,
    )
}

/**
 * The picture as big as an empty Box with [ArtworkBox]'s modifier would
 * be, the least its constraints allow, whatever the picture's own size:
 * the size its painter is given, as a Box's child that matched it was.
 */
private val BoxSized = Modifier.layout { measurable, constraints ->
    val placeable = measurable.measure(Constraints.fixed(constraints.minWidth, constraints.minHeight))
    layout(placeable.width, placeable.height) { placeable.place(0, 0) }
}

/**
 * Draws the placeholder behind the content while [load] hasn't loaded, then
 * [well] (when specified) under pictures that don't cover the box. Given a
 * [shape], all of it takes that shape: the placeholder and the well are
 * drawn in it, and the content (the picture and what's drawn over it) is
 * clipped to it only while there is a picture, fading in or in. A tile
 * shaped this way has no clip while its picture waits, which is every tile
 * a held key brings in: Modifier.clip would give each its own layer and
 * the GPU a clip pass.
 */
internal fun Modifier.artworkPlaceholder(
    title: String,
    titleStyle: TextStyle?,
    load: ArtworkLoad,
    well: Color = Color.Unspecified,
    shape: Shape? = null,
): Modifier = this then PlaceholderElement(title, titleStyle, load, well, shape)

private data class PlaceholderElement(
    val title: String,
    val titleStyle: TextStyle?,
    val load: ArtworkLoad,
    val well: Color,
    val shape: Shape?,
) : ModifierNodeElement<PlaceholderNode>() {
    override fun create() = PlaceholderNode(title, titleStyle, load, well, shape)

    override fun update(node: PlaceholderNode) = node.update(title, titleStyle, load, well, shape)

    override fun InspectorInfo.inspectableProperties() {
        name = "artworkPlaceholder"
        properties["title"] = title
    }
}

private class PlaceholderNode(
    var title: String,
    var titleStyle: TextStyle?,
    var load: ArtworkLoad,
    var well: Color,
    var shape: Shape?,
) : Modifier.Node(), LayoutModifierNode, DrawModifierNode, CompositionLocalConsumerModifierNode {
    private var look: Placeholder.Look? = null
    private var layout: TextLayoutResult? = null
    private var layoutKey: TitleKey? = null
    private var request: TitleRequest? = null
    private var outline: Outline? = null
    private var outlinePath: Path? = null
    private var outlineSize = Size.Unspecified
    private var outlineDirection = LayoutDirection.Ltr

    override val shouldAutoInvalidate: Boolean get() = false

    fun update(title: String, titleStyle: TextStyle?, load: ArtworkLoad, well: Color, shape: Shape?) {
        val retitled = title != this.title || titleStyle != this.titleStyle
        if (retitled) {
            look = null
            forgetTitle()
        }
        if (shape != this.shape) outline = null
        this.title = title
        this.titleStyle = titleStyle
        this.load = load
        this.well = well
        this.shape = shape
        // A lazy list's item used again for another title keeps its constraints, so nothing measures it again by
        // itself: the new title is asked for as it's measured ahead of the screen, as a new item's is, not a frame
        // or two late as it's first drawn.
        if (retitled && isAttached && !load.known && !load.loaded) invalidateMeasurement()
        invalidateDraw()
    }

    /**
     * The title is asked for here rather than as it's first drawn: a lazy
     * list's prefetch measures the tiles it brings in between frames, so
     * it's ready by the frame that shows them. Not for a picture shown
     * before, which comes straight back. The picture's state is read
     * unobserved: its coming measures nothing again (drawing asks too).
     */
    override fun MeasureScope.measure(measurable: Measurable, constraints: Constraints): MeasureResult {
        val placeable = measurable.measure(constraints)
        val waiting = Snapshot.withoutReadObservation { !load.known && !load.loaded }
        if (waiting) askTitle(Size(placeable.width.toFloat(), placeable.height.toFloat()))
        return layout(placeable.width, placeable.height) { placeable.place(0, 0) }
    }

    override fun onDetach() = forgetTitle()

    override fun ContentDrawScope.draw() {
        val outline = outline()
        if (!load.loaded && load.known && !load.failed) {
            // Shown before and on its way back: the well as it is under the picture.
            if (well.isSpecified) fill(outline, well)
        } else if (!load.loaded) {
            with(Placeholder) { drawGradient(look ?: look(title).also { look = it }, outline) }
            askTitle(size)
            layout?.let { text ->
                drawText(
                    text,
                    color = PleyColors.Fg.copy(alpha = 0.7f),
                    topLeft = Offset(
                        (size.width - text.size.width) / 2f,
                        (size.height - text.size.height) / 2f,
                    ),
                )
            }
        } else if (well.isSpecified && !load.opaque) {
            // Under an opaque picture (filling the box, ContentScale.Crop) the well would never show.
            fill(outline, well)
        }
        val path = outlinePath
        // A picture is drawn over the whole box from the first frame of its fade, before it counts as loaded.
        val picture = load.known || (load.settled && !load.failed)
        if (outline != null && path != null && picture) {
            clipPath(path) { this@draw.drawContent() }
        } else {
            drawContent()
        }
    }

    /** The shape's outline at this size, kept while the size and direction are the same; null without a shape. */
    private fun ContentDrawScope.outline(): Outline? {
        val shape = shape ?: return null
        val kept = outline
        if (kept != null && outlineSize == size && outlineDirection == layoutDirection) return kept
        return shape.createOutline(size, layoutDirection, this).also {
            outline = it
            outlineSize = size
            outlineDirection = layoutDirection
            outlinePath = Path().apply { addOutline(it) }
        }
    }

    private fun DrawScope.fill(outline: Outline?, color: Color) {
        if (outline == null) drawRect(color) else drawOutline(outline, color)
    }

    /**
     * The title for a box of [size]: kept from before, or laid out away from
     * the main thread ([TitleLayouts]) and drawn when it comes.
     */
    private fun Density.askTitle(size: Size) {
        val style = titleStyle ?: return
        if (title.isEmpty()) return
        val padding = 12.dp.roundToPx()
        val key = TitleKey(
            title = title,
            style = style,
            width = (size.width.toInt() - 2 * padding).coerceAtLeast(0),
            height = (size.height.toInt() - 2 * padding).coerceAtLeast(0),
            density = density,
            fontScale = fontScale,
            direction = currentValueOf(LocalLayoutDirection),
        )
        if (key == layoutKey) return
        forgetTitle()
        layoutKey = key
        layout = TitleLayouts.kept(key)
        if (layout != null) return
        val ask = TitleRequest().also { request = it }
        TitleLayouts.layOut(key, currentValueOf(LocalContext), ask) { text ->
            if (request === ask && isAttached) {
                layout = text
                invalidateDraw()
            }
        }
    }

    private fun forgetTitle() {
        request?.dropped = true
        request = null
        layout = null
        layoutKey = null
    }
}

/** At smallest, a placeholder's title is set this much of its style's size so that a word fits whole. */
internal const val MIN_TITLE_SCALE = 0.7f

/**
 * Whether lines of [text] ending at [lineEnds] broke a word, as a word
 * wider than the box is ("Casablan/ca" on a poster): a line ends between
 * two of a word's letters. Breaks at spaces and after hyphens are the
 * title's own.
 */
internal fun brokeAWord(text: String, lineEnds: List<Int>): Boolean =
    lineEnds.any { end -> end in 1 until text.length && text[end - 1].inWord() && text[end].inWord() }

/**
 * How much smaller to set [text] so that its widest word fits [width]
 * whole: 1 when every word fits, else down to [MIN_TITLE_SCALE] (a word
 * wider still breaks where it must). [widthOf] measures a word on one line
 * in the style's own size.
 */
internal fun wholeWordScale(text: String, width: Int, widthOf: (String) -> Int): Float {
    val widest = wordsOf(text).maxOfOrNull(widthOf) ?: return 1f
    return if (width <= 0 || widest <= width) 1f else (width.toFloat() / widest).coerceAtLeast(MIN_TITLE_SCALE)
}

/** [text]'s words, which lines break only between: runs of letters and digits. */
private fun wordsOf(text: String): List<String> {
    val out = ArrayList<String>()
    var start = -1
    for (i in 0..text.length) {
        val letter = i < text.length && text[i].inWord()
        if (letter && start < 0) start = i
        if (!letter && start >= 0) {
            out += text.substring(start, i)
            start = -1
        }
    }
    return out
}

/**
 * A letter or digit of a word. Not in the scripts whose lines may break
 * between any two letters (Chinese, Japanese, Korean, Thai and their
 * neighbours): there a break is no broken word.
 */
private fun Char.inWord(): Boolean = isLetterOrDigit() && when (Character.UnicodeScript.of(code)) {
    Character.UnicodeScript.HAN, Character.UnicodeScript.HIRAGANA, Character.UnicodeScript.KATAKANA,
    Character.UnicodeScript.HANGUL, Character.UnicodeScript.THAI, Character.UnicodeScript.LAO,
    Character.UnicodeScript.KHMER, Character.UnicodeScript.MYANMAR,
    -> false
    else -> true
}

/** This style [scale] of its size, down to the half sp, and its line height with it. */
internal fun TextStyle.scaled(scale: Float): TextStyle {
    if (!fontSize.isSp) return this
    val size = floor(fontSize.value * scale * 2f) / 2f
    val ratio = size / fontSize.value
    return copy(fontSize = size.sp, lineHeight = if (lineHeight.isSp) (lineHeight.value * ratio).sp else lineHeight)
}
