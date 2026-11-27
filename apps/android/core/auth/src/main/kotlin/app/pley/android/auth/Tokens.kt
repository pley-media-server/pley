package app.pley.android.auth

import app.pley.android.common.Addresses
import app.pley.android.common.UiText
import java.io.IOException
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import okhttp3.FormBody
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONException
import org.json.JSONObject

/** A token answer (RFC 6749 §5.1). */
data class TokenResponse(
    val accessToken: String,
    val expiresIn: Long,
    val refreshToken: String?,
    val scope: String,
    /** What tells the server from another device later; absent from servers before it. */
    val proof: ServerProof? = null,
) {
    override fun toString(): String = "TokenResponse(expiresIn=$expiresIn, scope=$scope)"

    /**
     * The tokens as kept. A refresh or a profile switch is the same sign-in,
     * whose key never changes: [before]'s proof stays, whatever the answer
     * says, and an answer brings one only to a sign-in without.
     */
    fun stored(
        issuer: String,
        tokenEndpoint: String,
        revocationEndpoint: String,
        now: Long,
        before: StoredTokens? = null,
    ): StoredTokens =
        StoredTokens(
            issuer = issuer,
            tokenEndpoint = tokenEndpoint,
            revocationEndpoint = revocationEndpoint,
            accessToken = accessToken,
            accessExpiresAt = now + expiresIn * 1000,
            refreshToken = refreshToken,
            scope = scope,
            proof = before?.proof ?: proof,
        )

    companion object {
        /** pley's access tokens last an hour; an answer without expires_in is read as that. */
        private const val DEFAULT_EXPIRES_IN = 3600L

        fun parse(text: String, askedScope: String = ""): TokenResponse {
            val bad = AuthException(UNREADABLE)
            val o = try {
                JSONObject(text)
            } catch (_: JSONException) {
                throw bad
            }
            val access = o.optString("access_token")
            if (access.isEmpty()) throw bad
            if (!o.optString("token_type").equals("Bearer", ignoreCase = true)) throw bad
            val expiresIn = if (o.has("expires_in")) o.optLong("expires_in", -1) else DEFAULT_EXPIRES_IN
            if (expiresIn <= 0) throw bad
            val refresh = if (o.isNull("refresh_token")) null else o.optString("refresh_token").ifEmpty { null }
            // Without a scope, the server issued what was asked (RFC 6749 §5.1).
            val scope = if (o.isNull("scope")) askedScope else o.optString("scope").ifEmpty { askedScope }
            return TokenResponse(access, expiresIn, refresh, scope, ServerProof.of(o))
        }
    }
}

/**
 * An error of the token and revocation endpoints (RFC 6749 §5.2), with
 * pley's `reason` for a profile switch that didn't happen.
 */
data class OAuthError(val error: String, val description: String?, val reason: String?) {
    companion object {
        /** The error in [text], or `null` when it isn't one. */
        fun parse(text: String): OAuthError? {
            val o = try {
                JSONObject(text)
            } catch (_: JSONException) {
                return null
            }
            val error = o.optString("error").ifEmpty { return null }
            val description = if (o.isNull("error_description")) null else o.optString("error_description").ifEmpty { null }
            val reason = if (o.isNull("reason")) null else o.optString("reason").ifEmpty { null }
            return OAuthError(error, description?.let(::plain), reason)
        }
    }
}

/** A sentence from the server or a browser, safe to show: one short line without control characters. */
internal fun plain(text: String): String =
    text.filter { !Character.isISOControl(it) && Character.getType(it) != Character.FORMAT.toInt() }.trim().take(200)

/** An answer of the server's that isn't what it should be. */
val UNREADABLE: UiText = UiText.res(R.string.auth_unreadable)

/** The token endpoint said no: in its own words when it gave some. */
class TokenException(val status: Int, val oauth: OAuthError?) :
    AuthException((oauth?.description ?: oauth?.error)?.let(UiText::raw) ?: UiText.res(R.string.auth_refused_status, status))

/** The server no longer knows this app's sign-in: the person signs in again. */
class SignedOutException : AuthException(SignOutReason.ENDED.text)

/** The calls to the token and revocation endpoints: forms, no cookies, no secrets, no redirects. */
internal object TokenEndpoint {
    suspend fun post(http: OkHttpClient, url: String, form: List<Pair<String, String>>): String =
        withContext(Dispatchers.IO) {
            val body = FormBody.Builder().apply { form.forEach { (name, value) -> add(name, value) } }.build()
            val request = Request.Builder().url(url).header("Accept", "application/json").post(body).build()
            http.newCall(request).execute().use { response ->
                val text = response.body.string()
                if (!response.isSuccessful) throw TokenException(response.code, OAuthError.parse(text))
                text
            }
        }
}

/**
 * The tokens of one server: the access token while it works, a new pair
 * from the refresh token when it doesn't. Refreshes happen one at a time
 * (the server ends the sign-in when a refresh token is used twice), and the
 * new refresh token is saved before the new access token is used.
 */
class Tokens(
    val issuer: String,
    private val store: TokenStore,
    private val http: OkHttpClient,
    /** Told when the server ends the sign-in, or the scope guard does. */
    private val events: AuthEvents? = null,
    /** The challenges' random bytes. */
    private val random: (Int) -> ByteArray = ServerProofs::randomBytes,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val mutex = Mutex()

    /** The access token the last refresh or profile switch gave, and when (guarded by [mutex]). */
    private var lastIssued: Pair<String, Long>? = null

    /**
     * Where proofs that outlive the request that started them run. Their
     * failures are the addresses' (ServerAddresses), never the app's.
     */
    private val background = CoroutineScope(SupervisorJob() + Dispatchers.IO + CoroutineExceptionHandler { _, _ -> })

    /**
     * The server's addresses, and the one its requests go to (docs/ANDROID.md,
     * "Home and away"): each proves itself with this sign-in's key.
     */
    private val book: ServerAddresses by lazy {
        ServerAddresses(
            issuer,
            prove = { address, timeoutMs ->
                val proof = load()?.proof ?: throw SignedOutException()
                ServerProofs.prove(http, address, proof, random, timeoutMs)
            },
            keep = { store.saveAddresses(issuer, it) },
            scope = background,
            clock = clock,
        ).also { it.restore(store.addresses(issuer)) }
    }

    // Without the Keystore, so the tokens can be made on any thread: the
    // first request opens them.
    private val _signedIn = MutableStateFlow(store.has(issuer))

    /** Whether the app is signed in to this server; false means: show sign-in. */
    val signedIn: StateFlow<Boolean> = _signedIn.asStateFlow()

    /** What the server allowed, or nothing when signed out. */
    fun scopes(): Set<String> = load()?.scopes().orEmpty()

    /** The server's addresses and what the app knows of each, for the screens. */
    val addresses: StateFlow<AddressesState> get() = book.state

    /**
     * Where the server's requests go now: an address that proved itself on
     * these networks, chosen among the server's when the one in use hasn't
     * (docs/ANDROID.md, "Home and away"). The sign-in address while signed
     * out, or signed in to a server without proofs: nothing could tell its
     * other addresses from another device's.
     */
    suspend fun route(): String {
        val tokens = load() ?: return issuer
        if (tokens.proof == null) return issuer
        return book.proved()
    }

    /** The address of this server that [url] is on; `null` when it is on none. */
    fun addressOf(url: HttpUrl): String? = book.addressOf(url)

    /**
     * The address in use, proved or not: where a read goes when no address
     * proves itself, for what the HTTP cache keeps (a car's covers). No
     * token goes there before it proves itself.
     */
    val addressInUse: String get() = book.inUse

    /**
     * A request to [address] got no answer: it proves itself again before
     * its next token. Unreached ([sent] false): it waits before it is tried
     * again, and the next requests may go elsewhere.
     */
    fun unanswered(address: String, sent: Boolean = false) = if (sent) book.doubt(address) else book.unanswered(address)

    /** Keeps the server's own list of its addresses (GET /api/server/addresses). */
    suspend fun serverAddresses(urls: List<String>) = withContext(Dispatchers.IO) { book.fromServer(urls) }

    /** Adds [url], typed on this device: `false` when it isn't an address, or is the server's already. */
    suspend fun addAddress(url: String): Boolean = withContext(Dispatchers.IO) { book.add(url) }

    /** Removes [url], when it was added on this device. */
    suspend fun removeAddress(url: String): Boolean = withContext(Dispatchers.IO) { book.remove(url) }

    /** Every address proves itself again, now, and the best that does is used. */
    suspend fun checkAddresses() {
        if (movesBetweenAddresses()) book.checkAgain()
    }

    /**
     * Whether the server's other addresses are used: a sign-in with a proof
     * key, which tells them from another device's. A Pley from before
     * proofs keeps to the sign-in address.
     */
    suspend fun movesBetweenAddresses(): Boolean = withContext(Dispatchers.IO) { load()?.proof != null }

    /**
     * The access token to send to [address] (one of the server's),
     * refreshed when it ends within a minute; `null` when signed out. The
     * server at [address] has proved itself when it returns.
     */
    suspend fun accessToken(address: String = issuer): String? {
        val tokens = load() ?: return null
        // A refresh waits for a proof of its own: one before it would be wasted.
        val token = if (tokens.accessUsable(clock())) tokens.accessToken else refresh(tokens.accessToken)
        proved(tokens, address)
        return token
    }

    /**
     * A new access token in place of [stale] (the one the server refused, or
     * that is about to end). When another caller refreshed meanwhile, its
     * token is used and nothing is sent.
     */
    suspend fun refresh(stale: String?): String = mutex.withLock {
        val tokens = load() ?: throw SignedOutException()
        if (tokens.accessToken != stale && tokens.accessUsable(clock())) {
            // Refreshed by another caller meanwhile, maybe on another network since.
            proved(tokens)
            return@withLock tokens.accessToken
        }
        val refresh = tokens.refreshToken ?: run { signedOut() }
        // The refresh token goes only to a server that proves itself now.
        val address = proved(tokens, fresh = true)
        val answer = try {
            TokenEndpoint.post(
                http,
                at(tokens.tokenEndpoint, address),
                listOf("grant_type" to "refresh_token", "refresh_token" to refresh, "client_id" to CLIENT_ID),
            )
        } catch (e: TokenException) {
            if (e.oauth?.error == "invalid_grant") signedOut()
            throw e
        }
        val parsed = TokenResponse.parse(answer, tokens.scope)
        val fresh = parsed.stored(issuer, tokens.tokenEndpoint, tokens.revocationEndpoint, clock(), tokens)
        if (ScopeGuard.extra(parsed, tokens.scope).isNotEmpty()) widened(fresh, address)
        // A refresh token is good once: the new one is kept before anything else.
        store.save(fresh)
        lastIssued = fresh.accessToken to clock()
        fresh.accessToken
    }

    /**
     * The server answered invalid_token to [sent]. When [sent] is the token
     * a refresh gave moments ago, another refresh won't do better: the
     * server no longer accepts this sign-in. It ends as on invalid_grant
     * (the tokens go, [AuthEvent.SignedOut] with [SignOutReason.ENDED]) and
     * this answers true. False when [sent] is older: refresh it.
     */
    suspend fun refusedAfterRefresh(sent: String): Boolean = mutex.withLock {
        val (token, at) = lastIssued ?: return@withLock false
        if (token != sent || clock() - at > JUST_ISSUED_MS) return@withLock false
        // Another refresh since, or signed out already: nothing to end here.
        if (store.load(issuer)?.accessToken != sent) return@withLock false
        end(SignOutReason.ENDED)
        true
    }

    /**
     * Moves the sign-in to another profile of the household (the
     * switch-profile grant of pley's own apps). A wrong PIN changes nothing.
     * [ticket]: a cast's `switch_ticket`, from a remote on [profile], which
     * stands for its PIN; spent or expired, the grant refuses it with reason
     * `ticket_spent`, and nothing changes either.
     */
    suspend fun switchProfile(profile: Long, pin: String?, ticket: String? = null): Unit = mutex.withLock {
        val tokens = load() ?: throw SignedOutException()
        val refresh = tokens.refreshToken ?: signedOut()
        val form = mutableListOf(
            "grant_type" to SWITCH_PROFILE_GRANT,
            "client_id" to CLIENT_ID,
            "refresh_token" to refresh,
            "profile" to profile.toString(),
        )
        if (pin != null) form += "pin" to pin
        if (ticket != null) form += "ticket" to ticket
        val address = proved(tokens, fresh = true)
        val answer = try {
            TokenEndpoint.post(http, at(tokens.tokenEndpoint, address), form)
        } catch (e: TokenException) {
            // Without a reason the refresh token itself is spent or unknown.
            if (e.oauth?.error == "invalid_grant" && e.oauth.reason == null) signedOut()
            throw e
        }
        val parsed = TokenResponse.parse(answer, tokens.scope)
        val fresh = parsed.stored(issuer, tokens.tokenEndpoint, tokens.revocationEndpoint, clock(), tokens)
        if (ScopeGuard.extra(parsed, tokens.scope).isNotEmpty()) widened(fresh, address)
        store.save(fresh)
        lastIssued = fresh.accessToken to clock()
    }

    /**
     * Ends this app's sign-in on the server (RFC 7009), then forgets the
     * tokens whatever it answered. A server that can't prove itself gets no
     * token: the person can sign the app out in Account → Apps.
     */
    suspend fun signOut() {
        mutex.withLock {
            val tokens = store.load(issuer)
            try {
                val address = tokens?.let { provedNow(it) }
                if (tokens != null && address != null) revoke(http, tokens, at(tokens.revocationEndpoint, address))
            } finally {
                store.wipe(issuer)
                book.forget()
                _signedIn.value = false
            }
        }
    }

    /** A request to the server got no answer: the next token waits for a new proof (the device may be on another network). */
    fun doubtServer() = book.doubt(book.inUse)

    /**
     * The address, one of the server's, that proved within the last half
     * hour and on these networks that it is the server signed in to:
     * [address] when given, else the one in use or another chosen now.
     * [fresh]: one that proves itself now, as a refresh token or a PIN
     * needs. No token leaves before. A sign-in from a server without proofs
     * has nothing to check: its address is the sign-in's.
     */
    private suspend fun proved(tokens: StoredTokens, address: String? = null, fresh: Boolean = false): String {
        if (tokens.proof == null) return issuer
        if (address != null && !fresh) {
            book.ensureProved(address)
            return address
        }
        return book.proved(fresh)
    }

    /** [proved] now, or `null` when no address of the server proves itself. */
    private suspend fun provedNow(tokens: StoredTokens): String? = try {
        proved(tokens, fresh = true)
    } catch (_: IOException) {
        null
    }

    /** [endpoint], on the sign-in address, at [address], another of the server's. */
    private fun at(endpoint: String, address: String): String {
        if (address == issuer || Addresses.origin(endpoint) != issuer) return endpoint
        val to = address.toHttpUrlOrNull() ?: return endpoint
        return endpoint.toHttpUrlOrNull()?.newBuilder()?.scheme(to.scheme)?.host(to.host)?.port(to.port)?.build()?.toString() ?: endpoint
    }

    /** Whether the server proves it holds [tokens]' key (none: nothing to check). */
    internal suspend fun proves(tokens: StoredTokens): Boolean {
        val proof = tokens.proof ?: return true
        return try {
            ServerProofs.prove(http, issuer, proof, random)
            true
        } catch (_: IOException) {
            false
        }
    }

    /**
     * Keeps the tokens of a sign-in that just finished, between refreshes
     * (a refresh of the sign-in before can't overwrite them); the tokens
     * they replace.
     */
    internal suspend fun install(fresh: StoredTokens): StoredTokens? = mutex.withLock {
        require(fresh.issuer == issuer)
        val previous = store.load(issuer)
        store.save(fresh)
        // A new sign-in, a new key: the next token waits for its proof.
        book.forgetProofs()
        _signedIn.value = true
        previous
    }

    /**
     * The tokens kept, opened. None (signed out, or kept but unreadable:
     * the Keystore's key went) means signed out, which [signedIn] says at
     * once, whoever read them first; unless a sign-in kept new ones since.
     */
    private fun load(): StoredTokens? = store.load(issuer).also { if (it == null && !store.has(issuer)) _signedIn.value = false }

    private fun signedOut(): Nothing {
        end(SignOutReason.ENDED)
        throw SignedOutException()
    }

    /** Forgets the tokens and says why: the screens show sign-in. */
    private fun end(reason: SignOutReason) {
        store.wipe(issuer)
        lastIssued = null
        book.forget()
        _signedIn.value = false
        events?.signedOut(issuer, reason)
    }

    /**
     * A refresh or switch gave more than this sign-in holds: the grant is
     * ended with the new refresh token (the old one is spent), at
     * [address], where the exchange just proved the server, and the app is
     * signed out.
     */
    private suspend fun widened(fresh: StoredTokens, address: String): Nothing {
        revoke(http, fresh, at(fresh.revocationEndpoint, address))
        store.wipe(issuer)
        book.forget()
        _signedIn.value = false
        events?.signedOut(issuer, SignOutReason.WIDER_SCOPE)
        throw WiderScopeException()
    }

    companion object {
        /** How long a refreshed token counts as just issued: refused within this, the sign-in is over. */
        const val JUST_ISSUED_MS = 60_000L

        /** Ends the grant of [tokens] on its server, at [endpoint]; errors are left to the caller. */
        internal suspend fun revoke(http: OkHttpClient, tokens: StoredTokens, endpoint: String = tokens.revocationEndpoint) {
            // The refresh token ends the whole grant; the access token too, when there is no other.
            val token = tokens.refreshToken ?: tokens.accessToken
            try {
                TokenEndpoint.post(http, endpoint, listOf("token" to token, "client_id" to CLIENT_ID))
            } catch (_: IOException) {
                // Offline or refused: the person can still sign the app out in Account → Apps.
            }
        }
    }
}
