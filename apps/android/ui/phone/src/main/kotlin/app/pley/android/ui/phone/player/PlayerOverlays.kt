package app.pley.android.ui.phone.player

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.pley.android.common.asSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.UpNextUi
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import kotlinx.coroutines.delay

/** How far Skip and Up Next go up while the controls show, clear of the seek bar and the row under it. */
private val AboveControls = 112.dp

/**
 * The spinner, after [PlayerScreenState.SPINNER_DELAY_MS] of waiting, so
 * short waits never flash one: a 48 dp ring in accent, centred. While the
 * server is out of reach ([reconnecting]), "Reconnecting…" 16 dp under it,
 * backed like the fit mode's label.
 */
@Composable
internal fun BoxScope.Spinner(spinning: Boolean, reconnecting: Boolean = false) {
    if (!spinning) return
    val label = stringResource(R.string.player_loading)
    CircularProgressIndicator(
        modifier = Modifier.align(Alignment.Center).size(SPINNER_SIZE).semantics { contentDescription = label },
        color = PleyColors.Accent,
        trackColor = PleyColors.Fg.copy(alpha = 0.2f),
        strokeWidth = 3.dp,
    )
    if (reconnecting) {
        Text(
            stringResource(R.string.player_reconnecting),
            style = MaterialTheme.typography.labelLarge,
            color = PleyColors.Fg,
            maxLines = 1,
            modifier = Modifier
                .align(Alignment.Center)
                .underSpinner(16.dp)
                .background(Color.Black.copy(alpha = 0.6f), PleyShape.Control)
                .padding(horizontal = 12.dp, vertical = 6.dp)
                .testTag(TAG_RECONNECTING)
                .semantics { liveRegion = LiveRegionMode.Polite },
        )
    }
}

private val SPINNER_SIZE = 48.dp

/** Lays the label [gap] under the spinner, centred as the spinner is: the spinner stays where it is. */
private fun Modifier.underSpinner(gap: Dp): Modifier = layout { measurable, constraints ->
    val label = measurable.measure(constraints)
    layout(label.width, label.height) { label.place(0, (SPINNER_SIZE / 2 + gap).roundToPx() + label.height / 2) }
}

/** The tag of "Reconnecting…", for tests. */
internal const val TAG_RECONNECTING = "phone-player-reconnecting"

/**
 * Whether the spinner shows: waiting for longer than 400 ms, or 2.5 s while
 * the last picture stands in for a stream made ready beside it (a quality
 * or track change, [PlayerScreenState.handoff]).
 */
@Composable
internal fun rememberSpinning(state: PlayerScreenState): Boolean {
    val waiting = state.waiting
    val handoff = state.handoff
    val shown by produceState(false, waiting, handoff) {
        if (!waiting) {
            value = false
            return@produceState
        }
        delay(if (handoff) PlayerUiState.HANDOFF_SPINNER_DELAY_MS else PlayerScreenState.SPINNER_DELAY_MS)
        value = true
    }
    return shown && waiting
}

/**
 * Skip Intro or Skip Credits (design spec §3.5): a quiet pill at the bottom
 * right, dark and see-through with a faint outline, so it asks little of
 * the eye over the picture. Shown even with the controls hidden, above the
 * seek bar while they show. It steps aside for Up Next and while the seek
 * bar is dragged, and is announced when it appears.
 */
@Composable
internal fun BoxScope.SkipButton(state: PlayerScreenState) {
    PlayerProbe.composed?.invoke("skip")
    val label = state.skipLabel
    val shown = label != null && state.upNext == null && !state.scrubbing && !state.locked && state.sheet == null
    val lift by animateDpAsState(if (state.controlsVisible) AboveControls else 0.dp, PleyMotion.overlay(reducedMotion()), label = "skip")
    var last by remember { mutableStateOf(label) }
    if (label != null) last = label
    AnimatedVisibility(
        shown,
        Modifier
            .align(Alignment.BottomEnd)
            .windowInsetsPadding(WindowInsets.player)
            .padding(end = 24.dp, bottom = 24.dp)
            .offset { IntOffset(0, -lift.roundToPx()) },
        enter = fadeIn(PleyMotion.reveal()),
        exit = fadeOut(PleyMotion.reveal()),
    ) {
        Row(
            Modifier
                // As tall as the player's other buttons (Unlock, the bar's), for a thumb.
                .heightIn(min = 48.dp)
                .clip(PleyShape.Pill)
                .background(Color.Black.copy(alpha = 0.45f))
                .border(1.dp, PleyColors.Fg.copy(alpha = 0.25f), PleyShape.Pill)
                .clickable(role = Role.Button) {
                    state.touched()
                    state.controller.skip()
                }
                .semantics { liveRegion = LiveRegionMode.Polite }
                .padding(horizontal = 18.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(last.orEmpty(), style = MaterialTheme.typography.labelMedium, color = PleyColors.Fg.copy(alpha = 0.9f))
            Icon(PleyIcons.ChevronsRight, contentDescription = null, size = 16.dp, tint = PleyColors.Fg.copy(alpha = 0.9f))
        }
    }
}

/**
 * What plays next, from the credits on: its still, "Up next · S1 E4", the
 * title, Play now with the seconds left (a fill sweeping across it once a
 * second) and Cancel. It is announced once, not every second. While the
 * controls show, it goes up above them without its still, so on a phone on
 * its side it stays on screen and clear of the top bar.
 */
@Composable
internal fun BoxScope.UpNextCard(state: PlayerScreenState) {
    val card = state.upNext
    val lift by animateDpAsState(if (state.controlsVisible) AboveControls else 0.dp, PleyMotion.overlay(reducedMotion()), label = "upNext")
    var last by remember { mutableStateOf(card) }
    if (card != null) last = card
    AnimatedVisibility(
        card != null && !state.locked && state.sheet == null,
        Modifier
            .align(Alignment.BottomEnd)
            .windowInsetsPadding(WindowInsets.player)
            .padding(end = 24.dp, bottom = 24.dp)
            .offset { IntOffset(0, -lift.roundToPx()) },
        enter = fadeIn(PleyMotion.reveal()),
        exit = fadeOut(PleyMotion.reveal()),
    ) {
        last?.let { UpNextContent(it, state) }
    }
}

@Composable
private fun UpNextContent(card: UpNextUi, state: PlayerScreenState) {
    PlayerProbe.composed?.invoke("upNext")
    val reduced = reducedMotion()
    Column(
        Modifier
            .width(280.dp)
            .clip(PleyShape.Dialog)
            .background(PleyColors.Surface2.copy(alpha = 0.96f))
            .padding(12.dp),
    ) {
        AnimatedVisibility(
            !state.controlsVisible,
            enter = if (reduced) EnterTransition.None else fadeIn(PleyMotion.overlay()) + expandVertically(PleyMotion.overlay()),
            exit = if (reduced) ExitTransition.None else fadeOut(PleyMotion.overlay()) + shrinkVertically(PleyMotion.overlay()),
        ) {
            Box(Modifier.fillMaxWidth().padding(bottom = 10.dp).aspectRatio(16f / 9f).clip(PleyShape.Card)) {
                Artwork(card.imageUrl?.let { ArtworkImage(listOf(it)) }, card.title, Modifier.fillMaxSize())
            }
        }
        Column(Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }) {
            Text(card.heading, style = MaterialTheme.typography.labelMedium, color = PleyColors.AccentBright, maxLines = 1)
            Text(
                card.title,
                style = MaterialTheme.typography.titleMedium,
                color = PleyColors.Fg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.semantics { heading() },
            )
        }
        Row(Modifier.padding(top = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            val left = card.secondsLeft
            val label = when {
                left == null -> stringResource(R.string.player_play_now)
                reduced -> stringResource(R.string.player_next_in, left)
                else -> stringResource(R.string.player_play_now_in, left)
            }
            val total = UP_NEXT_SECONDS.toFloat()
            val done = if (left == null) 0f else ((total - left) / total).coerceIn(0f, 1f)
            Box(Modifier.weight(1f)) {
                PrimaryButton(
                    label,
                    onClick = {
                        state.touched()
                        state.controller.playNext()
                    },
                    icon = PleyIcons.Play,
                    modifier = Modifier
                        .fillMaxWidth()
                        // The countdown: a darker sweep across the button, stepped once a
                        // second. Over it, not behind: the button's own fill would hide it.
                        .drawWithContent {
                            drawContent()
                            if (done > 0f) {
                                drawRoundRect(
                                    Color.Black.copy(alpha = 0.18f),
                                    topLeft = Offset.Zero,
                                    size = Size(size.width * done, size.height),
                                    cornerRadius = CornerRadius(PleyShape.ControlRadius.toPx()),
                                )
                            }
                        },
                )
            }
            Spacer(Modifier.width(4.dp))
            PleyTextButton(stringResource(R.string.player_cancel), onClick = {
                state.touched()
                state.controller.cancelUpNext()
            })
        }
    }
}

/** The countdown's length, as the session counts it. */
private const val UP_NEXT_SECONDS = 10

/** A run of double-taps: "+20 s" in a circle on its side, gone 0.7 s after the last tap. */
@Composable
internal fun BoxScope.SeekRunBadge(state: PlayerScreenState) {
    val run = state.seekRun ?: return
    LaunchedEffect(run) {
        delay(SEEK_RUN_MS)
        if (state.seekRun === run) state.seekRun = null
    }
    val seconds = (run.totalMs / 1000).toInt()
    val text = stringResource(if (run.side > 0) R.string.player_seek_forward else R.string.player_seek_back, seconds)
    Box(
        Modifier
            .align(if (run.side > 0) Alignment.CenterEnd else Alignment.CenterStart)
            .fillMaxHeight()
            .fillMaxWidth(1f / 3f)
            .drawBehind {
                // A soft half-moon on the side tapped.
                val r = size.height * 0.75f
                val cx = if (run.side > 0) size.width + r * 0.35f else -r * 0.35f
                drawCircle(Color.White.copy(alpha = 0.10f), r, Offset(cx, size.height / 2f))
            },
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(if (run.side > 0) PleyIcons.RotateCw else PleyIcons.RotateCcw, contentDescription = null, size = 28.dp)
            Text(text, style = MaterialTheme.typography.labelLarge, color = PleyColors.Fg, modifier = Modifier.padding(top = 6.dp))
        }
    }
}

/** The volume or brightness while a drag sets it: an icon over a 4 dp bar, on the side dragged. */
@Composable
internal fun BoxScope.LevelIndicator(state: PlayerScreenState) {
    PlayerProbe.composed?.invoke("level")
    // Which level shows is read here; how far, only while drawing, so a drag moves the bar without recomposing.
    val shown by remember(state) { derivedStateOf { state.levelDrag?.volume } }
    val volume = shown ?: return
    val label = stringResource(if (volume) R.string.player_volume else R.string.player_brightness)
    Column(
        Modifier
            .align(if (volume) Alignment.CenterEnd else Alignment.CenterStart)
            .windowInsetsPadding(WindowInsets.player)
            .padding(horizontal = 40.dp)
            .clip(PleyShape.Pill)
            .background(Color.Black.copy(alpha = 0.6f))
            .padding(horizontal = 14.dp, vertical = 16.dp)
            .semantics { contentDescription = label },
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(if (volume) PleyIcons.Volume2 else PleyIcons.Sun, contentDescription = null, size = 20.dp)
        Box(
            Modifier
                .padding(top = 12.dp)
                .size(4.dp, 120.dp)
                .drawBehind {
                    val r = CornerRadius(size.width / 2f)
                    drawRoundRect(PleyColors.Fg.copy(alpha = 0.25f), cornerRadius = r)
                    val h = size.height * state.levelDrag?.level.let { it ?: 0f }
                    drawRoundRect(PleyColors.Accent, topLeft = Offset(0f, size.height - h), size = Size(size.width, h), cornerRadius = r)
                },
        )
    }
}

/** "Fill", "Fit", "Zoom" for a moment after the picture's mode changes. */
@Composable
internal fun BoxScope.ModeLabel(state: PlayerScreenState) {
    val mode = state.modeShown ?: return
    LaunchedEffect(mode) {
        delay(MODE_LABEL_MS)
        state.modeShown = null
    }
    val text = stringResource(
        when (mode) {
            FitMode.Fit -> R.string.player_fit
            FitMode.Fill -> R.string.player_fill
            FitMode.Zoom -> R.string.player_zoom
            FitMode.Free -> R.string.player_free
        },
    )
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        color = PleyColors.Fg,
        modifier = Modifier
            .align(Alignment.TopCenter)
            .windowInsetsPadding(WindowInsets.player)
            .padding(top = 72.dp)
            .background(Color.Black.copy(alpha = 0.6f), PleyShape.Pill)
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .semantics { liveRegion = LiveRegionMode.Polite },
    )
}

/** While locked, a tap shows this at the top for 3 s; it unlocks. */
@Composable
internal fun BoxScope.UnlockButton(state: PlayerScreenState) {
    if (!state.locked || !state.unlockShown) return
    LaunchedEffect(state.unlockShown) {
        delay(PlayerScreenState.HIDE_AFTER_MS)
        state.unlockShown = false
    }
    Row(
        Modifier
            // At the top, clear of subtitles.
            .align(Alignment.TopCenter)
            .windowInsetsPadding(WindowInsets.player)
            .padding(top = 24.dp)
            .heightIn(min = 48.dp)
            .clip(PleyShape.Pill)
            .background(PleyColors.Surface3.copy(alpha = 0.92f))
            .clickable(role = Role.Button, onClick = state::unlock)
            .padding(horizontal = 20.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(PleyIcons.LockOpen, contentDescription = null, size = 20.dp)
        Text(stringResource(R.string.player_unlock), style = MaterialTheme.typography.labelLarge, color = PleyColors.Fg)
    }
}

/**
 * A playback that stopped (design spec §8): "Can't play this item", the
 * error's own sentence, Try again when it may help, and Back, on black.
 */
@Composable
internal fun ErrorOverlay(error: PlayerPhase.Error, onRetry: () -> Unit, onBack: () -> Unit, title: String = stringResource(R.string.player_error_title)) {
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black)
            // Touches stop here.
            .clickable(enabled = false) {},
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier.widthIn(max = 480.dp).padding(24.dp).semantics { liveRegion = LiveRegionMode.Assertive },
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(if (error.offline) PleyIcons.ServerOff else PleyIcons.TriangleAlert, contentDescription = null, size = 40.dp, tint = PleyColors.Muted)
            Text(
                title,
                style = MaterialTheme.typography.titleLarge,
                color = PleyColors.Fg,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 16.dp).semantics { heading() },
            )
            Text(
                asSentence(error.message),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 8.dp),
            )
            Row(Modifier.padding(top = 24.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                if (error.canRetry) PrimaryButton(stringResource(R.string.player_try_again), onClick = onRetry, icon = PleyIcons.RefreshCw)
                PleyTextButton(stringResource(R.string.player_back), onClick = onBack)
            }
        }
    }
}

/** How long "Fill" shows after a change. */
private const val MODE_LABEL_MS = 1_200L
