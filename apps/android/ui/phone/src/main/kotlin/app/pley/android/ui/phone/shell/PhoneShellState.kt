package app.pley.android.ui.phone.shell

import android.content.res.Resources
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import app.pley.android.auth.AuthEvent
import app.pley.android.common.describe
import app.pley.android.data.Loadable
import app.pley.android.data.RequestNotice
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.player.NowPlaying
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.ToastHostState
import app.pley.android.screens.R as ScreensR

/**
 * What the shell shows around every screen, and how screens tell it
 * about what went wrong (design spec §3.4, §8):
 *
 * - the toasts, above the navigation bar;
 * - the offline banner, while what shows came from what was saved
 *   because the server can't be reached;
 * - a refusal for want of a scope (403 insufficient_scope): a toast
 *   offering to sign in again, which asks for it;
 * - being signed out by the server: back to signing in, keeping the
 *   server, with the reason.
 *
 * Screens reach it through [LocalPhoneShell].
 */
@Stable
class PhoneShellState internal constructor(
    private val navigation: PhoneNavigation,
    private val resources: Resources,
    server: String?,
) {
    val toasts = ToastHostState()

    internal val offlineState = mutableStateOf(false)

    /** Whether what shows came from what was saved, the server out of reach. */
    val offline: Boolean get() = offlineState.value

    /** The server in use (its origin); null before one is chosen. */
    var server: String? by mutableStateOf(server)
        internal set

    /** Whether the server is scanning a library now (only known while the app is in the foreground). */
    var scanning: Boolean by mutableStateOf(false)
        internal set

    /** The music playing in the background, for the mini player. */
    var nowPlaying: NowPlaying? by mutableStateOf(null)
        internal set

    /** Casting's devices are followed (after the first frame): its bar and remotes may show. */
    var casting: Boolean by mutableStateOf(false)
        internal set

    /** The ways into requests and their number, for Me's tab. */
    var requests: RequestsEntry by mutableStateOf(RequestsEntry.None)
        internal set

    /**
     * A screen's [load], [before] it the one it reported last ([ReportLoad]):
     * saved data shown because the server couldn't be reached raises the
     * offline banner; a fresh answer lowers it only after the same screen's
     * failure, as a fresh one may be the cache's of a minute ago.
     */
    fun report(load: Loadable<*>, before: Loadable<*>?) {
        when {
            load is Loadable.Failed && load.error is OfflineException -> offlineState.value = true
            load is Loadable.Ready && load.fresh && before is Loadable.Failed -> offlineState.value = false
        }
    }

    /** The server's events connected again: it answers now. */
    fun answered() {
        offlineState.value = false
    }

    /**
     * Something a person did failed with [error]: the scope toast for a
     * missing scope, the offline banner when the server is out of reach,
     * otherwise a toast with the error's own sentence.
     */
    fun report(error: Throwable) {
        when {
            error is ApiException && error.needsScope -> toasts.show(
                message = resources.getString(R.string.shell_scope),
                actionLabel = resources.getString(R.string.shell_scope_action),
                onAction = ::signInAgain,
                error = true,
            )
            error is OfflineException -> {
                offlineState.value = true
                toasts.show(sentence(describe(error)), error = true)
            }
            else -> toasts.show(sentence(describe(error)), error = true)
        }
    }

    /** Signs in to the server again (keeping it), or chooses one when there is none. */
    fun signInAgain() {
        navigation.restart(server?.let { PleyKey.SignIn(it) } ?: PleyKey.Servers)
    }

    /** Whether the person is in, past getting in: requests' news waits until then. */
    internal val inside: Boolean get() = navigation.inside

    private val requestTexts by lazy { RequestTexts(resources) }

    /**
     * Tells [notice], news of the person's requests, in a
     * toast with Open: a title ready to watch opens it, the rest open the
     * requests. One that couldn't be sent reads as an error.
     */
    internal fun tell(notice: RequestNotice) {
        val open = (notice as? RequestNotice.Ready)?.itemId?.let { PleyKey.Item(it) } ?: PleyKey.Requests
        toasts.show(
            message = requestTexts.notice(notice),
            actionLabel = resources.getString(ScreensR.string.requests_open),
            onAction = { navigation.open(open) },
            error = notice is RequestNotice.Failed,
        )
    }

    /** The sign-in module ended a sign-in: if it was this server's, back to signing in to it, saying why. */
    internal fun on(event: AuthEvent) {
        when (event) {
            is AuthEvent.SignedOut -> if (signedOut(event.issuer)) toasts.show(sentence(event.reason.message), error = true)
        }
    }

    /**
     * The session says the app is signed out of [issuer] (however that came
     * about, before the shell listened too): if it is this server's, back
     * to signing in to it. Whether the app is signing in to it now.
     */
    internal fun signedOut(issuer: String): Boolean {
        val current = server
        if (current != null && current != issuer) return false
        if (!navigation.inside) return navigation.root.lastOrNull() == PleyKey.SignIn(issuer)
        offlineState.value = false
        navigation.restart(PleyKey.SignIn(issuer))
        return true
    }

    private fun sentence(text: String): String {
        val capital = text.replaceFirstChar { it.uppercase() }
        return if (capital.endsWith('.') || capital.endsWith('…')) capital else "$capital."
    }
}

/** The shell around the screen: toasts, the offline banner, error handling. */
val LocalPhoneShell = staticCompositionLocalOf<PhoneShellState> { error("No phone shell here") }

/** Tells the shell how a screen's [load] went whenever it changes, with the one before (the offline banner). */
@Composable
fun ReportLoad(load: Loadable<*>?) {
    val shell = LocalPhoneShell.current
    val before = remember { arrayOfNulls<Loadable<*>>(1) }
    LaunchedEffect(load, shell) {
        if (load == null || load is Loadable.Loading) return@LaunchedEffect
        shell.report(load, before[0])
        before[0] = load
    }
}
