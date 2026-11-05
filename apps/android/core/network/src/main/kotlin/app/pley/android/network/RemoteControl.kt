package app.pley.android.network

import android.content.Context
import app.pley.android.common.DeviceProfile
import app.pley.android.common.UserAgent
import app.pley.android.model.DeviceKind

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

/**
 * Why POST /api/remote/devices/{id}/cast refused (docs/API.md, "Casting"),
 * as Play on… says it.
 */
enum class CastRefusal {
    /** `404`: the device isn't a receiver any more (or what was sent isn't there for the profile). */
    Gone,

    /** `409` `not_castable`: the device doesn't play that kind from a remote. */
    NotCastable,

    /** `429`: more than 10 casts a second to one device. */
    TooMany,

    /** `403` `pin_needed`: to another profile's device, the caller's profile gives its PIN first. */
    PinNeeded,

    /** `403` `wrong_pin`: the PIN sent isn't the caller's profile's. */
    WrongPin,

    /** `403` `pin_locked`: the caller's profile's PIN is locked after too many wrong ones. */
    PinLocked,

    /** `429` to a cast with a PIN: too many wrong PINs lately; the error's sentence says how long to wait. */
    Throttled,

    /** The server can't be reached. */
    Offline,

    /** Anything else (`400`, `403`…): the error's own sentence. */
    Other,
    ;

    companion object {
        /**
         * Why [error] refused a cast; [pinSent]: the cast carried a PIN, whose
         * check comes before the casts are counted, so a `429` is the PIN's.
         */
        fun of(error: Throwable, pinSent: Boolean = false): CastRefusal = when {
            error is OfflineException -> Offline
            error !is ApiException -> Other
            error.status == 404 -> Gone
            error.status == 409 && error.code == "not_castable" -> NotCastable
            error.status == 403 && error.code == "pin_needed" -> PinNeeded
            error.status == 403 && error.code == "wrong_pin" -> WrongPin
            error.status == 403 && error.code == "pin_locked" -> PinLocked
            error.status == 429 && pinSent -> Throttled
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

/**
 * What kind of device this is, sent with its playbacks (`device_kind`), so
 * remotes can pick TVs out: the server can't tell, the same app running
 * on TVs, phones and tablets.
 */
object DeviceKinds {
    /** A phone interface this wide at its narrowest, or wider, is a tablet's: Android's own line between them. */
    const val TABLET_DP = 600

    /** A TV when the app runs its TV interface, else a tablet or a phone by the screen's smallest width. */
    fun of(context: Context, profile: DeviceProfile): DeviceKind =
        of(profile.isTv, context.resources.configuration.smallestScreenWidthDp)

    fun of(tv: Boolean, smallestWidthDp: Int): DeviceKind = when {
        tv -> DeviceKind.Tv
        smallestWidthDp >= TABLET_DP -> DeviceKind.Tablet
        else -> DeviceKind.Phone
    }
}
