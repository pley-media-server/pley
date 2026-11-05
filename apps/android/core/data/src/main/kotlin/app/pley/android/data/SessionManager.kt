@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.data

import app.pley.android.auth.AuthEvent
import app.pley.android.auth.AuthGraph
import app.pley.android.model.Me
import app.pley.android.model.PleyJson
import app.pley.android.network.PleyApi
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/** A value and whether the server just gave it (else it was saved). */
internal data class Fresh<T>(val value: T, val fresh: Boolean)

/**
 * Who is signed in where, from this device first:
 *
 * 1. the active server (DataStoreServers) and whether the app holds its
 *    tokens (Tokens.signedIn, without the Keystore);
 * 2. the profile last used there and its saved GET /api/auth/me: the
 *    profile's scope is known at once, so its snapshots show;
 * 3. then GET /api/auth/me itself, which may name another profile (switched
 *    elsewhere): the scope follows it.
 *
 * A profile switch drops what is held in memory ([onScopeChange]) and
 * keeps the snapshots of the server's last three profiles; signing out,
 * or being signed out, deletes the server's snapshots and HTTP caches
 * ([forgetData]). Whether the profile is an administrator is never read.
 */
internal class SessionManager(
    private val servers: DataStoreServers,
    private val auth: AuthGraph,
    /** The API of a server before its profile is known: no HTTP cache. */
    private val meApi: (issuer: String) -> PleyApi,
    private val snapshots: FileSnapshotStore,
    private val forgetData: suspend (issuer: String) -> Unit,
    private val app: CoroutineScope,
    private val compute: CoroutineDispatcher,
) : SessionRepository {
    private val _state = MutableStateFlow<SessionState>(SessionState.Starting)
    override val state: StateFlow<SessionState> = _state.asStateFlow()

    override val session: StateFlow<Session?> = _state.map { (it as? SessionState.SignedIn)?.session }
        .stateIn(app, SharingStarted.Eagerly, null)

    private val _scopes = MutableStateFlow<ScopeState>(ScopeState.Waiting)

    /** Whose answers the repositories read. */
    val scopes: StateFlow<ScopeState> = _scopes.asStateFlow()

    /** The profile, and whether the server just said so. */
    val meState = MutableStateFlow<Fresh<Me>?>(null)

    /** Told when the profile in use changes (`null`: none): memory caches go. */
    var onScopeChange: (ProfileScope?) -> Unit = {}

    private val lock = Mutex()

    @Volatile
    private var origin: String? = null

    private var started = false

    /** Starts following the active server and its sign-in; once. */
    fun start() {
        synchronized(this) {
            if (started) return
            started = true
        }
        app.launch {
            servers.activeRead.filterNotNull().collectLatest { active ->
                if (active.isEmpty()) {
                    origin = null
                    meState.value = null
                    setScope(null)
                    _state.value = SessionState.NoServer
                } else {
                    follow(active)
                }
            }
        }
        app.launch {
            auth.events.collect { event ->
                if (event is AuthEvent.SignedOut) {
                    forgetData(event.issuer)
                    servers.setProfile(event.issuer, null)
                }
            }
        }
    }

    /** The server in use, while signed in to it. */
    fun issuer(): String? = (state.value as? SessionState.SignedIn)?.session?.issuer

    private suspend fun follow(active: String) {
        origin = active
        val tokens = withContext(compute) { auth.tokens(active) }
        tokens.signedIn.collectLatest { signedIn ->
            if (!signedIn) {
                meState.value = null
                // The screens' loads fail ("sign in again") rather than wait for a profile.
                setScope(null, ScopeState.SignedOut)
                _state.value = SessionState.SignedOut(active)
            } else {
                resolve(active)
            }
        }
    }

    private suspend fun resolve(active: String) {
        val userId = servers.profileOf(active)
        if (userId != null) {
            val scope = ProfileScope(active, userId)
            val me = readMe(scope)
            lock.withLock {
                if (origin != active) return
                activate(scope)
                meState.value = me?.let { Fresh(it, fresh = false) }
                publish(active, me)
            }
        } else {
            lock.withLock {
                if (origin != active) return
                meState.value = null
                setScope(null)
                publish(active, null)
            }
        }
        fetchMe(active)
    }

    private suspend fun fetchMe(active: String) {
        try {
            updateMe(active, meApi(active).me())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            lock.withLock {
                // Nothing saved says who: the repositories show why.
                if (origin == active && _scopes.value !is ScopeState.Ready && state.value is SessionState.SignedIn) {
                    _scopes.value = ScopeState.Unavailable(e)
                }
            }
        }
    }

    /** [me] is who the app is on [issuer] now: its scope is the one in use, and it is saved. */
    suspend fun updateMe(issuer: String, me: Me) {
        val scope = ProfileScope(issuer, me.id)
        lock.withLock {
            // Signed out or moved on meanwhile: nothing to keep.
            if (origin != issuer || state.value !is SessionState.SignedIn) return
            servers.setProfile(issuer, me.id)
            activate(scope)
            meState.value = Fresh(me, fresh = true)
            publish(issuer, me)
        }
        val bytes = withContext(compute) { PleyJson.encodeToString(Me.serializer(), me).encodeToByteArray() }
        snapshots.write(scope, SnapshotKeys.ME, bytes)
    }

    /** Changes the profile shown at once (an optimistic change); the undo. */
    fun patchMe(change: (Me) -> Me): () -> Unit {
        val before = meState.value ?: return {}
        val after = Fresh(change(before.value), before.fresh)
        meState.value = after
        republish()
        return {
            if (meState.compareAndSet(after, before)) republish()
        }
    }

    private fun republish() {
        val signedIn = state.value as? SessionState.SignedIn ?: return
        meState.value?.value?.let { _state.value = SessionState.SignedIn(signedIn.session.copy(me = it)) }
    }

    override suspend fun refresh() {
        val active = origin ?: return
        if (state.value is SessionState.SignedIn) fetchMe(active)
    }

    override suspend fun switchProfile(profileId: Long, pin: String?) {
        val active = issuer() ?: throw IllegalStateException("not signed in")
        auth.tokens(active).switchProfile(profileId, pin)
        switched(active, profileId)
    }

    override suspend fun switchForCast(profileId: Long, ticket: String) {
        val active = issuer() ?: throw IllegalStateException("not signed in")
        auth.tokens(active).switchProfile(profileId, pin = null, ticket = ticket)
        switched(active, profileId)
    }

    /** The sign-in on [active] moved to [profileId]: its saved things show, as a profile switch shows them. */
    private suspend fun switched(active: String, profileId: Long) {
        servers.setProfile(active, profileId)
        val scope = ProfileScope(active, profileId)
        val me = readMe(scope)
        lock.withLock {
            if (origin != active) return
            activate(scope)
            meState.value = me?.let { Fresh(it, fresh = false) }
            publish(active, me)
        }
        // The profile's saved things show now; who it is exactly follows.
        app.launch { fetchMe(active) }
    }

    override suspend fun signOut() {
        val active = origin ?: return
        // Revokes the grant, then forgets the tokens: Tokens.signedIn turns false.
        auth.tokens(active).signOut()
        // The shells follow the session to sign-in at once, which ends the screen
        // that asked (and its scope): what was saved from the server goes all the same.
        withContext(NonCancellable) {
            forgetData(active)
            servers.setProfile(active, null)
        }
    }

    /** Must hold [lock]. */
    private suspend fun activate(scope: ProfileScope) {
        val current = (_scopes.value as? ScopeState.Ready)?.profile
        if (current == scope) return
        onScopeChange(scope)
        snapshots.activate(scope)
        _scopes.value = ScopeState.Ready(scope)
    }

    /** [scope] `null`: no profile's answers are read, for the reason [none] says. */
    private suspend fun setScope(scope: ProfileScope?, none: ScopeState = ScopeState.Waiting) {
        if (scope == null) {
            if (_scopes.value != none) {
                _scopes.value = none
                onScopeChange(null)
            }
            if (snapshots.active != null) snapshots.activate(null)
        }
    }

    private fun publish(issuer: String, me: Me?) {
        val scopes = me?.scopes?.toSet().orEmpty()
        _state.value = SessionState.SignedIn(Session(issuer = issuer, me = me, scopes = scopes))
    }

    private suspend fun readMe(scope: ProfileScope): Me? {
        val bytes = snapshots.read(scope, SnapshotKeys.ME) ?: return null
        return withContext(compute) {
            try {
                PleyJson.decodeFromString(Me.serializer(), bytes.decodeToString())
            } catch (_: SerializationException) {
                null
            } catch (_: IllegalArgumentException) {
                null
            }
        }
    }
}
