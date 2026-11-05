package app.pley.android.ui.tv.livetv

import android.view.KeyEvent
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import app.pley.android.model.LiveCategory
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.rememberTextFieldState
import app.pley.android.ui.tv.search.TvSearchField
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.nativeKeyCode
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.Surface
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.BusyConnection
import app.pley.android.model.LiveChannel
import app.pley.android.player.engine.VideoSurface
import app.pley.android.player.live.LivePhase
import app.pley.android.player.live.LiveState
import app.pley.android.player.live.LiveTrouble
import app.pley.android.screens.livetv.ChannelDigits
import app.pley.android.screens.livetv.LivePlayerViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.initialFocus
import kotlinx.coroutines.delay

/** What shows over the picture. */
private enum class Overlay { None, Controls, Channels }

/**
 * The TV's live player (the web's LivePlayerPage, for a remote), the
 * channel full screen. With nothing over it:
 *
 * - Up and Down, Channel up and down, and Previous and Next change
 *   channel (Up the next number), digits tune a number;
 * - OK opens the channel list, as Menu and Guide do; held, it shows the
 *   controls (the channel list, the favourite) over what's on; Back hides
 *   what shows, then leaves.
 *
 * Live TV is watched as it airs, as the web's: no pause, no going back,
 * so play, pause, rewind and fast forward, Left and Right do nothing.
 * What's on and next shows for 5 s when asked for (Info), never on its
 * own as a channel tunes, with Live. A channel that doesn't start says why
 * (the server's sentence), with Try again and Other channels.
 */
@Composable
internal fun TvLivePlayerScreen(vm: LivePlayerViewModel, onExit: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    val engine by vm.engine.collectAsStateWithLifecycle()
    val channel by vm.channel.collectAsStateWithLifecycle()
    val origin by vm.origin.collectAsStateWithLifecycle()
    val now by vm.now.collectAsStateWithLifecycle()
    val digits by vm.digits.collectAsStateWithLifecycle()
    var overlay by remember { mutableStateOf(Overlay.None) }
    // What's on shows when asked for (Info), never on its own as a channel tunes.
    var peek by remember { mutableIntStateOf(0) }
    var poked by remember { mutableIntStateOf(0) }
    var showInfo by remember { mutableStateOf(false) }
    // OK went down with nothing over the picture: its release opens the channel list, unless it was held.
    var okPressed by remember { mutableStateOf(false) }
    // OK held showed the controls: the rest of that press is the picture's.
    var okHeld by remember { mutableStateOf(false) }
    val root = remember { FocusRequester() }
    val playing = state.phase == LivePhase.Playing
    val failed = state.phase is LivePhase.Failed

    // Asked for, what's on shows for a while; another channel puts it away.
    LaunchedEffect(state.channelId) { showInfo = false }
    LaunchedEffect(peek) {
        if (peek == 0) return@LaunchedEffect
        showInfo = true
        snapshotFlow { state.phase == LivePhase.Playing }.first { it }
        delay(INFO_MS)
        showInfo = false
    }
    // The controls hide when left alone while it plays.
    LaunchedEffect(overlay, poked, playing) {
        if (overlay == Overlay.Controls && playing) {
            delay(CONTROLS_MS)
            overlay = Overlay.None
        }
    }
    // Focus goes back to the picture when nothing shows over it.
    LaunchedEffect(overlay, failed) {
        if (overlay == Overlay.None && !failed) runCatching { root.requestFocus() }
    }
    BackHandler(enabled = overlay != Overlay.None) { overlay = Overlay.None }
    // The channel list opens on the category zapped through.
    LaunchedEffect(overlay == Overlay.Channels) { if (overlay == Overlay.Channels) vm.openList() }

    Box(
        Modifier
            .fillMaxSize()
            // A window of its own: UI Automator (the benchmarks' journeys) finds its tags as resource ids.
            .semantics { testTagsAsResourceId = true }
            .background(Color.Black)
            .testTag(TV_LIVE_PLAYER)
            .onPreviewKeyEvent { event ->
                val code = event.key.nativeKeyCode
                val down = event.type == KeyEventType.KeyDown
                // Or its release would press the button the controls focused.
                if (okHeld && code in OK_KEYS) {
                    if (!down) okHeld = false
                    return@onPreviewKeyEvent true
                }
                if (overlay == Overlay.Controls) poked++
                when {
                    code == KeyEvent.KEYCODE_CHANNEL_UP || code == KeyEvent.KEYCODE_MEDIA_NEXT -> {
                        if (down) vm.step(1)
                        true
                    }
                    code == KeyEvent.KEYCODE_CHANNEL_DOWN || code == KeyEvent.KEYCODE_MEDIA_PREVIOUS -> {
                        if (down) vm.step(-1)
                        true
                    }
                    code in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 -> {
                        if (down) vm.digit('0' + (code - KeyEvent.KEYCODE_0))
                        true
                    }
                    code == KeyEvent.KEYCODE_GUIDE || code == KeyEvent.KEYCODE_MENU -> {
                        if (!down) overlay = if (overlay == Overlay.Channels) Overlay.None else Overlay.Channels
                        true
                    }
                    code == KeyEvent.KEYCODE_INFO -> {
                        if (!down) peek++
                        true
                    }
                    // No pause, no going back on live.
                    code in NOT_LIVE_KEYS -> true
                    overlay != Overlay.None || failed -> false
                    else -> when (code) {
                        KeyEvent.KEYCODE_DPAD_UP -> {
                            if (down) vm.step(1)
                            true
                        }
                        KeyEvent.KEYCODE_DPAD_DOWN -> {
                            if (down) vm.step(-1)
                            true
                        }
                        KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT -> true
                        // The channel list on the way up, as OK selects elsewhere; held, the controls, as a long press opens actions elsewhere.
                        in OK_KEYS -> {
                            if (down && event.nativeKeyEvent.repeatCount == 0) {
                                okPressed = true
                            } else if (down && okPressed) {
                                okPressed = false
                                okHeld = true
                                overlay = Overlay.Controls
                            } else if (!down && okPressed) {
                                okPressed = false
                                overlay = Overlay.Channels
                            }
                            true
                        }
                        else -> false
                    }
                }
            }
            .focusRequester(root)
            .focusable(),
    ) {
        VideoSurface(engine?.player, Modifier.fillMaxSize().testTag(TestTags.PLAYER_SURFACE))
        val shown = channel
        if (state.audioOnly && playing && shown != null) SoundOnly(shown, origin, now)
        when (val phase = state.phase) {
            // The channel not known yet (opened elsewhere than a page that had it): the spinner alone.
            LivePhase.Tuning, LivePhase.Idle -> if (shown != null) Tuning(vm, shown, origin, now) else Spinner(Modifier.align(Alignment.Center))
            is LivePhase.Failed -> Trouble(
                trouble = phase.trouble,
                channel = shown,
                origin = origin,
                onRetry = if (phase.trouble.canRetry) vm::retry else null,
                onTakeOver = vm::takeOver,
                onChannels = { overlay = Overlay.Channels },
                onBack = onExit,
            )
            LivePhase.Playing -> Unit
        }
        if (state.buffering && playing) Spinner(Modifier.align(Alignment.Center))
        // What's on, with the window and the controls when asked for.
        AnimatedVisibility(
            visible = shown != null && playing && overlay != Overlay.Channels && (overlay == Overlay.Controls || showInfo),
            enter = fadeIn() + slideInVertically { it / 3 },
            exit = fadeOut() + slideOutVertically { it / 3 },
            modifier = Modifier.align(Alignment.BottomCenter),
        ) {
            if (shown != null) {
                InfoBar(
                    channel = shown,
                    state = state,
                    origin = origin,
                    now = now,
                    controls = overlay == Overlay.Controls,
                    vm = vm,
                    onChannels = { overlay = Overlay.Channels },
                )
            }
        }
        Digits(digits, Modifier.align(Alignment.TopEnd))
        TvSidePanel(visible = overlay == Overlay.Channels, onDismiss = { overlay = Overlay.None }) {
            ChannelList(
                vm = vm,
                current = state.channelId,
                origin = origin,
                now = now,
                onTune = { channel, from ->
                    overlay = Overlay.None
                    vm.tune(channel.id, channel, from)
                },
            )
        }
    }
}

/** The channel's logo, name and what's on while it is tuned. */
@Composable
private fun Tuning(vm: LivePlayerViewModel, channel: LiveChannel, origin: String, now: Long) {
    val category by vm.category.collectAsStateWithLifecycle()
    Column(Modifier.fillMaxSize().background(Color.Black), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        ChannelTile(channel, origin, Modifier.size(width = 200.dp, height = 112.dp), name = true, style = PleyType.Tv.Title, well = Color.White.copy(alpha = 0.06f), shape = PleyShape.Dialog)
        Spacer(Modifier.height(22.dp))
        Spinner()
        Spacer(Modifier.height(18.dp))
        BasicText(listOfNotNull(channel.number, LiveTv.channelName(channel, category)).joinToString("  "), style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
        BasicText(
            channel.onAt(now)?.title ?: stringResource(R.string.tv_live_player_tuning),
            style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
            modifier = Modifier.padding(top = 6.dp),
        )
    }
}

/** A channel with sound only (radio): its logo where the picture would be, with what's on. */
@Composable
private fun SoundOnly(channel: LiveChannel, origin: String, now: Long) {
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        ChannelTile(channel, origin, Modifier.size(width = 240.dp, height = 135.dp), name = true, style = PleyType.Tv.Title, well = Color.White.copy(alpha = 0.06f), shape = PleyShape.Dialog)
        Spacer(Modifier.height(18.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(PleyIcons.Radio, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(8.dp))
            BasicText(stringResource(R.string.tv_live_player_radio), style = PleyType.Tv.Label.copy(color = PleyColors.Muted))
        }
        channel.onAt(now)?.let { BasicText(it.title, style = PleyType.Tv.Section.copy(color = PleyColors.Fg), modifier = Modifier.padding(top = 8.dp)) }
    }
}

/**
 * The bar at the foot: the channel's logo, number and name, what's on with
 * its times, how far it is and what's next; Live; and with [controls],
 * the buttons, the channel list first.
 */
@Composable
private fun InfoBar(
    channel: LiveChannel,
    state: LiveState,
    origin: String,
    now: Long,
    controls: Boolean,
    vm: LivePlayerViewModel,
    onChannels: () -> Unit,
) {
    val clock = rememberClock()
    val category by vm.category.collectAsStateWithLifecycle()
    val on = channel.onAt(now)
    val next = channel.nextAt(now)
    Column(
        Modifier
            .fillMaxWidth()
            .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.75f), Color.Black.copy(alpha = 0.92f))))
            .padding(start = TvSafeArea.Horizontal, end = TvSafeArea.Horizontal, top = 64.dp, bottom = TvSafeArea.Vertical)
            .testTag(TV_LIVE_PLAYER_INFO),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            ChannelTile(channel, origin, Modifier.size(width = 112.dp, height = 63.dp), name = true, well = Color.White.copy(alpha = 0.08f))
            Spacer(Modifier.width(18.dp))
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    channel.number?.let { BasicText(it, style = PleyType.Tv.Label.copy(color = PleyColors.Muted), modifier = Modifier.padding(end = 8.dp)) }
                    BasicText(LiveTv.channelName(channel, category), style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 1)
                    if (channel.favorite) Icon(PleyIcons.Star, contentDescription = null, size = 14.dp, tint = PleyColors.Accent, modifier = Modifier.padding(start = 6.dp))
                }
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 2.dp)) {
                    if (on != null) BadgeOf(on, Modifier.padding(end = 8.dp))
                    BasicText(
                        on?.title ?: stringResource(if (channel.hasGuide) R.string.tv_livetv_nothing_now else R.string.tv_livetv_no_guide),
                        style = PleyType.Tv.Section.copy(color = PleyColors.Fg),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (on != null) {
                    Row(Modifier.padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        BasicText("${spanOf(on, clock)} · ${timeLeft(on, now)}", style = PleyType.Tv.Caption.copy(color = PleyColors.Muted))
                        Spacer(Modifier.width(12.dp))
                        ProgramProgress(on, now, Modifier.width(180.dp), track = Color.White.copy(alpha = 0.18f))
                    }
                }
                if (next != null) {
                    BasicText(
                        stringResource(R.string.tv_livetv_next, clock(next.start), next.title),
                        style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                }
            }
            if (state.phase == LivePhase.Playing) LiveBadge()
        }
        if (controls) {
            Row(
                Modifier.padding(top = 16.dp).focusGroup(),
                horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap),
            ) {
                TvRoundButton(
                    stringResource(R.string.tv_live_player_channels),
                    PleyIcons.List,
                    onClick = onChannels,
                    modifier = Modifier.initialFocus().testTag(TV_LIVE_PLAYER_CHANNEL_LIST),
                )
                // The star alone, as in Live TV's details: its label, wider than both buttons, ran under Channels.
                TvRoundButton(
                    stringResource(if (channel.favorite) R.string.tv_livetv_favorite_remove else R.string.tv_livetv_favorite_add),
                    if (channel.favorite) PleyIcons.Star else PleyIcons.StarOutline,
                    onClick = vm::toggleFavorite,
                    showLabel = false,
                )
            }
        } else {
            BasicText(stringResource(R.string.tv_live_player_hint), style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle), modifier = Modifier.padding(top = 12.dp))
        }
    }
}

/** Live, red with a dot: a badge. */
@Composable
private fun LiveBadge() {
    Row(
        Modifier
            .clip(PleyShape.Circle)
            .background(PleyColors.Danger)
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .testTag(TV_LIVE_PLAYER_LIVE),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(7.dp).background(Color.White, PleyShape.Circle))
        Spacer(Modifier.width(7.dp))
        BasicText(stringResource(R.string.tv_live_player_live).uppercase(), style = PleyType.Tv.Badge.copy(color = Color.White))
    }
}

/** A channel number being typed on the remote, or the one that matched nothing. */
@Composable
private fun Digits(digits: ChannelDigits, modifier: Modifier = Modifier) {
    val text = digits.typed.ifEmpty { null } ?: digits.notFound?.let { stringResource(R.string.tv_live_player_no_channel, it) } ?: return
    BasicText(
        text,
        style = (if (digits.typed.isNotEmpty()) PleyType.Tv.Code else PleyType.Tv.Section).copy(color = PleyColors.Fg),
        modifier = modifier
            .padding(top = TvSafeArea.Vertical, end = TvSafeArea.Horizontal)
            .clip(PleyShape.Dialog)
            .background(Color.Black.copy(alpha = 0.85f))
            .padding(horizontal = 24.dp, vertical = 12.dp)
            .testTag(TV_LIVE_PLAYER_DIGITS),
    )
}

/**
 * Why the channel doesn't play, with Try again, Other channels and Back to
 * the guide. Every connection in use, what holds each (the web's
 * TroubleCard), with Watch here instead where it may be taken over
 * ([onTakeOver]), focused first.
 */
@Composable
private fun Trouble(
    trouble: LiveTrouble,
    channel: LiveChannel?,
    origin: String,
    onRetry: (() -> Unit)?,
    onTakeOver: (Long) -> Unit,
    onChannels: () -> Unit,
    onBack: () -> Unit,
) {
    val connections = trouble.connections
    val firstToTake = connections.firstOrNull { it.canTakeOver }
    val name = channel?.name.orEmpty()
    val title = when (trouble.kind) {
        LiveTrouble.Kind.Busy -> stringResource(R.string.tv_live_player_busy)
        LiveTrouble.Kind.Stopped -> stringResource(R.string.tv_live_player_stopped)
        LiveTrouble.Kind.Missing -> stringResource(R.string.tv_live_player_missing)
        LiveTrouble.Kind.NotAllowed -> stringResource(R.string.tv_live_player_not_allowed)
        LiveTrouble.Kind.Unavailable -> stringResource(R.string.tv_live_player_unavailable, name)
        LiveTrouble.Kind.Offline -> stringResource(R.string.tv_live_player_offline)
        LiveTrouble.Kind.Other -> stringResource(R.string.tv_live_player_failed, name)
    }
    Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.88f)), contentAlignment = Alignment.Center) {
        Column(
            Modifier
                .widthIn(max = 520.dp)
                .clip(PleyShape.Dialog)
                .background(PleyColors.Surface2)
                .border(1.dp, PleyColors.Surface4, PleyShape.Dialog)
                .padding(32.dp)
                .testTag(TV_LIVE_PLAYER_TROUBLE),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (channel != null) {
                ChannelTile(channel, origin, Modifier.size(width = 120.dp, height = 68.dp), name = true, well = Color.White.copy(alpha = 0.06f))
            } else {
                Icon(PleyIcons.TriangleAlert, contentDescription = null, size = 40.dp, tint = PleyColors.Accent)
            }
            Spacer(Modifier.height(18.dp))
            BasicText(title, style = PleyType.Tv.Section.copy(color = PleyColors.Fg, textAlign = TextAlign.Center))
            if (connections.isNotEmpty()) {
                BasicText(
                    pluralStringResource(R.plurals.tv_live_player_connections_watching, connections.size, connections.size),
                    style = PleyType.Tv.Body.copy(color = PleyColors.Muted, textAlign = TextAlign.Center),
                    modifier = Modifier.padding(top = 10.dp, bottom = 14.dp),
                )
                for (c in connections) BusyRow(c, onTakeOver, focusFirst = c == firstToTake)
                if (firstToTake == null) {
                    BasicText(
                        stringResource(R.string.tv_live_player_none_to_take),
                        style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle, textAlign = TextAlign.Center),
                        modifier = Modifier.padding(top = 6.dp),
                    )
                }
            } else if (trouble.message.isNotBlank()) {
                BasicText(
                    trouble.message,
                    style = PleyType.Tv.Body.copy(color = PleyColors.Muted, textAlign = TextAlign.Center),
                    modifier = Modifier.padding(top = 10.dp),
                )
            }
            Spacer(Modifier.height(24.dp))
            Row(Modifier.focusGroup(), horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap)) {
                if (onRetry != null) {
                    TvPillButton(
                        stringResource(if (trouble.kind == LiveTrouble.Kind.Stopped) R.string.tv_live_player_watch_again else R.string.tv_live_player_retry),
                        onClick = onRetry,
                        icon = PleyIcons.RefreshCw,
                        modifier = if (firstToTake == null) Modifier.initialFocus() else Modifier,
                    )
                }
                TvPillButton(
                    stringResource(R.string.tv_live_player_other_channels),
                    onClick = onChannels,
                    icon = PleyIcons.List,
                    modifier = if (onRetry == null && firstToTake == null) Modifier.initialFocus() else Modifier,
                )
                TvPillButton(stringResource(R.string.tv_live_player_back_to_guide), onClick = onBack)
            }
        }
    }
}

/** A connection in use: its channel, who holds it, and Watch here instead where it may be taken. */
@Composable
private fun BusyRow(connection: BusyConnection, onTakeOver: (Long) -> Unit, focusFirst: Boolean) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(bottom = 8.dp)
            .clip(PleyShape.Control)
            .background(Color.White.copy(alpha = 0.04f))
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .testTag(TV_LIVE_PLAYER_BUSY),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            BasicText(connection.channelName, style = PleyType.Tv.Label.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicText(holders(connection), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (connection.canTakeOver) {
            Spacer(Modifier.width(12.dp))
            TvPillButton(
                stringResource(R.string.tv_live_player_take_over),
                onClick = { onTakeOver(connection.channelId) },
                modifier = if (focusFirst) Modifier.initialFocus() else Modifier,
            )
        }
    }
}

/** Who holds a connection: "You, on another device", their names (to administrators), or how many. */
@Composable
private fun holders(c: BusyConnection): String = when {
    c.yours -> stringResource(R.string.tv_live_player_held_by_you)
    c.watchers.isNotEmpty() -> c.watchers.joinToString(", ")
    else -> pluralStringResource(R.plurals.tv_live_player_held_by_people, c.people.coerceAtLeast(1), c.people.coerceAtLeast(1))
}

/** A spinning ring in the accent, while a channel tunes or waits for data. */
@Composable
private fun Spinner(modifier: Modifier = Modifier) {
    val turn = rememberInfiniteTransition(label = "spinner")
    val angle by turn.animateFloat(0f, 360f, infiniteRepeatable(tween(900, easing = LinearEasing)), label = "angle")
    Canvas(modifier.size(36.dp)) {
        val stroke = 3.dp.toPx()
        drawArc(PleyColors.Accent, startAngle = angle, sweepAngle = 270f, useCenter = false, style = Stroke(stroke, cap = StrokeCap.Round))
    }
}

/**
 * A category's channels in the side panel (the web's ChannelPanel), a page
 * at a time as they scroll: what's on each, the channel watched first in
 * focus. It opens on the category zapped through; its heading, Up from
 * the first channel, opens the categories to list another's ([onTune] says
 * which it was chosen in).
 */
@Composable
private fun ColumnScope.ChannelList(vm: LivePlayerViewModel, current: Long?, origin: String, now: Long, onTune: (LiveChannel, LiveCategory) -> Unit) {
    val shown by vm.listCategory.collectAsStateWithLifecycle()
    val listed by vm.listed.collectAsStateWithLifecycle()
    val groups by vm.groups.collectAsStateWithLifecycle()
    val search = rememberTextFieldState()
    LaunchedEffect(search) { snapshotFlow { search.text.toString() }.collect(vm::searchList) }
    var picking by remember { mutableStateOf(false) }
    // Back from the groups, the row that had focus is gone: the channels take it.
    var picked by remember { mutableStateOf(false) }
    val selected = remember { FocusRequester() }
    BackHandler(enabled = picking) { picking = false }
    Surface(
        onClick = {
            picked = true
            picking = !picking
        },
        modifier = Modifier.fillMaxWidth().testTag(TV_LIVE_PLAYER_GROUP),
        shape = ClickableSurfaceDefaults.shape(PleyShape.Control),
        colors = ClickableSurfaceDefaults.colors(
            containerColor = Color.Transparent,
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ClickableSurfaceDefaults.scale(focusedScale = 1f, pressedScale = 1f),
        glow = ClickableSurfaceDefaults.glow(Glow.None, Glow.None, Glow.None),
    ) {
        val content = androidx.tv.material3.LocalContentColor.current
        Row(Modifier.padding(horizontal = 8.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            if (picking) {
                Icon(PleyIcons.ArrowLeft, contentDescription = null, size = 20.dp, tint = content)
                Spacer(Modifier.width(10.dp))
            } else if (shown == LiveCategory.Favorites) {
                Icon(PleyIcons.Star, contentDescription = null, size = 18.dp, tint = content)
                Spacer(Modifier.width(8.dp))
            }
            BasicText(
                if (picking) stringResource(R.string.tv_live_player_groups) else categoryLabel(shown),
                style = PleyType.Tv.Title.copy(color = content),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (!picking) {
                Spacer(Modifier.width(8.dp))
                Icon(PleyIcons.ChevronDown, contentDescription = stringResource(R.string.tv_live_player_change_group), size = 18.dp, tint = content)
            }
        }
    }
    Spacer(Modifier.height(8.dp))
    if (picking) {
        CategoryColumn(
            groups = groups,
            selected = shown,
            selectedRequester = selected,
            onSelect = {
                search.clearText()
                vm.showList(it)
                picking = false
            },
            modifier = Modifier.fillMaxWidth().weight(1f),
        )
        LaunchedEffect(Unit) { runCatching { selected.requestFocus() } }
    } else {
        TvSearchField(
            state = search,
            placeholder = stringResource(R.string.tv_live_player_search),
            onSearch = {},
            modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
            focusModifier = Modifier.testTag(TV_LIVE_PLAYER_SEARCH),
        )
        // Focus starts on the channel playing as the list opens, not each time a search lists others.
        val first = remember { mutableStateOf(true) }
        key(listed) {
            val initial = remember { first.value }
            SideEffect { first.value = false }
            CategoryChannels(vm, listed, current, origin, now, initial = initial, takeFocus = picked, onTookFocus = { picked = false }, onTune = onTune)
        }
    }
}

/**
 * [category]'s channels in the side panel, from the channel playing when
 * it is among them; with [takeFocus], that row (or the first) takes focus
 * once it is in.
 */
@Composable
private fun CategoryChannels(
    vm: LivePlayerViewModel,
    category: LiveCategory,
    current: Long?,
    origin: String,
    now: Long,
    initial: Boolean,
    takeFocus: Boolean,
    onTookFocus: () -> Unit,
    onTune: (LiveChannel, LiveCategory) -> Unit,
) {
    val pages by vm.list.collectAsStateWithLifecycle()
    val start by vm.listIndex.collectAsStateWithLifecycle()
    val list = rememberLazyListState()
    val index = start ?: 0
    val target = remember { FocusRequester() }
    LaunchedEffect(start != null) { list.scrollToItem((index - 2).coerceAtLeast(0)) }
    LaunchedEffect(takeFocus, index, pages[index] != null) {
        if (takeFocus && pages[index] != null) {
            list.scrollToItem((index - 2).coerceAtLeast(0))
            withFrameNanos { }
            if (runCatching { target.requestFocus() }.isSuccess) onTookFocus()
        }
    }
    LaunchedEffect(list) {
        snapshotFlow { list.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
            .distinctUntilChanged()
            .collect { (first, last) -> vm.showingListRows(first, last) }
    }
    if (pages.total == 0) {
        BasicText(stringResource(R.string.tv_livetv_no_match), style = PleyType.Tv.Label.copy(color = PleyColors.Subtle), modifier = Modifier.padding(8.dp))
    }
    LazyColumn(state = list, contentPadding = PaddingValues(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.heldKeyStaysIn(HeldEnds.UpDown).testTag(TV_LIVE_PLAYER_CHANNELS)) {
        items(count = pages.total ?: 0, key = { i -> pages[i]?.id ?: -i - 1L }, contentType = { i -> if (pages[i] == null) LOADING_ROW else CHANNEL_ROW }) { i ->
            val channel = pages[i]
            if (channel == null) {
                Box(Modifier.fillMaxWidth().height(52.dp).background(Color.White.copy(alpha = 0.04f), PleyShape.Control))
                return@items
            }
            ChannelRow(
                channel = channel,
                label = listOfNotNull(channel.number, LiveTv.channelName(channel, category)).joinToString("  "),
                // What's on, not the time: the clock's tick draws a row again only when its programme changes.
                on = channel.onAt(now)?.title,
                origin = origin,
                playing = channel.id == current,
                onClick = { onTune(channel, category) },
                modifier = if (i == index) Modifier.initialFocus(enabled = initial).focusRequester(target) else Modifier,
            )
        }
    }
}

/** A channel in the list: its logo, number and name, what's on, a star for a favourite; the one [playing] marked. */
@Composable
private fun ChannelRow(channel: LiveChannel, label: String, on: String?, origin: String, playing: Boolean, onClick: () -> Unit, modifier: Modifier) {
    Surface(
        onClick = onClick,
        modifier = modifier.fillMaxWidth(),
        shape = ClickableSurfaceDefaults.shape(PleyShape.Control),
        colors = ClickableSurfaceDefaults.colors(
            containerColor = if (playing) PleyColors.Surface3 else Color.Transparent,
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ClickableSurfaceDefaults.scale(focusedScale = 1f, pressedScale = 1f),
        glow = ClickableSurfaceDefaults.glow(Glow.None, Glow.None, Glow.None),
    ) {
        val content = androidx.tv.material3.LocalContentColor.current
        Row(Modifier.padding(horizontal = 10.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            ChannelTile(channel, origin, Modifier.size(width = 52.dp, height = 32.dp), well = Color.White.copy(alpha = 0.08f))
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                BasicText(label, style = PleyType.Tv.Label.copy(color = content), maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (on != null) {
                    BasicText(on, style = PleyType.Tv.Caption.copy(color = content.copy(alpha = 0.75f)), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (channel.favorite) Icon(PleyIcons.Star, contentDescription = null, size = 14.dp, tint = content)
        }
    }
}

/** The channel list's heading: the category it shows. */
@Composable
private fun categoryLabel(category: LiveCategory): String = when (category) {
    LiveCategory.All -> stringResource(R.string.tv_livetv_category_all)
    LiveCategory.Favorites -> stringResource(R.string.tv_livetv_category_favorites)
    LiveCategory.Recent -> stringResource(R.string.tv_livetv_category_recent)
    is LiveCategory.Group -> LiveTv.splitRegion(category.name)?.let { (region, label) -> "$region · $label" } ?: category.name
}

/** What's on shows this long after tuning or a seek. */
private const val INFO_MS = 5_000L

/** The controls hide after this long untouched while playing. */
private const val CONTROLS_MS = 6_000L

/** The channel list's rows: a channel, or one whose page is still loading. */
private const val CHANNEL_ROW = "channel"
private const val LOADING_ROW = "loading"

/** The keys that select: the remote's OK, and Enter on a keyboard or keypad. */
private val OK_KEYS = setOf(KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER)

/** Media keys that would pause, seek or skip: Live TV has none of that. */
private val NOT_LIVE_KEYS = setOf(
    KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE,
    KeyEvent.KEYCODE_MEDIA_PLAY,
    KeyEvent.KEYCODE_MEDIA_PAUSE,
    KeyEvent.KEYCODE_MEDIA_REWIND,
    KeyEvent.KEYCODE_MEDIA_FAST_FORWARD,
    KeyEvent.KEYCODE_MEDIA_SKIP_BACKWARD,
    KeyEvent.KEYCODE_MEDIA_SKIP_FORWARD,
    KeyEvent.KEYCODE_MEDIA_STEP_BACKWARD,
    KeyEvent.KEYCODE_MEDIA_STEP_FORWARD,
)

internal const val TV_LIVE_PLAYER = "pley.tv.live.player"
internal const val TV_LIVE_PLAYER_INFO = "pley.tv.live.player.info"
internal const val TV_LIVE_PLAYER_CHANNEL_LIST = "pley.tv.live.player.channel.list"
internal const val TV_LIVE_PLAYER_LIVE = "pley.tv.live.player.live"
internal const val TV_LIVE_PLAYER_DIGITS = "pley.tv.live.player.digits"
internal const val TV_LIVE_PLAYER_TROUBLE = "pley.tv.live.player.trouble"
internal const val TV_LIVE_PLAYER_BUSY = "pley.tv.live.player.busy"
internal const val TV_LIVE_PLAYER_CHANNELS = "pley.tv.live.player.channels"
internal const val TV_LIVE_PLAYER_GROUP = "pley.tv.live.player.group"
internal const val TV_LIVE_PLAYER_SEARCH = "pley.tv.live.player.search"
