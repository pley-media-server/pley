package app.pley.android.auth

import app.pley.android.common.AddressKind
import app.pley.android.common.Addresses
import app.pley.android.common.Servers
import java.io.IOException
import java.net.URI
import java.net.URISyntaxException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ConcurrentLinkedQueue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/** Where the app heard of one of a server's addresses. */
enum class AddressSource {
    /** The address signed in at: the server's name on this device. */
    SIGN_IN,

    /** The server's own list (GET /api/server/addresses). */
    SERVER,

    /** Added on this device. */
    ADDED,
}

/** One of a server's addresses: its origin, and where the app heard of it. */
data class ServerAddress(val url: String, val source: AddressSource) {
    val kind: AddressKind get() = Servers.kindOf(url)
}

/** What the app knows of an address, on the networks the device is on. */
enum class AddressStatus {
    /** The server's requests go there. */
    IN_USE,

    /** It proved it is the server, and another address is in use. */
    READY,

    /** Not tried on these networks yet. */
    UNTRIED,

    /** Nothing answered there. */
    UNREACHABLE,

    /** Something answers there that can't prove it is the server. */
    OTHER_DEVICE,

    /** A home address, while the device is on no Wi-Fi or Ethernet network. */
    NEEDS_HOME_NETWORK,

    /** Plain http beyond the home network: no token goes there. */
    UNENCRYPTED,
}

/** An address and what the app knows of it. */
data class AddressState(val address: ServerAddress, val status: AddressStatus)

/** A server's addresses as the screens show them: the one in use, and each in the order they're tried. */
data class AddressesState(val inUse: String, val addresses: List<AddressState>)

/**
 * The addresses of the server signed in at [issuer], and the one its
 * requests go to (docs/ANDROID.md, "Home and away").
 *
 * The sign-in address stays the server's name on this device: tokens,
 * caches and saved things are kept under it, whichever address answers.
 * Before a token goes to an address, the server there proves it holds the
 * sign-in's key ([prove]), as at the sign-in address. Choosing one has
 * every address prove itself at once and takes the best that does (a home
 * network's, then a VPN's, then the internet's) without waiting long for a
 * better one; a better one that proves itself later takes over. An address
 * that failed is left alone for a while (5 s, then 15 s, a minute, five
 * minutes), until the device changes networks.
 */
internal class ServerAddresses(
    val issuer: String,
    /**
     * Has the server at an address prove it holds the sign-in's key, within
     * the time given; throws [NotYourServerException], or an IOException
     * when nothing answers.
     */
    private val prove: suspend (address: String, timeoutMs: Long) -> Unit,
    /** Keeps the addresses besides the sign-in's (TokenStore.saveAddresses). */
    private val keep: (StoredAddresses) -> Unit,
    /** Where proofs that outlive their caller run. */
    private val scope: CoroutineScope,
    private val clock: () -> Long,
    /** Whether the device is on a Wi-Fi or Ethernet network, where a home address can answer. */
    private val homeNetwork: () -> Boolean = { ServerProofs.homeNetwork },
) {
    private class Proof(val at: Long, val network: Int)

    private class Failure(val count: Int, val until: Long, val network: Int, val otherDevice: Boolean)

    /** The server's list and the ones added here (guarded by this). */
    private var listed = StoredAddresses()

    @Volatile
    private var urls: List<String> = listOf(issuer)

    /** [urls] as OkHttp reads them, to tell a request's address the way it does. */
    @Volatile
    private var parsed: List<Pair<String, HttpUrl>> = parse(urls)

    /** Where the server's requests go: the sign-in address until another is chosen. */
    @Volatile
    var inUse: String = issuer
        private set

    private val proofs = ConcurrentHashMap<String, Proof>()
    private val failures = ConcurrentHashMap<String, Failure>()

    /** Better addresses being tried while another is in use. */
    private val trying = ConcurrentHashMap.newKeySet<String>()

    /** The choice under way, which every caller waits for together (guarded by this). */
    private var choosing: CompletableDeferred<String>? = null

    /** One proof at a time of an address asked for itself. */
    private val proving = Mutex()

    private val _state = MutableStateFlow(stateNow())

    /** The addresses and what the app knows of each, for the screens. */
    val state: StateFlow<AddressesState> = _state.asStateFlow()

    /** Every address: the sign-in's, then the server's in its order, then the ones added here. */
    fun all(): List<ServerAddress> {
        val (server, added) = synchronized(this) { listed.server to listed.added }
        val seen = mutableSetOf<String>()
        return buildList {
            if (seen.add(issuer)) add(ServerAddress(issuer, AddressSource.SIGN_IN))
            for (url in server) if (seen.add(url)) add(ServerAddress(url, AddressSource.SERVER))
            for (url in added) if (seen.add(url)) add(ServerAddress(url, AddressSource.ADDED))
        }
    }

    /**
     * The address of this server that [url] is on (the same scheme, host
     * and port, as OkHttp parsed both); `null` when it is on none.
     */
    fun addressOf(url: HttpUrl): String? = parsed.firstOrNull { (_, address) -> onServer(url, address) }?.first

    /** Takes back what [keep] kept. */
    fun restore(kept: StoredAddresses?) {
        if (kept == null) return
        synchronized(this) { listed = StoredAddresses(clean(kept.server, MAX_LISTED), clean(kept.added, MAX_ADDED)) }
        changed(save = false)
    }

    /** The server's own list (GET /api/server/addresses): kept until it gives another. */
    fun fromServer(list: List<String>) {
        val server = clean(list, MAX_LISTED)
        val updated = synchronized(this) { (server != listed.server).also { if (it) listed = listed.copy(server = server) } }
        if (updated) changed()
    }

    /**
     * Adds [url], typed on this device: `false` when it isn't an address,
     * is one of the server's already, or is the internet's, which only the
     * server's own list gives (an address anyone may name would otherwise
     * be one step from a token).
     */
    fun add(url: String): Boolean {
        val origin = originOf(url) ?: return false
        if (Servers.kindOf(origin) == AddressKind.INTERNET) return false
        val added = synchronized(this) {
            if (origin in urls || listed.added.size >= MAX_ADDED) return false
            listed = listed.copy(added = listed.added + origin)
            true
        }
        if (added) changed()
        return added
    }

    /** Removes [url], when it was added on this device. */
    fun remove(url: String): Boolean {
        val origin = originOf(url) ?: return false
        val removed = synchronized(this) {
            if (origin !in listed.added) return false
            listed = listed.copy(added = listed.added - origin)
            true
        }
        proofs.remove(origin)
        failures.remove(origin)
        if (inUse == origin) inUse = issuer
        changed()
        return removed
    }

    /** Forgets everything but the sign-in address: signed out. */
    fun forget() {
        synchronized(this) { listed = StoredAddresses() }
        forgetProofs()
        changed(save = false)
    }

    /** Forgets what the addresses proved and how they failed: a new sign-in, a new key. */
    fun forgetProofs() {
        proofs.clear()
        failures.clear()
        inUse = issuer
        publish()
    }

    /**
     * An address that proved itself on these networks within
     * [ServerProofs.TTL_MS]: the one in use while it has, else one chosen
     * now. [fresh]: one that proves itself now, as a refresh token or a PIN
     * needs. Throws [NotYourServerException] when every address answered
     * with something that can't prove it is the server, or an IOException
     * when none can be reached.
     */
    suspend fun proved(fresh: Boolean = false): String {
        val current = inUse
        if (fresh) {
            try {
                proveNow(current, raced = false)
                return current
            } catch (e: IOException) {
                if (urls.size == 1) throw e
            }
        } else if (isProved(current)) {
            tryBetter()
            return current
        }
        return choose()
    }

    /** Has [address] prove itself, unless it did on these networks within [ServerProofs.TTL_MS]. */
    suspend fun ensureProved(address: String) {
        if (isProved(address)) return
        proving.withLock {
            // Proved by another caller meanwhile.
            if (!isProved(address)) proveNow(address, raced = false)
        }
    }

    /**
     * A request to [address] couldn't reach it: it waits before it is tried
     * again, and the next requests go to the address chosen then.
     */
    fun unanswered(address: String) = failed(address, otherDevice = false)

    /**
     * A request to [address] reached it and got no answer in the end (a
     * read that timed out, a connection dropped): it proves itself again
     * before its next token, without waiting first.
     */
    fun doubt(address: String) {
        proofs.remove(address)
        publish()
    }

    /** Every address proves itself again, now; the best that does is used. */
    suspend fun checkAgain() {
        proofs.clear()
        failures.clear()
        publish()
        try {
            choose()
        } catch (_: IOException) {
            // What each address answered shows in [state].
        }
    }

    /**
     * Has every candidate prove itself at once and returns the best that
     * does, without waiting for worse ones: the first that proves itself
     * once every better one failed, or [HEAD_START_MS] after it proved
     * itself while a better one still hasn't answered. A better one that
     * proves itself later still takes over. Callers that come while a
     * choice is under way wait for that one: requests that find the server
     * out of reach fail together, not one after another.
     */
    private suspend fun choose(): String {
        val winner = synchronized(this) {
            // Chosen by another caller meanwhile.
            inUse.takeIf { isProved(it) }?.let { return it }
            choosing?.takeIf { !it.isCompleted } ?: race().also { choosing = it }
        }
        return winner.await()
    }

    /** Starts a choice among the candidates; its winner. */
    private fun race(): CompletableDeferred<String> {
        val candidates = candidates()
        if (candidates.isEmpty()) return CompletableDeferred<String>().apply { completeExceptionally(IOException("no address of the server can be used")) }
        val race = Race(candidates.map { it.url })
        // Alone, an address has the time a request has; among others, a limit, as a better one may answer meanwhile.
        val raced = candidates.size > 1
        for ((i, address) in candidates.withIndex()) {
            // In the background: an address that answers late still counts, and may be better.
            scope.launch {
                try {
                    proveNow(address.url, raced)
                    race.proved(i)
                } catch (e: IOException) {
                    race.failed(i, e)
                }
            }
        }
        return race.winner
    }

    /** One choice among [urls], best first (see [choose]). */
    private inner class Race(private val urls: List<String>) {
        val winner = CompletableDeferred<String>()

        /** Per candidate: `null` while it hasn't answered, then whether it proved itself. */
        private val results = arrayOfNulls<Boolean>(urls.size)
        private val errors = ConcurrentLinkedQueue<IOException>()

        /** The candidate taken (guarded by this). */
        private var chosen: String? = null

        fun proved(i: Int) {
            val (waitForBetter, taken) = synchronized(this) {
                results[i] = true
                val decided = decide()
                (!decided && chosen == null) to chosen
            }
            if (waitForBetter) {
                scope.launch {
                    delay(HEAD_START_MS)
                    synchronized(this@Race) { if (chosen == null) take(results.indexOfFirst { it == true }) }
                }
            }
            // Proved after another was taken: it takes over when better.
            if (taken != null && taken != urls[i]) upgrade(urls[i])
        }

        fun failed(i: Int, error: IOException) {
            errors += error
            synchronized(this) {
                results[i] = false
                decide()
            }
        }

        /** Takes the best proved candidate once no better one may still prove itself; whether one is taken. */
        private fun decide(): Boolean {
            if (chosen != null) return true
            val best = results.indexOfFirst { it == true }
            if (best < 0) {
                if (results.all { it == false }) winner.completeExceptionally(failureOf(errors))
                return false
            }
            if ((0 until best).any { results[it] == null }) return false
            take(best)
            return true
        }

        private fun take(i: Int) {
            // Removed meanwhile (Addresses, or the server's list): the next best.
            if (urls[i] !in this@ServerAddresses.urls) {
                results[i] = false
                decide()
                return
            }
            chosen = urls[i]
            inUse = urls[i]
            winner.complete(urls[i])
            publish()
        }
    }

    /** [address] proved itself after another was chosen: in use when it is better and still proved. */
    private fun upgrade(address: String) {
        synchronized(this) {
            if (rank(address) < rank(inUse) && isProved(address)) inUse = address
        }
        publish()
    }

    /**
     * While an address is in use, the better ones that failed are tried
     * again in the background once their wait is over: coming home, the
     * home address takes over without a request failing first.
     */
    private fun tryBetter() {
        val current = inUse
        val better = candidates().takeWhile { it.url != current }.filter { !isProved(it.url) && !waiting(it.url) }
        for (address in better) {
            if (!trying.add(address.url)) continue
            scope.launch {
                try {
                    proveNow(address.url, raced = true)
                    upgrade(address.url)
                } catch (_: IOException) {
                    // Tried again after its wait.
                } finally {
                    trying -= address.url
                }
            }
        }
    }

    /**
     * Has [address] prove itself: within [RACED_PROOF_MS] when [raced]
     * among others, else in the time any request has. Whatever goes wrong
     * on the way is an IOException: the address failed, never the app.
     */
    private suspend fun proveNow(address: String, raced: Boolean) {
        val network = ServerProofs.network
        try {
            prove(address, if (raced) RACED_PROOF_MS else 0)
        } catch (e: CancellationException) {
            throw e
        } catch (e: AuthException) {
            // The sign-in, not the address: signed out meanwhile, the tokens unreadable.
            throw e
        } catch (e: NotYourServerException) {
            failed(address, otherDevice = true)
            throw e
        } catch (e: IOException) {
            failed(address, otherDevice = false)
            throw e
        } catch (e: Exception) {
            failed(address, otherDevice = false)
            throw IOException("$address can't be asked", e)
        }
        proofs[address] = Proof(clock(), network)
        failures.remove(address)
        publish()
    }

    private fun failed(address: String, otherDevice: Boolean) {
        proofs.remove(address)
        val network = ServerProofs.network
        val count = (failures[address]?.takeIf { it.network == network }?.count ?: 0) + 1
        val wait = BACKOFF_MS[minOf(count, BACKOFF_MS.size) - 1]
        failures[address] = Failure(count, clock() + wait, network, otherDevice)
        publish()
    }

    // A clock set back makes an age below zero: proved again. An address removed meanwhile isn't the server's.
    private fun isProved(address: String): Boolean = address in urls &&
        proofs[address]?.let { it.network == ServerProofs.network && clock() - it.at in 0 until ServerProofs.TTL_MS } == true

    /** Whether [address] failed on these networks and its wait isn't over. */
    private fun waiting(address: String): Boolean =
        failures[address]?.let { it.network == ServerProofs.network && clock() < it.until } == true

    /**
     * The addresses to try, best first: none waiting after a failure, and
     * no plain http beyond the home network; all of them rather than none.
     */
    private fun candidates(): List<ServerAddress> {
        val usable = ranked().filter { !Servers.unencrypted(it.url) }
        return usable.filter { !waiting(it.url) }.ifEmpty { usable }
    }

    /**
     * The addresses in the order they're tried: by kind (home, VPN,
     * internet), then encrypted first, then as listed. Without a Wi-Fi or
     * Ethernet network, home addresses come last: a VPN of the person's own
     * may still reach them.
     */
    private fun ranked(): List<ServerAddress> {
        val home = homeNetwork()
        val order = { address: ServerAddress ->
            if (!home && address.kind == AddressKind.HOME && !isLoopback(address.url)) AddressKind.entries.size else address.kind.ordinal
        }
        return all().sortedWith(compareBy(order, { if (it.url.startsWith("https:")) 0 else 1 }))
    }

    private fun rank(address: String): Int = ranked().indexOfFirst { it.url == address }.let { if (it < 0) Int.MAX_VALUE else it }

    private fun changed(save: Boolean = true) {
        val (server, added) = synchronized(this) { listed.server to listed.added }
        urls = (listOf(issuer) + server + added).distinct()
        parsed = parse(urls)
        if (inUse !in urls) inUse = issuer
        if (save) keep(StoredAddresses(server, added))
        publish()
    }

    private fun publish() {
        _state.value = stateNow()
    }

    private fun stateNow(): AddressesState {
        val current = inUse
        val home = homeNetwork()
        val rows = ranked().map { address ->
            val failure = failures[address.url]?.takeIf { it.network == ServerProofs.network }
            val status = when {
                Servers.unencrypted(address.url) -> AddressStatus.UNENCRYPTED
                address.url == current && failure == null -> AddressStatus.IN_USE
                isProved(address.url) -> AddressStatus.READY
                address.kind == AddressKind.HOME && !home && !isLoopback(address.url) -> AddressStatus.NEEDS_HOME_NETWORK
                failure != null -> if (failure.otherDevice) AddressStatus.OTHER_DEVICE else AddressStatus.UNREACHABLE
                else -> AddressStatus.UNTRIED
            }
            AddressState(address, status)
        }
        return AddressesState(current, rows)
    }

    companion object {
        /** The server's addresses kept: its administrator's 16 at most, and as many it found. */
        const val MAX_LISTED = 32

        /** Addresses added on this device, at most. */
        const val MAX_ADDED = 16

        /** How long a failed address is left alone, after each failure in a row. */
        val BACKOFF_MS = longArrayOf(5_000, 15_000, 60_000, 300_000)

        /**
         * How long a better address that hasn't answered yet is waited for
         * once a worse one proved itself: at home the home address answers
         * well within it, and the internet's isn't used for a moment first.
         */
        const val HEAD_START_MS = 300L

        /**
         * How long an address racing others has to prove itself, all in:
         * one that doesn't connect fails sooner (a home address within
         * HttpRoot's 3 s).
         */
        const val RACED_PROOF_MS = 10_000L

        /** [url]'s origin when it is an http or https address OkHttp can reach (a port from 1 to 65535). */
        fun originOf(url: String): String? = Addresses.origin(url.trim())
            ?.takeIf { (it.startsWith("http://") || it.startsWith("https://")) && it.toHttpUrlOrNull() != null }

        private fun clean(list: List<String>, max: Int): List<String> = list.mapNotNull(::originOf).distinct().take(max)

        private fun parse(urls: List<String>): List<Pair<String, HttpUrl>> = urls.mapNotNull { url -> url.toHttpUrlOrNull()?.let { url to it } }

        /** This device (an address reached through `adb reverse`): no home network needed. */
        private fun isLoopback(url: String): Boolean {
            val host = try {
                URI(url).host
            } catch (_: URISyntaxException) {
                null
            }?.lowercase() ?: return false
            return host == "localhost" || host.startsWith("127.") || host == "[::1]"
        }

        /** Why no address can be used: another device at each, or nothing reached. */
        private fun failureOf(errors: Collection<IOException>): IOException =
            if (errors.isNotEmpty() && errors.all { it is NotYourServerException }) {
                NotYourServerException()
            } else {
                errors.firstOrNull { it !is NotYourServerException } ?: IOException("no address of the server answers")
            }
    }
}
