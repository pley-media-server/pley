package app.pley.android.network

import app.pley.android.auth.NOT_AN_ADDRESS
import app.pley.android.common.Addresses
import app.pley.android.common.DEFAULT_PORT
import app.pley.android.common.FoundServer
import app.pley.android.common.LocalNetwork
import app.pley.android.common.Servers
import app.pley.android.common.UiText
import app.pley.android.common.errorText
import app.pley.android.model.ServerInfo
import java.io.IOException
import java.net.URI
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import app.pley.android.common.R as CommonR

/** What GET /api/server at [origin] answered, if it is pley: a name and a version. */
internal suspend fun OkHttpClient.pleyAt(origin: String): ServerInfo {
    val url = "$origin/api/server".toHttpUrlOrNull() ?: throw IOException("that isn't a server address")
    val request = Request.Builder().url(url).header("Accept", "application/json").get().build()
    return fetch(request) { response ->
        if (response.isRedirect) throw RedirectException(response.code, response.header("Location"))
        val info = response.decode(ServerInfo.serializer())
        if (info.version.isBlank()) throw UnreadableAnswerException(response.code, IllegalStateException("no version"))
        info
    }
}

/** The address answered with a redirect to [location]. */
internal class RedirectException(val status: Int, val location: String?) : IOException("redirected (HTTP $status)")

/** One address of a sweep, and the client that reaches it (through its home network). */
internal class SweepTarget(val origin: String, val client: OkHttpClient)

/**
 * "Search this network": GET /api/server on every address of the device's
 * /24 ([targets]), [parallel] at a time, the whole within [budgetMs]. For
 * servers that never hear broadcasts (Docker on a bridge network). Servers
 * come as they answer; a refusal by the system ends the flow with an
 * [OfflineException] whose `blocked` is true.
 */
internal class HttpNetworkSweep(
    private val targets: () -> List<SweepTarget>,
    private val parallel: Int = PARALLEL,
    private val budgetMs: Long = BUDGET_MS,
) : NetworkSweep {
    override fun sweep(): Flow<FoundServer> = channelFlow {
        val gate = Semaphore(parallel)
        withTimeoutOrNull(budgetMs) {
            coroutineScope {
                for (target in targets()) {
                    launch {
                        gate.withPermit {
                            val info = try {
                                target.client.pleyAt(target.origin)
                            } catch (e: CancellationException) {
                                throw e
                            } catch (e: OfflineException) {
                                if (e.blocked) throw e
                                null
                            } catch (_: IOException) {
                                null
                            }
                            if (info != null) send(FoundServer(target.origin, info.name, id = null, version = info.version))
                        }
                    }
                }
            }
        }
    }

    companion object {
        /** Addresses asked at once. */
        const val PARALLEL = 32

        /** How long an address has to accept the connection: a home network answers in a few ms. */
        const val CONNECT_MS = 400L

        /** How long a sweep takes at most. */
        const val BUDGET_MS = 5_000L

        /** The addresses of [lans]' /24s, on pley's port, each through its own network, made from [base]. */
        fun targetsOf(lans: List<Lan>, base: OkHttpClient, port: Int = DEFAULT_PORT): List<SweepTarget> {
            val seen = mutableSetOf<String>()
            return lans.flatMap { lan ->
                val client = base.newBuilder()
                    .socketFactory(lan.socketFactory)
                    .connectTimeout(CONNECT_MS, TimeUnit.MILLISECONDS)
                    .readTimeout(2, TimeUnit.SECONDS)
                    .followRedirects(false)
                    .build()
                lan.addresses.flatMap { it.neighbours() }
                    .map { "http://${it.hostAddress}:$port" }
                    .filter { seen.add(it) }
                    .map { SweepTarget(it, client) }
            }
        }
    }
}

/**
 * Checks what someone typed is a pley server: GET /api/server, without a
 * token, at each of Servers.candidates in turn (a name on the internet
 * with https first). [client] reaches names on the internet; [localClient]
 * reaches home addresses, giving up on a connection sooner. Neither
 * follows a redirect; one to https on the same host is tried next instead.
 */
internal class HttpServerProbe(
    private val client: OkHttpClient,
    private val localClient: OkHttpClient,
) : ServerProbe {
    override suspend fun probe(input: String): ProbeResult {
        val candidates = ArrayDeque(Servers.candidates(input))
        if (candidates.isEmpty()) return ProbeResult.Unreachable(NOT_AN_ADDRESS)
        val tried = mutableSetOf<String>()
        var notPley: ProbeResult.NotPley? = null
        var failure: Throwable? = null
        while (candidates.isNotEmpty()) {
            val url = candidates.removeFirst()
            if (!tried.add(url)) continue
            val host = try {
                URI(url).host
            } catch (_: Exception) {
                null
            } ?: continue
            val http = if (Servers.isLocalHost(host)) localClient else client
            try {
                return ProbeResult.Pley(url, http.pleyAt(url))
            } catch (e: CancellationException) {
                throw e
            } catch (e: RedirectException) {
                upgradeOf(url, e.location)?.let { candidates.addFirst(it) } ?: run {
                    notPley = notPley ?: ProbeResult.NotPley(url)
                }
            } catch (e: ApiException) {
                notPley = notPley ?: ProbeResult.NotPley(url)
            } catch (e: UnreadableAnswerException) {
                notPley = notPley ?: ProbeResult.NotPley(url)
            } catch (e: IOException) {
                if (LocalNetwork.isPermissionBlock(e)) return ProbeResult.Blocked
                failure = e
            }
        }
        return notPley ?: ProbeResult.Unreachable(failure?.let { errorText(it.cause ?: it) } ?: UiText.res(CommonR.string.error_nothing_answers))
    }

    /** A redirect from http to https on the same host (a proxy that wants https): that origin, else `null`. */
    private fun upgradeOf(url: String, location: String?): String? {
        if (location == null) return null
        val target = url.toHttpUrlOrNull()?.resolve(location) ?: return null
        val from = url.toHttpUrlOrNull() ?: return null
        if (target.scheme != "https" || target.host != from.host) return null
        return Addresses.origin(target.toString())
    }

    companion object {
        /** How long a home address has to accept the connection. */
        const val LOCAL_CONNECT_SECONDS = HttpRoot.HOME_CONNECT_SECONDS

        /** How long a name on the internet has to accept it. */
        const val CONNECT_SECONDS = 10L

        /** How long an answer may take, once connected. */
        const val READ_SECONDS = 10L
    }
}
