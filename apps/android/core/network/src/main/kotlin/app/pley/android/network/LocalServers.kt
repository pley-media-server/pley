package app.pley.android.network

import app.pley.android.common.FoundServer
import app.pley.android.common.UiText
import app.pley.android.model.ServerInfo
import kotlinx.coroutines.flow.Flow

/** What a search of the local network brings, as it happens. */
sealed interface DiscoveryEvent {
    /** A server answered (each one once). */
    data class Found(val server: FoundServer) : DiscoveryEvent

    /** The system keeps the app off the network: ask for local network access. */
    data object Blocked : DiscoveryEvent

    /** Nobody else is going to answer. */
    data object Done : DiscoveryEvent
}

/**
 * Finding servers by asking the network: `pley?` broadcast to UDP port
 * 7539 on every Wi-Fi and Ethernet network, a few times, answers listened
 * for a moment.
 */
fun interface Discovery {
    fun search(): Flow<DiscoveryEvent>
}

/**
 * "Search this network": asking every address of the device's network for
 * GET /api/server, for servers that never hear broadcasts (Docker on a
 * bridge network). Servers come as they answer; the flow ends when all did
 * (within 5 s), or fails with an [OfflineException] whose `blocked` is true
 * when the system keeps the app off the network.
 */
fun interface NetworkSweep {
    fun sweep(): Flow<FoundServer>
}

/** What an address turned out to be. */
sealed interface ProbeResult {
    /** A pley server, at [url] (https tried first for names on the internet). */
    data class Pley(val url: String, val info: ServerInfo) : ProbeResult

    /** Something answered, and it isn't pley. */
    data class NotPley(val url: String) : ProbeResult

    /** Nothing answered: [reason] says how, for people ("nothing answers there"). */
    data class Unreachable(val reason: UiText) : ProbeResult

    /** The system keeps the app off the home network. */
    data object Blocked : ProbeResult
}

/** Checks what someone typed is a pley server (GET /api/server, no token). */
fun interface ServerProbe {
    suspend fun probe(input: String): ProbeResult
}

/**
 * The permissions reaching the home network takes (ACCESS_LOCAL_NETWORK on
 * Android 17, NEARBY_WIFI_DEVICES on Fire OS 16), whether the app has them,
 * and whether an error is the system's refusal.
 */
interface LocalNetworkAccess {
    /** Those still to ask for; empty when the app may reach the network. */
    fun missing(): List<String>

    fun isPermissionBlock(error: Throwable): Boolean

    /** Those this device asks for at all, held or not: none below Android 17 (or Fire OS 16). */
    fun required(): List<String> = missing()

    /**
     * Those still to ask for before connecting to [url]: Android 17's
     * local network access for an address at home; nothing for this device
     * itself, Tailscale's addresses or the internet. Searching the network
     * takes all of [missing].
     */
    fun missingFor(url: String): List<String> = missing()
}
