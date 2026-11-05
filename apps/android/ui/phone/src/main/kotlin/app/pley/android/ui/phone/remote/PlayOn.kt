package app.pley.android.ui.phone.remote

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.pley.android.common.sentenceOf
import app.pley.android.data.OtherDevice
import app.pley.android.data.RemoteRepository
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.CastKind
import app.pley.android.model.CastRequest
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlayingSession
import app.pley.android.model.format.isInProgress
import app.pley.android.network.CastRefusal
import app.pley.android.screens.item.TrackChoices
import app.pley.android.screens.onboarding.PinEntry
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PinPad
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.TonalButton
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * What Play on… sends another of the profile's devices (docs/API.md,
 * "Casting"): one of a title, a playlist and a channel; [startMs] where a
 * title starts, where the page's own Play would resume it, and
 * [audioIndex] and [subtitleIndex] ([PlayingSession.SUBTITLES_OFF]: none)
 * the tracks a movie's or an episode's page picked, which the device plays
 * with. What can be shuffled ([shuffles]: an album, an artist, a playlist)
 * is offered shuffled too.
 */
@Immutable
internal data class CastTarget(
    val itemId: Long? = null,
    val playlistId: Long? = null,
    val channelId: Long? = null,
    val startMs: Long? = null,
    val shuffles: Boolean = false,
    val audioIndex: Int? = null,
    val subtitleIndex: Int? = null,
) {
    /** What a device must take to be offered: Live TV for a channel, titles for the rest. */
    val kind: CastKind get() = if (channelId != null) CastKind.Live else CastKind.Item

    /** The body of the cast, [shuffle]d when asked. */
    fun request(shuffle: Boolean): CastRequest = CastRequest(
        itemId = itemId,
        playlistId = playlistId,
        channelId = channelId,
        startMs = startMs,
        shuffle = true.takeIf { shuffle && shuffles },
        audioIndex = audioIndex,
        subtitleIndex = subtitleIndex,
    )

    companion object {
        /** The kinds a remote casts from the library: what plays itself, a show or a season, an album or an artist. */
        private val CASTABLE = setOf(ItemKind.Movie, ItemKind.Episode, ItemKind.Show, ItemKind.Season, ItemKind.Track, ItemKind.Album, ItemKind.Artist)

        /**
         * [item] as its own Play plays it: a movie or an episode from where
         * it was left, when it resumes; a show, a season, an album or an
         * artist as the device's own Play plays them. `null` for what isn't
         * cast (an extra, a collection).
         */
        fun of(item: ItemSummary): CastTarget? {
            if (item.kind !in CASTABLE) return null
            val resumes = (item.kind == ItemKind.Movie || item.kind == ItemKind.Episode) && isInProgress(item)
            return CastTarget(
                itemId = item.id,
                startMs = item.userState.positionMs.takeIf { resumes },
                shuffles = item.kind == ItemKind.Album || item.kind == ItemKind.Artist,
            )
        }

        fun playlist(playlistId: Long): CastTarget = CastTarget(playlistId = playlistId, shuffles = true)

        fun channel(channelId: Long): CastTarget = CastTarget(channelId = channelId)

        /**
         * [item]'s page's Play on…: as [of], with the tracks its pickers
         * show ([tracks], `null` when it has none to pick): the audio when
         * one is picked (otherwise the device plays its best track of the
         * language, as Play does here), the subtitles or none when the file
         * has any.
         */
        fun page(item: ItemSummary, tracks: TrackChoices?): CastTarget? {
            val target = of(item) ?: return null
            if (tracks == null || (item.kind != ItemKind.Movie && item.kind != ItemKind.Episode)) return target
            return target.copy(
                audioIndex = tracks.audioIndex?.takeIf { tracks.audio.isNotEmpty() },
                subtitleIndex = if (tracks.subtitles.isEmpty()) null else tracks.subtitleIndex ?: PlayingSession.SUBTITLES_OFF,
            )
        }
    }
}

/**
 * Where Play on… finds the devices it offers and opens its chooser: the
 * shell's, and the live player's over its own window. Without one (a
 * screen shown on its own), Play on… shows nowhere.
 */
internal interface PlayOnHost {
    /** The profile's other devices that are on, as the remote lists them. */
    val devices: List<OtherDevice>

    /** Opens the chooser for [target]. */
    fun playOn(target: CastTarget)
}

internal val LocalPlayOn = staticCompositionLocalOf<PlayOnHost?> { null }

/** Whether another of the profile's devices that is on plays [kind] when cast to: Play on… shows only then. */
@Composable
internal fun canPlayOn(kind: CastKind): Boolean = LocalPlayOn.current?.devices.orEmpty().any { it.takes(kind) }

/** "Play on…", or "Watch on…" for a channel. */
@Composable
private fun playOnLabel(kind: CastKind): String = stringResource(if (kind == CastKind.Live) R.string.play_on_live else R.string.play_on)

/**
 * Play on… (Watch on… for a channel) beside a page's own Play: a 48 dp
 * square button with the cast icon, as tall as Play and with its corners, or,
 * [labelled], a secondary button with its words where its row has no room
 * left, or, [plain], the icon alone, as a player's bar has its buttons.
 * Nothing while no other device takes what [target] is; it opens
 * the chooser ([LocalPlayOn]), and [onOpen] is told (a sheet it is on
 * makes way).
 */
@Composable
internal fun PlayOnButton(
    target: CastTarget,
    modifier: Modifier = Modifier,
    labelled: Boolean = false,
    plain: Boolean = false,
    onOpen: () -> Unit = {},
) {
    val host = LocalPlayOn.current ?: return
    if (!canPlayOn(target.kind)) return
    val label = playOnLabel(target.kind)
    val open = {
        onOpen()
        host.playOn(target)
    }
    if (labelled) {
        TonalButton(label, onClick = open, icon = PleyIcons.Cast, modifier = modifier.heightIn(min = 48.dp).testTag(PLAY_ON))
    } else if (plain) {
        PleyIconButton(PleyIcons.Cast, label, onClick = open, modifier = modifier.testTag(PLAY_ON))
    } else {
        Box(
            modifier
                .size(48.dp)
                .clip(PleyShape.Control)
                .background(PleyColors.Surface3)
                .clickable(role = Role.Button, onClick = open)
                .semantics { contentDescription = label }
                .testTag(PLAY_ON),
            contentAlignment = Alignment.Center,
        ) {
            Icon(PleyIcons.Cast, contentDescription = null, size = 24.dp, tint = PleyColors.Fg)
        }
    }
}

/**
 * The PIN pad of a cast to a device of another profile ([deviceId]), while
 * it is asked for: [error] after a refusal ([shakes] counts the wrong
 * ones), [lockedOut] once the PIN is locked.
 */
@Immutable
private data class PinAsk(val deviceId: String, val error: String? = null, val shakes: Int = 0, val lockedOut: Boolean = false)

/**
 * Play on…'s chooser, over the page it was asked on: the profile's other
 * devices that take [target], as the remote lists them (their kind's icon,
 * their name, what each plays now or that it is ready), then the
 * household's other profiles' (with the profile each is on). Picking one
 * sends [target] there; once it is on its way, [onCast] is told with the
 * name the device goes by. A refusal says why under the list, and another
 * device can be picked. What shuffles has a Shuffle switch, which makes it
 * Shuffle on…. With no device left to take it, it closes.
 *
 * A device of another profile switches to this one first: the cast names
 * its profile, and, when this profile has a PIN
 * ([RemoteRepository.castNeedsPin], or the server asks for it), the PIN pad
 * shows in the list's place and the cast goes again with it. A wrong PIN
 * shakes and can be typed again; throttled, the server says how long to
 * wait; locked, the pad stops. Back, or the arrow, goes back to the list.
 */
@Composable
internal fun PlayOnSheet(
    target: CastTarget,
    devices: List<OtherDevice>,
    remote: RemoteRepository,
    onCast: (device: OtherDevice, name: String) -> Unit,
    onDismiss: () -> Unit,
) {
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    var shuffle by remember(target) { mutableStateOf(false) }
    var sending by remember(target) { mutableStateOf<String?>(null) }
    var error by remember(target) { mutableStateOf<String?>(null) }
    var asking by remember(target) { mutableStateOf<PinAsk?>(null) }
    // Never shown, never kept: only its length is state.
    val pin = remember(target) { PinEntry() }
    var digits by remember(target) { mutableIntStateOf(0) }
    val names = deviceNames(devices)
    val choices = devices.filter { it.takes(target.kind) && it.receiver != null }
    LaunchedEffect(choices.isEmpty()) { if (choices.isEmpty()) onDismiss() }

    fun ask(next: PinAsk?) {
        pin.clear()
        digits = 0
        asking = next
    }

    fun send(device: OtherDevice, name: String, typed: String?) {
        val receiver = device.receiver ?: return
        error = null
        sending = device.deviceId
        scope.launch {
            try {
                // Another profile's device goes as the one listed, or not at all.
                val profile = device.profile
                val request = target.request(shuffle)
                    .copy(profileId = profile?.id, deviceId = receiver.deviceId.takeIf { profile != null }, pin = typed)
                remote.cast(receiver.receiverId, request)
                ask(null)
                onCast(device, name)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val pad = asking?.takeIf { it.deviceId == device.deviceId } ?: PinAsk(device.deviceId)
                when (CastRefusal.of(e, pinSent = typed != null)) {
                    // A PIN set since GET /api/auth/me said there was none.
                    CastRefusal.PinNeeded -> ask(pad.copy(error = null))
                    CastRefusal.WrongPin -> ask(pad.copy(error = resources.getString(R.string.play_on_wrong_pin), shakes = pad.shakes + 1))
                    CastRefusal.PinLocked -> ask(pad.copy(error = resources.getString(R.string.play_on_pin_locked), lockedOut = true))
                    // The server's sentence says how long to wait.
                    CastRefusal.Throttled -> ask(pad.copy(error = sentenceOf(e)))
                    else -> {
                        ask(null)
                        error = when (CastRefusal.of(e)) {
                            CastRefusal.Gone -> resources.getString(R.string.play_on_gone, name)
                            CastRefusal.NotCastable -> resources.getString(R.string.play_on_cant, name)
                            CastRefusal.TooMany -> resources.getString(R.string.remote_too_many)
                            else -> sentenceOf(e)
                        }
                    }
                }
            } finally {
                sending = null
            }
        }
    }

    // The device the pad is for, while it is still there to cast to.
    val padFor = asking?.let { open -> choices.firstOrNull { it.deviceId == open.deviceId } }
    LaunchedEffect(asking != null, padFor == null) { if (asking != null && padFor == null) ask(null) }
    PleySheet(onDismiss = onDismiss, modifier = Modifier.testTag(PLAY_ON_SHEET)) {
        val shown = asking
        if (padFor != null && shown != null) {
            val name = names.getValue(padFor.deviceId)
            val back = { ask(null) }
            BackHandler(onBack = back)
            Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = 4.dp, end = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                PleyIconButton(PleyIcons.ArrowLeft, stringResource(R.string.play_on_back), onClick = back)
                Spacer(Modifier.width(4.dp))
                Text(
                    name,
                    style = MaterialTheme.typography.titleMedium,
                    color = PleyColors.Fg,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.semantics { heading() },
                )
            }
            PinPad(
                title = stringResource(R.string.play_on_pin_title),
                body = padFor.profile?.let { stringResource(R.string.play_on_pin_body, name, it.name) },
                digits = digits,
                onDigit = { d ->
                    if (!shown.lockedOut && sending == null && pin.add(d)) digits = pin.length
                },
                onDelete = { if (pin.delete()) digits = pin.length },
                onSubmit = {
                    if (!shown.lockedOut && sending == null && pin.complete) {
                        val typed = pin.take()
                        digits = 0
                        send(padFor, name, typed)
                    }
                },
                onCancel = back,
                lockedOutLabel = stringResource(R.string.play_on_back),
                modifier = Modifier.fillMaxWidth().testTag(PLAY_ON_PIN),
                error = shown.error,
                shakes = shown.shakes,
                lockedOut = shown.lockedOut,
                busy = sending != null,
            ) {
                KindBadge(padFor.kind, playing = false)
            }
        } else {
            Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 24.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    when {
                        target.kind == CastKind.Live -> stringResource(R.string.play_on_live)
                        shuffle -> stringResource(R.string.play_on_shuffle)
                        else -> stringResource(R.string.play_on)
                    },
                    style = MaterialTheme.typography.titleMedium,
                    color = PleyColors.Fg,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                if (target.shuffles) {
                    FilterChip(
                        selected = shuffle,
                        onClick = { shuffle = !shuffle },
                        label = { Text(stringResource(R.string.play_on_shuffle_switch)) },
                        leadingIcon = { Icon(PleyIcons.Shuffle, contentDescription = null, size = 18.dp, tint = if (shuffle) PleyColors.AccentBright else PleyColors.Muted) },
                        colors = FilterChipDefaults.filterChipColors(
                            containerColor = PleyColors.Surface3,
                            labelColor = PleyColors.Fg,
                            selectedContainerColor = PleyColors.Surface4,
                            selectedLabelColor = PleyColors.Fg,
                        ),
                        modifier = Modifier.heightIn(min = 48.dp).testTag(PLAY_ON_SHUFFLE),
                    )
                }
            }
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 8.dp)) {
                choices.forEach { device ->
                    key(device.deviceId) {
                        val name = names.getValue(device.deviceId)
                        ChooserRow(device, name, enabled = sending == null) {
                            // Another profile's device, with a PIN to give: the pad first.
                            if (device.profile != null && remote.castNeedsPin) {
                                error = null
                                ask(PinAsk(device.deviceId))
                            } else {
                                send(device, name, typed = null)
                            }
                        }
                    }
                }
                error?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodyMedium,
                        color = PleyColors.DangerText,
                        modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp).semantics { liveRegion = LiveRegionMode.Polite },
                    )
                }
            }
        }
    }
}

/**
 * Play on… over a window of its own, outside the shell (the live player):
 * the devices from [remote] while it shows, and the chooser over it.
 * [onCast] is told once a device took what was sent.
 */
@Composable
internal fun PlayOnWindow(remote: RemoteRepository, onCast: (device: OtherDevice, name: String) -> Unit, content: @Composable () -> Unit) {
    val devices = remote.devices.collectAsStateWithLifecycle()
    val asked = remember { mutableStateOf<CastTarget?>(null) }
    val host = remember(devices) {
        object : PlayOnHost {
            override val devices: List<OtherDevice> get() = devices.value

            override fun playOn(target: CastTarget) {
                asked.value = target
            }
        }
    }
    CompositionLocalProvider(LocalPlayOn provides host, content = content)
    asked.value?.let { target ->
        PlayOnSheet(
            target,
            devices.value,
            remote,
            onCast = { device, name ->
                asked.value = null
                onCast(device, name)
            },
            onDismiss = { asked.value = null },
        )
    }
}

/**
 * A cast sent from a window outside the shell (the live player, which
 * closes then): the shell says "Playing on" the device once it shows
 * again, and opens the remote on it.
 */
internal object CastsSent {
    class Sent(val deviceId: String, val name: String)

    private val sent = MutableStateFlow<Sent?>(null)

    /** The last one not yet said. */
    val pending: StateFlow<Sent?> = sent.asStateFlow()

    fun tell(deviceId: String, name: String) {
        sent.value = Sent(deviceId, name)
    }

    /** [said] was said: it isn't again. */
    fun said(said: Sent) {
        sent.compareAndSet(said, null)
    }
}

/** A device to send to: its kind's icon, its name, and what it plays now or that it is ready (on whose profile, for another's). */
@Composable
private fun ChooserRow(device: OtherDevice, name: String, enabled: Boolean, onClick: () -> Unit) {
    val session = device.session
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 24.dp, vertical = 10.dp)
            .testTag(PLAY_ON_DEVICE + device.deviceId),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        KindBadge(device.kind, playing = session != null)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.Center) {
            Text(name, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Spacer(Modifier.height(2.dp))
            Text(
                session?.let { lineOf(it, stringResource(R.string.remote_live_tv)) } ?: readyLine(device),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** Play on…'s button beside a page's Play. */
internal const val PLAY_ON = "pley.phone.play_on"

/** Its chooser, its Shuffle switch, and a device in it: this and its device id. */
internal const val PLAY_ON_SHEET = "pley.phone.play_on_sheet"
internal const val PLAY_ON_SHUFFLE = "pley.phone.play_on_shuffle"
internal const val PLAY_ON_DEVICE = "pley.phone.play_on_device."

/** The PIN pad a cast to another profile's device asks first. */
internal const val PLAY_ON_PIN = "pley.phone.play_on_pin"
