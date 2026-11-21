package app.pley.android.ui.phone.music

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkLoad
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.MusicControl
import app.pley.android.player.MusicPosition
import app.pley.android.player.NowPlaying
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.music.AlbumRow
import app.pley.android.ui.phone.R
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first

/**
 * Where the track is, refreshed every [everyMs] while [playing]. Paused,
 * it is read until it has stood still for a moment (what is buffered goes
 * on growing after a seek), then no more until [key] changes: the track,
 * or a seek made from anywhere ([trackKey]). Read it while drawing (a
 * progress line), or in a small composable of its own (the times), so the
 * rest of the screen doesn't recompose with it.
 * [MusicPositionState.refresh] reads it at once, after a seek made here:
 * the scrubber is where it was sent, not where the last reading had it.
 */
@Composable
internal fun rememberMusicPosition(position: () -> MusicPosition, playing: Boolean, key: Any?, everyMs: Long = POSITION_EVERY_MS): MusicPositionState {
    val state = remember { MusicPositionState(position) }
    state.read = position
    LaunchedEffect(playing, key) {
        state.refresh()
        while (playing) {
            delay(everyMs)
            state.refresh()
        }
        var still = 0
        while (still < SETTLED_READS) {
            delay(everyMs)
            val before = state.value
            state.refresh()
            still = if (state.value == before) still + 1 else 0
        }
    }
    return state
}

/** What [rememberMusicPosition] reads the position again for: another track of the queue, or a seek. */
internal fun trackKey(now: NowPlaying): Any = Triple(now.itemId, now.index, now.seeks)

/** A paused track's position has settled once this many readings in a row are the same. */
private const val SETTLED_READS = 3

/** The track's position as last read ([rememberMusicPosition]). */
@Stable
internal class MusicPositionState(internal var read: () -> MusicPosition) : State<MusicPosition> {
    private val last = mutableStateOf(read())

    override val value: MusicPosition get() = last.value

    fun refresh() {
        last.value = read()
    }
}

/** How often a playing track's position is read. */
internal const val POSITION_EVERY_MS = 500L

/**
 * The cover of what plays, as the track changes: the cover before stays
 * until the next one has loaded, which then fades in over it, so the art
 * never blinks to an empty box between two tracks (tracks of one album
 * share a cover, and nothing changes at all). The first cover comes in as
 * any picture does. Without one, [title]'s placeholder shows when
 * [placeholder]. With reduced motion the next cover takes the place of
 * the last at once.
 */
@Composable
internal fun TrackArt(image: ArtworkImage?, title: String, modifier: Modifier = Modifier, placeholder: Boolean = true) {
    // The cover shown in full: under the one coming until that is in.
    var under by remember { mutableStateOf<ArtworkImage?>(null) }
    // A track without a cover shows its placeholder: the cover after it has none to fade in over.
    LaunchedEffect(image) { if (image == null) under = null }
    Box(modifier) {
        if (image == null) {
            if (placeholder) Artwork(null, title, Modifier.matchParentSize())
            return@Box
        }
        val below = under.takeIf { it != image }
        // Keyed, from one place: the cover that was on top is the same picture underneath, not one loaded again.
        for (cover in listOfNotNull(below, image)) {
            key(cover) { TrackArtLayer(cover, title, over = cover == image && below != null, onShown = { if (cover == image) under = cover }) }
        }
    }
}

/** One cover; [over] another, it is unseen until it has loaded and then fades in. */
@Composable
private fun TrackArtLayer(image: ArtworkImage, title: String, over: Boolean, onShown: () -> Unit) {
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
    Artwork(model, title, Modifier.fillMaxSize().graphicsLayer { this.alpha = alpha }, load = load)
    LaunchedEffect(Unit) {
        snapshotFlow { load.loaded || load.failed }.first { it }
        if (fades && !reduced) delay(COVER_FADE_MS.toLong())
        shown()
    }
}

/** A cover fading in over the last one. */
private const val COVER_FADE_MS = 300

/** The words that change with the track: [key] says which track of the queue they are, [index] where it is. */
internal data class TrackWords(val key: Any, val index: Int, val title: String, val byline: String)

internal fun trackWords(now: NowPlaying): TrackWords = TrackWords(now.itemId to now.index, now.index, now.title, now.byline)

/**
 * [words] drawn by [content], changing with the track: the next track's
 * come in from the side it is on in the queue (from the right going on,
 * from the left going back) as the last one's fade out, a short way, and
 * the block's height follows. With reduced motion they change in place.
 */
@Composable
internal fun TrackChange(words: TrackWords, modifier: Modifier = Modifier, alignment: Alignment = Alignment.TopStart, content: @Composable (TrackWords) -> Unit) {
    val reduced = reducedMotion()
    AnimatedContent(
        targetState = words,
        modifier = modifier,
        contentKey = { it.key },
        contentAlignment = alignment,
        transitionSpec = {
            if (reduced) {
                ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
            } else {
                val forward = targetState.index >= initialState.index
                val enter = fadeIn(tween(TRACK_IN_MS, delayMillis = TRACK_OUT_MS / 2)) +
                    slideInHorizontally(tween(TRACK_IN_MS, easing = PleyMotion.EaseOut)) { width -> (if (forward) width else -width) / TRACK_SLIDE }
                val exit = fadeOut(tween(TRACK_OUT_MS)) +
                    slideOutHorizontally(tween(TRACK_OUT_MS, easing = PleyMotion.EaseIn)) { width -> (if (forward) -width else width) / TRACK_SLIDE }
                (enter togetherWith exit).using(SizeTransform(clip = false))
            }
        },
        label = "track",
    ) { shown -> content(shown) }
}

private const val TRACK_IN_MS = 260
private const val TRACK_OUT_MS = 140

/** The words move this share of their width. */
private const val TRACK_SLIDE = 10

/** Play or Pause's glyph, one giving way to the other in place. */
@Composable
internal fun PlayPauseGlyph(playing: Boolean, size: Dp, tint: Color) {
    val reduced = reducedMotion()
    AnimatedContent(
        targetState = playing,
        contentAlignment = Alignment.Center,
        transitionSpec = {
            if (reduced) {
                ContentTransform(EnterTransition.None, ExitTransition.None, sizeTransform = null)
            } else {
                (fadeIn(tween(GLYPH_MS)) + scaleIn(tween(GLYPH_MS), initialScale = 0.7f)) togetherWith (fadeOut(tween(GLYPH_MS)) + scaleOut(tween(GLYPH_MS), targetScale = 0.7f))
            }
        },
        label = "play",
    ) { pause -> Icon(if (pause) PleyIcons.Pause else PleyIcons.Play, contentDescription = null, size = size, tint = tint) }
}

private const val GLYPH_MS = 120

/** The app's music, for the mini player; `null` in tests whose Application holds no graph. */
@Composable
internal fun rememberMusicControl(): MusicControl? {
    val app = LocalContext.current.applicationContext
    return remember(app) { (app as? PleyGraphOwner)?.screensGraph?.player?.music }
}

/**
 * A line of an album's tracks (MusicViews.tsx TrackRow): the number, or
 * the playing glyph in [accent][PleyColors.Accent] for the track playing;
 * the title, its own artist when it isn't the album's, and the length.
 * The track playing reads in accent; the one the album was opened from is
 * tinted.
 */
@Composable
internal fun TrackLine(
    row: AlbumRow.Track,
    current: Boolean,
    playing: Boolean,
    highlighted: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tint = when {
        current -> PleyColors.Fg.copy(alpha = 0.05f)
        highlighted -> PleyColors.Accent.copy(alpha = 0.12f)
        else -> Color.Transparent
    }
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clip(PleyShape.Field)
            .background(tint)
            .semantics { selected = current }
            .clickable(role = Role.Button, onClickLabel = null, onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(32.dp), contentAlignment = Alignment.CenterEnd) {
            if (current) {
                Icon(if (playing) PleyIcons.AudioLines else PleyIcons.Play, contentDescription = null, size = 18.dp, tint = PleyColors.Accent)
            } else {
                Text(row.number, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle, textAlign = TextAlign.End)
            }
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(
                row.title,
                style = MaterialTheme.typography.titleMedium,
                color = if (current) PleyColors.AccentBright else PleyColors.Fg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (row.artist != null) {
                Text(row.artist!!, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.width(12.dp))
        Text(row.duration, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
    }
}

/** "Disc 2", over its tracks. */
@Composable
internal fun DiscHeader(number: Int, modifier: Modifier = Modifier) {
    Text(
        androidx.compose.ui.res.stringResource(R.string.music_disc, number),
        style = MaterialTheme.typography.labelLarge,
        color = PleyColors.Subtle,
        modifier = modifier.padding(start = 8.dp, top = 16.dp, bottom = 4.dp),
    )
}

/** A path from the server made absolute on [origin]. */
internal fun absolute(path: String, origin: String): String =
    if (path.startsWith("http://") || path.startsWith("https://")) path else origin.trimEnd('/') + "/" + path.trimStart('/')
