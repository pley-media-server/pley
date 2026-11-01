package app.pley.android.network

import app.pley.android.common.LocalNetwork
import app.pley.android.common.UserFacing
import app.pley.android.common.describe
import app.pley.android.model.ApiErrorBody
import app.pley.android.model.BusyConnection
import app.pley.android.model.PleyJson
import java.io.IOException
import kotlinx.serialization.SerializationException
import okhttp3.Response

/**
 * An answer the server refused: its status, with the `code` and `message`
 * of its `{ code, message }` JSON (apps/server/src/error.rs), or axum's
 * own text for a request it couldn't read (then [code] is `null`). The
 * message is the server's sentence, written for people.
 *
 * Refusals of an app's token also say why in `WWW-Authenticate`
 * (RFC 6750): [bearerError] is its `error` ("invalid_token",
 * "invalid_request", "insufficient_scope"). A 403 `insufficient_scope`
 * keeps the [scope] that would open the route, `null` when none does.
 *
 * [details] is the rest of pley's JSON body, for the refusals that say
 * more (requests: `setup`, `retry`, `retry_at`, the request as it is now).
 */
class ApiException(
    val status: Int,
    val code: String?,
    message: String,
    val bearerError: String? = null,
    val scope: String? = null,
    /** A `409` from watching a live channel: the connections in use, and which may be taken over. */
    val connections: List<BusyConnection> = emptyList(),
    val details: ApiErrorBody? = null,
) : java.io.IOException(message), UserFacing {
    /** The token doesn't work any more: expired, signed out, or its account disabled. */
    val signedOut: Boolean get() = status == 401 && (bearerError == "invalid_token" || code == "unauthorized")

    /** The app wasn't allowed this: signing in again can ask for [scope]. */
    val needsScope: Boolean get() = status == 403 && (code == "insufficient_scope" || bearerError == "insufficient_scope")

    /** When asking again may work: a decline's 30 days, a limit's 7 (`retry_at`). */
    val retryAt: Long? get() = details?.retryAt

    override fun toString(): String = "ApiException($status, $code, $message)"

    companion object {
        /** The most of a refusal's body read: they are a sentence. */
        private const val MAX_BODY = 16L * 1024

        /** The longest message kept from a body that isn't pley's JSON. */
        private const val MAX_TEXT = 300

        /**
         * The refusal [response] is (a status that isn't 2xx). Reads its
         * body: pley's JSON, or the plain text axum answers for a body or a
         * query it couldn't read (400, 415, 422).
         */
        fun from(response: Response): ApiException {
            val text = try {
                response.peekBody(MAX_BODY).string()
            } catch (_: IOException) {
                ""
            }
            val json = if (text.trimStart().startsWith("{")) {
                try {
                    PleyJson.decodeFromString(ApiErrorBody.serializer(), text)
                } catch (_: SerializationException) {
                    null
                } catch (_: IllegalArgumentException) {
                    null
                }
            } else {
                null
            }
            val challenge = BearerChallenge.of(response.headers("WWW-Authenticate"))
            val message = json?.message?.let(::plain)?.takeIf { it.isNotEmpty() }
                ?: plain(text).takeIf { json == null && it.isNotEmpty() && !text.trimStart().startsWith("<") }
                ?: "the server refused (HTTP ${response.code})"
            return ApiException(
                status = response.code,
                code = json?.code?.takeIf { it.isNotEmpty() },
                message = message,
                bearerError = challenge?.error,
                scope = challenge?.scope,
                connections = json?.connections.orEmpty(),
                details = json,
            )
        }

        /** One short line, without control characters: what a server wrote, safe to show. */
        private fun plain(text: String): String = text
            .lineSequence().firstOrNull { it.isNotBlank() }.orEmpty()
            .filter { !Character.isISOControl(it) && Character.getType(it) != Character.FORMAT.toInt() }
            .trim()
            .take(MAX_TEXT)
    }
}

/**
 * A Bearer challenge of `WWW-Authenticate` (RFC 6750 §3):
 * `Bearer error="insufficient_scope", scope="play"`.
 */
internal data class BearerChallenge(val error: String?, val scope: String?) {
    companion object {
        private val PARAM = Regex("""([A-Za-z_]+)\s*=\s*(?:"((?:[^"\\]|\\.)*)"|([^\s,]+))""")

        /** The first Bearer challenge of [headers], `null` without one. */
        fun of(headers: List<String>): BearerChallenge? {
            val bearer = headers.map { it.trim() }.firstOrNull {
                it.startsWith("Bearer", ignoreCase = true) && (it.length == 6 || it[6].isWhitespace())
            } ?: return null
            val params = PARAM.findAll(bearer.substring(6)).associate { m ->
                m.groupValues[1].lowercase() to (m.groups[2]?.value?.replace(Regex("""\\(.)"""), "$1") ?: m.groupValues[3])
            }
            return BearerChallenge(params["error"], params["scope"]?.takeIf { it.isNotEmpty() })
        }
    }
}

/**
 * The server couldn't be reached: no network, nothing answering, no answer
 * in time. What was saved can still be shown. [blocked]: the system keeps
 * the app off the home network (Android 17's local network access, Fire
 * OS 16's nearby devices), so asking for that permission is the way out.
 */
class OfflineException(cause: IOException) : IOException(messageFor(cause), cause), UserFacing {
    val blocked: Boolean = LocalNetwork.isPermissionBlock(cause)

    private companion object {
        fun messageFor(cause: IOException): String =
            if (LocalNetwork.isPermissionBlock(cause)) "Pley isn't allowed on your home network: allow it in the app's permissions"
            else describe(cause)
    }
}

/** The server answered with something that isn't what pley answers: another program, or a much older pley. */
class UnreadableAnswerException(val status: Int, cause: Throwable) :
    IOException("the server's answer can't be read: is it Pley, and up to date?", cause), UserFacing
