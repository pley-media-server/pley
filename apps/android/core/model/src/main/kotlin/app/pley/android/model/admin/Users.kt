package app.pley.android.model.admin

import app.pley.android.model.RequestAccess
import kotlinx.serialization.Serializable

/**
 * An account or a managed profile, as administrators see it: GET
 * /api/users, and what POST /api/users and PATCH /api/users/{id} answer
 * (apps/server/src/accounts.rs, `UserDto`). Only what Users shows.
 */
@Serializable
data class UserAccount(
    val id: Long,
    val username: String,
    /** The display name, or the username: what to show. */
    val name: String,
    val displayName: String? = null,
    val isAdmin: Boolean = false,
    /** A profile without a password, reached by switching from a home member. */
    val managed: Boolean = false,
    /** Listed in the household's profile switcher. */
    val home: Boolean = false,
    val hasPin: Boolean = false,
    /** "/api/users/2/avatar?v=…", or `null` without a picture. */
    val avatarUrl: String? = null,
    /** "#38bdf8": the circle behind the initial when there's no picture. */
    val avatarColor: String? = null,
    /** False: only the libraries in [libraryIds]. */
    val allLibraries: Boolean = true,
    val libraryIds: List<Long> = emptyList(),
    /** The age limit for movies and shows; `null`: none. */
    val maxAge: Int? = null,
    val disabled: Boolean = false,
    /** False for a managed profile, and for an account without a password until it is given one. */
    val hasPassword: Boolean = true,
    /** Signs in with a code from an authenticator app after the password. */
    val twoStep: Boolean = false,
    /** May it ask for movies and shows ("auto" for administrators); `null` from servers before requests. */
    val requestAccess: RequestAccess? = null,
)

/**
 * POST /api/users: a new account, or a managed profile ([managed], no
 * password, never an administrator). Fields left `null` are left out, so
 * the server's defaults apply.
 */
@Serializable
data class NewUserAccount(
    val username: String,
    val password: String? = null,
    val displayName: String? = null,
    val isAdmin: Boolean = false,
    val managed: Boolean = false,
    val home: Boolean = true,
    val allLibraries: Boolean = true,
    val libraryIds: List<Long> = emptyList(),
    val maxAge: Int? = null,
    val pin: String? = null,
    val requestAccess: RequestAccess? = null,
)

/**
 * A signed-in session of an account (a browser, an app's sign-in before
 * apps had their own): GET /api/users/{id}/devices
 * (apps/server/src/api/accounts.rs, `DeviceDto`).
 */
@Serializable
data class UserDevice(
    val id: String,
    val name: String,
    val userAgent: String? = null,
    val ip: String? = null,
    val lastSeenAt: Long = 0,
)

/**
 * An app using an account, or allowed by it for another profile: GET
 * /api/users/{id}/apps (apps/server/src/api/oauth.rs, `AppGrant`). It acts
 * as [profile]; [approvedBy] allowed it.
 */
@Serializable
data class UserApp(
    val id: Long,
    /** The address of the app's description. */
    val clientId: String,
    /** Pley's own apps: their name. Others: what the app calls itself, which Pley can't check. */
    val name: String? = null,
    /** One of Pley's own apps, built into the server. */
    val firstParty: Boolean = false,
    /** The host of [clientId]. */
    val host: String,
    /** What it may do: "library", "play", "playlists", "profiles"… */
    val scopes: List<String> = emptyList(),
    val profile: UserAppPerson,
    val approvedBy: UserAppPerson,
    /** Of its last sign-in: "Android", "Firefox on Windows". */
    val device: String? = null,
    val ip: String? = null,
    val lastUsedAt: Long = 0,
) {
    /** How it is called: Pley's own apps by name, others by host. */
    val label: String get() = if (firstParty && !name.isNullOrEmpty()) name else host
}

/** Who a [UserApp] acts as, or who allowed it. */
@Serializable
data class UserAppPerson(val id: Long, val name: String)
