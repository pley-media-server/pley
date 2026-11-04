package app.pley.android.data

import app.pley.android.model.Control
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.network.ApiException
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.abs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
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
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
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
 * A command sent from the remote ([command]), what its session looked
 * like then ([before]), and when, by the remote's clock ([sentAt]).
 * [target]: where it should be once the player acted, for a seek, a skip
 * and a previous that starts the title over.
 */
data class SentCommand(val command: Control, val before: RemoteSession, val sentAt: Long, val target: Long? = null)

/**
 * The remote of the profile's other devices (docs/API.md, "Remote
 * control"): their sessions, from GET /api/sessions, and the commands
 * sent to them.
 *
 * [sessions] are read while something follows them: when first followed,
 * after a profile switch, on `sessions_changed` and `session_stopped`, and
 * each time the event stream connects (nothing is replayed). Those this
 * device plays are left out: its players say which ([here]).
 */
interface RemoteRepository {
    /** The profile's sessions on its other devices, oldest first; empty while none (signed out, an older server). */
    val sessions: StateFlow<List<RemoteSession>>

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
     * [player], on this device, plays [sessionId] now (`null`: none): it
     * is left out of [sessions], as no remote for itself.
     */
    fun here(player: Any, sessionId: String?)

    companion object {
        /** How long the remote waits for a player to show it acted on a command (the web's). */
        const val ANSWER_WAIT_MS = 5_000L

        /** How far the remote's skip buttons go back and forward. */
        const val SKIP_BACK_MS = 10_000L
        const val SKIP_FORWARD_MS = 30_000L

        /** Farther than this from where it should be, a report has moved (the server's rule for a seek). */
        private const val JUMP_MS = 5_000L

        /**
         * Whether the list shows the player acted on [sent], by [now]
         * (the remote's clock), as the web's remote tells (lib/remote.ts):
         * it is gone (next and previous end a session to start the
         * next), plays something else, changed state, or, for a seek, a
         * skip or a previous on the same title, reported since from
         * somewhere new or from where it was sent to. Server times are
         * only compared with server times, and this device's with its own.
         */
        fun answered(sent: SentCommand, current: RemoteSession?, now: Long): Boolean {
            val before = sent.before
            if (current == null) return true
            if (current.session.item?.id != before.session.item?.id || current.session.channel?.id != before.session.channel?.id) return true
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
         * What the remote calls each session's device, by session id: its
         * name, else [unnamed]. Two of the same name (two of the same TV)
         * are numbered by [numbered] ("Living room TV (2)"), so every label
         * and message says which one it means.
         */
        fun deviceNames(sessions: List<RemoteSession>, unnamed: String, numbered: (name: String, n: Int) -> String): Map<String, String> {
            val total = sessions.groupingBy { it.session.device ?: unnamed }.eachCount()
            val seen = HashMap<String, Int>()
            return sessions.associate { s ->
                val name = s.session.device ?: unnamed
                val n = seen.merge(name, 1, Int::plus)!!
                s.sessionId to if ((total[name] ?: 0) > 1) numbered(name, n) else name
            }
        }

        /** No remote: graphs made without one (tests of other screens, previews). */
        val None: RemoteRepository = object : RemoteRepository {
            override val sessions: StateFlow<List<RemoteSession>> = MutableStateFlow(emptyList<RemoteSession>()).asStateFlow()
            override val clock: () -> Long = { System.nanoTime() / 1_000_000 }
            override suspend fun refresh(): List<RemoteSession>? = null
            override suspend fun command(sessionId: String, command: PlayerCommandRequest) = throw unavailable()
            override suspend fun stop(sessionId: String) = throw unavailable()
            override fun here(player: Any, sessionId: String?) = Unit

            private fun unavailable() = ApiException(404, "not_found", "Not found")
        }
    }
}

/** [RemoteRepository] over [cache]'s profile and API, following the server's events from [live]. */
internal class ServerRemote(
    private val cache: ResourceCache,
    private val live: LiveUpdates,
    app: CoroutineScope,
    override val clock: () -> Long,
) : RemoteRepository {
    /** The last list read, and whose. */
    private class Listed(val profile: ProfileScope?, val sessions: List<RemoteSession>)

    private val listed = MutableStateFlow(Listed(null, emptyList()))

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

    override val sessions: StateFlow<List<RemoteSession>> = channelFlow {
        launch { follow() }
        val profile = cache.scopes.map { (it as? ScopeState.Ready)?.profile }
        combine(listed, players, released, profile) { list, here, gone, current ->
            if (current == null || list.profile != current) return@combine emptyList()
            list.sessions.filter { it.sessionId !in here.values && it.sessionId !in gone }
        }.collect { send(it) }
    }.distinctUntilChanged().stateIn(app, SharingStarted.WhileSubscribed(), emptyList())

    /** Reads the list for the profile in use, again at each event that may change it. */
    private suspend fun follow() {
        cache.scopes.map { (it as? ScopeState.Ready)?.profile }.distinctUntilChanged().collectLatest { profile ->
            if (profile == null) return@collectLatest
            merge(flowOf(Unit), asks, live.answered, live.events.filter(::changesSessions).map { })
                .conflate()
                .collect { read(profile) }
        }
    }

    private fun changesSessions(event: ServerEvent): Boolean = event == ServerEvent.SessionsChanged || event is ServerEvent.SessionStopped

    /** GET /api/sessions as [profile], kept with when it came; `null` when the server didn't answer. */
    private suspend fun read(profile: ProfileScope): List<RemoteSession>? {
        val answer = try {
            cache.api(profile).playingSessions()
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            // An older server has no remote: nothing to show.
            if (e.status != 404) return null
            emptyList()
        } catch (_: Exception) {
            // What was shown stays; the next event or connection asks again.
            return null
        }
        val now = clock()
        val sessions = answer.map { RemoteSession(it, now - it.ageMs) }
        // Another profile since it was asked: its list isn't overwritten with this one's.
        if ((cache.scopes.value as? ScopeState.Ready)?.profile != profile) return null
        listed.value = Listed(profile, sessions)
        return sessions
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
