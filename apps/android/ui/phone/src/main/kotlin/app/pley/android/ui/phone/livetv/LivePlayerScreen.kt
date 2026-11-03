package app.pley.android.ui.phone.livetv

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.LiveCategory
import app.pley.android.screens.livetv.LiveTv
import kotlinx.coroutines.flow.distinctUntilChanged
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
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
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyChip
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import kotlin.math.abs
import kotlinx.coroutines.delay

/**
 * The phone's live player (the web's LivePlayerPage): the channel on its
 * SurfaceView; while tuning its logo and what's on; a card that says why
 * it doesn't play (the server's own sentence: which channels hold every
 * connection) with Try again and Other channels. A tap shows the
 * controls: Back, the channel and what's on, the favourite, the channel
 * list; Live; channel down and up. Live TV is watched as it airs: no
 * pause, no going back (as the web's). A swipe up or
 * down changes channel. What's on and next shows for 5 s when a tap brings
 * the controls, not on its own as a channel tunes.
 */
@Composable
internal fun LivePlayerScreen(vm: LivePlayerViewModel, onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    val engine by vm.engine.collectAsStateWithLifecycle()
    val channel by vm.channel.collectAsStateWithLifecycle()
    val origin by vm.origin.collectAsStateWithLifecycle()
    val now by vm.now.collectAsStateWithLifecycle()
    val digits by vm.digits.collectAsStateWithLifecycle()
    val category by vm.category.collectAsStateWithLifecycle()
    var controls by remember { mutableStateOf(true) }
    var poked by remember { mutableIntStateOf(0) }
    var panel by rememberSaveable { mutableStateOf(false) }
    // What's on and next shows when asked for (a tap that brings the controls), never on its own as a channel tunes.
    var banner by remember(state.channelId) { mutableStateOf(false) }
    val keyboard = LocalSoftwareKeyboardController.current
    val playing = state.phase == LivePhase.Playing
    val poke: () -> Unit = {
        controls = true
        poked++
    }

    BackHandler(enabled = panel) { panel = false }
    // Each opening starts on the category zapped through.
    LaunchedEffect(panel) { if (panel) vm.openList() }
    LaunchedEffect(poked, controls, playing, panel) {
        if (controls && playing && !panel) {
            delay(HIDE_CONTROLS_MS)
            controls = false
        }
    }
    LaunchedEffect(banner, playing) {
        if (banner && playing) {
            delay(BANNER_MS)
            banner = false
        }
    }

    CompositionLocalProvider(LocalCategory provides category) {
    // A window of its own: UI Automator (the benchmarks' journeys) finds its tags as resource ids.
    Box(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }.background(Color.Black).testTag(LIVE_PLAYER)) {
        VideoSurface(engine?.player, Modifier.fillMaxSize().testTag(TestTags.PLAYER_SURFACE))
        val density = LocalDensity.current
        Box(
            Modifier
                .fillMaxSize()
                .pointerInput(panel) {
                    detectTapGestures(
                        onTap = {
                            when {
                                panel -> panel = false
                                controls -> controls = false
                                else -> {
                                    poke()
                                    banner = true
                                }
                            }
                        },
                    )
                }
                .pointerInput(Unit) {
                    var dragged = 0f
                    detectVerticalDragGestures(
                        onDragStart = { dragged = 0f },
                        onDragEnd = {
                            if (abs(dragged) > with(density) { SWIPE.toPx() }) vm.step(if (dragged < 0) 1 else -1)
                        },
                        onVerticalDrag = { change, amount ->
                            change.consume()
                            dragged += amount
                        },
                    )
                },
        )
        val shown = channel
        if (state.audioOnly && playing && shown != null) SoundOnly(shown, origin, now)
        when (val phase = state.phase) {
            LivePhase.Tuning, LivePhase.Idle -> if (shown != null) Tuning(shown, origin, now)
            is LivePhase.Failed -> TroubleCard(
                trouble = phase.trouble,
                channel = shown,
                origin = origin,
                onRetry = if (phase.trouble.canRetry) vm::retry else null,
                onTakeOver = vm::takeOver,
                onChannels = { panel = true },
                onBack = onBack,
            )
            LivePhase.Playing -> if (state.buffering) {
                CircularProgressIndicator(Modifier.align(Alignment.Center).size(48.dp), color = PleyColors.Fg, trackColor = Color.Transparent, strokeWidth = 3.dp)
            }
        }
        AnimatedVisibility(controls && state.phase !is LivePhase.Failed, enter = fadeIn(), exit = fadeOut()) {
            Controls(state, shown, origin, now, vm, onBack = onBack, onChannels = { panel = true }, onPoke = poke)
        }
        if (shown != null) {
            AnimatedVisibility(
                banner && playing && !panel,
                enter = fadeIn() + slideInVertically { it / 4 },
                exit = fadeOut() + slideOutVertically { it / 4 },
                modifier = Modifier.align(Alignment.BottomStart),
            ) {
                // Above the bottom row when the controls show.
                Banner(shown, origin, now, Modifier.padding(start = 16.dp, end = 16.dp, bottom = if (controls) 56.dp else 24.dp))
            }
        }
        Digits(digits, Modifier.align(Alignment.TopEnd))
        AnimatedVisibility(
            panel,
            enter = slideInHorizontally { it },
            exit = slideOutHorizontally { it },
            modifier = Modifier.align(Alignment.CenterEnd),
        ) {
            ChannelPanel(
                vm = vm,
                current = state.channelId,
                origin = origin,
                now = now,
                onTune = { channel, from ->
                    keyboard?.hide()
                    panel = false
                    vm.tune(channel.id, channel, from)
                },
            )
        }
    }
    }
}

/** The category the player was opened from: names leave out the region its group says. */
private val LocalCategory = staticCompositionLocalOf<LiveCategory> { LiveCategory.All }

/** The channel's logo, name and what's on while it is tuned. */
@Composable
private fun Tuning(channel: LiveChannel, origin: String, now: Long) {
    Column(
        Modifier.fillMaxSize().background(Color.Black),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        ChannelTile(channel, origin, Modifier.size(width = 176.dp, height = 100.dp), name = true, style = MaterialTheme.typography.titleMedium, well = Color.White.copy(alpha = 0.06f), shape = PleyShape.Dialog)
        Spacer(Modifier.height(20.dp))
        CircularProgressIndicator(Modifier.size(28.dp), color = PleyColors.Accent, trackColor = Color.Transparent, strokeWidth = 3.dp)
        Spacer(Modifier.height(16.dp))
        Text(listOfNotNull(channel.number, LiveTv.channelName(channel, LocalCategory.current)).joinToString("  "), style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg)
        Text(
            channel.onAt(now)?.title ?: stringResource(R.string.live_player_tuning),
            style = MaterialTheme.typography.bodyMedium,
            color = PleyColors.Muted,
            modifier = Modifier.padding(top = 4.dp),
        )
    }
}

/** A channel with sound only (radio): its logo where the picture would be, with what's on. */
@Composable
private fun SoundOnly(channel: LiveChannel, origin: String, now: Long) {
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        ChannelTile(channel, origin, Modifier.size(width = 200.dp, height = 112.dp), name = true, style = MaterialTheme.typography.titleMedium, well = Color.White.copy(alpha = 0.06f), shape = PleyShape.Dialog)
        Spacer(Modifier.height(16.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(PleyIcons.Radio, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(6.dp))
            Text(stringResource(R.string.live_player_radio), style = MaterialTheme.typography.labelLarge, color = PleyColors.Muted)
        }
        channel.onAt(now)?.let { Text(it.title, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, modifier = Modifier.padding(top = 6.dp)) }
    }
}

/** The controls over the picture: a top bar, then Live, what's on's times and channel up and down below. */
@Composable
private fun Controls(
    state: LiveState,
    channel: LiveChannel?,
    origin: String,
    now: Long,
    vm: LivePlayerViewModel,
    onBack: () -> Unit,
    onChannels: () -> Unit,
    onPoke: () -> Unit,
) {
    val clock = rememberClock()
    val on = channel?.onAt(now)
    Box(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.8f), Color.Transparent)))
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(start = 4.dp, end = 8.dp, top = 4.dp, bottom = 32.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PleyIconButton(PleyIcons.ArrowLeft, stringResource(R.string.live_player_back), onClick = onBack)
            if (channel != null) {
                ChannelTile(channel, origin, Modifier.padding(start = 4.dp).size(width = 56.dp, height = 34.dp), well = Color.White.copy(alpha = 0.1f))
                Column(Modifier.weight(1f).padding(start = 12.dp)) {
                    Text(
                        listOfNotNull(channel.number, LiveTv.channelName(channel, LocalCategory.current)).joinToString("  "),
                        style = MaterialTheme.typography.titleMedium,
                        color = PleyColors.Fg,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    on?.let { Text(it.title, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                }
                PleyIconButton(
                    if (channel.favorite) PleyIcons.Star else PleyIcons.StarOutline,
                    stringResource(if (channel.favorite) R.string.livetv_favorite_remove else R.string.livetv_favorite_add),
                    onClick = {
                        onPoke()
                        vm.toggleFavorite()
                    },
                    tint = if (channel.favorite) PleyColors.Accent else PleyColors.Fg,
                )
            } else {
                Spacer(Modifier.weight(1f))
            }
            PleyIconButton(PleyIcons.List, stringResource(R.string.live_player_channels), onClick = onChannels, modifier = Modifier.testTag(LIVE_PLAYER_CHANNELS))
        }
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.85f))))
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(start = 16.dp, end = 8.dp, top = 40.dp, bottom = 8.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (state.phase == LivePhase.Playing) {
                    LiveBadge()
                    Spacer(Modifier.width(12.dp))
                }
                Text(
                    on?.let { spanOf(it, clock) }.orEmpty(),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Muted,
                    modifier = Modifier.weight(1f),
                )
                PleyIconButton(PleyIcons.ChevronDown, stringResource(R.string.live_player_channel_down), onClick = {
                    onPoke()
                    vm.step(-1)
                })
                PleyIconButton(PleyIcons.ChevronUp, stringResource(R.string.live_player_channel_up), onClick = {
                    onPoke()
                    vm.step(1)
                }, modifier = Modifier.testTag(LIVE_PLAYER_UP))
            }
        }
    }
}

/** Live, red with a dot: a badge, nothing to press. */
@Composable
private fun LiveBadge() {
    Row(
        Modifier
            .clip(PleyShape.Circle)
            .background(PleyColors.Danger)
            .padding(horizontal = 10.dp, vertical = 5.dp)
            .testTag(LIVE_PLAYER_LIVE),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(6.dp).background(Color.White, PleyShape.Circle))
        Spacer(Modifier.width(6.dp))
        Text(stringResource(R.string.live_player_live).uppercase(), style = MaterialTheme.typography.labelSmall, color = Color.White)
    }
}

/** What's on and next, on tuning: the logo, the title, its times and what's left, how far it is, what's next. */
@Composable
private fun Banner(channel: LiveChannel, origin: String, now: Long, modifier: Modifier = Modifier) {
    val clock = rememberClock()
    val on = channel.onAt(now)
    val next = channel.nextAt(now)
    Row(
        modifier
            .widthIn(max = 460.dp)
            .clip(PleyShape.Dialog)
            .background(Color.Black.copy(alpha = 0.82f))
            .border(1.dp, Color.White.copy(alpha = 0.1f), PleyShape.Dialog)
            .padding(12.dp)
            .testTag(LIVE_PLAYER_BANNER),
    ) {
        ChannelTile(channel, origin, Modifier.size(width = 88.dp, height = 52.dp), name = true, well = Color.White.copy(alpha = 0.07f))
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f, fill = false)) {
            if (on != null) {
                Text(on.title, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text("${spanOf(on, clock)} · ${timeLeft(on, now)}", style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
                ProgramProgress(on, now, Modifier.padding(top = 6.dp).fillMaxWidth(), track = Color.White.copy(alpha = 0.15f))
            } else {
                Text(
                    stringResource(if (channel.hasGuide) R.string.livetv_nothing_now else R.string.livetv_no_guide),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                )
            }
            if (next != null) {
                Text(
                    stringResource(R.string.livetv_next, clock(next.start), next.title),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Subtle,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }
        }
    }
}

/** A channel number being typed (a keyboard on a tablet), or the one that matched nothing. */
@Composable
private fun Digits(digits: ChannelDigits, modifier: Modifier = Modifier) {
    val text = digits.typed.ifEmpty { null } ?: digits.notFound?.let { stringResource(R.string.live_player_no_channel, it) } ?: return
    Text(
        text,
        style = if (digits.typed.isNotEmpty()) MaterialTheme.typography.displaySmall else MaterialTheme.typography.titleMedium,
        color = PleyColors.Fg,
        modifier = modifier
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .padding(24.dp)
            .clip(PleyShape.Dialog)
            .background(Color.Black.copy(alpha = 0.85f))
            .padding(horizontal = 20.dp, vertical = 10.dp),
    )
}

/**
 * Why the channel doesn't play, with what can be done: try again, another
 * channel, back to the guide. Every connection in use, what holds each
 * (the web's TroubleCard), with "Watch here instead" where it may be taken
 * over ([onTakeOver]).
 */
@Composable
private fun TroubleCard(
    trouble: LiveTrouble,
    channel: LiveChannel?,
    origin: String,
    onRetry: (() -> Unit)?,
    onTakeOver: (Long) -> Unit,
    onChannels: () -> Unit,
    onBack: () -> Unit,
) {
    val name = channel?.name.orEmpty()
    val title = when (trouble.kind) {
        LiveTrouble.Kind.Busy -> stringResource(R.string.live_player_busy)
        LiveTrouble.Kind.Stopped -> stringResource(R.string.live_player_stopped)
        LiveTrouble.Kind.Missing -> stringResource(R.string.live_player_missing)
        LiveTrouble.Kind.NotAllowed -> stringResource(R.string.live_player_not_allowed)
        LiveTrouble.Kind.Unavailable -> stringResource(R.string.live_player_unavailable, name)
        LiveTrouble.Kind.Offline -> stringResource(R.string.live_player_offline)
        LiveTrouble.Kind.Other -> stringResource(R.string.live_player_failed, name)
    }
    Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.85f)).windowInsetsPadding(WindowInsets.safeDrawing), contentAlignment = Alignment.Center) {
        Column(
            Modifier
                .padding(24.dp)
                .widthIn(max = 440.dp)
                .clip(PleyShape.Dialog)
                .background(PleyColors.Surface2)
                .verticalScroll(rememberScrollState())
                .padding(24.dp)
                .testTag(LIVE_PLAYER_TROUBLE),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (channel != null) {
                ChannelTile(channel, origin, Modifier.size(width = 96.dp, height = 56.dp), name = true, well = Color.White.copy(alpha = 0.06f))
            } else {
                Icon(if (trouble.kind == LiveTrouble.Kind.Busy) PleyIcons.TvMinimal else PleyIcons.TriangleAlert, contentDescription = null, size = 40.dp, tint = PleyColors.Accent)
            }
            Spacer(Modifier.height(16.dp))
            Text(title, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg, textAlign = TextAlign.Center)
            val connections = trouble.connections
            if (connections.isNotEmpty()) {
                Text(
                    pluralStringResource(R.plurals.live_player_connections_watching, connections.size, connections.size),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(top = 8.dp),
                )
                Spacer(Modifier.height(12.dp))
                for (c in connections) BusyRow(c, onTakeOver)
                if (connections.none { it.canTakeOver }) {
                    Text(
                        stringResource(R.string.live_player_none_to_take),
                        style = MaterialTheme.typography.bodySmall,
                        color = PleyColors.Subtle,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                }
            } else if (trouble.message.isNotBlank()) {
                Text(
                    trouble.message,
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }
            Spacer(Modifier.height(20.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                if (onRetry != null) PrimaryButton(stringResource(R.string.live_player_retry), onClick = onRetry, icon = PleyIcons.RefreshCw)
                TonalButton(stringResource(R.string.live_player_other_channels), onClick = onChannels, icon = PleyIcons.List)
            }
            PleyTextButton(stringResource(R.string.live_player_back_to_guide), onClick = onBack)
        }
    }
}

/** A connection in use: its channel, who holds it, and "Watch here instead" where it may be taken. */
@Composable
private fun BusyRow(connection: BusyConnection, onTakeOver: (Long) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(bottom = 8.dp)
            .clip(PleyShape.Field)
            .background(Color.White.copy(alpha = 0.04f))
            .padding(horizontal = 12.dp, vertical = 8.dp)
            .testTag(LIVE_PLAYER_BUSY),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(connection.channelName, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(holders(connection), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (connection.canTakeOver) {
            Spacer(Modifier.width(8.dp))
            TonalButton(stringResource(R.string.live_player_take_over), onClick = { onTakeOver(connection.channelId) })
        }
    }
}

/** Who holds a connection: "You, on another device", their names (to administrators), or how many. */
@Composable
private fun holders(c: BusyConnection): String = when {
    c.yours -> stringResource(R.string.live_player_held_by_you)
    c.watchers.isNotEmpty() -> c.watchers.joinToString(", ")
    else -> pluralStringResource(R.plurals.live_player_held_by_people, c.people.coerceAtLeast(1), c.people.coerceAtLeast(1))
}

/**
 * The channels beside the picture (the web's ChannelPanel): a category's
 * channels a page at a time, what's on each, to change to one. It opens on
 * the category zapped through, at the channel playing; its heading opens
 * the categories to list another's ([onTune] says which it was chosen in).
 */
@Composable
private fun ChannelPanel(
    vm: LivePlayerViewModel,
    current: Long?,
    origin: String,
    now: Long,
    onTune: (LiveChannel, LiveCategory) -> Unit,
) {
    val shown by vm.listCategory.collectAsStateWithLifecycle()
    val listed by vm.listed.collectAsStateWithLifecycle()
    val query by vm.listQuery.collectAsStateWithLifecycle()
    val groups by vm.groups.collectAsStateWithLifecycle()
    var picking by rememberSaveable { mutableStateOf(false) }
    BackHandler(enabled = picking) { picking = false }
    Column(
        Modifier
            .fillMaxHeight()
            .width(340.dp)
            .background(PleyColors.Surface)
            .drawBehind { drawLine(PleyColors.Surface4, Offset(0f, 0f), Offset(0f, size.height), 1.dp.toPx()) }
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .testTag(LIVE_PLAYER_PANEL),
    ) {
        Row(
            Modifier
                .padding(start = 8.dp, end = 8.dp, top = 8.dp, bottom = 4.dp)
                .fillMaxWidth()
                .clip(PleyShape.Field)
                .clickable(role = Role.Button) { picking = !picking }
                .padding(horizontal = 8.dp, vertical = 10.dp)
                .testTag(LIVE_PLAYER_GROUP),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (picking) {
                Icon(PleyIcons.ArrowLeft, contentDescription = null, size = 18.dp, tint = PleyColors.Fg)
                Spacer(Modifier.width(10.dp))
            } else if (shown == LiveCategory.Favorites) {
                Icon(PleyIcons.Star, contentDescription = null, size = 16.dp, tint = PleyColors.Accent)
                Spacer(Modifier.width(8.dp))
            }
            Text(
                if (picking) stringResource(R.string.livetv_categories) else categoryLabel(shown),
                style = MaterialTheme.typography.titleMedium,
                color = PleyColors.Fg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (!picking) {
                Spacer(Modifier.width(6.dp))
                Icon(PleyIcons.ChevronDown, contentDescription = stringResource(R.string.live_player_change_group), size = 16.dp, tint = PleyColors.Muted)
            }
        }
        if (picking) {
            CategoryList(
                groups = groups,
                selected = shown,
                onSelect = {
                    vm.showList(it)
                    picking = false
                },
                modifier = Modifier.fillMaxWidth().weight(1f),
                surface = PleyColors.Surface,
            )
        } else {
            PanelSearch(query, vm::searchList)
            key(listed) { PanelChannels(vm, listed, current, origin, now, onTune) }
        }
    }
}

/** Searches the channel list, among every channel. */
@Composable
private fun PanelSearch(value: String, onValue: (String) -> Unit) {
    val label = stringResource(R.string.live_player_search)
    val focus = LocalFocusManager.current
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 4.dp)
            .height(40.dp)
            .clip(PleyShape.Pill)
            .background(PleyColors.Surface3)
            .padding(start = 12.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.Search, contentDescription = null, size = 16.dp, tint = PleyColors.Muted)
        Spacer(Modifier.width(8.dp))
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (value.isEmpty()) Text(label, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            BasicTextField(
                value = value,
                onValueChange = onValue,
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyMedium.copy(color = PleyColors.Fg),
                cursorBrush = SolidColor(PleyColors.AccentBright),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { focus.clearFocus() }),
                modifier = Modifier.fillMaxWidth().semantics { contentDescription = label }.testTag(LIVE_PLAYER_SEARCH),
            )
        }
        if (value.isNotEmpty()) {
            PleyIconButton(PleyIcons.X, stringResource(R.string.livetv_search_clear), onClick = { onValue("") }, tint = PleyColors.Muted)
        }
    }
}

/** [category]'s channels in the panel, from the channel playing when it is among them. */
@Composable
private fun PanelChannels(
    vm: LivePlayerViewModel,
    category: LiveCategory,
    current: Long?,
    origin: String,
    now: Long,
    onTune: (LiveChannel, LiveCategory) -> Unit,
) {
    val pages by vm.list.collectAsStateWithLifecycle()
    val start by vm.listIndex.collectAsStateWithLifecycle()
    val list = rememberLazyListState()
    val skeleton = rememberSkeleton(animated = true)
    LaunchedEffect(start != null) { start?.let { list.scrollToItem((it - 2).coerceAtLeast(0)) } }
    LaunchedEffect(list) {
        snapshotFlow { list.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
            .distinctUntilChanged()
            .collect { (first, last) -> vm.showingListRows(first, last) }
    }
    Column {
        if (pages.total == 0) {
            Text(
                stringResource(R.string.livetv_no_match_group),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Subtle,
                modifier = Modifier.padding(16.dp),
            )
        }
        LazyColumn(state = list, contentPadding = PaddingValues(bottom = 16.dp)) {
            items(
                count = pages.total ?: 0,
                key = { index -> pages[index]?.id ?: -index - 1L },
                contentType = { index -> if (pages[index] == null) "skeleton" else "channel" },
            ) { index ->
                val channel = pages[index]
                if (channel == null) {
                    Box(Modifier.fillMaxWidth().height(58.dp).padding(horizontal = 16.dp, vertical = 8.dp).skeleton(skeleton, PleyShape.Field))
                    return@items
                }
                val on = channel.onAt(now)
                val isCurrent = channel.id == current
                Row(
                    Modifier
                        .fillMaxWidth()
                        .background(if (isCurrent) PleyColors.Accent.copy(alpha = 0.15f) else Color.Transparent)
                        .clickable(role = Role.Button) { onTune(channel, category) }
                        .padding(horizontal = 16.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    ChannelTile(channel, origin, Modifier.size(width = 56.dp, height = 34.dp), well = Color.White.copy(alpha = 0.08f))
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            channel.number?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, modifier = Modifier.padding(end = 6.dp)) }
                            Text(
                                LiveTv.channelName(channel, category),
                                style = MaterialTheme.typography.titleSmall,
                                color = if (isCurrent) PleyColors.AccentBright else PleyColors.Fg,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.weight(1f, fill = false),
                            )
                            if (channel.favorite) Icon(PleyIcons.Star, contentDescription = null, size = 12.dp, tint = PleyColors.Accent, modifier = Modifier.padding(start = 4.dp))
                        }
                        if (on != null) {
                            Text(on.title, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            ProgramProgress(on, now, Modifier.padding(top = 4.dp).fillMaxWidth(), track = Color.White.copy(alpha = 0.12f))
                        }
                    }
                }
            }
        }
    }
}

/** The channel list's heading: the category it shows. */
@Composable
private fun categoryLabel(category: LiveCategory): String = when (category) {
    LiveCategory.All -> stringResource(R.string.livetv_category_all)
    LiveCategory.Favorites -> stringResource(R.string.livetv_category_favorites)
    LiveCategory.Recent -> stringResource(R.string.livetv_category_recent)
    is LiveCategory.Group -> LiveTv.splitRegion(category.name)?.let { (region, label) -> "$region · $label" } ?: category.name
}

/** Controls hide after this long untouched while playing. */
private const val HIDE_CONTROLS_MS = 3_500L

/** The banner saying what's on shows this long after tuning. */
private const val BANNER_MS = 5_000L

/** A swipe this long changes channel. */
private val SWIPE = 72.dp

internal const val LIVE_PLAYER = "pley.live.player"
internal const val LIVE_PLAYER_UP = "pley.live.player.up"
internal const val LIVE_PLAYER_LIVE = "pley.live.player.live"
internal const val LIVE_PLAYER_CHANNELS = "pley.live.player.channels"
internal const val LIVE_PLAYER_PANEL = "pley.live.player.panel"
internal const val LIVE_PLAYER_GROUP = "pley.live.player.group"
internal const val LIVE_PLAYER_SEARCH = "pley.live.player.search"
internal const val LIVE_PLAYER_BANNER = "pley.live.player.banner"
internal const val LIVE_PLAYER_TROUBLE = "pley.live.player.trouble"
internal const val LIVE_PLAYER_BUSY = "pley.live.player.busy"
