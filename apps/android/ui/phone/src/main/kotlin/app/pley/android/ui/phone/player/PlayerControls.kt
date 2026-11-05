package app.pley.android.ui.phone.player

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsIgnoringVisibility
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.PlaybackMethod
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyIconButton

/** Heights of the scrim's darker ends, behind the top and bottom controls (design spec §3.5). */
private val TopScrim = 110.dp
private val BottomScrim = 150.dp

/** How much higher subtitles go while the controls show: the seek bar and the row under it. */
internal val ControlsLift = 120.dp

/**
 * What the player keeps its controls clear of: the camera cutout, and the
 * system bars where they are, shown or not. The player hides the bars,
 * but they go a moment after its first frame, and a swipe brings them
 * back over it for a while: padding by the bars as they show would move
 * every control as they come and go.
 */
@OptIn(ExperimentalLayoutApi::class)
internal val WindowInsets.Companion.player: WindowInsets
    @Composable get() = systemBarsIgnoringVisibility.union(displayCutout)

/**
 * The scrim over the picture while the controls show, as streaming apps'
 * players dim it: black 40 % all over, so the glyphs in the middle read
 * on any picture without a disc behind them, darkening to 60 % behind the
 * top row and 80 % behind the bottom one. Drawn under the subtitles. It
 * fades by drawing its gradient at its alpha (read while drawing), never
 * as a layer over the video.
 */
@Composable
internal fun Scrims(visible: Boolean) {
    val alpha = animateFloatAsState(if (visible) 1f else 0f, PleyMotion.reveal(), label = "scrims")
    Spacer(
        Modifier
            .fillMaxSize()
            .drawWithCache {
                val top = (TopScrim.toPx() / size.height).coerceIn(0f, 0.5f)
                val bottom = (1f - BottomScrim.toPx() / size.height).coerceIn(0.5f, 1f)
                val scrim = Brush.verticalGradient(
                    0f to Color.Black.copy(alpha = 0.6f),
                    top to Color.Black.copy(alpha = MID_DIM),
                    bottom to Color.Black.copy(alpha = MID_DIM),
                    1f to Color.Black.copy(alpha = 0.8f),
                )
                onDrawBehind {
                    val a = alpha.value
                    if (a > 0f) drawRect(scrim, alpha = a)
                }
            },
    )
}

/** How much the picture dims behind the controls between the scrim's darker ends. */
private const val MID_DIM = 0.4f

/** Back (large, as the hybrid player's), the title and the episode, how it plays, then picture in picture and Settings. */
@Composable
internal fun TopBar(state: PlayerScreenState, host: PlayerHost, modifier: Modifier = Modifier) {
    PlayerProbe.composed?.invoke("top")
    Row(
        modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.player)
            .padding(start = 8.dp, end = 12.dp, top = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BackButton(onClick = host::back)
        Column(
            Modifier
                .weight(1f)
                .clip(PleyShape.Card)
                // The title is also a way back, for a thumb that misses the arrow.
                .clickable(onClickLabel = stringResource(R.string.player_back), onClick = host::back)
                .padding(horizontal = 8.dp, vertical = 4.dp),
        ) {
            Text(
                state.title,
                style = MaterialTheme.typography.titleMedium,
                color = PleyColors.Fg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            state.subtitle?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        state.method?.let {
            MethodBadge(it) { state.open(SheetPage.Details) }
            Spacer(Modifier.size(4.dp))
        }
        if (host.pipSupported) PleyIconButton(PleyIcons.PictureInPicture2, stringResource(R.string.player_pip), onClick = host::enterPip)
        PleyIconButton(PleyIcons.SlidersHorizontal, stringResource(R.string.player_settings), onClick = { state.open(SheetPage.Settings) })
    }
}

/** A 56 dp back arrow, 28 dp tall: easy to hit with a thumb in a corner. */
@Composable
private fun BackButton(onClick: () -> Unit) {
    val label = stringResource(R.string.player_back)
    Box(
        Modifier
            .size(56.dp)
            .clip(PleyShape.Circle)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Icon(PleyIcons.ArrowLeft, contentDescription = null, size = 28.dp)
    }
}

/**
 * Play/Pause, as streaming apps' players draw it: a glyph alone on the
 * dimmed picture. It steps aside for the spinner.
 */
@Composable
internal fun CentreControls(state: PlayerScreenState, spinning: Boolean, modifier: Modifier = Modifier) {
    PlayerProbe.composed?.invoke("centre")
    // The seek preview takes the middle while the seek bar is dragged.
    val scrubbing = state.scrubbing
    val playing = state.playing
    Box(modifier.size(PlayTarget).graphicsLayer { alpha = if (scrubbing || spinning) 0f else 1f }) {
        GlyphButton(stringResource(if (playing) R.string.player_pause else R.string.player_play), PlayTarget, onClick = state::togglePlay) {
            Icon(if (playing) PleyIcons.Pause else PleyIcons.Play, contentDescription = null, size = PlayGlyph)
        }
    }
}

/** The centre's touch target and glyph: 80 dp around 52. */
private val PlayTarget = 80.dp
private val PlayGlyph = 52.dp

/** A [size] round touch target, read out as [label], around its [glyph]. */
@Composable
private fun GlyphButton(label: String, size: Dp, onClick: () -> Unit, glyph: @Composable () -> Unit) {
    Box(
        Modifier
            .size(size)
            .clip(PleyShape.Circle)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        glyph()
    }
}

/**
 * The seek bar with its times, then Subtitles · Audio · Quality · Fit ·
 * Chapters · Lock · Next episode, spread across the width as far as they
 * fit and scrolling when they don't.
 */
@Composable
internal fun BottomBar(state: PlayerScreenState, picture: PictureState, screenAspect: () -> Float, modifier: Modifier = Modifier) {
    PlayerProbe.composed?.invoke("bottom")
    Column(
        modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.player)
            .padding(start = 16.dp, end = 16.dp, bottom = 4.dp),
    ) {
        TimesAndSeekBar(state)
        BottomButtons(state, picture, screenAspect)
    }
}

/**
 * The bottom row's buttons, once the server has said how the item plays.
 * What they offer arrives in steps as the player opens (the item's
 * details, then the server's answer), and each step would move every
 * button along the row. The row keeps its height meanwhile, so the seek
 * bar above it stays where it is.
 */
@Composable
private fun BottomButtons(state: PlayerScreenState, picture: PictureState, screenAspect: () -> Float) {
    Box(Modifier.fillMaxWidth().heightIn(min = BarItemHeight)) {
        AnimatedVisibility(state.answered, enter = fadeIn(PleyMotion.reveal()), exit = fadeOut(PleyMotion.reveal())) {
            BoxWithConstraints(Modifier.fillMaxWidth()) {
                Row(
                    Modifier
                        .horizontalScroll(rememberScrollState())
                        .widthIn(min = maxWidth),
                    horizontalArrangement = Arrangement.SpaceEvenly,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (state.subtitles.isNotEmpty()) {
                        BarItem(PleyIcons.Captions, stringResource(R.string.player_subtitles)) { state.open(SheetPage.Subtitles) }
                    }
                    if (state.audio.size > 1) {
                        BarItem(PleyIcons.AudioLines, stringResource(R.string.player_audio)) { state.open(SheetPage.Audio) }
                    }
                    if (state.qualities.isNotEmpty()) {
                        BarItem(PleyIcons.Gauge, stringResource(R.string.player_quality)) { state.open(SheetPage.Quality) }
                    }
                    FitItem(state, picture, screenAspect)
                    if (state.chapters.size > 1) {
                        BarItem(PleyIcons.ListOrdered, stringResource(R.string.player_chapters)) { state.open(SheetPage.Chapters) }
                    }
                    BarItem(PleyIcons.Lock, stringResource(R.string.player_lock), onClick = state::lock)
                    if (state.episode) {
                        BarItem(PleyIcons.SkipForward, stringResource(R.string.player_next_episode)) {
                            state.touched()
                            state.controller.playNext()
                        }
                    }
                }
            }
        }
    }
}

/** Fit, Fill or Zoom: the button names what it does next, as the hybrid player's did. */
@Composable
private fun FitItem(state: PlayerScreenState, picture: PictureState, screenAspect: () -> Float) {
    // Read again when the mode or the video's shape changes.
    val next by remember(picture) { derivedStateOf { picture.nextMode(screenAspect()) } }
    val (label, hint) = when (next) {
        FitMode.Fill -> R.string.player_fill to R.string.player_fill_hint
        FitMode.Zoom -> R.string.player_zoom to R.string.player_zoom_hint
        else -> R.string.player_fit to R.string.player_fit_hint
    }
    BarItem(if (next == FitMode.Fit) PleyIcons.Minimize else PleyIcons.Maximize, stringResource(label), description = stringResource(hint)) {
        state.touched()
        state.modeShown = picture.step(screenAspect())
    }
}

/** An icon and a labelMedium label, 48 dp tall. */
@Composable
internal fun BarItem(icon: PleyIcon, label: String, description: String? = null, onClick: () -> Unit) {
    Row(
        Modifier
            .heightIn(min = BarItemHeight)
            .clip(PleyShape.Pill)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = ripple(color = PleyColors.Fg),
                role = Role.Button,
                onClick = onClick,
            )
            .then(if (description != null) Modifier.semantics { contentDescription = description } else Modifier)
            .padding(horizontal = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, size = 20.dp)
        Text(label, style = MaterialTheme.typography.labelMedium, color = PleyColors.Fg, maxLines = 1)
    }
}

/** The bottom row's buttons' height, and the row's. */
private val BarItemHeight = 48.dp

/** The badge's colours for [method], the session's name for how it plays, or for [type] where the details know it. */
@Composable
@ReadOnlyComposable
internal fun methodColors(method: String, type: PlaybackMethod? = null): BadgeColors =
    PlayerLook.badgeColors(type ?: PlayerLook.methodOf(LocalResources.current, method))

/** "Direct play", "Direct stream" or "Transcoding" in its colour; it opens Playback details. */
@Composable
internal fun MethodBadge(method: String, onClick: () -> Unit) {
    val colors = methodColors(method)
    val description = stringResource(R.string.player_method_hint, method)
    Box(
        Modifier
            .heightIn(min = 48.dp)
            .clip(PleyShape.Badge)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            method,
            style = MaterialTheme.typography.labelSmall,
            color = colors.text,
            maxLines = 1,
            // On bg 80 %, as badges over pictures are, so bright video doesn't wash it out.
            modifier = Modifier
                .background(PleyColors.OnArt, PleyShape.Badge)
                .background(colors.fill, PleyShape.Badge)
                .padding(horizontal = 8.dp, vertical = 3.dp),
        )
    }
}

/** Fades [content] in and out with the controls, on its own small layer. */
@Composable
internal fun ControlsGroup(visible: Boolean, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    AnimatedVisibility(visible, modifier, enter = fadeIn(PleyMotion.overlay()), exit = fadeOut(PleyMotion.overlay())) {
        content()
    }
}
