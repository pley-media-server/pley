package app.pley.android.testing.screens.onboarding

import android.app.Activity
import app.pley.android.auth.DeviceCode
import app.pley.android.auth.DevicePoll
import app.pley.android.auth.DeviceSignIn
import app.pley.android.auth.Needs
import app.pley.android.auth.PinCheck
import app.pley.android.auth.Profile
import app.pley.android.auth.SignInState
import app.pley.android.common.FoundServer
import app.pley.android.common.SavedServer
import app.pley.android.common.Servers
import app.pley.android.common.UiText
import app.pley.android.data.ServersRepository
import app.pley.android.data.Session
import app.pley.android.data.SessionRepository
import app.pley.android.data.SessionState
import app.pley.android.model.Me
import app.pley.android.model.ServerInfo
import app.pley.android.network.Discovery
import app.pley.android.network.DiscoveryEvent
import app.pley.android.network.LocalNetworkAccess
import app.pley.android.network.NetworkSweep
import app.pley.android.network.ProbeResult
import app.pley.android.network.ServerProbe
import app.pley.android.screens.onboarding.HouseholdStore
import app.pley.android.screens.onboarding.OnboardingSource
import app.pley.android.screens.onboarding.ProfilesSource
import java.net.URI
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import app.pley.android.common.R as CommonR

/**
 * Getting in, as a test drives it: servers saved and found, addresses that
 * answer ([answers]), local network permissions ([missing]), the device
 * grant ([codes], [polls]), the household ([household]) and the session.
 */
class FakeOnboardingSource(
    saved: List<SavedServer> = emptyList(),
    found: List<FoundServer> = emptyList(),
) : OnboardingSource {
    // ---------------------------------------------------------------- servers

    val savedServers = MutableStateFlow(saved)
    val active = MutableStateFlow<String?>(null)
    val used = CopyOnWriteArrayList<SavedServer>()
    val forgotten = CopyOnWriteArrayList<String>()

    override val servers: ServersRepository = object : ServersRepository {
        override val saved: Flow<List<SavedServer>> = savedServers.map { list -> list.sortedByDescending { it.lastUsed } }
        override val active: StateFlow<String?> = this@FakeOnboardingSource.active

        override suspend fun use(server: SavedServer) {
            used += server
            this@FakeOnboardingSource.active.value = server.url
            savedServers.value = savedServers.value.filterNot { it.url == server.url } + server.copy(lastUsed = savedServers.value.size + 1L)
        }

        override suspend fun forget(origin: String) {
            forgotten += origin
            savedServers.value = savedServers.value.filterNot { it.url == origin }
        }

        override suspend fun clientId(): String = "android-test"
    }

    /** What discovery finds, each search. */
    var foundServers: List<FoundServer> = found
    var discoveryBlocked = false
    var searches = 0
    override val discovery: Discovery = Discovery {
        flow {
            searches++
            if (discoveryBlocked) emit(DiscoveryEvent.Blocked)
            foundServers.forEach { emit(DiscoveryEvent.Found(it)) }
            emit(DiscoveryEvent.Done)
        }
    }

    var sweepFinds: List<FoundServer> = emptyList()
    var sweeps = 0
    override val sweep: NetworkSweep = NetworkSweep {
        flow {
            sweeps++
            sweepFinds.forEach { emit(it) }
        }
    }

    /** What each typed text or address answers; anything else is unreachable ([NOTHING_ANSWERS]). */
    val answers = mutableMapOf<String, ProbeResult>()
    val probed = CopyOnWriteArrayList<String>()
    override val probe: ServerProbe = ServerProbe { input ->
        probed += input
        answers[input] ?: ProbeResult.Unreachable(NOTHING_ANSWERS)
    }

    /** Pley at [url], named [name], requiring two-step sign-in or not. */
    fun answer(input: String, url: String = input, name: String = "Living room NAS", twoStepRequired: Boolean = false) {
        answers[input] = ProbeResult.Pley(url, ServerInfo(name, "1.0.0", twoStepRequired = twoStepRequired))
    }

    /** Permissions not held yet (Android 17's local network access…). */
    var missing: List<String> = emptyList()

    /** Addresses whose host is on the internet: they need no permission. */
    var internet: (String) -> Boolean = { url -> runCatching { !Servers.isLocalHost(URI(url).host) }.getOrDefault(false) }
    override val localNetwork: LocalNetworkAccess = object : LocalNetworkAccess {
        override fun missing(): List<String> = this@FakeOnboardingSource.missing
        override fun isPermissionBlock(error: Throwable): Boolean = false
        override fun missingFor(url: String): List<String> = if (internet(url)) emptyList() else this@FakeOnboardingSource.missing
    }

    // ---------------------------------------------------------------- the device grant

    /** The codes [DeviceSignIn.start] gives, in turn (the last one again after). */
    val codes = ArrayDeque<DeviceCode>()
    var startError: Throwable? = null
    var starts = 0

    /** What each poll says after Waiting: the test sends them. */
    val polls = Channel<DevicePoll>(Channel.UNLIMITED)

    /** Polls being collected now. */
    var polling = 0
        private set

    override val deviceSignIn: DeviceSignIn = object : DeviceSignIn {
        override suspend fun start(server: String): DeviceCode {
            starts++
            startError?.let { throw it }
            return if (codes.size > 1) codes.removeFirst() else codes.firstOrNull() ?: code(server)
        }

        override fun poll(code: DeviceCode): Flow<DevicePoll> = flow {
            polling++
            try {
                emit(DevicePoll.Waiting)
                for (poll in polls) {
                    emit(poll)
                    if (poll != DevicePoll.Waiting && poll !is DevicePoll.Unreachable) break
                }
            } finally {
                polling--
            }
        }
    }

    // ---------------------------------------------------------------- the session and the household

    val sessionFlow = MutableStateFlow<Session?>(null)
    var refreshes = 0
    val switches = CopyOnWriteArrayList<Pair<Long, String?>>()

    /** What a switch does: nothing (it works) by default; throw to refuse. */
    var onSwitch: suspend (Long, String?) -> Unit = { _, _ -> }

    override val session: SessionRepository = object : SessionRepository {
        override val session: StateFlow<Session?> = this@FakeOnboardingSource.sessionFlow
        override val state: StateFlow<SessionState> = MutableStateFlow(SessionState.Starting)
        override suspend fun signOut() = Unit

        override suspend fun switchProfile(profileId: Long, pin: String?) {
            switches += profileId to pin
            onSwitch(profileId, pin)
            val issuer = this@FakeOnboardingSource.sessionFlow.value?.issuer ?: return
            val target = profiles.firstOrNull { it.id == profileId } ?: return
            this@FakeOnboardingSource.sessionFlow.value = Session(issuer, Me(target.id, target.name.lowercase(), target.name), emptySet())
        }

        override suspend fun refresh() {
            refreshes++
        }
    }

    /** Signs the session in to [issuer] as [me]. */
    fun signIn(issuer: String, me: Me) {
        sessionFlow.value = Session(issuer, me, setOf("library", "play", "playlists", "profiles"))
    }

    var profiles: List<Profile> = emptyList()
    var canSwitch = true
    var profilesError: Throwable? = null
    val pinChecks = CopyOnWriteArrayList<Pair<Long, String>>()
    var pinCheck: (Profile, String) -> PinCheck = { _, _ -> PinCheck.Confirmed }
    private val stored = mutableMapOf<String, List<Profile>>()
    override val household: HouseholdStore = object : HouseholdStore {
        override suspend fun read(server: String): List<Profile>? = stored[server]
        override suspend fun write(server: String, profiles: List<Profile>) {
            stored[server] = profiles
        }
    }

    /** What the household store holds for [server]. */
    fun savedHousehold(server: String): List<Profile>? = stored[server]

    fun saveHousehold(server: String, profiles: List<Profile>) {
        stored[server] = profiles
    }

    override fun profiles(server: String): ProfilesSource = object : ProfilesSource {
        override fun available(): Boolean = canSwitch
        override suspend fun list(): List<Profile> {
            profilesError?.let { throw it }
            return profiles
        }

        override suspend fun confirmPin(profile: Profile, pin: String): PinCheck {
            pinChecks += profile.id to pin
            return pinCheck(profile, pin)
        }
    }

    // ---------------------------------------------------------------- the browser

    override val signInState = MutableStateFlow<SignInState>(SignInState.Idle)
    val browserSignIns = CopyOnWriteArrayList<String>()
    var browserError: Throwable? = null

    override suspend fun signInWithBrowser(activity: Activity?, server: String) {
        browserSignIns += server
        browserError?.let { throw it }
        signInState.value = SignInState.Idle
    }

    val tokens = mutableSetOf<String>()
    override fun hasTokens(server: String): Boolean = server in tokens

    var demoUrl: String? = null
    override val demo: (suspend () -> String)?
        get() = demoUrl?.let { url -> { url } }

    var homeSubnet: String? = "192.168.1."
    override suspend fun subnet(): String? = homeSubnet

    companion object {
        const val SERVER = "http://192.168.1.20:7539"

        /** A code for [server] that ends [expiresAt] (ms). */
        fun code(server: String, user: String = "BCDF-GHJK", expiresAt: Long = Long.MAX_VALUE, interval: Int = 2) = DeviceCode(
            server = server,
            deviceCode = "device-$user",
            userCode = user,
            verificationUri = "$server/link",
            verificationUriComplete = "$server/link?code=${user.replace("-", "")}",
            expiresAt = expiresAt,
            intervalSeconds = interval,
        )

        /**
         * Anna, Sam (a PIN: 1234), Robin (a password) and Kids (locked), as
         * the profile [currentId] sees them: its own needs nothing.
         */
        fun household(currentId: Long = 1) = listOf(
            Profile(1, "Anna", null, "#e07a5f", current = currentId == 1L, needs = Needs.NONE),
            Profile(2, "Sam", null, "#3d9970", current = currentId == 2L, needs = if (currentId == 2L) Needs.NONE else Needs.PIN),
            Profile(3, "Robin", "/api/users/3/avatar?v=1", null, current = currentId == 3L, needs = if (currentId == 3L) Needs.NONE else Needs.PASSWORD),
            Profile(4, "Kids", null, "#f2cc8f", current = false, needs = Needs.LOCKED),
        )
    }
}

/** What the probe says of an address nothing answers at. */
val NOTHING_ANSWERS: UiText = UiText.res(CommonR.string.error_nothing_answers)
