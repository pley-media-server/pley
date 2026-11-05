package app.pley.android.ui.phone.remote

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TooltipAnchorPosition
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pley.android.common.errorSentence
import app.pley.android.common.resolve
import app.pley.android.data.OtherDevice
import app.pley.android.data.RemoteRepository
import app.pley.android.data.RemoteSession
import app.pley.android.data.SentCommand
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ChannelLogo
import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
import app.pley.android.model.ItemKind
import app.pley.android.model.MediaStream
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.model.StreamKind
import app.pley.android.model.TrackOption
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatClockMs
import app.pley.android.model.format.fullTitle
import app.pley.android.model.format.streamLabel
import app.pley.android.network.CommandRefusal
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.livetv.rememberClock
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** A request to open the remote on a device ([RemoteSlot]): each one opens it, the same device's too. */
internal class RemoteAsk(val deviceId: String)

/**
 * The remote's place above the tabs (over the mini player when both
 * show): the bar while another of the profile's devices is on, playing or
 * ready to ([devices]), and the sheet it opens, which closes when no
 * device is left and opens closed when one is on again. A command waiting
 * for its answer keeps the sheet: next ends a session before its player
 * lists the next one. The device chosen in the sheet is kept while it is
 * open. [asked] opens it on a device (Play on… sent it something).
 */
@Composable
internal fun RemoteSlot(devices: List<OtherDevice>, remote: RemoteRepository?, origin: String?, asked: RemoteAsk? = null) {
    if (remote == null) return
    var open by rememberSaveable { mutableStateOf(false) }
    var chosen by rememberSaveable { mutableStateOf<String?>(null) }
    var holding by remember { mutableStateOf(false) }
    var commands by remember { mutableIntStateOf(0) }
    val none = devices.isEmpty()
    LaunchedEffect(asked) {
        val ask = asked ?: return@LaunchedEffect
        chosen = ask.deviceId
        open = true
    }
    LaunchedEffect(commands) {
        if (commands == 0) return@LaunchedEffect
        delay(RemoteRepository.ANSWER_WAIT_MS)
        holding = false
    }
    LaunchedEffect(none, holding) { if (none && !holding) open = false }
    RemoteBar(
        devices,
        onOpen = {
            chosen = null
            open = true
        },
    )
    if (open && (!none || holding)) {
        RemoteSheet(
            devices,
            remote,
            origin,
            chosen = chosen,
            onChoose = { chosen = it },
            onCommand = {
                holding = true
                commands++
            },
            onDismiss = { open = false },
        )
    }
}

/** What the remote calls each device ([RemoteRepository.deviceNames]), numbered as the app says it. */
@Composable
internal fun deviceNames(devices: List<OtherDevice>): Map<String, String> {
    val unnamed = stringResource(R.string.remote_unknown_device)
    val resources = LocalResources.current
    return remember(devices, unnamed, resources) {
        RemoteRepository.deviceNames(devices, unnamed) { name, n -> resources.getString(R.string.remote_device_numbered, name, n) }
    }
}

/**
 * The bar above the tabs while another of the profile's devices is on
 * (apps/web/src/components/Remote.tsx, as a bar): "Playing on Living room
 * TV" over what plays there, or how many devices play and their names, or,
 * while none plays, "Your devices" and their names. Tapping it opens the
 * remote ([RemoteSheet]).
 */
@Composable
internal fun RemoteBar(devices: List<OtherDevice>, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    if (devices.isEmpty()) return
    val names = deviceNames(devices)
    val playing = devices.filter { it.playing }
    val single = playing.singleOrNull()
    val headline = when {
        single != null -> stringResource(R.string.remote_playing_on, names.getValue(single.deviceId))
        playing.isNotEmpty() -> pluralStringResource(R.plurals.remote_playing_on_devices, playing.size, playing.size)
        else -> stringResource(R.string.remote_title)
    }
    val liveTv = stringResource(R.string.remote_live_tv)
    val separator = stringResource(R.string.remote_device_separator)
    val detail = single?.session?.let { titleOf(it, liveTv, formatWords()) }
        ?: playing.ifEmpty { devices }.joinToString(separator) { names.getValue(it.deviceId) }
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .background(PleyColors.Surface2)
            .clickable(role = Role.Button, onClickLabel = stringResource(R.string.remote_open), onClick = onOpen)
            .padding(horizontal = 16.dp, vertical = 6.dp)
            .testTag(REMOTE_BAR),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(if (playing.isEmpty()) PleyIcons.Cast else PleyIcons.MonitorPlay, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(headline, style = MaterialTheme.typography.labelMedium, color = PleyColors.AccentBright, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (detail.isNotEmpty()) {
                Text(detail, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Icon(PleyIcons.ChevronUp, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
    }
}

/**
 * The remote: "Your devices", a row for each of the profile's other
 * devices that is on, those that play first; choosing one shows its
 * controls in the list's place, Back (or the arrow) going back to it.
 * With one device only, its controls show at once. Positions are
 * estimated each second while a device plays. Pictures come from
 * [origin], the server in use; [onCommand] is told of each command sent.
 */
@Composable
internal fun RemoteSheet(
    devices: List<OtherDevice>,
    remote: RemoteRepository,
    origin: String?,
    chosen: String? = null,
    onChoose: (String?) -> Unit = {},
    onCommand: () -> Unit = {},
    onDismiss: () -> Unit,
) {
    val names = deviceNames(devices)
    val now by produceState(remote.clock(), remote) {
        while (true) {
            delay(TICK_MS)
            value = remote.clock()
        }
    }
    val single = devices.singleOrNull()
    val shown = single ?: devices.firstOrNull { it.deviceId == chosen }
    PleySheet(
        onDismiss = onDismiss,
        title = stringResource(R.string.remote_title).takeIf { shown == null },
        modifier = Modifier.testTag(REMOTE_SHEET),
    ) {
        if (shown == null) {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 8.dp)) {
                devices.forEach { device ->
                    key(device.deviceId) {
                        DeviceRow(device, names.getValue(device.deviceId), now) { onChoose(device.deviceId) }
                    }
                }
            }
        } else {
            // The list again: only when there is one to go back to.
            val back = { onChoose(null) }.takeIf { single == null }
            BackHandler(enabled = back != null) { onChoose(null) }
            key(shown.deviceId) {
                DeviceControls(shown, names.getValue(shown.deviceId), now, remote, origin, onCommand, back)
            }
        }
    }
}

/** A device in the list: its kind's icon, its name, and what it plays with how far, or "Ready to play". */
@Composable
private fun DeviceRow(device: OtherDevice, name: String, now: Long, onClick: () -> Unit) {
    val session = device.session
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clickable(role = Role.Button, onClickLabel = stringResource(R.string.remote_open_device), onClick = onClick)
            // In line with the sheet's title, as Play on…'s devices are.
            .padding(horizontal = 24.dp, vertical = 10.dp)
            .testTag(REMOTE_DEVICE + device.deviceId),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        KindBadge(device.kind, playing = session != null)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(name, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (session == null) {
                Text(stringResource(R.string.remote_ready), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            } else {
                Text(
                    lineOf(session, stringResource(R.string.remote_live_tv), formatWords()),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val duration = session.session.durationMs?.takeIf { it > 0 && session.session.kind != PlayingKind.Live }
                if (duration != null) {
                    Spacer(Modifier.height(6.dp))
                    // How far, drawn only: the row is read as its name and what it plays.
                    Box(Modifier.fillMaxWidth().height(2.dp).clip(PleyShape.Badge).background(PleyColors.Surface4).clearAndSetSemantics {}) {
                        Box(Modifier.fillMaxHeight().fillMaxWidth((session.positionAt(now).toFloat() / duration).coerceIn(0f, 1f)).background(PleyColors.Accent))
                    }
                }
            }
        }
        Spacer(Modifier.width(8.dp))
        Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
    }
}

/** A device's kind as an icon in a 40 dp circle: in the accent while it plays. */
@Composable
internal fun KindBadge(kind: DeviceKind, playing: Boolean) {
    Box(Modifier.size(40.dp).clip(PleyShape.Circle).background(PleyColors.Surface3), contentAlignment = Alignment.Center) {
        Icon(kindIcon(kind), contentDescription = null, size = 20.dp, tint = if (playing) PleyColors.Accent else PleyColors.Fg)
    }
}

/** The icon of a device of [kind]: a TV, a phone, a tablet, a computer, a browser, or one of those. */
internal fun kindIcon(kind: DeviceKind): PleyIcon = when (kind) {
    DeviceKind.Tv -> PleyIcons.Tv
    DeviceKind.Phone -> PleyIcons.Smartphone
    DeviceKind.Tablet -> PleyIcons.Tablet
    DeviceKind.Computer -> PleyIcons.Laptop
    DeviceKind.Browser -> PleyIcons.Globe
    DeviceKind.Other, DeviceKind.Unknown -> PleyIcons.MonitorSmartphone
}

/**
 * One device's controls: its name (with the way back to the list, [onBack],
 * when there is one), then what it plays and the controls its player
 * obeys, or, while it plays nothing, that it is ready for something.
 */
@Composable
private fun DeviceControls(
    device: OtherDevice,
    name: String,
    now: Long,
    remote: RemoteRepository,
    origin: String?,
    onCommand: () -> Unit,
    onBack: (() -> Unit)?,
) {
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 8.dp).testTag(REMOTE_CONTROLS + device.deviceId)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = if (onBack != null) 4.dp else 16.dp, end = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                PleyIconButton(PleyIcons.ArrowLeft, stringResource(R.string.remote_back_to_devices), onClick = onBack)
                Spacer(Modifier.width(4.dp))
            }
            Icon(kindIcon(device.kind), contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(10.dp))
            Text(name, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        val session = device.session
        if (session == null) {
            Idle()
        } else {
            Playing(device.deviceId, session, name, now, remote, origin, onCommand)
        }
    }
}

/** A device ready to play, under its name: nothing plays there, and how to send it something. */
@Composable
private fun Idle() {
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 24.dp).testTag(REMOTE_IDLE),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(stringResource(R.string.remote_nothing_playing), style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg)
        Spacer(Modifier.height(6.dp))
        Text(
            stringResource(R.string.remote_idle_hint),
            style = MaterialTheme.typography.bodyMedium,
            color = PleyColors.Muted,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * What a device plays, top to bottom: its picture and title, how far (a
 * channel's programme), the transport, then its audio, subtitles and
 * quality on the left and Stop on the right; Stop asks first. A command it
 * didn't act on within [RemoteRepository.ANSWER_WAIT_MS] says so. Its
 * session may change under it (the next episode, a track that needed a new
 * session): the device's newest is followed.
 */
@Composable
private fun Playing(
    deviceId: String,
    session: RemoteSession,
    device: String,
    now: Long,
    remote: RemoteRepository,
    origin: String?,
    onCommand: () -> Unit,
) {
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val current by rememberUpdatedState(session)
    var sent by remember { mutableStateOf<SentCommand?>(null) }
    var sending by remember { mutableStateOf(false) }
    var stopping by remember { mutableStateOf(false) }
    var confirmStop by rememberSaveable { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    // The report the error was said against: a newer one (the device acted late, or on something else) takes it away.
    var errorAt by remember { mutableStateOf<Long?>(null) }
    val noAnswer = stringResource(R.string.remote_no_answer, device)

    fun say(message: String?, reportedAt: Long? = current.session.updatedAt) {
        error = message
        errorAt = reportedAt.takeIf { message != null }
    }
    LaunchedEffect(session) {
        if (errorAt != null && session.session.updatedAt != errorAt) say(null)
    }

    // The list shows the change (the player's report brings sessions_changed): it answered.
    LaunchedEffect(sent, session) {
        sent?.let { if (RemoteRepository.answered(it, session, remote.clock())) sent = null }
    }
    // Not yet after a while: the server is asked once more before saying so.
    LaunchedEffect(sent) {
        val waiting = sent ?: return@LaunchedEffect
        delay(RemoteRepository.ANSWER_WAIT_MS)
        val list = remote.refresh()
        sent = null
        // A player with nothing next or before stays where it is, as it should.
        if (waiting.command == Control.Next || waiting.command == Control.Previous) return@LaunchedEffect
        val now = list?.lastOrNull { RemoteRepository.deviceOf(it) == deviceId }
        if (list != null && !RemoteRepository.answered(waiting, now, remote.clock())) say(noAnswer, now?.session?.updatedAt)
    }

    fun refused(e: Exception): String = when (CommandRefusal.of(e)) {
        CommandRefusal.Gone -> resources.getString(R.string.remote_gone, device)
        CommandRefusal.NotControllable -> resources.getString(R.string.remote_cant, device)
        CommandRefusal.TooMany -> resources.getString(R.string.remote_too_many)
        CommandRefusal.Offline, CommandRefusal.Other -> errorSentence(e).resolve(resources)
    }

    fun send(request: PlayerCommandRequest) {
        say(null)
        sending = true
        onCommand()
        scope.launch {
            val before = current
            val sentAt = remote.clock()
            val target = RemoteRepository.target(before, request.command, sentAt, request.positionMs, request.offsetMs)
            try {
                remote.command(before.sessionId, request)
                sent = SentCommand(request.command, before, sentAt, target, request.streamIndex, request.quality)
                // Its answer is waited for from now.
                onCommand()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                say(refused(e))
            } finally {
                sending = false
            }
        }
    }

    fun stop() {
        say(null)
        stopping = true
        scope.launch {
            try {
                remote.stop(current.sessionId)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                say(refused(e))
            } finally {
                stopping = false
            }
        }
    }

    val waiting = sending || sent != null
    val live = session.session.kind == PlayingKind.Live
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .testTag(REMOTE_CARD + session.sessionId),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Header(session, origin)
        // The controls stay as they are while a command waits for its answer (the web's): another can follow it.
        if (live) {
            session.session.channel?.let { LiveProgress(it) }
        } else {
            Progress(session, device, now, onSeek = { send(PlayerCommandRequest(Control.Seek, positionMs = it)) })
        }
        Transport(session, device, live, ::send)
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            // The icons' glyphs, and Stop's, line up with the artwork and the times above:
            // their touch targets reach past the edges instead.
            Choices(session, device, ::send, Modifier.weight(1f).offset(x = (-12).dp))
            val stopLabel = stringResource(R.string.remote_stop_on, device)
            TextButton(
                onClick = { confirmStop = true },
                enabled = !stopping,
                modifier = Modifier.offset(x = 12.dp).heightIn(min = 48.dp).semantics { contentDescription = stopLabel },
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg, disabledContentColor = PleyColors.Disabled),
            ) {
                Icon(PleyIcons.CircleStop, contentDescription = null, size = 16.dp, tint = LocalContentColor.current)
                Spacer(Modifier.width(6.dp))
                Text(stringResource(R.string.remote_stop), style = MaterialTheme.typography.labelLarge)
            }
        }
        val said = error
        val (line, color) = when {
            said != null -> said to PleyColors.DangerText
            waiting -> stringResource(R.string.remote_waiting, device) to PleyColors.Muted
            else -> null to PleyColors.Muted
        }
        line?.let {
            Text(
                it,
                style = MaterialTheme.typography.bodySmall,
                color = color,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
    }
    if (confirmStop) {
        // Stopping ends what someone may be watching there: it asks first.
        ConfirmDialog(
            title = stringResource(R.string.remote_stop_confirm, device),
            body = null,
            confirmLabel = stringResource(R.string.remote_stop),
            onConfirm = {
                confirmStop = false
                stop()
            },
            onDismiss = { confirmStop = false },
            destructive = true,
        )
    }
}

/**
 * What plays, beside its poster or the channel's logo: an episode as its
 * show and place over its title, a track over its artist, a channel's
 * number and name over Live TV (its programme is under it, with how far
 * it is: [LiveProgress]), a movie over its year.
 */
@Composable
private fun Header(session: RemoteSession, origin: String?) {
    val playing = session.session
    val (title, under) = linesOf(playing, stringResource(R.string.remote_live_tv), formatWords())
    Row(verticalAlignment = Alignment.CenterVertically) {
        val channel = playing.channel
        val item = playing.item
        if (playing.kind == PlayingKind.Live && channel != null) {
            val logo = remember(channel.logo, origin) { origin?.let { ArtworkImage.of(channel.logo, it) } }
            ChannelLogo(
                image = logo,
                label = channel.number ?: channel.name.take(3),
                textStyle = MaterialTheme.typography.labelMedium,
                modifier = Modifier.size(80.dp, 56.dp).clip(PleyShape.Field).background(PleyColors.Surface3),
            )
        } else if (item != null) {
            val poster = remember(item, origin) { origin?.let { ArtworkCandidates.poster(item, it) } }
            Artwork(
                poster,
                item.title,
                Modifier.width(56.dp).aspectRatio(if (item.kind == ItemKind.Track) 1f else 2f / 3f).clip(PleyShape.TilePhone),
            )
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
            under?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis) }
        }
    }
}

/**
 * A channel's programme on, in a title's progress' place: its title, how
 * far it is by the clock (drawn only), its start and end times as the
 * device writes them ("20:00 – 21:00"), and the one after it. Nothing
 * without a guide.
 */
@Composable
private fun LiveProgress(channel: PlayingChannel) {
    val programme = channel.now?.takeIf { it.isNotEmpty() }
    val next = channel.next?.takeIf { it.isNotEmpty() }
    if (programme == null && next == null) return
    val start = channel.nowStart
    val stop = channel.nowStop?.takeIf { start != null && it > start }
    val clock = rememberClock()
    val seconds by produceState(System.currentTimeMillis() / 1000) {
        while (true) {
            delay(TICK_MS)
            value = System.currentTimeMillis() / 1000
        }
    }
    Column(Modifier.fillMaxWidth().testTag(REMOTE_LIVE), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        programme?.let { Text(it, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 2, overflow = TextOverflow.Ellipsis) }
        if (programme != null && start != null && stop != null) {
            val fraction = ((seconds - start).toFloat() / (stop - start)).coerceIn(0f, 1f)
            Box(Modifier.fillMaxWidth().height(3.dp).clip(PleyShape.Badge).background(PleyColors.Surface4).clearAndSetSemantics {}) {
                Box(Modifier.fillMaxHeight().fillMaxWidth(fraction).background(PleyColors.Accent))
            }
            Text(
                stringResource(R.string.remote_live_span, clock(start), clock(stop)),
                style = MaterialTheme.typography.labelSmall,
                color = PleyColors.Subtle,
            )
        }
        next?.let {
            Text(
                stringResource(R.string.remote_live_next, it),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** How far it is, with its times: a slider when its player seeks, a bar otherwise; nothing without a length. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Progress(session: RemoteSession, device: String, now: Long, onSeek: (Long) -> Unit) {
    val duration = session.session.durationMs?.takeIf { it > 0 } ?: return
    val position = session.positionAt(now)
    var dragging by remember { mutableStateOf<Float?>(null) }
    val fraction = dragging ?: (position.toFloat() / duration).coerceIn(0f, 1f)
    val shown = (fraction * duration).toLong()
    val stateText = stringResource(R.string.remote_position, formatClockMs(shown), formatClockMs(duration))
    // The slider or bar says both times: the times beside it aren't read again.
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(formatClockMs(shown), style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle, modifier = Modifier.clearAndSetSemantics {})
        Spacer(Modifier.width(8.dp))
        if (session.can(Control.Seek)) {
            val label = stringResource(R.string.remote_seek, device)
            val colors = SliderDefaults.colors(
                thumbColor = PleyColors.Accent,
                activeTrackColor = PleyColors.Accent,
                inactiveTrackColor = PleyColors.Surface4,
            )
            val interactions = remember { MutableInteractionSource() }
            Slider(
                value = fraction,
                onValueChange = { dragging = it },
                onValueChangeFinished = {
                    dragging?.let { onSeek((it * duration).toLong()) }
                    dragging = null
                },
                colors = colors,
                interactionSource = interactions,
                // Material's thumb in a 48 dp high box: the slider takes touches over 48 dp, not its 44.
                thumb = {
                    Box(Modifier.height(48.dp), contentAlignment = Alignment.Center) {
                        SliderDefaults.Thumb(interactions, colors = colors)
                    }
                },
                modifier = Modifier.weight(1f).semantics {
                    contentDescription = label
                    stateDescription = stateText
                },
            )
        } else {
            val label = stringResource(R.string.remote_progress, device)
            Box(
                Modifier
                    .weight(1f)
                    .height(4.dp)
                    .clip(PleyShape.Badge)
                    .background(PleyColors.Surface4)
                    .semantics {
                        contentDescription = label
                        stateDescription = stateText
                        progressBarRangeInfo = ProgressBarRangeInfo(fraction, 0f..1f)
                    },
            ) {
                Box(Modifier.fillMaxHeight().fillMaxWidth(fraction).background(PleyColors.Accent))
            }
        }
        Spacer(Modifier.width(8.dp))
        Text(formatClockMs(duration), style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle, modifier = Modifier.clearAndSetSemantics {})
    }
}

/**
 * The transport, centred. A title's in the web's order: previous, back
 * 10 s, play or pause, forward 30 s, where the player declares them, and
 * next where the player declares it or what plays has a next by nature (an
 * episode, a track), dimmed when the player can't go there; a movie whose
 * player declares none has no Next. A channel's: play or pause where the
 * player declares pause, then the channel rocker ([ChannelRocker]).
 */
@Composable
private fun Transport(session: RemoteSession, device: String, live: Boolean, send: (PlayerCommandRequest) -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
        val paused = session.session.state == ProgressState.Paused
        val toggle = when {
            live && !session.can(Control.Pause) -> null
            paused && session.can(Control.Play) -> Control.Play
            !paused && session.can(Control.Pause) -> Control.Pause
            else -> null
        }
        if (live) {
            toggle?.let { PlayPause(it, device, send) }
            ChannelRocker(session, device, send)
            return@Row
        }
        if (session.can(Control.Previous)) {
            PleyIconButton(
                PleyIcons.SkipBack,
                stringResource(R.string.remote_previous, device),
                onClick = { send(PlayerCommandRequest(Control.Previous)) },
            )
        }
        if (session.can(Control.Skip)) {
            PleyIconButton(
                PleyIcons.RotateCcw,
                stringResource(R.string.remote_back, device),
                onClick = { send(PlayerCommandRequest(Control.Skip, offsetMs = -RemoteRepository.SKIP_BACK_MS)) },
            )
        }
        toggle?.let { PlayPause(it, device, send) }
        if (session.can(Control.Skip)) {
            PleyIconButton(
                PleyIcons.RotateCw,
                stringResource(R.string.remote_forward, device),
                onClick = { send(PlayerCommandRequest(Control.Skip, offsetMs = RemoteRepository.SKIP_FORWARD_MS)) },
            )
        }
        val kind = session.session.item?.kind
        if (session.can(Control.Next) || kind == ItemKind.Episode || kind == ItemKind.Track) {
            PleyIconButton(
                PleyIcons.SkipForward,
                stringResource(R.string.remote_next, device),
                onClick = { send(PlayerCommandRequest(Control.Next)) },
                enabled = session.can(Control.Next),
            )
        }
    }
}

/** Play or pause ([toggle]): a 48 dp circle in a 56 dp touch target, between its neighbours. */
@Composable
private fun PlayPause(toggle: Control, device: String, send: (PlayerCommandRequest) -> Unit) {
    val label = stringResource(if (toggle == Control.Play) R.string.remote_play else R.string.remote_pause, device)
    Box(
        Modifier
            .padding(horizontal = 8.dp)
            .size(56.dp)
            .clip(PleyShape.Circle)
            .clickable(role = Role.Button) { send(PlayerCommandRequest(toggle)) }
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(48.dp).clip(PleyShape.Circle).background(PleyColors.Fg), contentAlignment = Alignment.Center) {
            Icon(if (toggle == Control.Play) PleyIcons.Play else PleyIcons.Pause, contentDescription = null, size = 24.dp, tint = PleyColors.Bg)
        }
    }
}

/**
 * A channel's up and down, as a TV remote's rocker: one rounded pill
 * holding channel down, the word CHANNEL and channel up, each button
 * dimmed when the player can't change channel that way. The word is drawn
 * only: the buttons say what they do.
 */
@Composable
private fun ChannelRocker(session: RemoteSession, device: String, send: (PlayerCommandRequest) -> Unit) {
    Row(
        Modifier
            .padding(horizontal = 8.dp)
            .clip(PleyShape.Circle)
            .background(PleyColors.Surface3)
            .border(1.dp, PleyColors.Border, PleyShape.Circle)
            .testTag(REMOTE_ROCKER),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PleyIconButton(
            PleyIcons.ChevronDown,
            stringResource(R.string.remote_previous_channel, device),
            onClick = { send(PlayerCommandRequest(Control.Previous)) },
            enabled = session.can(Control.Previous),
        )
        Text(
            stringResource(R.string.remote_channel).uppercase(),
            style = MaterialTheme.typography.labelSmall.copy(letterSpacing = 1.5.sp),
            color = PleyColors.Muted,
            modifier = Modifier.padding(horizontal = 4.dp).clearAndSetSemantics {},
        )
        PleyIconButton(
            PleyIcons.ChevronUp,
            stringResource(R.string.remote_next_channel, device),
            onClick = { send(PlayerCommandRequest(Control.Next)) },
            enabled = session.can(Control.Next),
        )
    }
}

/**
 * Audio, Subtitles and Quality, as icons, each where the player declares
 * it and has something to pick: more than one audio track, any subtitles,
 * the qualities its menu offers. Each opens a menu labelled as the
 * player's own menu labels it, what plays ticked (subtitles' Off first; no
 * quality ticked until the player says which).
 */
@Composable
private fun Choices(session: RemoteSession, device: String, send: (PlayerCommandRequest) -> Unit, modifier: Modifier = Modifier) {
    val playing = session.session
    val audio = playing.tracks.audio.takeIf { session.can(Control.Audio) && it.size > 1 }.orEmpty()
    val subtitles = playing.tracks.subtitles.takeIf { session.can(Control.Subtitles) }.orEmpty()
    val qualities = playing.qualities.takeIf { session.can(Control.Quality) }.orEmpty()
    val words = formatWords()
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        if (audio.isNotEmpty()) {
            ChoiceMenu(
                icon = PleyIcons.AudioLines,
                hint = stringResource(R.string.remote_audio),
                description = stringResource(R.string.remote_audio_on, device),
                choices = audio.map { it.index to labelOf(it, StreamKind.Audio, words) },
                current = playing.audioIndex,
                tag = REMOTE_AUDIO,
                onPick = { send(PlayerCommandRequest(Control.Audio, streamIndex = it)) },
            )
        }
        if (subtitles.isNotEmpty()) {
            ChoiceMenu(
                icon = PleyIcons.Captions,
                hint = stringResource(R.string.remote_subtitles),
                description = stringResource(R.string.remote_subtitles_on, device),
                choices = listOf(PlayingSession.SUBTITLES_OFF to stringResource(R.string.remote_subtitles_off)) +
                    subtitles.map { it.index to labelOf(it, StreamKind.Subtitle, words) },
                current = playing.subtitleIndex,
                tag = REMOTE_SUBTITLES,
                onPick = { send(PlayerCommandRequest(Control.Subtitles, streamIndex = it)) },
            )
        }
        if (qualities.isNotEmpty()) {
            ChoiceMenu(
                icon = PleyIcons.SlidersHorizontal,
                hint = stringResource(R.string.remote_quality),
                description = stringResource(R.string.remote_quality_on, device),
                choices = qualities.map { it.id to it.label.ifEmpty { it.id } },
                current = playing.quality,
                tag = REMOTE_QUALITY,
                onPick = { send(PlayerCommandRequest(Control.Quality, quality = it)) },
            )
        }
    }
}

/**
 * A 48 dp icon button, read out as [description] and hinted as [hint] on a
 * long press, that opens a menu of [choices] (a key and its label),
 * [current] ticked; it shows as selected while the menu is open. Picking
 * one calls [onPick] with its key and closes the menu.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun <T> ChoiceMenu(
    icon: PleyIcon,
    hint: String,
    description: String,
    choices: List<Pair<T, String>>,
    current: T?,
    tag: String,
    onPick: (T) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        TooltipBox(
            positionProvider = TooltipDefaults.rememberTooltipPositionProvider(TooltipAnchorPosition.Above),
            tooltip = { PlainTooltip { Text(hint) } },
            state = rememberTooltipState(),
        ) {
            Box(
                Modifier
                    .size(48.dp)
                    .clip(PleyShape.Circle)
                    .background(if (open) PleyColors.Surface3 else Color.Transparent)
                    .clickable(role = Role.Button) { open = true }
                    .semantics {
                        contentDescription = description
                        selected = open
                    }
                    .testTag(tag),
                contentAlignment = Alignment.Center,
            ) {
                Icon(icon, contentDescription = null, size = 24.dp, tint = if (open) PleyColors.AccentBright else PleyColors.Fg)
            }
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = PleyColors.Surface3) {
            choices.forEach { (key, text) ->
                val ticked = key == current
                DropdownMenuItem(
                    // A long track name takes two lines, with room above and below as a one-line row has.
                    text = { Text(text, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                    contentPadding = PaddingValues(horizontal = 16.dp, vertical = 10.dp),
                    onClick = {
                        open = false
                        onPick(key)
                    },
                    trailingIcon = if (ticked) ({ Icon(PleyIcons.Check, contentDescription = null, size = 20.dp, tint = PleyColors.AccentBright) }) else null,
                    modifier = Modifier.heightIn(min = 48.dp).semantics { selected = ticked },
                )
            }
        }
    }
}

/** A track as the player's own menu labels it ([streamLabel]) in [words]: "English (EAC3 5.1)", "French (SRT, Forced)". */
private fun labelOf(track: TrackOption, kind: StreamKind, words: FormatWords): String = streamLabel(
    MediaStream(
        index = track.index,
        kind = kind,
        codec = track.codec.orEmpty(),
        language = track.language,
        title = track.title,
        default = track.default,
        forced = track.forced,
        channels = track.channels,
    ),
    words,
)

/** What plays, in one line, as the bar says it: the title as the web writes it ("Show — S1 · E2 — Title"), or the channel ([liveTv] without one). */
private fun titleOf(session: RemoteSession, liveTv: String, words: FormatWords): String = when (session.session.kind) {
    PlayingKind.Live -> session.session.channel?.name?.takeIf { it.isNotEmpty() } ?: liveTv
    else -> session.session.item?.let { fullTitle(it, words) }.orEmpty()
}

/** What plays, as the list says it: the title as the web writes it, or the channel's number and name. */
internal fun lineOf(session: RemoteSession, liveTv: String, words: FormatWords): String {
    val channel = session.session.channel
    return when {
        session.session.kind != PlayingKind.Live -> session.session.item?.let { fullTitle(it, words) }.orEmpty()
        channel != null -> listOfNotNull(channel.number, channel.name.takeIf { it.isNotEmpty() }).joinToString(" ").ifEmpty { liveTv }
        else -> liveTv
    }
}

/** What plays, in two lines ([Header]); the second `null` when there is nothing to say. */
private fun linesOf(playing: PlayingSession, liveTv: String, words: FormatWords): Pair<String, String?> {
    val channel = playing.channel
    val item = playing.item
    return when {
        playing.kind == PlayingKind.Live -> {
            // The programme on is under it, with how far it is (LiveProgress).
            val name = channel?.let { listOfNotNull(it.number, it.name.takeIf { n -> n.isNotEmpty() }).joinToString(" ") }.orEmpty()
            if (name.isEmpty()) liveTv to null else name to liveTv
        }
        item == null -> "" to null
        item.kind == ItemKind.Episode ->
            listOf(item.grandparentTitle.orEmpty(), episodeLabel(item, words)).filter { it.isNotEmpty() }.joinToString(" · ") to item.title
        item.kind == ItemKind.Track -> item.title to (item.grandparentTitle ?: item.parentTitle)?.takeIf { it.isNotEmpty() }
        else -> item.title to item.year?.toString()
    }
}

/** How often the sheet moves its estimates on. */
private const val TICK_MS = 1_000L

/** The bar's test tag. */
internal const val REMOTE_BAR = "pley.phone.remote_bar"

/** The sheet's test tag. */
internal const val REMOTE_SHEET = "pley.phone.remote_sheet"

/** A device's row in the list: this and its device id. */
internal const val REMOTE_DEVICE = "pley.phone.remote_device."

/** A device's controls: this and its device id. */
internal const val REMOTE_CONTROLS = "pley.phone.remote_controls."

/** What a device plays and its controls: this and its session id. */
internal const val REMOTE_CARD = "pley.phone.remote_card."

/** A device ready to play, playing nothing. */
internal const val REMOTE_IDLE = "pley.phone.remote_idle"

/** The Audio, Subtitles and Quality buttons. */
internal const val REMOTE_AUDIO = "pley.phone.remote_audio"
internal const val REMOTE_SUBTITLES = "pley.phone.remote_subtitles"
internal const val REMOTE_QUALITY = "pley.phone.remote_quality"

/** A channel's up and down. */
internal const val REMOTE_ROCKER = "pley.phone.remote_rocker"

/** A channel's programme, with how far it is. */
internal const val REMOTE_LIVE = "pley.phone.remote_live"
