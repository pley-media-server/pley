package app.pley.android.ui.phone.remote

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
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
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.common.sentenceOf
import app.pley.android.data.RemoteRepository
import app.pley.android.data.RemoteSession
import app.pley.android.data.SentCommand
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ChannelLogo
import app.pley.android.model.Control
import app.pley.android.model.ItemKind
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingKind
import app.pley.android.model.ProgressState
import app.pley.android.model.format.formatClockMs
import app.pley.android.model.format.fullTitle
import app.pley.android.network.CommandRefusal
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The remote's place above the tabs (over the mini player when both
 * show): the bar while [sessions] has any, and the sheet it opens, which
 * closes when nothing is left to control and opens closed when something
 * plays again.
 */
@Composable
internal fun RemoteSlot(sessions: List<RemoteSession>, remote: RemoteRepository?, origin: String?) {
    if (remote == null) return
    var open by rememberSaveable { mutableStateOf(false) }
    val none = sessions.isEmpty()
    LaunchedEffect(none) { if (none) open = false }
    RemoteBar(sessions, onOpen = { open = true })
    if (open && !none) RemoteSheet(sessions, remote, origin, onDismiss = { open = false })
}

/**
 * The bar above the tabs while the profile plays something on another of
 * its devices (apps/web/src/components/Remote.tsx, as a bar): "Playing on
 * Living room TV" over what plays there, or how many devices and their
 * names. Tapping it opens the remote ([RemoteSheet]).
 */
@Composable
internal fun RemoteBar(sessions: List<RemoteSession>, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    if (sessions.isEmpty()) return
    val unnamed = stringResource(R.string.remote_unknown_device)
    val names = remember(sessions, unnamed) { RemoteRepository.deviceNames(sessions, unnamed) }
    val single = sessions.singleOrNull()
    val headline = if (single != null) {
        stringResource(R.string.remote_playing_on, names.getValue(single.sessionId))
    } else {
        pluralStringResource(R.plurals.remote_playing_on_devices, sessions.size, sessions.size)
    }
    val liveTv = stringResource(R.string.remote_live_tv)
    val detail = if (single != null) titleOf(single, liveTv) else sessions.joinToString(", ") { names.getValue(it.sessionId) }
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
 * The remote: a card for each of the profile's other devices playing,
 * with what plays there, how far (estimated each second while it plays),
 * and only the controls its player obeys, Stop always. Pictures come from
 * [origin], the server in use.
 */
@Composable
internal fun RemoteSheet(sessions: List<RemoteSession>, remote: RemoteRepository, origin: String?, onDismiss: () -> Unit) {
    val unnamed = stringResource(R.string.remote_unknown_device)
    val names = remember(sessions, unnamed) { RemoteRepository.deviceNames(sessions, unnamed) }
    val now by produceState(remote.clock(), remote) {
        while (true) {
            delay(TICK_MS)
            value = remote.clock()
        }
    }
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.remote_title), modifier = Modifier.testTag(REMOTE_SHEET)) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState())) {
            sessions.forEachIndexed { index, session ->
                if (index > 0) HorizontalDivider(color = PleyColors.Surface4)
                key(session.sessionId) {
                    RemoteCard(session, names.getValue(session.sessionId), now, remote, origin)
                }
            }
        }
    }
}

/** One device: what plays there and its controls; a command it didn't act on within [RemoteRepository.ANSWER_WAIT_MS] says so. */
@Composable
private fun RemoteCard(session: RemoteSession, device: String, now: Long, remote: RemoteRepository, origin: String?) {
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val current by rememberUpdatedState(session)
    var sent by remember { mutableStateOf<SentCommand?>(null) }
    var sending by remember { mutableStateOf(false) }
    var stopping by remember { mutableStateOf(false) }
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
        val now = list?.find { it.sessionId == waiting.before.sessionId }
        if (list != null && !RemoteRepository.answered(waiting, now, remote.clock())) say(noAnswer, now?.session?.updatedAt)
    }

    fun refused(e: Exception): String = when (CommandRefusal.of(e)) {
        CommandRefusal.Gone -> resources.getString(R.string.remote_gone, device)
        CommandRefusal.NotControllable -> resources.getString(R.string.remote_cant, device)
        CommandRefusal.TooMany -> resources.getString(R.string.remote_too_many)
        CommandRefusal.Offline, CommandRefusal.Other -> sentenceOf(e)
    }

    fun send(command: Control, positionMs: Long? = null, offsetMs: Long? = null) {
        say(null)
        sending = true
        scope.launch {
            val before = current
            val sentAt = remote.clock()
            val target = RemoteRepository.target(before, command, sentAt, positionMs, offsetMs)
            try {
                remote.command(before.sessionId, PlayerCommandRequest(command, positionMs, offsetMs))
                sent = SentCommand(command, before, sentAt, target)
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
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .testTag(REMOTE_CARD + session.sessionId),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // Stop sits by the device's name, where the transport controls can't take its room.
        Header(session, device, origin) {
            val stopLabel = stringResource(R.string.remote_stop_on, device)
            TextButton(
                onClick = ::stop,
                enabled = !stopping,
                modifier = Modifier.heightIn(min = 48.dp).semantics { contentDescription = stopLabel },
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Muted, disabledContentColor = PleyColors.Disabled),
            ) {
                Icon(PleyIcons.CircleStop, contentDescription = null, size = 16.dp, tint = LocalContentColor.current)
                Spacer(Modifier.width(6.dp))
                Text(stringResource(R.string.remote_stop), style = MaterialTheme.typography.labelLarge)
            }
        }
        // The controls stay as they are while a command waits for its answer (the web's): another can follow it.
        if (!live) Progress(session, device, now, onSeek = { send(Control.Seek, positionMs = it) })
        Controls(session, device, live, ::send)
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
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
    }
}

/** The device's name over the title (or the channel and what's on), beside the poster or the channel's logo, then [end]. */
@Composable
private fun Header(session: RemoteSession, device: String, origin: String?, end: @Composable () -> Unit) {
    val playing = session.session
    val liveTv = stringResource(R.string.remote_live_tv)
    val title = titleOf(session, liveTv)
    Row(verticalAlignment = Alignment.CenterVertically) {
        val channel = playing.channel
        val item = playing.item
        if (playing.kind == PlayingKind.Live && channel != null) {
            val logo = remember(channel.logo, origin) { origin?.let { ArtworkImage.of(channel.logo, it) } }
            ChannelLogo(
                image = logo,
                label = channel.number ?: channel.name.take(3),
                textStyle = MaterialTheme.typography.labelMedium,
                modifier = Modifier.size(56.dp, 40.dp).clip(PleyShape.Field).background(PleyColors.Surface3),
            )
        } else if (item != null) {
            val poster = remember(item, origin) { origin?.let { ArtworkCandidates.poster(item, it) } }
            Artwork(
                poster,
                item.title,
                Modifier.width(40.dp).aspectRatio(if (item.kind == ItemKind.Track) 1f else 2f / 3f).clip(PleyShape.TilePhone),
            )
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(device, style = MaterialTheme.typography.labelMedium, color = PleyColors.Subtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(title, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
            channel?.now?.takeIf { playing.kind == PlayingKind.Live && it.isNotEmpty() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        end()
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

/** The controls the player listed, in the web's order: previous, back 10 s, play or pause, forward 30 s, next. */
@Composable
private fun Controls(
    session: RemoteSession,
    device: String,
    live: Boolean,
    send: (Control, Long?, Long?) -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (session.can(Control.Previous)) {
            PleyIconButton(
                PleyIcons.SkipBack,
                stringResource(if (live) R.string.remote_previous_channel else R.string.remote_previous, device),
                onClick = { send(Control.Previous, null, null) },
            )
        }
        if (session.can(Control.Skip)) {
            PleyIconButton(
                PleyIcons.RotateCcw,
                stringResource(R.string.remote_back, device),
                onClick = { send(Control.Skip, null, -RemoteRepository.SKIP_BACK_MS) },
            )
        }
        val paused = session.session.state == ProgressState.Paused
        val toggle = when {
            paused && session.can(Control.Play) -> Control.Play
            !paused && session.can(Control.Pause) -> Control.Pause
            else -> null
        }
        if (toggle != null) {
            val label = stringResource(if (toggle == Control.Play) R.string.remote_play else R.string.remote_pause, device)
            // A 40 dp circle in a 48 dp touch target.
            Box(
                Modifier
                    .size(48.dp)
                    .clip(PleyShape.Circle)
                    .clickable(role = Role.Button) { send(toggle, null, null) }
                    .semantics { contentDescription = label },
                contentAlignment = Alignment.Center,
            ) {
                Box(Modifier.size(40.dp).clip(PleyShape.Circle).background(PleyColors.Fg), contentAlignment = Alignment.Center) {
                    Icon(if (toggle == Control.Play) PleyIcons.Play else PleyIcons.Pause, contentDescription = null, size = 20.dp, tint = PleyColors.Bg)
                }
            }
        }
        if (session.can(Control.Skip)) {
            PleyIconButton(
                PleyIcons.RotateCw,
                stringResource(R.string.remote_forward, device),
                onClick = { send(Control.Skip, null, RemoteRepository.SKIP_FORWARD_MS) },
            )
        }
        if (session.can(Control.Next)) {
            PleyIconButton(
                PleyIcons.SkipForward,
                stringResource(if (live) R.string.remote_next_channel else R.string.remote_next, device),
                onClick = { send(Control.Next, null, null) },
            )
        }
    }
}

/** What plays: the title as the web writes it ("Severance — S1 · E2 — Half Loop"), or the channel ([liveTv] without one). */
private fun titleOf(session: RemoteSession, liveTv: String): String = when (session.session.kind) {
    PlayingKind.Live -> session.session.channel?.name?.takeIf { it.isNotEmpty() } ?: liveTv
    else -> session.session.item?.let(::fullTitle).orEmpty()
}

/** How often the sheet moves its estimates on. */
private const val TICK_MS = 1_000L

/** The bar's test tag. */
internal const val REMOTE_BAR = "pley.phone.remote_bar"

/** The sheet's test tag. */
internal const val REMOTE_SHEET = "pley.phone.remote_sheet"

/** A device's card's test tag: this and its session id. */
internal const val REMOTE_CARD = "pley.phone.remote_card."
