package app.pley.android.data

import app.pley.android.network.AdminApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

/** Whether the profile's sign-in manages the server ([ServerAdmin.access]). */
enum class AdminAccess {
    /** Not an administrator's account, or a server that can't let apps manage it: nothing to show. */
    None,

    /** An administrator's account signed in before the app could manage the server: signing in again allows it. */
    SignInAgain,

    /** Allowed `server` on an administrator's account. */
    Manages,
}

/**
 * Managing the server from the phone, as Settings in the web app do
 * (docs/ANDROID.md, "Managing the server"). Nothing of it is kept: every
 * page reads the server when it opens, as the web's do.
 */
interface ServerAdmin {
    /** Whether the signed-in profile manages the server, as its sign-in and the server say. */
    val access: Flow<AdminAccess>

    /** The administrators' routes for the signed-in profile; throws when signed out. */
    suspend fun api(): AdminApi

    /** No server to manage: TVs, and tests that don't make one. */
    object None : ServerAdmin {
        override val access: Flow<AdminAccess> = flowOf(AdminAccess.None)

        override suspend fun api(): AdminApi = AdminApi.Unavailable
    }

    companion object {
        /** The scope that lets Pley's own apps manage the server. */
        const val SCOPE = "server"

        /** What [session] allows: the account's role (which the server tells Pley's own apps) and the sign-in's scopes. */
        fun accessOf(session: Session?): AdminAccess = when {
            session?.me?.accountAdmin != true -> AdminAccess.None
            SCOPE in session.scopes -> AdminAccess.Manages
            else -> AdminAccess.SignInAgain
        }
    }
}

/** [ServerAdmin] for the session [sessions] holds, through [cache]'s clients. */
internal class SessionServerAdmin(
    sessions: SessionRepository,
    private val cache: ResourceCache,
) : ServerAdmin {
    override val access: Flow<AdminAccess> = sessions.session.map(ServerAdmin::accessOf).distinctUntilChanged()

    override suspend fun api(): AdminApi = cache.api(cache.profile()).admin
}
