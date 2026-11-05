package app.pley.android.auth

import app.pley.android.common.UiText
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/** Why the app was signed out of a server without being asked to, in [text]. */
enum class SignOutReason(val text: UiText) {
    /** The server ended the sign-in: a refresh or a profile switch answered invalid_grant. */
    ENDED(UiText.res(R.string.auth_signed_out)),

    /** This device can't read its tokens anymore (the Keystore's key went with a reset or a restore). */
    UNREADABLE(UiText.res(R.string.auth_unreadable_here)),

    /** The server allowed more than the app asks for, so the app ended that sign-in itself. */
    WIDER_SCOPE(app.pley.android.auth.WIDER_SCOPE),
}

/** Something about signing in that the screens should hear about, wherever it happens. */
sealed interface AuthEvent {
    /** The app holds no tokens for [issuer] anymore: show sign-in. */
    data class SignedOut(val issuer: String, val reason: SignOutReason) : AuthEvent
}

/**
 * The sign-in module's events, one stream per process. Sending never waits:
 * nobody listening means nobody misses it, and a slow listener loses the
 * oldest.
 */
class AuthEvents {
    private val flow = MutableSharedFlow<AuthEvent>(extraBufferCapacity = 16, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    val events: SharedFlow<AuthEvent> = flow.asSharedFlow()

    fun signedOut(issuer: String, reason: SignOutReason) {
        flow.tryEmit(AuthEvent.SignedOut(issuer, reason))
    }
}
