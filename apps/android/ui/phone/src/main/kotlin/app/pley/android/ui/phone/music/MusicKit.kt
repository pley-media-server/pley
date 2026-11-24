package app.pley.android.ui.phone.music

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.TrackWords
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.MusicControl
import app.pley.android.player.MusicPosition
import app.pley.android.player.NowPlaying
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.music.AlbumRow
import app.pley.android.ui.phone.R
import kotlinx.coroutines.delay

/**
 * Where the track is, refreshed every [everyMs] while [playing]. Paused,
 * it is read until it has stood still for a moment (what is buffered goes
 * on growing after a seek), then no more until [key] changes: the track,
 * or a seek made from anywhere ([trackKey]). Read it while drawing (a
 * progress line), or in a small composable of its own (the times), so the
 * rest of the screen doesn't recompose with it.
 * [MusicPositionState.refresh] reads it at once, after a seek made here:
 * the scrubber is where it was sent, not where the last reading had it.
 * Only while the screen is started: music plays on with the app out of
 * sight or the phone locked, and nothing then reads where it is; back in
 * view, it is read again at once.
 */
@Composable
internal fun rememberMusicPosition(position: () -> MusicPosition, playing: Boolean, key: Any?, everyMs: Long = POSITION_EVERY_MS): MusicPositionState {
    val state = remember { MusicPositionState(position) }
    state.read = position
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(playing, key, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
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
 * What plays, in words: the track and "Artist · Album"; a book's part with
 * chapters is named for the chapter playing (its own name, else "Chapter
 * 3"), the book under it, and its words change as the next chapter starts.
 */
@Composable
internal fun trackWords(now: NowPlaying): TrackWords {
    val chapter = now.currentChapter ?: return TrackWords(now.itemId to now.index, now.index, now.title, now.byline)
    val name = chapter.title?.trim()?.takeIf { it.isNotEmpty() } ?: stringResource(ScreensR.string.book_chapter, now.chapter + 1)
    return TrackWords(Triple(now.itemId, now.index, now.chapter), now.index * CHAPTERS_PER_PART + now.chapter, name, now.album ?: now.title)
}

/** More than a file has (the server sends 1,000 at most): a chapter's place in the queue orders as a part's does. */
private const val CHAPTERS_PER_PART = 10_000

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

/**
 * A sheet's choices ([MusicChoiceRow]), scrolling where the window is too
 * short for them all: a car's small screen, or a phone on its side, shows
 * only some.
 */
@Composable
internal fun MusicChoices(content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), content = content)
}

/**
 * One choice in a sheet of Now Playing's (a book's speed, the sleep timer):
 * its label, and the accent check at the end when it is the one in force.
 * Screen readers hear a radio button.
 */
@Composable
internal fun MusicChoiceRow(label: String, chosen: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .selectable(selected = chosen, role = Role.RadioButton, onClick = onClick)
            .padding(horizontal = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, modifier = Modifier.weight(1f))
        if (chosen) Icon(PleyIcons.Check, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
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
