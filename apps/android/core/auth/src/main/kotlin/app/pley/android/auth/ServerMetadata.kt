package app.pley.android.auth

import app.pley.android.common.Addresses
import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.common.UserFacing
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * A sign-in problem, in a few words for people ([text]). Its message is
 * those words in the language shown, for logs and for code that still
 * words errors from their message.
 */
open class AuthException(override val text: UiText) : IOException(), UserFacing {
    override val message: String get() = AppLanguage.wordsOf(text)
}

/** What older servers answer: no app sign-in at all. */
val TOO_OLD: UiText = UiText.res(R.string.auth_too_old)

/** What can't be a server's address. */
val NOT_AN_ADDRESS: UiText = UiText.res(R.string.auth_not_an_address)

/**
 * Where a server signs apps in (RFC 8414), read from
 * `<server>/.well-known/oauth-authorization-server`. Only kept when it is
 * this server's own: its issuer is the address the app used and every
 * endpoint is on it, so an answer can't send the app's codes or tokens
 * anywhere else.
 */
data class ServerMetadata(
    val issuer: String,
    val authorizationEndpoint: String,
    val tokenEndpoint: String,
    val revocationEndpoint: String,
    /** The scopes it offers; `null` when it doesn't say. */
    val scopesSupported: List<String>?,
    val grantTypesSupported: List<String>?,
    /**
     * Where a TV asks for a code (RFC 8628 §4), on this server too; `null`
     * when the server has no sign-in with a code.
     */
    val deviceAuthorizationEndpoint: String? = null,
) {
    /** Whether a TV can sign in here with a code: the endpoint, and the grant to collect it. */
    val signsInWithCodes: Boolean
        get() = deviceAuthorizationEndpoint != null && grantTypesSupported.orEmpty().contains(DEVICE_CODE_GRANT)

    companion object {
        const val PATH = "/.well-known/oauth-authorization-server"

        /**
         * Reads the metadata of the server at [server] (its address, as
         * Addresses.origin writes it). [http] follows no redirects.
         */
        suspend fun fetch(http: OkHttpClient, server: String): ServerMetadata = withContext(Dispatchers.IO) {
            val home = Addresses.origin(server) ?: throw AuthException(NOT_AN_ADDRESS)
            val request = Request.Builder().url(home + PATH).header("Accept", "application/json").get().build()
            http.newCall(request).execute().use { response ->
                if (response.isRedirect) {
                    // The issuer must be the address the app used, so another
                    // address can't be the answer.
                    throw AuthException(UiText.res(R.string.auth_redirected))
                }
                if (response.code == 404) throw AuthException(TOO_OLD)
                if (!response.isSuccessful) throw AuthException(UiText.res(R.string.auth_server_failed, response.code))
                val type = response.body.contentType()
                // Older servers answer their web app's page here.
                if (type == null || type.subtype.lowercase() != "json") throw AuthException(TOO_OLD)
                parse(response.body.string(), home)
            }
        }

        /** The checks of [fetch], on the answer [text] of the server at [server]. */
        fun parse(text: String, server: String): ServerMetadata {
            val home = Addresses.origin(server) ?: throw AuthException(NOT_AN_ADDRESS)
            val o = try {
                JSONObject(text)
            } catch (_: JSONException) {
                throw AuthException(TOO_OLD)
            }
            val issuer = o.optString("issuer")
            if (issuer.isEmpty()) throw AuthException(TOO_OLD)
            // RFC 8414 §3.3: the issuer is exactly the address asked at.
            if (issuer != home) {
                throw AuthException(UiText.res(R.string.auth_other_issuer, plain(issuer).take(100)))
            }
            val methods = strings(o.optJSONArray("code_challenge_methods_supported")).orEmpty()
            if ("S256" !in methods) throw AuthException(TOO_OLD)
            fun endpoint(name: String): String {
                val url = o.optString(name)
                if (url.isEmpty()) throw AuthException(TOO_OLD)
                if (Addresses.origin(url) != issuer || !url.startsWith("$issuer/")) {
                    throw AuthException(UiText.res(R.string.auth_other_endpoint))
                }
                return url
            }
            return ServerMetadata(
                issuer = issuer,
                authorizationEndpoint = endpoint("authorization_endpoint"),
                tokenEndpoint = endpoint("token_endpoint"),
                revocationEndpoint = endpoint("revocation_endpoint"),
                scopesSupported = strings(o.optJSONArray("scopes_supported")),
                grantTypesSupported = strings(o.optJSONArray("grant_types_supported")),
                // Optional, but on this server like the others.
                deviceAuthorizationEndpoint = o.optString("device_authorization_endpoint").ifEmpty { null }
                    ?.let { endpoint("device_authorization_endpoint") },
            )
        }

        private fun strings(array: JSONArray?): List<String>? =
            array?.let { a -> (0 until a.length()).mapNotNull { i -> a.optString(i).takeIf { it.isNotEmpty() } } }
    }
}
