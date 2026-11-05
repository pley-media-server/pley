package app.pley.android.model

import kotlinx.serialization.Serializable

/**
 * Who the app acts as: GET /api/auth/me with an app's token, the server's
 * user (apps/server/src/accounts.rs, `UserDto`) with the [scope] the app
 * was allowed. [isAdmin], [allLibraries] and [maxAge] are the app's
 * rights, not the account's: [isAdmin] is always false for an app.
 *
 * Times are unix seconds.
 */
@Serializable
data class Me(
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
    val createdAt: Long = 0,
    /** Default tracks for titles without a choice, see [TrackDefaults]. */
    val audioLanguage: String? = null,
    val subtitleLanguage: String? = null,
    val subtitleMode: SubtitleMode = SubtitleMode.Forced,
    val hasPassword: Boolean = false,
    /** Signs in with a code from an authenticator app after the password. */
    val twoStep: Boolean = false,
    /** What the app was allowed, separated by spaces ("library play"); `null` for a browser's session. */
    val scope: String? = null,
    /** The language Pley is in for the profile ("en", "fr", "es"); `null`: the server's. */
    val language: String? = null,
) {
    /** [scope] as a list: "library", "play", "playlists", "profiles". */
    val scopes: List<String> get() = scope?.split(' ')?.filter { it.isNotEmpty() }.orEmpty()

    /** The track defaults this profile has. */
    val trackDefaults: TrackDefaults get() = TrackDefaults(audioLanguage, subtitleLanguage, subtitleMode)
}

/** PUT /api/me/language: the profile's language ("en", "fr", "es"), `null` for the server's (left out on the wire). */
@Serializable
data class LanguageChoice(val language: String?)

/** What switching to a profile takes (`needs` of GET /api/oauth/profiles). */
@Serializable(with = ProfileNeeds.Serializer::class)
enum class ProfileNeeds(override val wire: String) : WireEnum {
    /** Nothing: it switches at once. */
    None("none"),

    /** Its PIN, typed into the app. */
    Pin("pin"),

    /** Its password, which only the browser asks for: the app signs in again. */
    Password("password"),

    /** It can't be used from the app. */
    Locked("locked"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<ProfileNeeds>("ProfileNeeds", ProfileNeeds.entries, ProfileNeeds.Unknown)
}

/**
 * One profile of the household, as GET /api/oauth/profiles lists them
 * (apps/server/src/api/oauth.rs, `AppProfile`). No usernames, roles or
 * limits: the server doesn't send them to apps. The sign-in module
 * (:core:auth) asks for these; this is their shape.
 */
@Serializable
data class AppProfile(
    val id: Long,
    val name: String,
    val avatarUrl: String? = null,
    val avatarColor: String? = null,
    /** The profile the app acts as. */
    val current: Boolean = false,
    val needs: ProfileNeeds = ProfileNeeds.Unknown,
)
