package app.pley.android.ui.tv.player

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Border
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.Text
import app.pley.android.common.Tier
import app.pley.android.common.asSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.GradientBitmaps
import app.pley.android.designsystem.image.drawStretched
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.PlayerUiState
import app.pley.android.player.UpNextUi
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.delay

/*
 * What shows over the video besides the controls (design spec §2.7): the
 * title as an item begins, Skip, Up Next, the seek badge, buffering, the
 * refresh-rate switch, errors and toasts. With the controls hidden, only
 * Skip's and Up Next's countdowns move, once a second (§6).
 */

/**
 * The title as each item begins (the first, the next episode, a queue's
 * next), as the native Samsung app shows it: the controls' top bar's title
 * and episode line, in its place, over a 120 dp band of black from 65 % to
 * clear, for [TvPlayerUi.BANNER_MS] while the controls are hidden. It fades
 * in and out as they do; showing them takes it at once
 * ([TvPlayerUi.banner]). The band fades by its alpha while drawing, as the
 * controls' scrim does, and the words on their own small layer.
 */
@Composable
internal fun BoxScope.TvTitleBanner(state: State<PlayerUiState>, ui: TvPlayerUi) {
    val alpha = animateFloatAsState(if (ui.banner != null) 1f else 0f, PleyMotion.overlay(reducedMotion()), label = "banner")
    val present by remember(ui) { derivedStateOf { ui.banner != null || alpha.value > 0f } }
    if (!present) return
    val title by remember(state) { derivedStateOf { state.value.title } }
    val line by remember(state) { derivedStateOf { TvPlayerText.episodeLine(state.value.subtitle) } }
    Box(
        Modifier
            .align(Alignment.TopStart)
            .fillMaxWidth()
            .height(BANNER_HEIGHT)
            .drawBehind { drawStretched(TitleBand, Offset.Zero, size, alpha = alpha.value) }
            .testTag(TAG_TITLE_BANNER),
    ) {
        Column(
            Modifier
                .padding(start = TvSafeArea.Horizontal, top = TvSafeArea.Vertical, end = 300.dp)
                .graphicsLayer { this.alpha = alpha.value },
        ) {
            BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
            line?.let {
                Spacer(Modifier.height(4.dp))
                BasicText(it, style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

private val BANNER_HEIGHT = 120.dp

/** The banner's band: black at the controls' scrim's top (65 %), to clear at its foot. One stretched picture (GradientBitmaps). */
private val TitleBand by lazy {
    GradientBitmaps.vertical(
        0f to Color.Black.copy(alpha = 0.65f),
        1f to Color.Black.copy(alpha = 0f),
    )
}

/**
 * Skip Intro / Skip Credits: a quiet button, its right edge at x = 912, its
 * bottom at y = 489 (or 355 with the controls shown). Dark and see-through
 * with a faint outline over the picture; with the focus, which the screen
 * gives it, only its outline firms up: no fill of the accent, no growing,
 * no countdown. It fades out over 150 ms when the marker ends. [visible]
 * says whether it shows.
 */
@Composable
internal fun BoxScope.TvSkipButton(
    state: State<PlayerUiState>,
    visible: Boolean,
    controlsShown: Boolean,
    ui: TvPlayerUi,
    onSkip: () -> Unit,
) {
    val label by remember(state) { derivedStateOf { state.value.skipLabel } }
    var lastLabel by remember { mutableStateOf("") }
    label?.let { lastLabel = it }
    AnimatedVisibility(
        visible = visible && label != null,
        modifier = Modifier
            .align(Alignment.BottomEnd)
            .padding(end = TvSafeArea.Horizontal, bottom = if (controlsShown) SKIP_BOTTOM_SHOWN else SKIP_BOTTOM_HIDDEN),
        enter = fadeIn(tween(SKIP_FADE_MS)),
        exit = fadeOut(tween(SKIP_FADE_MS)),
    ) {
        Button(
            onClick = onSkip,
            modifier = Modifier
                .heightIn(min = 40.dp)
                .testTag(TAG_SKIP)
                .focusRequester(ui.requester(PlayerFocus.Skip))
                .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.Skip else if (ui.focused == PlayerFocus.Skip) ui.focused = null }
                .focusProperties {
                    down = if (controlsShown) ui.requester(PlayerFocus.Scrubber) else FocusRequester.Cancel
                    left = FocusRequester.Cancel
                    right = FocusRequester.Cancel
                    up = FocusRequester.Cancel
                }
                .semantics { liveRegion = LiveRegionMode.Polite },
            shape = ButtonDefaults.shape(PleyShape.Control),
            colors = ButtonDefaults.colors(
                containerColor = SkipFill,
                contentColor = PleyColors.Fg.copy(alpha = 0.85f),
                focusedContainerColor = SkipFillFocused,
                focusedContentColor = PleyColors.Fg,
                pressedContainerColor = SkipFillFocused,
                pressedContentColor = PleyColors.Fg,
            ),
            scale = ButtonDefaults.scale(focusedScale = 1f, pressedScale = 1f),
            glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
            border = ButtonDefaults.border(
                border = Border(BorderStroke(1.dp, PleyColors.Fg.copy(alpha = 0.25f)), shape = PleyShape.Control),
                focusedBorder = Border(BorderStroke(2.dp, PleyColors.Fg.copy(alpha = 0.9f)), shape = PleyShape.Control),
                pressedBorder = Border(BorderStroke(2.dp, PleyColors.Fg), shape = PleyShape.Control),
            ),
            contentPadding = PaddingValues(horizontal = 18.dp, vertical = 2.dp),
        ) {
            Text(lastLabel, style = PleyType.Tv.Label)
            Spacer(Modifier.width(8.dp))
            Icon(PleyIcons.ChevronsRight, contentDescription = null, size = 18.dp, tint = LocalContentColor.current)
        }
    }
}

/**
 * Up Next (design spec §2.7): a 300 dp card, right edge at x = 912 and
 * bottom at y = 489, on surface2 at 96 %: the next one's still, "Up next ·
 * S2 · E6" and its title, then "Play now" with a ring filling over the
 * countdown (stepped once a second; "Next in 7" with reduced motion) and
 * Cancel.
 */
@Composable
internal fun BoxScope.TvUpNextCard(card: UpNextUi?, ui: TvPlayerUi, onPlay: () -> Unit, onCancel: () -> Unit) {
    var last by remember { mutableStateOf(card) }
    if (card != null) last = card
    val shown = last
    val reduced = reducedMotion()
    AnimatedVisibility(
        visible = card != null,
        modifier = Modifier
            .align(Alignment.BottomEnd)
            .padding(end = TvSafeArea.Horizontal, bottom = SKIP_BOTTOM_HIDDEN),
        enter = fadeIn(tween(if (reduced) 0 else SKIP_FADE_MS)),
        exit = fadeOut(tween(if (reduced) 0 else SKIP_FADE_MS)),
    ) {
        if (shown == null) return@AnimatedVisibility
        val tier = LocalTvTier.current
        Column(
            Modifier
                .width(UP_NEXT_WIDTH)
                .then(if (tier != Tier.T0) Modifier.shadow(16.dp, PleyShape.Dialog, ambientColor = Color.Black, spotColor = Color.Black) else Modifier)
                .clip(PleyShape.Dialog)
                .background(PleyColors.Surface2.copy(alpha = 0.96f))
                .testTag(TAG_UP_NEXT)
                .semantics { liveRegion = LiveRegionMode.Polite },
        ) {
            Artwork(
                shown.imageUrl?.let { ArtworkImage(listOf(it), maxWidth = UP_NEXT_IMAGE_WIDTH) },
                title = shown.title,
                modifier = Modifier.fillMaxWidth().height(UP_NEXT_IMAGE_HEIGHT),
            )
            Column(Modifier.padding(16.dp)) {
                BasicText(TvPlayerText.episodeLine(shown.heading) ?: shown.heading, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1)
                BasicText(shown.title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.height(16.dp))
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    PlayNowButton(shown.secondsLeft, reduced, ui, onPlay)
                    TvTextButton(
                        stringResource(R.string.tv_player_cancel),
                        onClick = onCancel,
                        modifier = Modifier
                            // As tall as Play now beside it.
                            .heightIn(min = 48.dp)
                            .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.UpNext }
                            .focusProperties {
                                up = FocusRequester.Cancel
                                down = FocusRequester.Cancel
                                right = FocusRequester.Cancel
                            },
                    )
                }
            }
        }
    }
}

/** "Play now", focused when the card shows, with the countdown's ring (or words) inside it. */
@Composable
private fun PlayNowButton(secondsLeft: Int?, reduced: Boolean, ui: TvPlayerUi, onPlay: () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val content = if (focused) PleyColors.OnAccent else PleyColors.Fg
    Button(
        onClick = onPlay,
        interactionSource = interactions,
        modifier = Modifier
            .heightIn(min = 48.dp)
            .testTag(TAG_PLAY_NOW)
            .focusRequester(ui.requester(PlayerFocus.UpNext))
            .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.UpNext }
            .focusProperties {
                up = FocusRequester.Cancel
                down = FocusRequester.Cancel
                left = FocusRequester.Cancel
            },
        shape = ButtonDefaults.shape(PleyShape.Control),
        colors = ButtonDefaults.colors(
            containerColor = PleyColors.Fg.copy(alpha = 0.12f),
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
        glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
        border = ButtonDefaults.border(
            border = Border(BorderStroke(1.dp, PleyColors.Control), shape = PleyShape.Control),
            focusedBorder = Border.None,
            pressedBorder = Border.None,
        ),
        contentPadding = PaddingValues(start = 20.dp, end = 16.dp, top = 4.dp, bottom = 4.dp),
    ) {
        Icon(PleyIcons.Play, contentDescription = null, size = 22.dp, tint = LocalContentColor.current)
        Spacer(Modifier.width(10.dp))
        Text(stringResource(R.string.tv_player_play_now), style = PleyType.Tv.Label)
        if (secondsLeft != null) {
            Spacer(Modifier.width(12.dp))
            if (reduced) {
                Text(stringResource(R.string.tv_player_next_in, secondsLeft), style = PleyType.Tv.Caption)
            } else {
                CountdownRing(secondsLeft, content)
            }
        }
    }
}

/** A 22 dp ring filling over the 10 s countdown, redrawn as the seconds change. */
@Composable
private fun CountdownRing(secondsLeft: Int, color: Color) {
    val stroke = 3.dp
    Box(
        Modifier
            .size(22.dp)
            .drawBehind {
                val w = stroke.toPx()
                val inset = w / 2f
                val arcSize = Size(size.width - w, size.height - w)
                drawArc(color.copy(alpha = 0.25f), 0f, 360f, false, Offset(inset, inset), arcSize, style = Stroke(w))
                val done = ((COUNTDOWN_S - secondsLeft).toFloat() / COUNTDOWN_S).coerceIn(0f, 1f)
                drawArc(color, -90f, 360f * done, false, Offset(inset, inset), arcSize, style = Stroke(w, cap = StrokeCap.Round))
            },
    )
}

/**
 * The seek badge with the controls hidden (design spec §2.7): a 64 dp `bg`
 * 60 % circle with "−10 s" or "+30 s", centred in the left or right third.
 */
@Composable
internal fun BoxScope.TvSeekBadge(deltaMs: Long?) {
    val delta = deltaMs ?: return
    if (delta == 0L) return
    val seconds = TvPlayerText.seconds(delta)
    val text = stringResource(if (delta < 0) R.string.tv_player_seek_back else R.string.tv_player_seek_forward, seconds)
    Box(
        Modifier
            .align(if (delta < 0) BiasCentreLeftThird else BiasCentreRightThird)
            .size(64.dp)
            .background(PleyColors.Bg.copy(alpha = 0.6f), PleyShape.Control)
            .testTag(TAG_BADGE),
        contentAlignment = Alignment.Center,
    ) {
        BasicText(text, style = PleyType.Tv.Label.copy(color = PleyColors.Fg, textAlign = TextAlign.Center), maxLines = 1)
    }
}

private val BiasCentreLeftThird = androidx.compose.ui.BiasAlignment(-2f / 3f, 0f)
private val BiasCentreRightThird = androidx.compose.ui.BiasAlignment(2f / 3f, 0f)

/**
 * Buffering (design spec §2.7): after 500 ms of waiting, a 40 dp ring, a
 * 2 dp fg stroke with a turning accent arc, centred. The one thing that
 * moves with the controls hidden besides the countdowns, and only while
 * playback waits; 2.5 s while the last picture stands in for a stream made
 * ready beside it ([handoff]). While the server is out of reach ([reconnecting]),
 * "Reconnecting…" 16 dp under it, on the seek badge's backing: the
 * picture behind it can be anything.
 */
@Composable
internal fun BoxScope.TvBufferingRing(waiting: Boolean, reconnecting: Boolean, handoff: Boolean = false) {
    var show by remember { mutableStateOf(false) }
    LaunchedEffect(waiting, handoff) {
        show = false
        if (waiting) {
            delay(if (handoff) PlayerUiState.HANDOFF_SPINNER_DELAY_MS else BUFFERING_DELAY_MS)
            show = true
        }
    }
    if (!show) return
    val label = stringResource(R.string.tv_player_buffering)
    val turn = rememberInfiniteTransition(label = "buffering")
    val angle by turn.animateFloat(0f, 360f, infiniteRepeatable(tween(1_000, easing = LinearEasing), RepeatMode.Restart), label = "angle")
    Box(
        Modifier
            .align(Alignment.Center)
            .size(RING_SIZE)
            .testTag(TAG_BUFFERING)
            .semantics { contentDescription = label }
            .drawBehind {
                val w = 2.dp.toPx()
                val inset = w / 2f
                val arcSize = Size(size.width - w, size.height - w)
                drawArc(PleyColors.Fg, 0f, 360f, false, Offset(inset, inset), arcSize, style = Stroke(w))
                drawArc(PleyColors.Accent, angle - 90f, 90f, false, Offset(inset, inset), arcSize, style = Stroke(w, cap = StrokeCap.Round))
            },
    )
    if (reconnecting) {
        BasicText(
            stringResource(R.string.tv_player_reconnecting),
            Modifier
                .align(Alignment.Center)
                .underRing(16.dp)
                .background(PleyColors.Bg.copy(alpha = 0.6f), PleyShape.Control)
                .padding(horizontal = 12.dp, vertical = 6.dp)
                .testTag(TAG_RECONNECTING)
                .semantics { liveRegion = LiveRegionMode.Polite },
            style = PleyType.Tv.Label.copy(color = PleyColors.Fg),
            maxLines = 1,
        )
    }
}

private val RING_SIZE = 40.dp

/** Lays the label [gap] under the ring, centred on the screen as the ring is: the ring stays where it is. */
private fun Modifier.underRing(gap: Dp): Modifier = layout { measurable, constraints ->
    val label = measurable.measure(constraints)
    layout(label.width, label.height) { label.place(0, (RING_SIZE / 2 + gap).roundToPx() + label.height / 2) }
}

/**
 * While the TV changes its display mode, or the playback its stream
 * (design spec §2.7): black, under the buffering ring, and nothing said,
 * as streaming apps' players do: the TV goes dark for a moment anyway.
 */
@Composable
internal fun BoxScope.TvSwitchingCover() {
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black)
            .testTag(TAG_SWITCHING),
    )
}

/**
 * A playback that stopped (design spec §8): "Can't play this item", the
 * reason, "Try again" (focused) when it may help, and Back, centred on
 * black.
 */
@Composable
internal fun BoxScope.TvPlayerError(
    message: String,
    canRetry: Boolean,
    ui: TvPlayerUi,
    onRetry: () -> Unit,
    onBack: () -> Unit,
    title: String = stringResource(R.string.tv_player_error_title),
) {
    Column(
        Modifier
            .fillMaxSize()
            .background(Color.Black)
            .padding(horizontal = 160.dp)
            .testTag(TAG_ERROR),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        BasicText(title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg, textAlign = TextAlign.Center))
        Spacer(Modifier.height(12.dp))
        BasicText(
            asSentence(message),
            style = PleyType.Tv.Body.copy(color = PleyColors.Muted, textAlign = TextAlign.Center),
            maxLines = 4,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(32.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            val first = Modifier
                .focusRequester(ui.requester(PlayerFocus.Error))
                .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.Error }
            if (canRetry) {
                TvPillButton(stringResource(R.string.tv_player_try_again), onClick = onRetry, modifier = first, icon = PleyIcons.RefreshCw)
                TvTextButton(stringResource(R.string.tv_player_back), onClick = onBack, modifier = Modifier.onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.Error })
            } else {
                TvPillButton(stringResource(R.string.tv_player_back), onClick = onBack, modifier = first, icon = PleyIcons.ArrowLeft)
            }
        }
    }
}

/**
 * A toast at the top right (design spec §2.10): surface3, label fg (errors
 * in danger text), one at a time; it goes after its time. Below the clock
 * while the controls show.
 */
@Composable
internal fun BoxScope.TvPlayerToast(toast: PlayerToast?, controlsShown: Boolean, onGone: (Int) -> Unit) {
    val t = toast ?: return
    LaunchedEffect(t.serial) {
        delay(t.ms)
        onGone(t.serial)
    }
    Box(
        Modifier
            .align(Alignment.TopEnd)
            .padding(end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical + if (controlsShown) 52.dp else 0.dp)
            .widthIn(max = 420.dp)
            .background(PleyColors.Surface3, PleyShape.Control)
            .padding(horizontal = 20.dp, vertical = 10.dp)
            .testTag(TAG_TOAST)
            .semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        BasicText(t.text, style = PleyType.Tv.Label.copy(color = if (t.error) PleyColors.DangerText else PleyColors.Fg), maxLines = 3)
    }
}

/** The video's own focus holder while the controls are hidden: Left, Right, Centre and the rest arrive through it. */
internal fun Modifier.videoFocus(ui: TvPlayerUi, enabled: Boolean, label: String): Modifier = this
    .focusRequester(ui.requester(PlayerFocus.Video))
    .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.Video else if (ui.focused == PlayerFocus.Video) ui.focused = null }
    .focusProperties { canFocus = enabled }
    .semantics { contentDescription = label }
    .focusable()

private const val SKIP_FADE_MS = 150
private const val COUNTDOWN_S = 10
private const val BUFFERING_DELAY_MS = 500L
private const val UP_NEXT_IMAGE_WIDTH = 640
private val UP_NEXT_WIDTH = 300.dp
private val UP_NEXT_IMAGE_HEIGHT = 169.dp

/** Skip's fill: the picture shows through, as Skip asks for little of the eye. */
private val SkipFill = Color.Black.copy(alpha = 0.45f)
private val SkipFillFocused = Color.Black.copy(alpha = 0.6f)

/** Skip's bottom edge, from the bottom of the 540 dp canvas: y = 489 hidden, 355 with the controls. */
private val SKIP_BOTTOM_HIDDEN = (540 - 489).dp
private val SKIP_BOTTOM_SHOWN = (540 - 355).dp

internal const val TAG_TITLE_BANNER = "pley.tv.player.titleBanner"
internal const val TAG_SKIP = "pley.tv.player.skip"
internal const val TAG_UP_NEXT = "pley.tv.player.upNext"
internal const val TAG_PLAY_NOW = "pley.tv.player.playNow"
internal const val TAG_BADGE = "pley.tv.player.badge"
internal const val TAG_BUFFERING = "pley.tv.player.buffering"
internal const val TAG_RECONNECTING = "pley.tv.player.reconnecting"
internal const val TAG_SWITCHING = "pley.tv.player.switching"
internal const val TAG_ERROR = "pley.tv.player.error"
internal const val TAG_TOAST = "pley.tv.player.toast"
