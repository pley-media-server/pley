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
import androidx.compose.ui.draw.alpha
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
import app.pley.android.common.sentenceOf
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

/**
 * A request to open the remote ([RemoteSlot]): on a device this phone sent something to ([deviceId], Play on…),
 * or on "Your devices" (`null`, Home's [RemoteButton]). Each one opens it, the same device's too.
 */
internal class RemoteAsk(val deviceId: String? = null)

/**
 * The remote's place above the tabs (over the mini player when both show)
 * and its sheet. The sheet lists the profile's other devices that are on
 * ([devices]); [asked] opens it, from Home's header or on a device Play on…
 * sent something to, and it closes when no device is left. The bar shows
 * only while this phone controls what a device plays: a command from the
 * sheet, or what it sent with Play on… once that plays (not what played
 * there before it). It goes once that device has played nothing for
 * [RemoteRepository.ANSWER_WAIT_MS], so a title ending into the next one
 * keeps it, and a cast that hasn't started within [CAST_START_WAIT_MS] isn't
 * waited for. A next, or a track that needs a new session, ends one session
 * to start another: until the device plays the new one (or the wait for it
 * is over), the one that ended stays, on the bar and in the sheet. The
 * device chosen in the sheet is kept while it is open.
 */
@Composable
internal fun RemoteSlot(devices: List<OtherDevice>, remote: RemoteRepository?, origin: String?, asked: RemoteAsk? = null) {
    if (remote == null) return
    var open by rememberSaveable { mutableStateOf(false) }
    var chosen by rememberSaveable { mutableStateOf<String?>(null) }
    var ended by remember { mutableStateOf<RemoteSession?>(null) }
    // The device this phone controls; until `started`, a cast on its way, which the session the device played
    // as it was sent (`castOver`) isn't. Each cast (`casts`) waits its own CAST_START_WAIT_MS.
    var controlling by rememberSaveable { mutableStateOf<String?>(null) }
    var started by rememberSaveable { mutableStateOf(false) }
    var castOver by rememberSaveable { mutableStateOf<String?>(null) }
    var casts by rememberSaveable { mutableIntStateOf(0) }
    val listed = remember(devices, ended) { RemoteRepository.withEnded(devices, ended) }
    val controlled = controlling?.let { id -> listed.firstOrNull { it.deviceId == id } }
    val playingThere = controlled?.session?.sessionId
    LaunchedEffect(devices, ended) {
        val gone = ended ?: return@LaunchedEffect
        val now = devices.firstOrNull { it.deviceId == RemoteRepository.deviceOf(gone) }?.session
        if (now != null && now.sessionId != gone.sessionId) ended = null
    }
    LaunchedEffect(ended) {
        if (ended == null) return@LaunchedEffect
        delay(RemoteRepository.ANSWER_WAIT_MS)
        ended = null
    }
    // What a cast sent plays: the device plays a session other than the one it played as it was sent.
    LaunchedEffect(playingThere, castOver) {
        if (controlling != null && !started && playingThere != null && playingThere != castOver) started = true
    }
    LaunchedEffect(controlling, started, casts) {
        if (controlling == null || started) return@LaunchedEffect
        delay(CAST_START_WAIT_MS)
        controlling = null
    }
    // Playing nothing a while (stopped there, or at the end): no longer this phone's. A moment between one title
    // and the next, or before the list is read again after a cold start, isn't enough.
    val idle = started && playingThere == null
    LaunchedEffect(idle) {
        if (!idle) return@LaunchedEffect
        delay(RemoteRepository.ANSWER_WAIT_MS)
        controlling = null
        started = false
    }
    val none = listed.isEmpty()
    LaunchedEffect(asked) {
        val ask = asked ?: return@LaunchedEffect
        chosen = ask.deviceId
        open = true
        // Sent something from here: what it plays is this phone's to control, once it plays.
        ask.deviceId?.let { id ->
            controlling = id
            started = false
            castOver = listed.firstOrNull { it.deviceId == id }?.session?.sessionId
            casts++
        }
    }
    LaunchedEffect(none) { if (none) open = false }
    val mine = controlled?.takeIf { started && it.playing }
    if (mine != null) {
        RemoteBar(
            mine,
            deviceNames(listed).getValue(mine.deviceId),
            onOpen = {
                chosen = mine.deviceId
                open = true
            },
        )
    }
    if (open && !none) {
        RemoteSheet(
            listed,
            remote,
            origin,
            chosen = chosen,
            onChoose = { chosen = it },
            onCommand = { session, command ->
                controlling = RemoteRepository.deviceOf(session)
                started = true
                if (RemoteRepository.mayEndSession(command)) ended = session
            },
            onDismiss = { open = false },
        )
    }
}

/**
 * The remote's button in Home's header while another of the profile's
 * devices is on (the web's header button): it opens "Your devices"
 * ([onOpen]). In the accent while one of them plays.
 */
@Composable
internal fun RemoteButton(devices: List<OtherDevice>, onOpen: () -> Unit) {
    if (devices.isEmpty()) return
    val playing = devices.count { it.playing }
    val label = if (playing > 0) {
        pluralStringResource(R.plurals.remote_button_playing, playing, playing)
    } else {
        pluralStringResource(R.plurals.remote_button_ready, devices.size, devices.size)
    }
    PleyIconButton(
        PleyIcons.MonitorSmartphone,
        label,
        onClick = onOpen,
        modifier = Modifier.testTag(REMOTE_BUTTON),
        tint = if (playing > 0) PleyColors.Accent else PleyColors.Fg,
    )
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
 * The bar above the tabs while this phone controls what [device] plays
 * ([RemoteSlot]): "Playing on Living room TV" ([name]), or "Paused on",
 * over what plays there. Tapping it opens that device's controls.
 */
@Composable
internal fun RemoteBar(device: OtherDevice, name: String, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val session = device.session ?: return
    val paused = session.session.state == ProgressState.Paused
    val headline = stringResource(if (paused) R.string.remote_paused_on else R.string.remote_playing_on, name)
    val detail = titleOf(session, stringResource(R.string.remote_live_tv))
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .background(PleyColors.Surface2)
            .clickable(role = Role.Button, onClickLabel = stringResource(R.string.remote_open_device), onClick = onOpen)
            .padding(horizontal = 16.dp, vertical = 6.dp)
            .testTag(REMOTE_BAR),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.MonitorPlay, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
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
 * devices that is on, those that play first; choosing one that plays shows
 * its controls in the list's place, Back (or the arrow) going back to it.
 * One ready to play is only listed: it has nothing to control. With one
 * device only, playing, its controls show at once. Positions are estimated
 * each second while a device plays. Pictures come from [origin], the
 * server in use; [onCommand] is told of each command sent, with the
 * session it went to.
 */
@Composable
internal fun RemoteSheet(
    devices: List<OtherDevice>,
    remote: RemoteRepository,
    origin: String?,
    chosen: String? = null,
    onChoose: (String?) -> Unit = {},
    onCommand: (RemoteSession, Control) -> Unit = { _, _ -> },
    onDismiss: () -> Unit,
) {
    val names = deviceNames(devices)
    val now by produceState(remote.clock(), remote) {
        while (true) {
            delay(TICK_MS)
            value = remote.clock()
        }
    }
    val single = devices.singleOrNull()?.takeIf { it.playing }
    val shown = single ?: devices.firstOrNull { it.deviceId == chosen && it.playing }
    val session = shown?.session
    PleySheet(
        onDismiss = onDismiss,
        title = stringResource(R.string.remote_title).takeIf { session == null },
        modifier = Modifier.testTag(REMOTE_SHEET),
    ) {
        if (shown == null || session == null) {
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
                DeviceControls(shown, session, names.getValue(shown.deviceId), now, remote, origin, onCommand, back)
            }
        }
    }
}

/**
 * A device in the list: its kind's icon, its name, and what it plays with
 * how far, or "Ready to play" (with the profile it is on, for another
 * profile's). One that plays is chosen with [onClick]; one ready to play
 * has no controls to show, and isn't.
 */
@Composable
private fun DeviceRow(device: OtherDevice, name: String, now: Long, onClick: () -> Unit) {
    val session = device.session
    val choose = if (session != null) {
        Modifier.clickable(role = Role.Button, onClickLabel = stringResource(R.string.remote_open_device), onClick = onClick)
    } else {
        Modifier
    }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .then(choose)
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
                Text(readyLine(device), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            } else {
                Text(
                    lineOf(session, stringResource(R.string.remote_live_tv)),
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
        if (session != null) {
            Spacer(Modifier.width(8.dp))
            Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
        }
    }
}

/**
 * What a device that plays nothing here says: "Ready to play", and a
 * device of another of the household's profiles whose it is: "On Sam's
 * profile", never what it plays.
 */
@Composable
internal fun readyLine(device: OtherDevice): String =
    device.profile?.let { stringResource(R.string.remote_on_profile, it.name) } ?: stringResource(R.string.remote_ready)

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
 * when there is one), then what it plays ([session]) and the controls its
 * player obeys.
 */
@Composable
private fun DeviceControls(
    device: OtherDevice,
    session: RemoteSession,
    name: String,
    now: Long,
    remote: RemoteRepository,
    origin: String?,
    onCommand: (RemoteSession, Control) -> Unit,
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
        Playing(device.deviceId, session, name, now, remote, origin, onCommand)
    }
}

/**
 * What a device plays, top to bottom: its picture and title, how far (a
 * channel's programme), the transport, then its audio, subtitles and
 * quality on the left and Stop on the right; Stop asks first. A command
 * shows what it asks at once ([RemoteRepository.asSent]); one it didn't act
 * on within [RemoteRepository.ANSWER_WAIT_MS] says so, and what it plays
 * shows again. Its session may change under it (the next episode, a track
 * that needed a new session): the device's newest is followed, and
 * [onCommand] is told of each command, with the session it went to.
 */
@Composable
private fun Playing(
    deviceId: String,
    session: RemoteSession,
    device: String,
    now: Long,
    remote: RemoteRepository,
    origin: String?,
    onCommand: (RemoteSession, Control) -> Unit,
) {
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val current by rememberUpdatedState(session)
    var sent by remember { mutableStateOf<SentCommand?>(null) }
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
        CommandRefusal.Offline, CommandRefusal.Other -> sentenceOf(e)
    }

    fun send(request: PlayerCommandRequest) {
        say(null)
        val before = current
        val sentAt = remote.clock()
        // A skip goes from where the bar shows, a seek still on its way too; a play or pause stays there.
        val shown = RemoteRepository.asSent(before, sent)
        val toggle = request.command == Control.Play || request.command == Control.Pause
        val target = RemoteRepository.target(shown, request.command, sentAt, request.positionMs, request.offsetMs)
            ?: shown.positionAt(sentAt).takeIf { toggle }
        val entry = SentCommand(request.command, before, sentAt, target, request.streamIndex, request.quality)
        // Shown at once, and waited for from now: the player's report confirms it.
        sent = entry
        onCommand(before, request.command)
        scope.launch {
            try {
                remote.command(before.sessionId, request)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (sent == entry) sent = null
                say(refused(e))
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

    // What the controls show: the session as its player reported it, with what was just asked of it.
    val shown = RemoteRepository.asSent(session, sent)
    // What a next or previous plays isn't known until it plays: what plays now is dimmed until then.
    val dim = if (sent?.command == Control.Next || sent?.command == Control.Previous) 0.5f else 1f
    val live = session.session.kind == PlayingKind.Live
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .testTag(REMOTE_CARD + session.sessionId),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Header(session, origin, Modifier.alpha(dim))
        // The controls stay usable while a command waits for its answer (the web's): another can follow it.
        if (live) {
            session.session.channel?.let { LiveProgress(it) }
        } else {
            Progress(shown, device, now, onSeek = { send(PlayerCommandRequest(Control.Seek, positionMs = it)) })
        }
        Transport(shown, device, live, ::send, Modifier.alpha(dim))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            // The icons' glyphs, and Stop's, line up with the artwork and the times above:
            // their touch targets reach past the edges instead.
            Choices(shown, device, ::send, Modifier.weight(1f).offset(x = (-12).dp))
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
        // A refusal, or a command not answered. While one waits nothing is said: the controls show what it asked.
        error?.let {
            Text(
                it,
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.DangerText,
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
private fun Header(session: RemoteSession, origin: String?, modifier: Modifier = Modifier) {
    val playing = session.session
    val (title, under) = linesOf(playing, stringResource(R.string.remote_live_tv))
    Row(modifier, verticalAlignment = Alignment.CenterVertically) {
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
 * The transport, one centred row. A title's: previous, back 10 s, play or
 * pause, forward 30 s and next, each where the player declares it. Next
 * shows where the player declares it or what plays has a next by nature
 * (an episode, a track), dimmed when the player can't go there; a movie
 * whose player declares none has no Next. Where only one of previous and
 * next shows, the other's place is kept, so play stays in the middle. A
 * channel's: play or pause where the player declares pause, then the
 * channel rocker ([ChannelRocker]).
 */
@Composable
private fun Transport(session: RemoteSession, device: String, live: Boolean, send: (PlayerCommandRequest) -> Unit, modifier: Modifier = Modifier) {
    val paused = session.session.state == ProgressState.Paused
    val toggle = when {
        live && !session.can(Control.Pause) -> null
        paused && session.can(Control.Play) -> Control.Play
        !paused && session.can(Control.Pause) -> Control.Pause
        else -> null
    }
    if (live) {
        Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
            toggle?.let { PlayPause(it, device, send) }
            ChannelRocker(session, device, send)
        }
        return
    }
    val previous = session.can(Control.Previous)
    val kind = session.session.item?.kind
    val next = session.can(Control.Next) || kind == ItemKind.Episode || kind == ItemKind.Track
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
        if (previous) {
            PleyIconButton(
                PleyIcons.SkipBack,
                stringResource(R.string.remote_previous, device),
                onClick = { send(PlayerCommandRequest(Control.Previous)) },
            )
        } else if (next) {
            Spacer(Modifier.size(48.dp))
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
        if (next) {
            PleyIconButton(
                PleyIcons.SkipForward,
                stringResource(R.string.remote_next, device),
                onClick = { send(PlayerCommandRequest(Control.Next)) },
                enabled = session.can(Control.Next),
            )
        } else if (previous) {
            Spacer(Modifier.size(48.dp))
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
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        if (audio.isNotEmpty()) {
            ChoiceMenu(
                icon = PleyIcons.AudioLines,
                hint = stringResource(R.string.remote_audio),
                description = stringResource(R.string.remote_audio_on, device),
                choices = audio.map { it.index to labelOf(it, StreamKind.Audio) },
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
                    subtitles.map { it.index to labelOf(it, StreamKind.Subtitle) },
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

/** A track as the player's own menu labels it ([streamLabel]): "English (EAC3 5.1)", "French (SRT, Forced)". */
private fun labelOf(track: TrackOption, kind: StreamKind): String = streamLabel(
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
)

/** What plays, in one line, as the bar says it: the title as the web writes it ("Show — S1 · E2 — Title"), or the channel ([liveTv] without one). */
private fun titleOf(session: RemoteSession, liveTv: String): String = when (session.session.kind) {
    PlayingKind.Live -> session.session.channel?.name?.takeIf { it.isNotEmpty() } ?: liveTv
    else -> session.session.item?.let(::fullTitle).orEmpty()
}

/** What plays, as the list says it: the title as the web writes it, or the channel's number and name. */
internal fun lineOf(session: RemoteSession, liveTv: String): String {
    val channel = session.session.channel
    return when {
        session.session.kind != PlayingKind.Live -> session.session.item?.let(::fullTitle).orEmpty()
        channel != null -> listOfNotNull(channel.number, channel.name.takeIf { it.isNotEmpty() }).joinToString(" ").ifEmpty { liveTv }
        else -> liveTv
    }
}

/** What plays, in two lines ([Header]); the second `null` when there is nothing to say. */
private fun linesOf(playing: PlayingSession, liveTv: String): Pair<String, String?> {
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
            listOf(item.grandparentTitle.orEmpty(), episodeLabel(item)).filter { it.isNotEmpty() }.joinToString(" · ") to item.title
        item.kind == ItemKind.Track -> item.title to (item.grandparentTitle ?: item.parentTitle)?.takeIf { it.isNotEmpty() }
        else -> item.title to item.year?.toString()
    }
}

/** How often the sheet moves its estimates on. */
private const val TICK_MS = 1_000L

/**
 * How long the bar waits for what this phone sent a device to play there
 * ([RemoteSlot]): long enough for a TV to wake its player and open the
 * title, not so long that a cast that failed makes a later one the phone's.
 */
internal const val CAST_START_WAIT_MS = 30_000L

/** The bar's test tag. */
internal const val REMOTE_BAR = "pley.phone.remote_bar"

/** The remote's button in Home's header. */
internal const val REMOTE_BUTTON = "pley.phone.remote_button"

/** The sheet's test tag. */
internal const val REMOTE_SHEET = "pley.phone.remote_sheet"

/** A device's row in the list: this and its device id. */
internal const val REMOTE_DEVICE = "pley.phone.remote_device."

/** A device's controls: this and its device id. */
internal const val REMOTE_CONTROLS = "pley.phone.remote_controls."

/** What a device plays and its controls: this and its session id. */
internal const val REMOTE_CARD = "pley.phone.remote_card."

/** The Audio, Subtitles and Quality buttons. */
internal const val REMOTE_AUDIO = "pley.phone.remote_audio"
internal const val REMOTE_SUBTITLES = "pley.phone.remote_subtitles"
internal const val REMOTE_QUALITY = "pley.phone.remote_quality"

/** A channel's up and down. */
internal const val REMOTE_ROCKER = "pley.phone.remote_rocker"

/** A channel's programme, with how far it is. */
internal const val REMOTE_LIVE = "pley.phone.remote_live"
