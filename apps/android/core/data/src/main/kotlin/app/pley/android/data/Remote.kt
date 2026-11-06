package app.pley.android.data

import app.pley.android.model.CastKind
import app.pley.android.model.CastRequest
import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
import app.pley.android.model.DeviceProfileName
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.model.RemoteDevice
import app.pley.android.model.ServerEvent
import app.pley.android.network.ApiException
import app.pley.android.network.PleyApi
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.abs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * One of the profile's sessions on another of its devices, as the remote
 * keeps it: [reportedAt] is when its player last reported, by this
 * device's clock ([RemoteRepository.clock]): the answer's arrival less its
 * `age_ms`. The server says how long ago a report was rather than when, so
 * the two clocks are never compared.
 */
data class RemoteSession(val session: PlayingSession, val reportedAt: Long) {
    val sessionId: String get() = session.sessionId

    /** Where it is at [now] (the same clock): the last report, moved on by the time since while it plays, never past the end. */
    fun positionAt(now: Long): Long {
        if (session.state != ProgressState.Playing) return session.positionMs
        val at = session.positionMs + maxOf(0, now - reportedAt)
        return session.durationMs?.let { minOf(at, it) } ?: at
    }

    /** Whether its player obeys [control]; Stop always works. */
    fun can(control: Control): Boolean = control in session.controls
}

/**
 * One of the profile's other devices that is on, as the remote lists it
 * ([RemoteRepository.devicesOf]): what it plays now ([session], the newest
 * of its sessions), and its [receiver] when it takes casts; one or both. A
 * device of another of the household's profiles ([profile]) is listed by
 * its receiver only: what it plays stays that profile's.
 */
data class OtherDevice(
    /** Which device, as its sessions and its receiver say. */
    val deviceId: String,
    val receiver: RemoteDevice? = null,
    val session: RemoteSession? = null,
) {
    /** What it is called; `null` when the server can't tell. */
    val name: String? get() = receiver?.name ?: session?.session?.device

    /** What kind of device it is, as its receiver or its player says; [DeviceKind.Unknown] when neither does. */
    val kind: DeviceKind
        get() = receiver?.kind?.takeIf { it != DeviceKind.Unknown } ?: session?.session?.deviceKind ?: DeviceKind.Unknown

    /** Whether it plays something now. */
    val playing: Boolean get() = session != null

    /** The household's other profile it is on, `null` for the profile's own: a cast there switches it first. */
    val profile: DeviceProfileName? get() = receiver?.profile

    /** Whether it plays [kind] when a remote casts it there. */
    fun takes(kind: CastKind): Boolean = receiver?.casts?.contains(kind) == true
}

/**
 * A command sent from the remote ([command]), what its session looked
 * like then ([before]), and when, by the remote's clock ([sentAt]).
 * [target]: where it should be once the player acted, for a seek, a skip
 * and a previous that starts the title over; [streamIndex]: the track an
 * audio or subtitle command picked; [quality]: the quality a quality
 * command picked.
 */
data class SentCommand(
    val command: Control,
    val before: RemoteSession,
    val sentAt: Long,
    val target: Long? = null,
    val streamIndex: Int? = null,
    val quality: String? = null,
)

/**
 * The remote of the profile's other devices (docs/API.md, "Remote
 * control"): their sessions, from GET /api/sessions, the devices that take
 * casts, from GET /api/remote/devices (the household's other profiles'
 * too, each with the profile it is on), and the commands sent to them.
 *
 * Both lists are read while something follows them: when first followed,
 * after a profile switch, on `sessions_changed` and `session_stopped`, and
 * each time the event stream connects (nothing is replayed). What this
 * device plays is left out (its players say which: [here]), and so is its
 * own receiver.
 */
interface RemoteRepository {
    /** The profile's sessions on its other devices, oldest first; empty while none (signed out, an older server). */
    val sessions: StateFlow<List<RemoteSession>>

    /** The profile's other devices that are on, playing or ready to ([devicesOf]); empty while none. */
    val devices: StateFlow<List<OtherDevice>>

    /** The clock [RemoteSession.reportedAt] is on, monotonic, in ms. */
    val clock: () -> Long

    /** Reads the sessions again now, and gives them; `null` when the server didn't answer, or another profile is in use since. */
    suspend fun refresh(): List<RemoteSession>?

    /**
     * POST /api/sessions/{id}/command. A refusal is thrown
     * ([app.pley.android.network.CommandRefusal.of] says which); a session
     * gone (`404`) or no longer obeying it (`409`) has the list read again.
     */
    suspend fun command(sessionId: String, command: PlayerCommandRequest)

    /** DELETE /api/sessions/{id}: stops it on its device at once; the list is read again. */
    suspend fun stop(sessionId: String)

    /**
     * POST /api/remote/devices/{id}/cast: [receiverId]'s device plays
     * [request] as its own Play would, and its session shows here once it
     * starts. To a device of another of the household's profiles, [request]
     * names that profile ([CastRequest.profileId]) and the device switches
     * to this one first, with this profile's PIN ([CastRequest.pin]) when it
     * has one ([castNeedsPin]). A refusal is thrown
     * ([app.pley.android.network.CastRefusal.of] says which); a device gone
     * (`404`) or no longer playing that (`409`) has the lists read again.
     */
    suspend fun cast(receiverId: String, request: CastRequest)

    /**
     * Whether a cast to a device of another profile asks this profile's PIN
     * first: it has one, as GET /api/auth/me last said. A PIN set since is
     * asked for once the server answers `pin_needed`.
     */
    val castNeedsPin: Boolean get() = false

    /**
     * [player], on this device, plays [sessionId] now (`null`: none): it
     * is left out of [sessions], as no remote for itself.
     */
    fun here(player: Any, sessionId: String?)

    companion object {
        /** How long the remote waits for a player to show it acted on a command (the web's). */
        const val ANSWER_WAIT_MS = 5_000L

        /** How far the remote's skip buttons go back and forward. */
        const val SKIP_BACK_MS = 30_000L
        const val SKIP_FORWARD_MS = 30_000L

        /** Farther than this from where it should be, a report has moved (the server's rule for a seek). */
        private const val JUMP_MS = 5_000L

        /**
         * Whether the list shows the player acted on [sent], by [now]
         * (the remote's clock), as the web's remote tells (lib/remote.ts):
         * it is gone (next and previous end a session to start the
         * next), plays something else, says it plays the track or the
         * quality picked, changed state, or, for a seek, a skip or a previous on the same
         * title, reported since from somewhere new or from where it was
         * sent to. Server times are only compared with server times, and
         * this device's with its own.
         */
        fun answered(sent: SentCommand, current: RemoteSession?, now: Long): Boolean {
            val before = sent.before
            if (current == null) return true
            if (current.session.item?.id != before.session.item?.id || current.session.channel?.id != before.session.channel?.id) return true
            // A track or a quality is answered by the player saying it plays it; a session started for it says so as it
            // starts.
            when (sent.command) {
                Control.Audio -> return current.session.audioIndex == sent.streamIndex
                Control.Subtitles -> return current.session.subtitleIndex == sent.streamIndex
                Control.Quality -> return current.session.quality == sent.quality
                else -> Unit
            }
            if (current.session.state != before.session.state) return true
            if (sent.command != Control.Seek && sent.command != Control.Skip && sent.command != Control.Previous) return false
            val reportedSince = current.session.updatedAt > before.session.updatedAt || current.session.positionMs != before.session.positionMs
            if (!reportedSince) return false
            val moved = abs(current.session.positionMs - before.positionAt(current.reportedAt)) > JUMP_MS
            // It may have played on from the target since, for as long as the command has been out.
            val there = sent.target != null && abs(current.session.positionMs - sent.target) <= JUMP_MS + maxOf(0, now - sent.sentAt)
            return moved || there
        }

        /**
         * Where [command] should take [session], sent at [now]: a seek's
         * [positionMs], a skip's [offsetMs] from where it is (within the
         * title), the start for a previous on a title; `null` otherwise.
         */
        fun target(session: RemoteSession, command: Control, now: Long, positionMs: Long? = null, offsetMs: Long? = null): Long? = when {
            positionMs != null -> positionMs
            offsetMs != null -> (session.positionAt(now) + offsetMs).coerceAtMost(session.session.durationMs ?: Long.MAX_VALUE).coerceAtLeast(0)
            // A previous a few seconds in starts the title over, as players do.
            command == Control.Previous && session.session.kind != PlayingKind.Live -> 0
            else -> null
        }

        /**
         * [session] as the remote shows it while [sent] is out (the web's
         * lib/remote.ts): as the command asks, played or paused, at the
         * place a seek or a skip goes, with the track or quality picked, so
         * the controls don't wait on the player's report to move. Once the
         * wait is over without an answer, the session as it is shows again.
         */
        fun asSent(session: RemoteSession, sent: SentCommand?): RemoteSession {
            if (sent == null) return session
            val playing = session.session
            // A place given at the moment the command went, moving on from there while it plays.
            fun at(positionMs: Long, state: ProgressState = playing.state) = RemoteSession(playing.copy(state = state, positionMs = positionMs), sent.sentAt)
            return when (sent.command) {
                // Where the bar showed it as it went: a skip just before it still counts.
                Control.Play -> at(sent.target ?: session.positionAt(sent.sentAt), ProgressState.Playing)
                Control.Pause -> at(sent.target ?: session.positionAt(sent.sentAt), ProgressState.Paused)
                Control.Audio -> session.copy(session = playing.copy(audioIndex = sent.streamIndex ?: playing.audioIndex))
                Control.Subtitles -> session.copy(session = playing.copy(subtitleIndex = sent.streamIndex ?: playing.subtitleIndex))
                Control.Quality -> session.copy(session = playing.copy(quality = sent.quality ?: playing.quality))
                else -> sent.target?.let { at(it) } ?: session
            }
        }

        /** Whether a player may end its session for [command], to start another: the next or previous title, or a track or a quality it plays anew. */
        fun mayEndSession(command: Control): Boolean =
            command == Control.Next || command == Control.Previous || command == Control.Audio || command == Control.Subtitles || command == Control.Quality

        /**
         * [devices] with [ended] still in, playing it, while its device plays
         * nothing else (the web's): a command may end a session to start
         * another ([mayEndSession]), and between the two the device would look
         * as if it played nothing, or leave the list if it takes no casts.
         */
        fun withEnded(devices: List<OtherDevice>, ended: RemoteSession?): List<OtherDevice> {
            if (ended == null || devices.any { it.deviceId == deviceOf(ended) && it.playing }) return devices
            return devicesOf(devices.mapNotNull { it.session } + ended, devices.mapNotNull { it.receiver })
        }

        /** Which device plays [session]: its device id, else (a server before it said) the session alone. */
        fun deviceOf(session: RemoteSession): String = session.session.deviceId.ifEmpty { "session:${session.sessionId}" }

        /**
         * The devices [sessions] and [receivers] are of, as the remote
         * lists them: each with its newest session (sessions come oldest
         * first) and its receiver, a session whose device takes no casts (a
         * Jellyfin or Plex app) included; those that play first, then the
         * profile's own, then the household's other profiles', each by
         * name, those without one last.
         */
        fun devicesOf(sessions: List<RemoteSession>, receivers: List<RemoteDevice>): List<OtherDevice> {
            val newest = LinkedHashMap<String, RemoteSession>()
            for (session in sessions) newest[deviceOf(session)] = session
            val byDevice = receivers.associateBy { it.deviceId.ifEmpty { "receiver:${it.receiverId}" } }
            val devices = (byDevice.keys + newest.keys).map { id -> OtherDevice(id, byDevice[id], newest[id]) }
            return devices.sortedWith(compareBy<OtherDevice>({ !it.playing }, { it.profile != null }, { it.name == null }, { it.name?.lowercase() }))
        }

        /**
         * What the remote calls each device, by its id: its name, else
         * [unnamed]. Two of the same name (two of the same TV) are numbered
         * by [numbered] ("Living room TV (2)") in [devices]' order, so every
         * label and message says which one it means.
         */
        fun deviceNames(devices: List<OtherDevice>, unnamed: String, numbered: (name: String, n: Int) -> String): Map<String, String> {
            val total = devices.groupingBy { it.name ?: unnamed }.eachCount()
            val seen = HashMap<String, Int>()
            return devices.associate { d ->
                val name = d.name ?: unnamed
                val n = seen.merge(name, 1, Int::plus)!!
                d.deviceId to if ((total[name] ?: 0) > 1) numbered(name, n) else name
            }
        }

        /** No remote: graphs made without one (tests of other screens, previews). */
        val None: RemoteRepository = object : RemoteRepository {
            override val sessions: StateFlow<List<RemoteSession>> = MutableStateFlow(emptyList<RemoteSession>()).asStateFlow()
            override val devices: StateFlow<List<OtherDevice>> = MutableStateFlow(emptyList<OtherDevice>()).asStateFlow()
            override val clock: () -> Long = { System.nanoTime() / 1_000_000 }
            override suspend fun refresh(): List<RemoteSession>? = null
            override suspend fun command(sessionId: String, command: PlayerCommandRequest) = throw unavailable()
            override suspend fun stop(sessionId: String) = throw unavailable()
            override suspend fun cast(receiverId: String, request: CastRequest) = throw unavailable()
            override fun here(player: Any, sessionId: String?) = Unit

            private fun unavailable() = ApiException(404, "not_found", "Not found")
        }
    }
}

/**
 * [RemoteRepository] over [cache]'s profile and API, following the server's
 * events from [live]. [ownReceiver] is this device's receiver id, which the
 * remote leaves out.
 */
internal class ServerRemote(
    private val cache: ResourceCache,
    private val live: LiveUpdates,
    app: CoroutineScope,
    override val clock: () -> Long,
    private val ownReceiver: suspend () -> String? = { null },
    /** Whether the profile in use has a PIN, as GET /api/auth/me last said. */
    private val hasPin: () -> Boolean = { false },
) : RemoteRepository {
    /** The last lists read, and whose. */
    private class Listed(val profile: ProfileScope?, val sessions: List<RemoteSession>, val receivers: List<RemoteDevice>)

    private val listed = MutableStateFlow(Listed(null, emptyList(), emptyList()))

    /** The sessions this device's players play, by player. */
    private val players = MutableStateFlow<Map<Any, String>>(emptyMap())

    /**
     * The last sessions this device's players let go, newest last: the
     * list read before the server heard of it still has them, as this
     * device's, and nothing can control them any more.
     */
    private val released = MutableStateFlow<List<String>>(emptyList())

    /** Asks to read the list again, however many come while a read is under way. */
    private val asks = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** What the remote shows of the lists: the profile's, without this device's own. */
    private class Shown(val sessions: List<RemoteSession>, val receivers: List<RemoteDevice>)

    private val shown: Flow<Shown> = channelFlow {
        launch { follow() }
        val profile = cache.scopes.map { (it as? ScopeState.Ready)?.profile }
        val own = flow { emit(ownId()) }
        combine(listed, players, released, profile, own) { list, here, gone, current, mine ->
            if (current == null || list.profile != current) return@combine Shown(emptyList(), emptyList())
            Shown(
                list.sessions.filter { it.sessionId !in here.values && it.sessionId !in gone },
                list.receivers.filter { it.receiverId != mine },
            )
        }.collect { send(it) }
    }.shareIn(app, SharingStarted.WhileSubscribed(), replay = 1)

    override val sessions: StateFlow<List<RemoteSession>> =
        shown.map { it.sessions }.distinctUntilChanged().stateIn(app, SharingStarted.WhileSubscribed(), emptyList())

    override val devices: StateFlow<List<OtherDevice>> =
        shown.map { RemoteRepository.devicesOf(it.sessions, it.receivers) }.distinctUntilChanged().stateIn(app, SharingStarted.WhileSubscribed(), emptyList())

    /** This device's receiver id; `null` when it can't be read (then none is left out). */
    private suspend fun ownId(): String? = try {
        ownReceiver()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    /** Reads the lists for the profile in use, again at each event that may change them. */
    private suspend fun follow() {
        cache.scopes.map { (it as? ScopeState.Ready)?.profile }.distinctUntilChanged().collectLatest { profile ->
            if (profile == null) return@collectLatest
            merge(flowOf(Unit), asks, live.answered, live.events.filter(::changesSessions).map { })
                .conflate()
                .collect { read(profile) }
        }
    }

    // A receiver appearing or going sends sessions_changed too.
    private fun changesSessions(event: ServerEvent): Boolean = event == ServerEvent.SessionsChanged || event is ServerEvent.SessionStopped

    /**
     * GET /api/sessions and GET /api/remote/devices as [profile], kept with
     * when they came; the sessions, or `null` when the server didn't answer.
     * Devices that couldn't be read stay as they were.
     */
    private suspend fun read(profile: ProfileScope): List<RemoteSession>? = coroutineScope {
        val api = cache.api(profile)
        val receivers = async { receiversOf(api) }
        val answer = try {
            api.playingSessions()
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            // An older server has no remote: nothing to show.
            if (e.status != 404) {
                receivers.cancel()
                return@coroutineScope null
            }
            emptyList()
        } catch (_: Exception) {
            // What was shown stays; the next event or connection asks again.
            receivers.cancel()
            return@coroutineScope null
        }
        val devices = receivers.await()
        val now = clock()
        val sessions = answer.map { RemoteSession(it, now - it.ageMs) }
        // Another profile since it was asked: its list isn't overwritten with this one's.
        if ((cache.scopes.value as? ScopeState.Ready)?.profile != profile) return@coroutineScope null
        val before = listed.value.takeIf { it.profile == profile }
        listed.value = Listed(profile, sessions, devices ?: before?.receivers.orEmpty())
        sessions
    }

    /** The profile's receivers; none from a server without them (404), `null` when it didn't answer. */
    private suspend fun receiversOf(api: PleyApi): List<RemoteDevice>? = try {
        api.remoteDevices()
    } catch (e: CancellationException) {
        throw e
    } catch (e: ApiException) {
        if (e.status == 404) emptyList() else null
    } catch (_: Exception) {
        null
    }

    override suspend fun refresh(): List<RemoteSession>? {
        val profile = (cache.scopes.value as? ScopeState.Ready)?.profile ?: return null
        return read(profile)
    }

    override suspend fun command(sessionId: String, command: PlayerCommandRequest) {
        try {
            api().sendCommand(sessionId, command)
        } catch (e: ApiException) {
            // Its player is gone, or no longer obeys that: the list says what is left.
            if (e.status == 404 || e.status == 409) asks.tryEmit(Unit)
            throw e
        }
    }

    override suspend fun stop(sessionId: String) {
        try {
            api().stopSession(sessionId)
        } finally {
            asks.tryEmit(Unit)
        }
    }

    override suspend fun cast(receiverId: String, request: CastRequest) {
        try {
            api().cast(receiverId, request)
        } catch (e: ApiException) {
            // Its app closed, or takes other things now: the lists say what is left.
            if (e.status == 404 || e.status == 409) asks.tryEmit(Unit)
            throw e
        }
    }

    override val castNeedsPin: Boolean get() = hasPin()

    override fun here(player: Any, sessionId: String?) {
        var before: String? = null
        players.update {
            before = it[player]
            if (sessionId != null) it + (player to sessionId) else it - player
        }
        val gone = before?.takeIf { it != sessionId } ?: return
        released.update { (it - gone + gone).takeLast(RELEASED_KEPT) }
    }

    private suspend fun api() = cache.api(cache.profile())

    private companion object {
        /** How many sessions let go are left out: more than a device ends before the list is read again. */
        const val RELEASED_KEPT = 32
    }
}
