package app.pley.android.network

import android.content.Context
import app.pley.android.common.UserAgent

/**
 * Why POST /api/sessions/{id}/command refused (docs/API.md, "Remote
 * control"), as a remote says it.
 */
enum class CommandRefusal {
    /** `404`: the session is unknown, stopped, or its player is gone. */
    Gone,

    /** `409` `not_controllable`: its player didn't list the command. */
    NotControllable,

    /** `429`: more than 10 commands a second to one session. */
    TooMany,

    /** The server can't be reached. */
    Offline,

    /** Anything else (`400`, `403`…): the error's own sentence. */
    Other,
    ;

    companion object {
        fun of(error: Throwable): CommandRefusal = when {
            error is OfflineException -> Offline
            error !is ApiException -> Other
            error.status == 404 -> Gone
            error.status == 409 && error.code == "not_controllable" -> NotControllable
            error.status == 429 -> TooMany
            else -> Other
        }
    }
}

/** What remotes call this device, sent with its playbacks (`device_name`). */
object DeviceNames {
    /** The server keeps at most this many characters of it. */
    const val MAX = 64

    /** Its name in Android's settings ("Living room TV"), else its model, as the user agent names it. */
    fun of(context: Context): String? = of(UserAgent.deviceName(context))

    /** [name] trimmed to what the server keeps; `null` when there's nothing left. */
    fun of(name: String?): String? = name?.trim()?.take(MAX)?.trimEnd()?.takeIf { it.isNotEmpty() }
}
