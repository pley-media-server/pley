package app.pley.android.ui.phone.cast

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.CastPlaying
import app.pley.android.data.CastTarget
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.model.CastPlatform
import app.pley.android.model.CastPlayState
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatClockMs
import app.pley.android.screens.cast.CastProblem
import app.pley.android.screens.cast.CastRemoteStatus
import app.pley.android.screens.cast.CastRemoteUi
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.cast.CastViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.SheetAction
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.kit.ToastHostState
import app.pley.android.ui.phone.shell.LocalPhoneShell
import kotlinx.coroutines.delay

/**
 * Casting's model: the shell's, the player's or a title page's, all
 * reading the same devices; `null` in tests whose Application holds no graph.
 */
@Composable
internal fun rememberCastModel(): CastViewModel? {
    if (LocalContext.current.applicationContext !is PleyGraphOwner) return null
    return viewModel(factory = CastViewModel.Factory)
}

/** The Cast button: there only while the profile has a device to cast to. */
@Composable
internal fun CastButton(cast: CastViewModel, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val available by cast.available.collectAsStateWithLifecycle()
    if (available) PleyIconButton(PleyIcons.Cast, stringResource(R.string.cast_button), onClick = onClick, modifier = modifier.testTag(CAST_BUTTON))
}

/**
 * The shell's part of casting, once in: the bar above the navigation, the
 * remote of the device just cast to (or picked, or the bar's), and what
 * went wrong, in [toasts]. Pictures come from [origin].
 */
@Composable
internal fun CastSlot(origin: String?, toasts: ToastHostState) {
    val cast = rememberCastModel() ?: return
    val bar by cast.bar.collectAsStateWithLifecycle()
    val remote by cast.remote.collectAsStateWithLifecycle()
    LaunchedEffect(cast) { cast.remotes.collect(cast::openRemote) }
    CastProblems(cast, toasts)
    bar?.let { device -> CastBar(device, cast, onOpen = { cast.openRemote(device.id) }) }
    remote?.let { CastRemote(it, cast, origin, onDismiss = cast::closeRemote) }
}

/**
 * A title page's Cast button, beside Play: "Play on" sends the device
 * [itemId] (its version [fileId]) to play from where the profile left off.
 */
@Composable
internal fun CastTitle(itemId: Long, fileId: Long?) {
    val cast = rememberCastModel() ?: return
    var picking by rememberSaveable { mutableStateOf(false) }
    CastProblems(cast, LocalPhoneShell.current.toasts)
    CastButton(cast, onClick = { picking = true })
    if (picking) {
        CastPicker(
            cast,
            onPick = { device ->
                picking = false
                cast.pick(device, itemId, fileId)
            },
            onDismiss = { picking = false },
        )
    }
}

/** "Play on": each device with its platform's icon, its name, and what it plays now. */
@Composable
internal fun CastPicker(cast: CastViewModel, onPick: (CastTarget) -> Unit, onDismiss: () -> Unit) {
    val devices by cast.devices.collectAsStateWithLifecycle()
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.cast_play_on)) {
        if (devices.isEmpty()) {
            Text(
                stringResource(R.string.cast_no_devices),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            )
        }
        devices.forEach { device ->
            SheetAction(
                label = device.name,
                onClick = { onPick(device) },
                icon = platformIcon(device.platform),
                supporting = device.playing?.let { playingLine(it) },
            )
        }
    }
}

/**
 * The bar above the navigation while the device last cast to plays
 * something, as the mini player is drawn: "Playing on <device>", the
 * title, and play or pause. Tapping it opens the remote.
 */
@Composable
internal fun CastBar(device: CastTarget, cast: CastViewModel, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val playing = device.playing ?: return
    val moving = playing.state != CastPlayState.Paused
    Row(
        modifier
            .fillMaxWidth()
            .height(64.dp)
            .background(PleyColors.Surface2)
            .clickable(role = Role.Button, onClickLabel = stringResource(R.string.cast_open_remote), onClick = onOpen)
            .padding(start = 16.dp, end = 4.dp)
            .testTag(CAST_BAR),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.Cast, contentDescription = null, tint = PleyColors.AccentBright)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(
                stringResource(R.string.cast_playing_on, device.name),
                style = MaterialTheme.typography.labelMedium,
                color = PleyColors.AccentBright,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(titleOf(playing.item), style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        PleyIconButton(
            if (moving) PleyIcons.Pause else PleyIcons.Play,
            stringResource(if (moving) R.string.cast_pause else R.string.cast_play),
            onClick = { if (moving) cast.pause(device) else cast.resume(device) },
        )
    }
}

/**
 * A device's remote, in a sheet: the poster, the title, the device and
 * where it stands; the position moving on while it plays, to drag;
 * play or pause, back 10 s and forward 30 s; Stop and Play here. Until the
 * device says it plays what it was sent, it says it waits for it.
 */
@Composable
internal fun CastRemote(remote: CastRemoteUi, cast: CastViewModel, origin: String?, onDismiss: () -> Unit) {
    val device = remote.device
    val status = remote.status
    PleySheet(onDismiss = onDismiss) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp).testTag(CAST_REMOTE)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(platformIcon(device.platform), contentDescription = null, tint = PleyColors.Muted, size = 20.dp)
                Spacer(Modifier.width(8.dp))
                Text(device.name, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.height(16.dp))
            when (status) {
                CastRemoteStatus.Waiting -> StatusText(stringResource(R.string.cast_waiting, device.name))
                CastRemoteStatus.NoAnswer -> StatusText(stringResource(R.string.cast_no_answer, device.name), problem = true)
                CastRemoteStatus.Idle -> StatusText(stringResource(R.string.cast_idle))
                is CastRemoteStatus.Playing -> Playing(device, status.playing, cast, origin)
            }
            Spacer(Modifier.height(16.dp))
        }
    }
}

@Composable
private fun StatusText(text: String, problem: Boolean = false) {
    Text(
        text,
        style = MaterialTheme.typography.bodyLarge,
        color = if (problem) PleyColors.DangerText else PleyColors.Muted,
        modifier = Modifier.padding(vertical = 24.dp),
    )
}

@Composable
private fun Playing(device: CastTarget, playing: CastPlaying, cast: CastViewModel, origin: String?) {
    val item = playing.item
    val poster = remember(item.id, origin) { origin?.let { ArtworkImage.of("/api/items/${item.id}/images/poster", it) } }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(width = 72.dp, height = 108.dp).clip(PleyShape.TilePhone).background(PleyColors.Surface3)) {
            Artwork(poster, item.title, Modifier.matchParentSize())
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(titleOf(item), style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (item.kind == ItemKind.Episode) {
                Text(item.title, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Text(playingLine(playing), style = MaterialTheme.typography.labelMedium, color = PleyColors.AccentBright, maxLines = 1)
        }
    }
    Spacer(Modifier.height(16.dp))
    Scrubber(device, playing, cast)
    Spacer(Modifier.height(8.dp))
    val moving = playing.state != CastPlayState.Paused
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.CenterVertically) {
        PleyIconButton(PleyIcons.RotateCcw, stringResource(R.string.cast_back_10), onClick = { cast.skip(device, -BACK_MS) })
        val label = stringResource(if (moving) R.string.cast_pause else R.string.cast_play)
        Box(
            Modifier
                .size(64.dp)
                .clip(PleyShape.Circle)
                .background(PleyColors.Accent)
                .clickable(role = Role.Button) { if (moving) cast.pause(device) else cast.resume(device) }
                .semantics { contentDescription = label },
            contentAlignment = Alignment.Center,
        ) {
            Icon(if (moving) PleyIcons.Pause else PleyIcons.Play, contentDescription = null, size = 28.dp, tint = PleyColors.OnAccent)
        }
        PleyIconButton(PleyIcons.RotateCw, stringResource(R.string.cast_forward_30), onClick = { cast.skip(device, FORWARD_MS) })
    }
    Spacer(Modifier.height(16.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        TonalButton(stringResource(R.string.cast_stop), onClick = { cast.stop(device) }, icon = PleyIcons.Square, modifier = Modifier.weight(1f))
        TonalButton(stringResource(R.string.cast_play_here), onClick = { cast.playHere(device) }, icon = PleyIcons.Smartphone, modifier = Modifier.weight(1f))
    }
}

/** The position, moving on each second while the device plays, and its times; tapped or dragged, it seeks. */
@Composable
private fun Scrubber(device: CastTarget, playing: CastPlaying, cast: CastViewModel) {
    val position by rememberCastPosition(playing, cast::now)
    var dragging by remember { mutableStateOf<Float?>(null) }
    val duration = (playing.durationMs ?: playing.item.durationMs ?: 0L).coerceAtLeast(0)
    val enabled = duration > 0
    val fraction = dragging ?: if (enabled) (position.toFloat() / duration).coerceIn(0f, 1f) else 0f
    val shownMs = if (dragging != null) (fraction * duration).toLong() else position
    val label = stringResource(R.string.cast_position, device.name)
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .fillMaxWidth()
                .height(40.dp)
                .semantics {
                    contentDescription = label
                    stateDescription = "${formatClockMs(shownMs)} / ${formatClockMs(duration)}"
                    progressBarRangeInfo = ProgressBarRangeInfo(fraction, 0f..1f)
                    if (enabled) {
                        setProgress { target ->
                            cast.seek(device, (target.coerceIn(0f, 1f) * duration).toLong())
                            true
                        }
                    }
                }
                .pointerInput(enabled, duration) {
                    if (!enabled) return@pointerInput
                    detectTapGestures { offset -> cast.seek(device, ((offset.x / size.width).coerceIn(0f, 1f) * duration).toLong()) }
                }
                .pointerInput(enabled, duration) {
                    if (!enabled) return@pointerInput
                    detectHorizontalDragGestures(
                        onDragStart = { offset -> dragging = (offset.x / size.width).coerceIn(0f, 1f) },
                        onDragEnd = {
                            dragging?.let { cast.seek(device, (it * duration).toLong()) }
                            dragging = null
                        },
                        onDragCancel = { dragging = null },
                    ) { change, _ -> dragging = (change.position.x / size.width).coerceIn(0f, 1f) }
                }
                .drawBehind {
                    // The phone's scrubber (design spec §3.5): a 4 dp track, accent played, a 16 dp thumb, 24 while dragged.
                    val h = 4.dp.toPx()
                    val y = size.height / 2f
                    val r = CornerRadius(h / 2f, h / 2f)
                    val f = dragging ?: if (enabled) (position.toFloat() / duration).coerceIn(0f, 1f) else 0f
                    drawRoundRect(PleyColors.Fg.copy(alpha = 0.25f), Offset(0f, y - h / 2f), Size(size.width, h), r)
                    drawRoundRect(if (enabled) PleyColors.Accent else PleyColors.Disabled, Offset(0f, y - h / 2f), Size(size.width * f, h), r)
                    val thumb = (if (dragging != null) 24.dp else 16.dp).toPx() / 2f
                    drawCircle(if (enabled) PleyColors.Accent else PleyColors.Disabled, thumb, Offset((size.width * f).coerceIn(thumb, size.width - thumb), y))
                },
        )
        Row(Modifier.fillMaxWidth()) {
            Text(formatClockMs(shownMs), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            Spacer(Modifier.weight(1f))
            if (enabled) Text("−" + formatClockMs((duration - shownMs).coerceAtLeast(0)), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
    }
}

/** Where [playing] is now, moved on each second while it plays. */
@Composable
private fun rememberCastPosition(playing: CastPlaying, now: () -> Long): State<Long> {
    val state = remember { mutableLongStateOf(playing.positionAt(now())) }
    LaunchedEffect(playing) {
        state.longValue = playing.positionAt(now())
        while (playing.state == CastPlayState.Playing) {
            delay(TICK_MS)
            state.longValue = playing.positionAt(now())
        }
    }
    return state
}

/** Something that went wrong, as a toast in [toasts], once. */
@Composable
internal fun CastProblems(cast: CastViewModel, toasts: ToastHostState) {
    val resources = LocalResources.current
    LaunchedEffect(cast, toasts) {
        cast.problem.collect { problem ->
            val text = when (problem) {
                is CastProblem.NotSent -> resources.getString(R.string.cast_not_sent, problem.device)
                is CastProblem.NotSteered -> resources.getString(R.string.cast_not_steered, problem.device)
                null -> return@collect
            }
            toasts.show(text, error = true)
            cast.dismissProblem()
        }
    }
}

/** A movie's title; an episode's show and number ("Show · S1 · E3"). */
internal fun titleOf(item: ItemSummary): String {
    if (item.kind != ItemKind.Episode) return item.title
    val show = item.grandparentTitle ?: return item.title
    return listOf(show, episodeLabel(item)).filter { it.isNotEmpty() }.joinToString(" · ")
}

/** "Playing <title>", "Paused · <title>", "Loading <title>". */
@Composable
private fun playingLine(playing: CastPlaying): String {
    val title = titleOf(playing.item)
    return when (playing.state) {
        CastPlayState.Paused -> stringResource(R.string.cast_paused, title)
        CastPlayState.Buffering -> stringResource(R.string.cast_loading, title)
        CastPlayState.Playing, CastPlayState.Unknown -> stringResource(R.string.cast_playing, title)
    }
}

/** The icon beside a device's name: a TV, a phone, a computer or a browser. */
internal fun platformIcon(platform: CastPlatform): PleyIcon = when (platform) {
    CastPlatform.Web -> PleyIcons.Globe
    CastPlatform.Desktop -> PleyIcons.Monitor
    CastPlatform.Android -> PleyIcons.Smartphone
    CastPlatform.AndroidTv, CastPlatform.Kodi, CastPlatform.Roku, CastPlatform.Samsung, CastPlatform.Lg, CastPlatform.Unknown -> PleyIcons.Tv
}

private const val BACK_MS = 10_000L
private const val FORWARD_MS = 30_000L

/** How often a playing position moves on screen. */
private const val TICK_MS = 1_000L

internal const val CAST_BUTTON = "pley.phone.cast.button"
internal const val CAST_BAR = "pley.phone.cast.bar"
internal const val CAST_REMOTE = "pley.phone.cast.remote"
