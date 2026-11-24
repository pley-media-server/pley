package app.pley.android.ui.tv.music

import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.player.MusicPosition
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.initialFocus
import kotlinx.coroutines.delay

/**
 * A track's line in a TV list (an album's tracks, the queue): the number,
 * or the playing glyph in [accent][PleyColors.Accent] for the track
 * playing, the title, its own artist, the length. Focused it is an accent
 * pill without scale, as rows in lists are (design spec §5.2 rule 1).
 */
@Composable
internal fun TvTrackLine(
    number: String,
    title: String,
    subtitle: String?,
    duration: String,
    current: Boolean,
    playing: Boolean,
    highlighted: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val fill = when {
        focused -> PleyColors.Accent
        highlighted -> PleyColors.Accent.copy(alpha = 0.14f)
        current -> PleyColors.Fg.copy(alpha = 0.06f)
        else -> Color.Transparent
    }
    val fg = when {
        focused -> PleyColors.OnAccent
        current -> PleyColors.AccentBright
        else -> PleyColors.Fg
    }
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 44.dp)
            .background(fill, PleyShape.Field)
            .semantics { selected = current }
            .combinedClickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(28.dp), contentAlignment = Alignment.CenterEnd) {
            if (current) {
                Icon(if (playing) PleyIcons.AudioLines else PleyIcons.Play, contentDescription = null, size = 18.dp, tint = if (focused) PleyColors.OnAccent else PleyColors.Accent)
            } else {
                BasicText(number, style = PleyType.Tv.Caption.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Subtle, textAlign = TextAlign.End))
            }
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            BasicText(title, style = PleyType.Tv.Body.copy(color = fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (!subtitle.isNullOrEmpty()) {
                BasicText(subtitle, style = PleyType.Tv.Caption.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.width(12.dp))
        BasicText(duration, style = PleyType.Tv.Caption.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted))
    }
}

/**
 * The album art over the whole page at [alpha] on [bg][PleyColors.Bg],
 * unblurred (design spec §4.10, §6), fading into the page to the left and
 * at the bottom so text stays on solid ground.
 */
@Composable
internal fun FaintArt(art: ArtworkImage?, title: String, alpha: Float = 0.2f) {
    if (art == null) return
    Artwork(
        art,
        title,
        Modifier
            .fillMaxSize()
            .graphicsLayer { this.alpha = alpha }
            .drawWithContent {
                drawContent()
                drawRect(Brush.verticalGradient(0.4f to Color.Transparent, 1f to PleyColors.Bg))
            },
    )
}

/**
 * Where the track is, refreshed every [everyMs] while [playing]. Paused,
 * it is read until it has stood still for a moment (what is buffered goes
 * on growing after a seek), then no more until [key] changes: the track,
 * or a seek made from anywhere ([NowPlaying.seeks]). Only while the
 * page is started: the music plays on with the TV showing another app,
 * and nothing then reads where it is; back, it is read again at once.
 */
@Composable
internal fun rememberPosition(position: () -> MusicPosition, playing: Boolean, key: Any?, everyMs: Long = 500): State<MusicPosition> {
    val state = remember { mutableStateOf(position()) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(playing, key, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            state.value = position()
            while (playing) {
                delay(everyMs)
                state.value = position()
            }
            var still = 0
            while (still < SETTLED_READS) {
                delay(everyMs)
                val before = state.value
                state.value = position()
                still = if (state.value == before) still + 1 else 0
            }
        }
    }
    return state
}

/** A paused track's position has settled once this many readings in a row are the same. */
private const val SETTLED_READS = 3

/** A small art well for lists. */
@Composable
internal fun ArtWell(art: ArtworkImage?, title: String, size: androidx.compose.ui.unit.Dp, modifier: Modifier = Modifier) {
    Box(modifier.size(size).background(PleyColors.Surface2, PleyShape.Badge)) {
        Artwork(art, title, Modifier.matchParentSize())
    }
}

/** A page-sized message with one focused action (not found, failed, nothing playing). */
@Composable
internal fun MusicMessage(title: String, body: String, action: String, onAction: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(start = PleyTv.Gutter, top = 60.dp, end = TvSafeArea.Horizontal),
        verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(10.dp),
    ) {
        BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
        BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
        Spacer(Modifier.size(12.dp))
        TvPillButton(action, onClick = onAction, modifier = Modifier.initialFocus())
    }
}

/** A path from the server made absolute on [origin]. */
internal fun absolute(path: String, origin: String): String =
    if (path.startsWith("http://") || path.startsWith("https://")) path else origin.trimEnd('/') + "/" + path.trimStart('/')
