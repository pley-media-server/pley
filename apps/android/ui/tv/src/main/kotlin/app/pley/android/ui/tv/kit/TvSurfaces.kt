package app.pley.android.ui.tv.kit

import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.BiasAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Surface
import androidx.tv.material3.SurfaceDefaults
import app.pley.android.common.Tier
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyElevation
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkLoad
import app.pley.android.designsystem.image.GradientBitmaps
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.designsystem.image.drawStretched
import app.pley.android.designsystem.reducedMotion
import app.pley.android.ui.tv.theme.LocalTvTier
import coil3.ImageLoader
import coil3.SingletonImageLoader
import coil3.annotation.ExperimentalCoilApi
import coil3.compose.LocalPlatformContext
import coil3.decode.BlackholeDecoder
import coil3.request.CachePolicy
import coil3.request.Disposable
import coil3.request.ImageRequest
import coil3.size.Precision
import coil3.size.Scale
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Where the bottom gradient over a backdrop runs (design spec §2.2, §4.1,
 * §4.3): [bg][PleyColors.Bg] at 0 % at [from] to 100 % at [to], from the
 * top of the screen.
 */
@Immutable
data class BackdropScrim(val from: Dp, val to: Dp) {
    companion object {
        /** Home at rest, under the hero. */
        val Hero = BackdropScrim(220.dp, 410.dp)

        /** Home in the rows, under the info block. */
        val Rows = BackdropScrim(120.dp, 300.dp)

        /** Item pages. */
        val Item = BackdropScrim(260.dp, 500.dp)
    }
}

/**
 * The page's backdrop (design spec §2.2, §3.6 of the TV addendum): one
 * picture layer under everything, cropped towards its subject (right of
 * centre, above the middle), with the two gradients that keep the text
 * column on at least 85 % [bg][PleyColors.Bg], drawn together.
 *
 * It follows [image] only once focus has rested ([rememberRested]) and
 * the new picture has loaded (or failed, or taken over a second): a
 * 300 ms crossfade on T1 and T2, the upper picture's kept layer drawn at
 * its alpha over the lower's; an instant swap on T0 and with
 * reduced motion. Each picture keeps its slot while it shows, so the one
 * going is never loaded again. Pictures are asked for at most 1280 wide,
 * 960 on T0; [rememberBackdropPrefetcher] loads the next ones ahead.
 * Never blurred.
 */
@Composable
fun TvBackdrop(
    image: ArtworkImage?,
    title: String,
    modifier: Modifier = Modifier,
    scrim: BackdropScrim = BackdropScrim.Hero,
    atOnce: Boolean = false,
) {
    val tier = LocalTvTier.current
    val cap = ArtworkImage.backdropCap(tier)
    val wanted = remember(image, title, cap) { Backdrop(image?.cappedAt(cap), title) }
    val shown = rememberRested(wanted, atOnce = atOnce)
    val instant = tier == Tier.T0 || reducedMotion()
    val slots = remember { BackdropSlots(shown) }
    LaunchedEffect(shown) { slots.show(shown, instant) }
    // Each picture, its placeholder and the scrims over it are drawn once into a
    // layer of their own that is kept: while focus moves over the rows nothing
    // in it changes, so each frame costs one copy of it instead of four or five
    // full-screen passes (43 ms of GPU a frame down to 13 on a Google TV
    // Streamer). A crossfade costs a copy of each, the upper at its alpha,
    // rather than drawing both pictures and the scrims again in every frame of
    // it; the scrims being the same over both, the blend is the same.
    Box(modifier.fillMaxSize()) {
        slots.lower?.let { slot ->
            key(slot) {
                BackdropPicture(
                    slot,
                    Modifier
                        .fillMaxSize()
                        .drawnOnceIn(slot)
                        .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
                        .backdropScrims(scrim),
                )
            }
        }
        slots.upper?.let { slot ->
            key(slot) {
                BackdropPicture(
                    slot,
                    Modifier
                        .fillMaxSize()
                        .drawnOnceIn(slot)
                        .graphicsLayer {
                            alpha = slots.upperAlpha.value
                            compositingStrategy = CompositingStrategy.Offscreen
                        }
                        .backdropScrims(scrim),
                )
            }
        }
    }
}

@Immutable
private data class Backdrop(val image: ArtworkImage?, val title: String)

/**
 * The backdrop's two slots. The next picture loads in the free one, out of
 * sight (under the upper one, or in it at alpha 0) and not drawn, and only
 * then does that slot draw and the upper one fade: in over the lower one,
 * or out to show it. Pictures are opaque, so either is the same crossfade.
 */
@Stable
private class BackdropSlots(first: Backdrop) {
    var lower by mutableStateOf<BackdropSlot?>(BackdropSlot(first, swapped = false))
        private set
    var upper by mutableStateOf<BackdropSlot?>(null)
        private set
    val upperAlpha = Animatable(0f)

    /** Whether the upper slot shows, or will once its fade ends. */
    private var upperShows = false

    suspend fun show(next: Backdrop, instant: Boolean) {
        // A swap cut short by the next one ends where it was going.
        upperAlpha.snapTo(if (upperShows) 1f else 0f)
        if (upperShows) lower = null else upper = null
        if ((if (upperShows) upper else lower)?.backdrop == next) return
        val incoming = BackdropSlot(next, swapped = true)
        if (upperShows) lower = incoming else upper = incoming
        incoming.awaitPicture()
        incoming.drawn = true
        upperShows = !upperShows
        val target = if (upperShows) 1f else 0f
        if (instant) upperAlpha.snapTo(target) else upperAlpha.animateTo(target, tween(PleyMotion.BACKDROP_MS, easing = LinearEasing))
        if (upperShows) lower = null else upper = null
    }
}

/** A backdrop in its slot. Swapped in, its picture doesn't fade in by itself: the slot does. */
private class BackdropSlot(val backdrop: Backdrop, swapped: Boolean) {
    val image: ArtworkImage? = backdrop.image?.let { if (swapped) it.copy(fade = false) else it }
    val load = ArtworkLoad()

    /** Whether the slot draws: swapped in, only once its picture is in (or will not come), when it fades. */
    var drawn by mutableStateOf(!swapped)

    /** Until the picture is in, or none of its addresses loads, for at most [PictureWaitMs]. */
    suspend fun awaitPicture() {
        if (image == null || image.isEmpty) return
        withTimeoutOrNull(PictureWaitMs) { snapshotFlow { load.settled }.first { it } }
    }
}

/** How long a backdrop waits for its picture before it swaps in anyway. */
private const val PictureWaitMs = 1_000L

/**
 * Draws a swapped-in [slot], and so its kept layer, only once it is
 * [drawn][BackdropSlot.drawn]: drawn while its picture loaded, out of sight,
 * the layer was made twice, with the placeholder and then with the
 * picture, each a full-screen pass (13 to 20 ms of the RenderThread on a
 * Fire TV).
 */
private fun Modifier.drawnOnceIn(slot: BackdropSlot): Modifier = drawWithContent { if (slot.drawn) drawContent() }

@Composable
private fun BackdropPicture(slot: BackdropSlot, modifier: Modifier) {
    Artwork(slot.image, slot.backdrop.title, modifier, alignment = SubjectAlignment, load = slot.load)
}

/** [this], asked for at most [cap] wide. */
private fun ArtworkImage.cappedAt(cap: Int): ArtworkImage = if (maxWidth in 1..cap) this else copy(maxWidth = cap)

/**
 * Loads backdrops before [TvBackdrop] shows them, at the size it asks for
 * (the window's, capped as it caps them), so it finds them in the cache
 * and swaps as soon as focus rests. Only the last ones asked for keep
 * loading. Not for every tile a held key passes: while [gate] holds, only
 * the last ones asked for load, once the key comes up.
 */
@Stable
class BackdropPrefetcher internal constructor(
    private val context: Context,
    private val loader: ImageLoader,
    private val tier: Tier,
    private val size: IntSize,
    private val gate: KeyRepeatGate,
) {
    /** What is loading or loaded, by picture and whether only to disk. */
    private var loading: Map<Pair<ArtworkImage, Boolean>, Disposable> = emptyMap()

    /** Asked for while a key was held: loaded as it comes up. */
    private var heldFocused: ArtworkImage? = null
    private var heldBeside: List<ArtworkImage?>? = null
    private val released: () -> Unit = {
        val focused = heldFocused
        val beside = heldBeside
        heldFocused = null
        heldBeside = null
        if (beside != null) prefetch(focused, beside)
    }

    /**
     * Loads [focused]'s backdrop and those [beside] it (only to the disk
     * cache on T0, whose memory is short), and stops loading any asked for
     * before that aren't among them.
     */
    fun prefetch(focused: ArtworkImage?, beside: List<ArtworkImage?>) {
        if (size.width <= 0 || size.height <= 0) return
        if (gate.holding) {
            heldFocused = focused
            heldBeside = beside
            gate.onRelease(released)
            return
        }
        val cap = ArtworkImage.backdropCap(tier)
        val wanted = LinkedHashMap<ArtworkImage, Boolean>()
        focused?.takeUnless { it.isEmpty }?.let { wanted[it.cappedAt(cap)] = false }
        beside.forEach { image -> image?.takeUnless { it.isEmpty }?.let { wanted.getOrPut(it.cappedAt(cap)) { tier == Tier.T0 } } }
        val next = wanted.entries.associate { (image, diskOnly) ->
            val key = image to diskOnly
            key to (loading[key] ?: loader.enqueue(request(image, diskOnly)))
        }
        loading.forEach { (key, job) -> if (key !in next) job.dispose() }
        loading = next
    }

    /** What TvBackdrop's picture asks for: its box's size, filled, never scaled up. */
    @OptIn(ExperimentalCoilApi::class)
    private fun request(image: ArtworkImage, diskOnly: Boolean): ImageRequest =
        ImageRequest.Builder(context)
            .data(image)
            .size(size.width, size.height)
            .scale(Scale.FILL)
            .precision(Precision.INEXACT)
            .apply {
                if (diskOnly) {
                    memoryCachePolicy(CachePolicy.DISABLED)
                    decoderFactory(BlackholeDecoder.Factory())
                }
            }
            .build()
}

/** A [BackdropPrefetcher] for this window's backdrop, on the app's image loader. */
@Composable
fun rememberBackdropPrefetcher(): BackdropPrefetcher {
    val context = LocalPlatformContext.current
    val loader = LocalArtworkImageLoader.current ?: SingletonImageLoader.get(context)
    val tier = LocalTvTier.current
    val size = LocalWindowInfo.current.containerSize
    val gate = LocalKeyRepeatGate.current
    return remember(context, loader, tier, size, gate) { BackdropPrefetcher(context, loader, tier, size, gate) }
}

/** Subjects sit right of centre and above the middle. */
private val SubjectAlignment = BiasAlignment(0.4f, -0.6f)

/**
 * The backdrop's two gradients in one pass over it: from the left, bg at
 * 100 % (x = 0), 90 % (300), 80 % at the text's end (520), then a long,
 * gentle fall to 20 % at the screen's edge (960), so no edge shows; from
 * the top, as [scrim] says.
 */
private fun Modifier.backdropScrims(scrim: BackdropScrim): Modifier = drawWithCache {
    // Stretched pictures, not gradient shaders (GradientBitmaps): over the whole
    // screen a shader cost about 13 ms a frame each on a TV's GPU.
    val leftEnd = minOf(960.dp.toPx(), size.width)
    val from = scrim.from.toPx()
    val to = scrim.to.toPx()
    onDrawWithContent {
        drawContent()
        drawStretched(LeftScrim, Offset.Zero, Size(leftEnd, size.height))
        drawStretched(BottomScrim, Offset(0f, from), Size(size.width, to - from))
        if (to < size.height) drawRect(PleyColors.Bg, topLeft = Offset(0f, to), size = Size(size.width, size.height - to))
    }
}

/**
 * From the left, over 960 dp: bg at 100 % (x = 0), 90 % (300), 80 % at the
 * text's end (520), then a long, gentle fall to a light 20 % at 960.
 */
private val LeftScrim by lazy {
    GradientBitmaps.horizontal(
        0f to PleyColors.Bg,
        300f / 960f to PleyColors.Bg.copy(alpha = 0.9f),
        520f / 960f to PleyColors.Bg.copy(alpha = 0.8f),
        620f / 960f to PleyColors.Bg.copy(alpha = 0.68f),
        720f / 960f to PleyColors.Bg.copy(alpha = 0.53f),
        820f / 960f to PleyColors.Bg.copy(alpha = 0.38f),
        1f to PleyColors.Bg.copy(alpha = 0.2f),
    )
}

/** From the top of its band: bg at 0 % to 100 %. */
private val BottomScrim by lazy { GradientBitmaps.vertical(0f to Color.Transparent, 1f to PleyColors.Bg) }

/**
 * A side panel (design spec §2.7; Subtitles, Audio, Quality, Info): 360 dp
 * from the right edge, full height, [surface2][PleyColors.Surface2] with
 * 16 dp left corners, over a [scrim][PleyColors.Scrim]. It slides in over
 * 200 ms, eased out as the Samsung app's panels slide in, and out eased
 * in, the scrim with it (in place with reduced motion); it takes focus
 * ([initialFocus] on the row to start from), and Back closes it
 * ([onDismiss]). A key held from before it came acts nowhere in it
 * ([heldFromBefore]): OK held opens a tile's actions, and lets go of them.
 */
@Composable
fun TvSidePanel(
    visible: Boolean,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    val reduced = reducedMotion()
    BackHandler(enabled = visible, onBack = onDismiss)
    OverlayShowing(visible)
    val ms = if (reduced) 0 else PleyMotion.OVERLAY_MS
    Box(Modifier.fillMaxSize()) {
        AnimatedVisibility(
            visible,
            enter = fadeIn(tween(ms, easing = PleyMotion.EaseOut)),
            exit = fadeOut(tween(ms, easing = PleyMotion.EaseIn)),
        ) {
            Box(Modifier.fillMaxSize().background(PleyColors.Scrim))
        }
        AnimatedVisibility(
            visible,
            modifier = Modifier.align(Alignment.CenterEnd),
            enter = slideInHorizontally(tween(ms, easing = PleyMotion.EaseOut)) { it },
            exit = slideOutHorizontally(tween(ms, easing = PleyMotion.EaseIn)) { it },
        ) {
            // The keys pressed while it shows, each time it comes.
            val pressedHere = remember { HashSet<Int>() }
            Surface(
                modifier = modifier
                    .width(SidePanelWidth)
                    .fillMaxHeight()
                    .onPreviewKeyEvent { event -> pressedHere.heldFromBefore(event) }
                    .focusGroup(),
                shape = RoundedCornerShape(topStart = PleyShape.DialogRadius, bottomStart = PleyShape.DialogRadius),
                colors = SurfaceDefaults.colors(containerColor = PleyColors.Surface2, contentColor = PleyColors.Fg),
            ) {
                Column(
                    Modifier
                        .fillMaxSize()
                        .topHairline()
                        .padding(start = 24.dp, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical, bottom = TvSafeArea.Vertical),
                    content = content,
                )
            }
        }
    }
}

/**
 * Whether [event] is of a key that went down before the panel came, which
 * acts nowhere in it, its repeats and its release alike: OK held on a tile
 * opens its actions at the long press, and the key, still held, went on to
 * choose the first of them. A key pressed while the panel shows (these keys)
 * goes on as it does anywhere.
 */
private fun MutableSet<Int>.heldFromBefore(event: KeyEvent): Boolean {
    val code = event.nativeKeyEvent.keyCode
    return when (event.type) {
        KeyEventType.KeyDown -> if (event.nativeKeyEvent.repeatCount == 0) {
            add(code)
            false
        } else {
            code !in this
        }
        KeyEventType.KeyUp -> !remove(code)
        else -> false
    }
}

/** Level 2's 1 dp [surface4][PleyColors.Surface4] line along the top. */
private fun Modifier.topHairline(): Modifier = drawWithCache {
    val h = PleyElevation.Level2Hairline.toPx()
    onDrawWithContent {
        drawContent()
        drawRect(PleyColors.Surface4, size = androidx.compose.ui.geometry.Size(size.width, h))
    }
}

private val SidePanelWidth = 360.dp
