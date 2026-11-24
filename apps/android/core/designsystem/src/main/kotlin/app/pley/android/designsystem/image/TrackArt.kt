package app.pley.android.designsystem.image

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import app.pley.android.designsystem.reducedMotion
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first

/**
 * The cover of what plays, as the track changes (Now Playing and the bar
 * of what plays on a phone, Now Playing on a TV): the cover before stays
 * until the next one has loaded, which then fades in over it, so the art
 * never blinks to an empty box between two tracks (tracks of one album
 * share a cover, and nothing changes at all). The first cover comes in as
 * any picture does. Without one, [title]'s placeholder shows when
 * [placeholder], with [mark]'s glyph. With reduced motion the next cover takes the place of
 * the last at once. [onCover] hears of each cover as its picture comes,
 * and of `null` when the cover now shown has none that will come (its
 * placeholder shows): what follows the cover doesn't keep the last one.
 */
@Composable
fun TrackArt(
    image: ArtworkImage?,
    title: String,
    modifier: Modifier = Modifier,
    placeholder: Boolean = true,
    mark: MarkGlyph = MarkGlyph.Music,
    onCover: (ArtworkImage?) -> Unit = {},
) {
    // The cover shown in full: under the one coming until that is in.
    var under by remember { mutableStateOf<ArtworkImage?>(null) }
    // A track without a cover shows its placeholder: the cover after it has none to fade in over.
    LaunchedEffect(image) { if (image == null) under = null }
    Box(modifier) {
        if (image == null) {
            if (placeholder) Artwork(null, title, Modifier.matchParentSize(), mark = mark)
            return@Box
        }
        val below = under.takeIf { it != image }
        // Keyed, from one place: the cover that was on top is the same picture underneath, not one loaded again.
        for (cover in listOfNotNull(below, image)) {
            key(cover) {
                TrackArtLayer(
                    cover,
                    title,
                    over = cover == image && below != null,
                    top = cover == image,
                    mark = mark,
                    onShown = { if (cover == image) under = cover },
                    onCover = onCover,
                )
            }
        }
    }
}

/**
 * One cover; [over] another, it is unseen until it has loaded and then fades in. [onCover] hears when its picture has
 * come, and, while it is the cover shown ([top]), of `null` when its picture won't.
 */
@Composable
private fun TrackArtLayer(image: ArtworkImage, title: String, over: Boolean, top: Boolean, mark: MarkGlyph, onShown: () -> Unit, onCover: (ArtworkImage?) -> Unit) {
    val load = remember { ArtworkLoad() }
    // Decided as it first shows: once it is the cover underneath, it stays as it is.
    val fades = remember { over }
    // The fade here is over the last cover whatever the picture came from, the memory included; Coil's own would add to it.
    val model = remember { if (fades) image.copy(fade = false) else image }
    val reduced = reducedMotion()
    // In, or never coming: its placeholder then shows, not the cover of the track before.
    val there = !fades || load.loaded || load.failed
    val alpha by animateFloatAsState(if (there) 1f else 0f, if (reduced) snap() else tween(COVER_FADE_MS), label = "cover")
    val shown by rememberUpdatedState(onShown)
    val came by rememberUpdatedState(onCover)
    val shownNow by rememberUpdatedState(top)
    Artwork(model, title, Modifier.fillMaxSize().graphicsLayer { this.alpha = alpha }, load = load, mark = mark)
    LaunchedEffect(Unit) {
        // As soon as the loader has it, before any fade: the memory cache holds it from then on.
        snapshotFlow { load.settled }.first { it }
        if (!load.failed) came(image) else if (shownNow) came(null)
    }
    LaunchedEffect(Unit) {
        snapshotFlow { load.loaded || load.failed }.first { it }
        if (fades && !reduced) delay(COVER_FADE_MS.toLong())
        shown()
    }
}

/** A cover fading in over the last one. */
private const val COVER_FADE_MS = 300
