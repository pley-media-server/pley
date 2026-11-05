package app.pley.android.screens.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.auth.AuthException
import app.pley.android.auth.DeviceCode
import app.pley.android.auth.DevicePoll
import app.pley.android.common.describe
import app.pley.android.screens.PleyKey
import java.io.IOException
import java.net.URI
import java.net.URISyntaxException
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** The code on screen and where to enter it. */
data class LinkCode(
    /** "BCDF-GHJK". */
    val userCode: String,
    /** Where to enter it, as people type it: "192.168.1.20:7539/link" (no scheme, with the port). */
    val address: String,
    /** What the QR code holds: the address with the code in it. */
    val qr: String,
    /** When the code stops working, in ms since the epoch (the sign-in module's clock, which the view model's is). */
    val expiresAt: Long,
) {
    /** The code's two halves, "BCDF" and "GHJK", for the two fields (one when it isn't 4 + 4). */
    val halves: List<String>
        get() = userCode.split('-').takeIf { it.size == 2 && it.all(String::isNotEmpty) } ?: listOf(userCode)
}

/** Where a code stands. */
sealed interface LinkStatus {
    /** Asking the server for a code. */
    data object Starting : LinkStatus

    /** "Waiting for approval…". */
    data object Waiting : LinkStatus

    /** The server can't be reached: asking again at [retryAt] (the view model's clock). */
    data class Unreachable(val retryAt: Long) : LinkStatus

    /** The approver said no: "Get a new code". */
    data object Declined : LinkStatus

    /** Renewed three times without an answer: "Code expired: press OK for a new one". */
    data object Expired : LinkStatus

    /** "Signed in as Anna", for a moment, then in. */
    data class SignedIn(val name: String?) : LinkStatus

    /** It can't go on: an older server, or a refusal, in the module's words. */
    data class Failed(val message: String) : LinkStatus
}

data class LinkUiState(
    val server: String,
    val code: LinkCode? = null,
    val status: LinkStatus = LinkStatus.Starting,
    /**
     * The server requires two-step sign-in: the phone that approves the
     * code sets it up for whoever hasn't. False until the server says so.
     */
    val twoStepRequired: Boolean = false,
)

/** What the code signs in for. */
sealed interface LinkPurpose {
    /** Signing in to the server (the TV's way in; phones without a browser). */
    data object SignIn : LinkPurpose

    /** Moving to profile [profileId], which needs its password: the approver picks it on /link. */
    data class Profile(val profileId: Long) : LinkPurpose
}

/**
 * Signing in with a code shown on screen (RFC 8628; tv-addendum §4.2-§4.3):
 * the TV's way in, approved on a phone or computer at the server's /link.
 *
 * The code is asked for while the screen is started only ([started],
 * [stopped]): Waiting goes on; slow_down slows down (the sign-in module);
 * a decline shows "Request declined" with "Get a new code"; an expired
 * code is renewed silently up to three times, then waits for OK; network
 * trouble counts down to the next try (2, 4, 8, 16, then 30 s). A code
 * that expired while the screen was away is replaced at once. Once
 * approved, "Signed in as <name>" shows for half a second, then Home.
 *
 * Signing in, the server is asked once whether it requires two-step
 * sign-in, for the screen to say so; the code never waits for the answer.
 */
class LinkViewModel(
    private val source: OnboardingSource,
    val server: String,
    private val purpose: LinkPurpose = LinkPurpose.SignIn,
    /** The sign-in module's clock: codes expire by it. */
    private val clock: () -> Long = System::currentTimeMillis,
) : ViewModel() {
    private val _state = MutableStateFlow(LinkUiState(server))
    val state: StateFlow<LinkUiState> = _state.asStateFlow()

    private val moves = Channel<OnboardingMove>(Channel.BUFFERED)

    /** Home once signed in. */
    val navigation: Flow<OnboardingMove> = moves.receiveAsFlow()

    private var device: DeviceCode? = null
    private var job: Job? = null
    private var visible = false
    private var renewals = 0
    private var startFailures = 0

    init {
        // A profile's approval is about the household, not the server's sign-in.
        if (purpose == LinkPurpose.SignIn) {
            viewModelScope.launch {
                if (source.twoStepRequired(server)) _state.update { it.copy(twoStepRequired = true) }
            }
        }
    }

    /** The screen is started: a code is asked about (a new one if the old one ran out). */
    fun started() {
        visible = true
        val status = _state.value.status
        if (status is LinkStatus.SignedIn || status is LinkStatus.Failed || status == LinkStatus.Declined || status == LinkStatus.Expired) return
        val code = device
        if (code == null || clock() >= code.expiresAt) {
            renewals = 0
            fetch()
        } else {
            poll(code)
        }
    }

    /** The screen is stopped: nothing is asked until it is started again. */
    fun stopped() {
        visible = false
        job?.cancel()
        job = null
    }

    /** "Get a new code" (after a decline, or once the code expired for good). */
    fun newCode() {
        val status = _state.value.status
        if (status is LinkStatus.SignedIn) return
        renewals = 0
        startFailures = 0
        fetch()
    }

    /** Asks for a new code; [silent] (a renewal) keeps the status line as it is meanwhile. */
    private fun fetch(silent: Boolean = false) {
        job?.cancel()
        if (!visible) return
        if (!silent) _state.update { it.copy(status = LinkStatus.Starting) }
        job = viewModelScope.launch {
            val code = try {
                source.deviceSignIn.start(server)
            } catch (e: CancellationException) {
                throw e
            } catch (e: AuthException) {
                _state.update { it.copy(status = LinkStatus.Failed(e.message.orEmpty())) }
                return@launch
            } catch (e: IOException) {
                // Can't reach the server: try again after a while, as the polling does.
                val wait = BACKOFF_MS[minOf(startFailures, BACKOFF_MS.size - 1)]
                startFailures++
                _state.update { it.copy(status = LinkStatus.Unreachable(clock() + wait)) }
                delay(wait)
                fetch(silent = true)
                return@launch
            }
            startFailures = 0
            device = code
            _state.update { it.copy(code = shown(code), status = LinkStatus.Waiting) }
            follow(code)
        }
    }

    private fun poll(code: DeviceCode) {
        job?.cancel()
        job = viewModelScope.launch { follow(code) }
    }

    private suspend fun follow(code: DeviceCode) {
        source.deviceSignIn.poll(code).collect { poll ->
            when (poll) {
                DevicePoll.Waiting -> _state.update { it.copy(status = LinkStatus.Waiting) }
                is DevicePoll.Unreachable -> _state.update { it.copy(status = LinkStatus.Unreachable(clock() + poll.retryInMs)) }
                DevicePoll.Denied -> _state.update { it.copy(status = LinkStatus.Declined) }
                DevicePoll.Expired -> expired()
                is DevicePoll.SignedIn -> signedIn()
                is DevicePoll.Failed -> _state.update { it.copy(status = LinkStatus.Failed(poll.message)) }
            }
        }
    }

    /** The code ran out: a new one, silently, up to [RENEWALS] times while the screen shows. */
    private fun expired() {
        if (renewals < RENEWALS && visible) {
            renewals++
            // Launched apart: this runs inside the job fetch replaces.
            viewModelScope.launch { fetch(silent = true) }
        } else {
            _state.update { it.copy(status = LinkStatus.Expired) }
        }
    }

    private suspend fun signedIn() {
        ProfileGate.passed = true
        val before = source.session.session.value?.me?.id
        // New tokens may replace others for this server (another profile): who the app is follows.
        runCatching { source.session.refresh() }
        // Who the approver signed the app in as, once the session knows (a moment; without the name after 3 s).
        val me = withTimeoutOrNull(NAME_WAIT_MS) {
            source.session.session.first { s ->
                val me = s?.me
                s != null && s.issuer.trimEnd('/') == server.trimEnd('/') && me != null &&
                    (purpose !is LinkPurpose.Profile || me.id != before || me.id == purpose.profileId)
            }?.me
        }
        _state.update { it.copy(status = LinkStatus.SignedIn(me?.name)) }
        viewModelScope.launch {
            runCatching {
                val profiles = source.profiles(server)
                if (profiles.available()) source.household.write(server, profiles.list())
            }
        }
        delay(SIGNED_IN_MS)
        moves.send(OnboardingMove.Restart(PleyKey.Home))
    }

    private fun shown(code: DeviceCode): LinkCode = LinkCode(
        userCode = code.userCode,
        address = addressOf(code.verificationUri),
        qr = code.verificationUriComplete ?: code.verificationUri,
        expiresAt = code.expiresAt,
    )

    companion object {
        /** Silent renewals before "Code expired" (30 minutes of 10-minute codes). */
        const val RENEWALS = 3

        /** "Signed in as Anna" stays this long. */
        const val SIGNED_IN_MS = 500L

        const val NAME_WAIT_MS = 3_000L

        /** The waits after failing to get a code, as the polling's. */
        val BACKOFF_MS = longArrayOf(2_000, 4_000, 8_000, 16_000, 30_000)

        /**
         * The address to show for [uri]: without its scheme, with its port
         * ("192.168.1.20:7539/link"). Not Servers.displayAddress, which
         * drops pley's port.
         */
        fun addressOf(uri: String): String {
            val parsed = try {
                URI(uri)
            } catch (_: URISyntaxException) {
                return uri
            }
            val host = parsed.host ?: return uri
            val port = if (parsed.port == -1) "" else ":${parsed.port}"
            return host + port + parsed.rawPath.orEmpty()
        }
    }
}

/**
 * Whether "Who's watching?" was answered in this process: the TV asks at a
 * cold start only, and not after signing in (the approver picked the
 * profile on /link). And whether it shows now, on a phone or a TV, its PIN
 * pad too: no cast plays over it, since whoever is at the device is
 * choosing, and a cast would choose for them.
 */
object ProfileGate {
    @Volatile
    var passed: Boolean = false

    /** The "Who's watching?" screens showing now (one, but two as one gives way to the next). */
    private val shown = AtomicInteger(0)

    /** Whether "Who's watching?" shows now. */
    val showing: Boolean get() = shown.get() > 0

    /** "Who's watching?" shows, until [hidden]. */
    fun shows() {
        shown.incrementAndGet()
    }

    /** One that [shows] said shows is gone. */
    fun hidden() {
        shown.updateAndGet { (it - 1).coerceAtLeast(0) }
    }

    /**
     * None shows: tests, which share the process, start from this, as a
     * screen torn down with its test needn't say it went.
     */
    fun reset() {
        shown.set(0)
    }
}
