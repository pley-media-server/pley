package app.pley.android.data

import app.pley.android.common.SavedServer
import app.pley.android.model.Me
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

/**
 * The servers this device knows, and the one it uses. The web view app's
 * saved servers move here once, keeping its device id for the player.
 */
interface ServersRepository {
    /** Saved servers, most recently used first. */
    val saved: Flow<List<SavedServer>>

    /** The server in use (its origin); `null` before one is chosen. */
    val active: StateFlow<String?>

    suspend fun use(server: SavedServer)

    /** Forgets [origin]: signs out there (ending the sign-in on the server) and deletes what was saved from it. */
    suspend fun forget(origin: String)

    /**
     * This installation as players tell it apart (`client_id` of
     * POST /api/playback): the server ends a client's earlier sessions by
     * it. The web view app's id is kept, so its sessions are this app's.
     */
    suspend fun clientId(): String
}

/** Who is signed in where. */
data class Session(
    /** The server (its origin, the tokens' issuer). */
    val issuer: String,
    /** The profile, from the snapshot first; `null` until known. */
    val me: Me?,
    /** What the sign-in allows ("library play playlists profiles"). */
    val scopes: Set<String>,
)

/**
 * The signed-in session and its profile. Everything saved is kept per
 * server and profile, so switching profiles shows the other profile's
 * things, never a mix. Whether the profile is an administrator is never
 * read: the app shows the same to everyone.
 */
interface SessionRepository {
    /** The session; `null` when signed out (or not read yet: [state] tells which). */
    val session: StateFlow<Session?>

    /** Where the app stands: still reading, no server, signed out of one, or signed in. */
    val state: StateFlow<SessionState>

    /** Signs out of the active server, ending the sign-in there, and deletes what was saved from it. */
    suspend fun signOut()

    /**
     * Moves the sign-in to [profileId] of the household (with its [pin]
     * when it has one), then shows that profile's things, from what was
     * saved for it first. A wrong PIN changes nothing (the sign-in
     * module's TokenException says why).
     */
    suspend fun switchProfile(profileId: Long, pin: String? = null) {}

    /**
     * Moves the sign-in to [profileId], the profile of a remote that cast
     * here, with the cast's [ticket] in place of a PIN, then shows that
     * profile's things as [switchProfile] does. A ticket spent or expired
     * changes nothing (TokenException, reason `ticket_spent`).
     */
    suspend fun switchForCast(profileId: Long, ticket: String) {}

    /** Asks the server again who the app is (GET /api/auth/me). */
    suspend fun refresh() {}
}

/** Where the app stands with its servers. */
sealed interface SessionState {
    /** Not read yet: a moment at start. */
    data object Starting : SessionState

    /** No server chosen: show the servers. */
    data object NoServer : SessionState

    /** No tokens for [issuer]: show sign-in for it. */
    data class SignedOut(val issuer: String) : SessionState

    data class SignedIn(val session: Session) : SessionState
}

/**
 * The last answer for each screen, on disk per server and profile, so the
 * app shows what it showed before at once, even offline. Read off the main
 * thread; written whole, so a crash never leaves half an answer.
 */
interface SnapshotStore {
    suspend fun read(key: String): ByteArray?

    suspend fun write(key: String, bytes: ByteArray)

    /** Deletes everything saved for [issuer]. */
    suspend fun clear(issuer: String)
}
