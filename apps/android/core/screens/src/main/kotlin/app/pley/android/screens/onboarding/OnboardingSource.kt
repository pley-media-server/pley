package app.pley.android.screens.onboarding

import android.annotation.SuppressLint
import android.app.Activity
import android.app.Application
import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.auth.AuthException
import app.pley.android.auth.DeviceCode
import app.pley.android.auth.DevicePoll
import app.pley.android.auth.DeviceSignIn
import app.pley.android.auth.PinCheck
import app.pley.android.auth.Profile
import app.pley.android.auth.SignInState
import app.pley.android.data.ServersRepository
import app.pley.android.data.SessionRepository
import app.pley.android.network.Discovery
import app.pley.android.network.LocalNetworkAccess
import app.pley.android.network.NetworkSweep
import app.pley.android.network.ProbeResult
import app.pley.android.network.ServerProbe
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.screensGraph
import java.net.Inet4Address
import java.util.ServiceLoader
import java.util.concurrent.Callable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext

/**
 * The household's profiles on one server, for switching between them:
 * the sign-in module's Profiles, or a fake in tests.
 */
interface ProfilesSource {
    /** Whether this sign-in may switch profiles (it was allowed the `profiles` scope). */
    fun available(): Boolean

    /** GET /api/oauth/profiles. */
    suspend fun list(): List<Profile>

    /** Checks the PIN of [profile], the one the app uses already (a TV starting as it). */
    suspend fun confirmPin(profile: Profile, pin: String): PinCheck
}

/**
 * What getting in reads and changes: the saved servers and the session,
 * finding servers and checking addresses, local network access, signing
 * in (in the browser or with a code) and the household. Made from the
 * app's graph ([of]); tests give their own.
 */
interface OnboardingSource {
    val servers: ServersRepository
    val session: SessionRepository
    val discovery: Discovery
    val sweep: NetworkSweep
    val probe: ServerProbe
    val localNetwork: LocalNetworkAccess
    val deviceSignIn: DeviceSignIn
    val household: HouseholdStore

    /** The browser sign-in's progress (the sign-in module's, one per process). */
    val signInState: StateFlow<SignInState>

    /** Whether the app holds tokens for [server], read without the Keystore. */
    fun hasTokens(server: String): Boolean

    /** The household of [server], over its signed-in client. */
    fun profiles(server: String): ProfilesSource

    /**
     * Opens [server]'s sign-in page from [activity], privately unless the
     * person turned that off. Without an activity there is nothing to open
     * it from.
     */
    suspend fun signInWithBrowser(activity: Activity?, server: String)

    /**
     * "Use demo server" (debug builds): starts the fake server in the
     * app's process and gives its address; `null` in release builds, which
     * don't have it.
     */
    val demo: (suspend () -> String)?

    /** The start of this device's home network's addresses ("192.168.1."); `null` off a home network. */
    suspend fun subnet(): String?

    companion object {
        fun of(graph: ScreensGraph, app: Application): OnboardingSource = GraphOnboardingSource(graph, app)
    }
}

/**
 * The app's [OnboardingSource], in a ViewModel factory's initializer:
 * `viewModel { ServersViewModel(onboardingSource(), …) }`.
 */
fun CreationExtras.onboardingSource(): OnboardingSource {
    val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY]
        ?: error("ViewModels are made with the Application in their CreationExtras")
    return OnboardingSource.of(screensGraph(), app)
}

/**
 * Whether [server] requires two-step sign-in (GET /api/server), for the
 * sign-in screens to say so: false when it says nothing of it or can't be
 * asked. A server at home isn't asked before the system allows the home
 * network: nothing here asks the system for that.
 */
internal suspend fun OnboardingSource.twoStepRequired(server: String): Boolean {
    if (localNetwork.missingFor(server).isNotEmpty()) return false
    val answer = try {
        probe.probe(server)
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        return false
    }
    return answer is ProbeResult.Pley && answer.info.twoStepRequired
}

/**
 * The app's graph, with the sign-in module's work moved off the main
 * thread: keeping tokens writes to disk and opens the Keystore, which the
 * view models' coroutines (on the main thread) mustn't do themselves.
 */
private class GraphOnboardingSource(private val graph: ScreensGraph, private val app: Application) : OnboardingSource {
    override val servers: ServersRepository by lazy {
        val inner = graph.data.servers
        object : ServersRepository by inner {
            override suspend fun forget(origin: String) = withContext(Dispatchers.IO) { inner.forget(origin) }
        }
    }
    override val session: SessionRepository by lazy {
        val inner = graph.data.session
        object : SessionRepository by inner {
            override suspend fun switchProfile(profileId: Long, pin: String?) = withContext(Dispatchers.IO) { inner.switchProfile(profileId, pin) }

            override suspend fun refresh() = withContext(Dispatchers.IO) { inner.refresh() }
        }
    }
    override val discovery: Discovery get() = graph.network.discovery
    override val sweep: NetworkSweep get() = graph.network.sweep
    override val probe: ServerProbe get() = graph.network.probe
    override val localNetwork: LocalNetworkAccess get() = graph.network.localNetwork
    override val deviceSignIn: DeviceSignIn by lazy {
        val inner = graph.auth.deviceSignIn
        object : DeviceSignIn {
            override suspend fun start(server: String): DeviceCode = withContext(Dispatchers.IO) { inner.start(server) }

            override fun poll(code: DeviceCode): Flow<DevicePoll> = inner.poll(code).flowOn(Dispatchers.IO)
        }
    }
    override val household: HouseholdStore by lazy { FileHouseholdStore.of(app) }
    override val signInState: StateFlow<SignInState> get() = graph.auth.signInState

    override fun hasTokens(server: String): Boolean = graph.auth.hasTokens(server)

    override fun profiles(server: String): ProfilesSource {
        // The media client is signed in to the server's own address, as the profiles route needs.
        val profiles = graph.auth.profiles(server, graph.network.media(server))
        return object : ProfilesSource {
            override fun available(): Boolean = profiles.available()

            override suspend fun list(): List<Profile> = profiles.list()

            override suspend fun confirmPin(profile: Profile, pin: String): PinCheck =
                withContext(Dispatchers.IO) { profiles.confirmPin(profile, pin) }
        }
    }

    override suspend fun signInWithBrowser(activity: Activity?, server: String) {
        if (activity == null) throw AuthException("there's no browser on this device to sign in with")
        val private = withContext(Dispatchers.Default) { graph.data.settings.settings.first().privateSignInTab }
        // Keeping the waiting sign-in writes to disk; the tab opens from any thread.
        withContext(Dispatchers.IO) { graph.auth.signInWithBrowser(activity, server, private) }
    }

    override val demo: (suspend () -> String)? = DemoServer.find()

    // Binder calls to the connectivity service: never on the main thread.
    override suspend fun subnet(): String? = withContext(Dispatchers.IO) { HomeNetwork.subnet(app) }
}

/**
 * The fake server debug builds carry (`:testing:fixtures`' DemoServerFactory),
 * found with ServiceLoader so release builds, which don't have it, find
 * nothing.
 */
internal object DemoServer {
    private const val FACTORY = "app.pley.android.fixtures.DemoServerFactory"

    fun find(): (suspend () -> String)? {
        val type = try {
            Class.forName(FACTORY)
        } catch (_: ClassNotFoundException) {
            return null
        }
        val factory = ServiceLoader.load(type, type.classLoader).firstOrNull() as? Callable<*> ?: return null
        return { withContext(Dispatchers.IO) { factory.call() as String } }
    }
}

/** The device's home network, for "Search this network". */
internal object HomeNetwork {
    /** "192.168.1." for a device at 192.168.1.20 on Wi-Fi or Ethernet; `null` without one. */
    // The app's manifest asks for ACCESS_NETWORK_STATE; this library's lint can't see it.
    @SuppressLint("MissingPermission")
    fun subnet(context: Context): String? = try {
        lanSubnet(context)
    } catch (_: SecurityException) {
        null
    }

    @SuppressLint("MissingPermission")
    private fun lanSubnet(context: Context): String? {
        val cm = context.getSystemService(ConnectivityManager::class.java) ?: return null
        val network = cm.activeNetwork ?: return null
        val caps = cm.getNetworkCapabilities(network) ?: return null
        val home = caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) || caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)
        if (!home) return null
        val address = cm.getLinkProperties(network)?.linkAddresses
            ?.map { it.address }
            ?.firstOrNull { it is Inet4Address && !it.isLoopbackAddress }
            ?.hostAddress ?: return null
        return address.substringBeforeLast('.') + "."
    }
}
