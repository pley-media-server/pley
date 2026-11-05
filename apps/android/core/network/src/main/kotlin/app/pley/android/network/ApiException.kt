package app.pley.android.network

import app.pley.android.auth.R as AuthR
import app.pley.android.common.AppLanguage
import app.pley.android.common.LocalNetwork
import app.pley.android.common.UiText
import app.pley.android.common.UserFacing
import app.pley.android.common.errorText
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
 *
 * A refusal without the server's words has the app's ([text]): its
 * message is them in the language shown when it was made.
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
    override val text: UiText? = null,
) : java.io.IOException(message), UserFacing {
    /** A refusal in the app's own words, [text], which are its message too. */
    constructor(status: Int, code: String?, text: UiText) : this(status, code, AppLanguage.wordsOf(text), text = text)

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
            val written = json?.message?.let(::plain)?.takeIf { it.isNotEmpty() }
                ?: plain(text).takeIf { json == null && it.isNotEmpty() && !text.trimStart().startsWith("<") }
            // Without the server's words, the app's, as signing in says them: "the server refused (HTTP 502)".
            val refused = UiText.res(AuthR.string.auth_refused_status, response.code)
            return ApiException(
                status = response.code,
                code = json?.code?.takeIf { it.isNotEmpty() },
                message = written ?: AppLanguage.wordsOf(refused),
                bearerError = challenge?.error,
                scope = challenge?.scope,
                connections = json?.connections.orEmpty(),
                details = json,
                text = refused.takeIf { written == null },
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
class OfflineException(cause: IOException) : IOException(cause), UserFacing {
    val blocked: Boolean = LocalNetwork.isPermissionBlock(cause)

    /** Why, in a few words: allowing the home network, or what [errorText] says of [cause]. */
    override val text: UiText = if (blocked) UiText.res(R.string.network_not_allowed) else errorText(cause)

    override val message: String get() = AppLanguage.wordsOf(text)
}

/** The server answered with something that isn't what pley answers: another program, or a much older pley. */
class UnreadableAnswerException(val status: Int, cause: Throwable) : IOException(cause), UserFacing {
    override val text: UiText = UNREADABLE_ANSWER

    override val message: String get() = AppLanguage.wordsOf(text)
}

/** What [UnreadableAnswerException] says: "the server's answer can't be read: is it Pley, and up to date?". */
val UNREADABLE_ANSWER: UiText = UiText.res(R.string.network_unreadable_answer)
