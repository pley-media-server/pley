package app.pley.android.auth

import app.pley.android.common.UiText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONException

/** What switching to a profile takes (`needs` of GET /api/oauth/profiles). */
enum class Needs {
    /** Nothing: it switches at once. */
    NONE,

    /** Its PIN. */
    PIN,

    /** Its password, which only the browser asks for: the app signs in again. */
    PASSWORD,

    /** It can't be used from the app. */
    LOCKED,
    ;

    companion object {
        /** Anything this app doesn't know is read as locked. */
        fun of(name: String): Needs = when (name) {
            "none" -> NONE
            "pin" -> PIN
            "password" -> PASSWORD
            else -> LOCKED
        }
    }
}

/** One profile of the household. No usernames, roles or limits: the server doesn't send them to apps. */
data class Profile(
    val id: Long,
    val name: String,
    val avatarUrl: String?,
    val avatarColor: String?,
    val current: Boolean,
    val needs: Needs,
) {
    companion object {
        fun parseList(text: String): List<Profile> {
            val array = try {
                JSONArray(text)
            } catch (_: JSONException) {
                throw AuthException(UNREADABLE)
            }
            return (0 until array.length()).mapNotNull { i ->
                val o = array.optJSONObject(i) ?: return@mapNotNull null
                val id = o.optLong("id", -1).takeIf { it > 0 } ?: return@mapNotNull null
                Profile(
                    id = id,
                    name = plain(o.optString("name")),
                    avatarUrl = if (o.isNull("avatar_url")) null else o.optString("avatar_url").ifEmpty { null },
                    avatarColor = if (o.isNull("avatar_color")) null else o.optString("avatar_color").ifEmpty { null },
                    current = o.optBoolean("current"),
                    needs = Needs.of(o.optString("needs")),
                )
            }
        }
    }
}

/** How a switch went. */
sealed interface SwitchResult {
    data object Switched : SwitchResult

    /** Ask for the profile's PIN, then switch again with it. */
    data object NeedsPin : SwitchResult

    /**
     * The profile needs its password: sign in again in the browser
     * (Auth.begin), where the person picks it. The old sign-in is ended
     * once the new one is in.
     */
    data object SignInAgain : SwitchResult

    /** Refused, in [text]. */
    data class Failed(val text: UiText) : SwitchResult
}

/** How a PIN check of the current profile went (Profiles.confirmPin). */
sealed interface PinCheck {
    data object Confirmed : PinCheck

    /** The server can't check a PIN for the profile the app uses: go on without it. */
    data object Unsupported : PinCheck

    /** Wrong, throttled or locked: the server's reason in words ([text]). Nothing is retried. */
    data class Failed(val text: UiText) : PinCheck
}

/**
 * The household's profiles for pley's own apps, and switching between them
 * with the switch-profile grant: with a PIN when the profile has one; never
 * with a password or a two-step code, which only the browser asks for.
 */
class Profiles(private val api: OkHttpClient, private val tokens: Tokens) {
    /** Whether this sign-in may switch profiles: the server offers it and allowed it. */
    fun available(): Boolean = "profiles" in tokens.scopes()

    /** The household (GET /api/oauth/profiles); empty for someone who isn't a home member. */
    suspend fun list(): List<Profile> = withContext(Dispatchers.IO) {
        val request = Request.Builder().url(tokens.issuer + PATH).header("Accept", "application/json").get().build()
        api.newCall(request).execute().use { response ->
            val text = response.body.string()
            when {
                response.isSuccessful -> Profile.parseList(text)
                response.code == 401 -> throw SignedOutException()
                response.code == 403 -> throw AuthException(CANT_SWITCH)
                else -> throw AuthException(UiText.res(R.string.auth_server_failed, response.code))
            }
        }
    }

    /** Switches to [profile], with [pin] when it needs one. */
    suspend fun switchTo(profile: Profile, pin: String? = null): SwitchResult {
        when (profile.needs) {
            Needs.NONE -> Unit
            Needs.PIN -> if (pin.isNullOrEmpty()) return SwitchResult.NeedsPin
            Needs.PASSWORD -> return SwitchResult.SignInAgain
            Needs.LOCKED -> return SwitchResult.Failed(LOCKED)
        }
        val sent = pin?.takeIf { profile.needs == Needs.PIN }
        return try {
            tokens.switchProfile(profile.id, sent)
            SwitchResult.Switched
        } catch (e: SignedOutException) {
            throw e
        } catch (e: TokenException) {
            val oauth = e.oauth
            when {
                oauth?.error == "invalid_grant" && oauth.reason == "browser" -> SwitchResult.SignInAgain
                // The list was older than the profile's new PIN: the server
                // answers wrong_pin to a switch without one. Ask for it.
                oauth?.reason == "wrong_pin" && sent == null -> SwitchResult.NeedsPin
                else -> SwitchResult.Failed(message(oauth))
            }
        }
    }

    /**
     * Checks [pin] for [profile], the one the app uses already (a TV that
     * starts as a PIN profile asks for it, as Plex does): the switch-profile
     * grant to the same profile, with its PIN. Servers that can't check it
     * for an app answer that the app uses that profile already: then
     * [PinCheck.Unsupported], and the app goes on without it.
     */
    suspend fun confirmPin(profile: Profile, pin: String): PinCheck = try {
        tokens.switchProfile(profile.id, pin)
        PinCheck.Confirmed
    } catch (e: SignedOutException) {
        throw e
    } catch (e: TokenException) {
        val oauth = e.oauth
        if (oauth?.error == "invalid_request" && oauth.reason == null) PinCheck.Unsupported else PinCheck.Failed(message(oauth))
    }

    companion object {
        const val PATH = "/api/oauth/profiles"

        /** A profile that can't be used from the app. */
        val LOCKED: UiText = UiText.res(R.string.auth_profile_locked)

        /** A PIN locked after too many wrong tries. */
        val PIN_LOCKED: UiText = UiText.res(R.string.auth_pin_locked)

        /** A sign-in that wasn't allowed to switch profiles. */
        val CANT_SWITCH: UiText = UiText.res(R.string.auth_cant_switch)

        /**
         * A refused switch in a few words, by the server's reason. Throttled,
         * the server's own sentence says how long to wait.
         */
        fun message(error: OAuthError?): UiText = when (error?.reason) {
            "wrong_pin" -> UiText.res(R.string.auth_wrong_pin)
            "throttled" -> error.description?.let(UiText::raw) ?: UiText.res(R.string.auth_throttled)
            "pin_locked" -> PIN_LOCKED
            "browser" -> UiText.res(R.string.auth_needs_password)
            "not_allowed" -> NOT_ALLOWED
            else -> when (error?.error) {
                "unauthorized_client" -> CANT_SWITCH
                "invalid_request" -> error.description?.let(UiText::raw) ?: NOT_ALLOWED
                else -> error?.description?.let(UiText::raw) ?: UiText.res(R.string.auth_refused)
            }
        }

        private val NOT_ALLOWED = UiText.res(R.string.auth_not_allowed)
    }
}
