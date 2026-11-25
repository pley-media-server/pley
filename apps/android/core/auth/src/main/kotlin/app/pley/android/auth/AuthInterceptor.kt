package app.pley.android.auth

import app.pley.android.common.Addresses
import app.pley.android.common.Servers
import app.pley.android.common.UiText
import java.io.IOException
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.net.URI
import java.net.URISyntaxException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import okhttp3.Authenticator
import okhttp3.Call
import okhttp3.EventListener
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route

/** Signing in over plain http beyond the home network. */
val UNENCRYPTED: UiText = UiText.res(R.string.auth_unencrypted)

/**
 * Plain http is for home networks: pley's apps send codes and tokens over
 * it only to private addresses, as browsers on the server's own network
 * do with passwords. Anything else needs https.
 */
object Transport {
    /** Refuses [server] when it is plain http to a host that isn't on the home network (Servers.isLocalHost). */
    fun checkAddress(server: String) {
        val uri = try {
            URI(server)
        } catch (_: URISyntaxException) {
            throw AuthException(NOT_AN_ADDRESS)
        }
        when (uri.scheme?.lowercase()) {
            "https" -> return
            "http" -> if (!Servers.isLocalHost(uri.host ?: "")) throw AuthException(UNENCRYPTED)
            else -> throw AuthException(NOT_AN_ADDRESS)
        }
    }

    /**
     * Whether [address] is this device or on a private network: loopback,
     * RFC 1918, link-local, CGNAT (Tailscale) and IPv6 unique local, the
     * addresses Servers.isLocalHost accepts.
     */
    fun isPrivate(address: InetAddress): Boolean {
        if (address.isLoopbackAddress || address.isLinkLocalAddress || address.isSiteLocalAddress) return true
        val b = address.address
        return when (address) {
            is Inet4Address -> (b[0].toInt() and 0xff) == 100 && (b[1].toInt() and 0xc0) == 64
            is Inet6Address -> (b[0].toInt() and 0xfe) == 0xfc
            else -> false
        }
    }

    /**
     * A plain http request goes out only to a private address, checked on
     * the connection itself (a name that turns out to be on the internet is
     * refused too). Behind a proxy, it is the proxy's address.
     */
    fun checkConnection(chain: Interceptor.Chain) {
        if (chain.request().url.isHttps) return
        val address = chain.connection()?.route()?.socketAddress?.address
        if (address == null || !isPrivate(address)) throw AuthException(UNENCRYPTED)
    }
}

/**
 * Adds `Authorization: Bearer` to requests for the server's own addresses
 * (the sign-in address, and the others it gave or that were added here:
 * scheme, host and port), on every hop, once the server there has proved
 * itself: a redirect or a playlist pointing elsewhere gets no token. A
 * network interceptor, so it also sees redirects. Tokens never go in a URL.
 */
class AuthInterceptor(private val tokens: Tokens) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        // Not the server: no token, whoever added one.
        val address = tokens.addressOf(request.url)
            ?: return chain.proceed(request.newBuilder().removeHeader("Authorization").build())
        Transport.checkConnection(chain)
        val token = try {
            runBlocking { tokens.accessToken(address) }
        } catch (e: IOException) {
            throw e
        } catch (e: Exception) {
            // The tokens couldn't be kept or read (storage, the Keystore):
            // this request fails, as OkHttp expects, rather than the app.
            throw IOException("the sign-in couldn't be read on this device", e)
        } ?: return chain.proceed(request)
        return chain.proceed(request.newBuilder().header("Authorization", "Bearer $token").build())
    }
}

/**
 * Sends the requests for the server's sign-in address to the address in
 * use ([Tokens.route]), and hands their answers back as the sign-in
 * address's: what reads them (a playlist, whose segments are named
 * relative to it; the HTTP cache) keeps to the sign-in address, whichever
 * answered.
 *
 * A request that gets no answer (no network, nothing at the address, no
 * answer in time) tells the server's addresses ([Tokens.unanswered]): the
 * address proves itself again before its next token, and when the request
 * couldn't reach it, the next requests go to another while it waits. The
 * request itself goes once to the address chosen next, when that is safe:
 * nothing of it was sent, or it only reads.
 *
 * The first application interceptor: the others see the address the
 * request goes to (HttpRoot's home connect limit among them), and a
 * connection that fails never reaches the network interceptors.
 */
class RouteInterceptor(private val tokens: Tokens) : Interceptor {
    private val issuer = tokens.issuer.toHttpUrlOrNull()

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val issuer = issuer
        if (issuer == null || !onServer(request.url, issuer)) return passedOn(chain)
        var address = try {
            route()
        } catch (e: IOException) {
            // No address proves itself: a read goes on to the one in use all the
            // same, for what the HTTP cache keeps (a car's covers); no token goes
            // there before it proves itself (AuthInterceptor).
            if (e is AuthException || request.method != "GET") throw e
            tokens.addressInUse
        }
        var again = false
        while (true) {
            val to = if (address == tokens.issuer) issuer else address.toHttpUrlOrNull() ?: issuer
            RequestsSent.forget(chain.call())
            try {
                val response = chain.proceed(routed(request, to, tokens.issuer))
                return if (to == issuer) response else relabeled(response, from = to, to = issuer)
            } catch (e: IOException) {
                if (chain.call().isCanceled() || e is AuthException) throw e
                val sent = RequestsSent.began(chain.call())
                // Its own proof failing told the addresses already.
                if (e !is NotYourServerException) tokens.unanswered(address, sent)
                val replayable = !sent || request.method == "GET" || request.method == "HEAD"
                if (again || !replayable) throw e
                val next = try {
                    route()
                } catch (_: IOException) {
                    throw e
                }
                if (next == address) throw e
                address = next
                again = true
            }
        }
    }

    /** A request that isn't for the sign-in address: as it is, telling the addresses when it is for another of them. */
    private fun passedOn(chain: Interceptor.Chain): Response = try {
        chain.proceed(chain.request())
    } catch (e: IOException) {
        if (!chain.call().isCanceled() && e !is AuthException && e !is NotYourServerException) {
            tokens.addressOf(chain.request().url)?.let { tokens.unanswered(it, RequestsSent.began(chain.call())) }
        }
        throw e
    }

    private fun route(): String = try {
        runBlocking { tokens.route() }
    } catch (e: IOException) {
        throw e
    } catch (e: Exception) {
        // The tokens couldn't be read (storage, the Keystore): as AuthInterceptor says.
        throw IOException("the sign-in couldn't be read on this device", e)
    }

    companion object {
        /**
         * [request] at [to], marked as [issuer]'s ([RoutedFrom]); the HTTP
         * cache keeps a read under the address it was asked for, so it is
         * the same at every address.
         */
        internal fun routed(request: Request, to: HttpUrl, issuer: String): Request {
            if (onServer(request.url, to)) return request
            val url = request.url.newBuilder().scheme(to.scheme).host(to.host).port(to.port).build()
            return request.newBuilder()
                .url(url)
                .tag(RoutedFrom::class.java, RoutedFrom(issuer))
                .apply { if (request.method == "GET") cacheUrlOverride(request.cacheUrlOverride ?: request.url) }
                .build()
        }

        /** [response] from [from], as if from [to]: the request's address, a redirect's included, is [to]'s. */
        internal fun relabeled(response: Response, from: HttpUrl, to: HttpUrl): Response {
            val url = response.request.url
            if (!onServer(url, from)) return response
            val shown = url.newBuilder().scheme(to.scheme).host(to.host).port(to.port).build()
            return response.newBuilder().request(response.request.newBuilder().url(shown).build()).build()
        }
    }
}

/**
 * The server a request was sent on for, to another of its addresses: says
 * whose tokens a client shared by every server uses. Only whose: the
 * server's rules still check the address the request goes to, a
 * redirect's included.
 */
class RoutedFrom(val issuer: String)

/**
 * Which calls began to send their request, for [RouteInterceptor]: one that
 * didn't can go to another address, whatever it does. Clients with routes
 * use it as their EventListener.Factory.
 */
object RequestsSent : EventListener.Factory {
    private val began = ConcurrentHashMap<Call, Boolean>()

    override fun create(call: Call): EventListener = object : EventListener() {
        override fun requestHeadersStart(call: Call) {
            began[call] = true
        }

        override fun callEnd(call: Call) {
            began.remove(call)
        }

        override fun callFailed(call: Call, ioe: IOException) {
            began.remove(call)
        }

        override fun canceled(call: Call) {
            began.remove(call)
        }
    }

    /** Whether [call] began to send its request since [forget]. */
    fun began(call: Call): Boolean = began[call] == true

    /** [call] goes out again: what it sent before doesn't count. */
    fun forget(call: Call) {
        began.remove(call)
    }
}

/**
 * Whether [url] is on the server whose address is [issuer]: the same
 * scheme, host and port, as OkHttp parsed both (a second parser could read
 * the same address otherwise).
 */
internal fun onServer(url: HttpUrl, issuer: HttpUrl?): Boolean =
    issuer != null && url.scheme == issuer.scheme && url.host == issuer.host && url.port == issuer.port

/**
 * On a 401 from the server whose WWW-Authenticate says
 * `error="invalid_token"`, refreshes once (one refresh at a time for the
 * whole app) and tries again. A token a refresh gave moments ago that is
 * refused too means the server no longer accepts this sign-in: it ends, as
 * on invalid_grant ([Tokens.refusedAfterRefresh]). Anything else, a 403
 * insufficient_scope among them, is the answer.
 */
class TokenAuthenticator(private val tokens: Tokens) : Authenticator {
    override fun authenticate(route: Route?, response: Response): Request? {
        if (tokens.addressOf(response.request.url) == null) return null
        val challenge = response.headers("WWW-Authenticate").any { invalidToken(it) }
        if (!challenge) return null
        // The token the server refused, as the interceptor sent it.
        val sent = (response.networkResponse ?: response).request.header("Authorization")
            ?.takeIf { it.startsWith("Bearer ", ignoreCase = true) }?.substring(7) ?: return null
        // Refused moments after a refresh: the sign-in ends, and the 401 is the answer.
        val ended = try {
            runBlocking { tokens.refusedAfterRefresh(sent) }
        } catch (_: Exception) {
            // The tokens couldn't be read or wiped (storage, the Keystore).
            false
        }
        if (ended) return null
        // Once: a 401 after a refresh is the answer.
        if (generateSequence(response.priorResponse) { it.priorResponse }.any { it.code == 401 }) return null
        return try {
            runBlocking { tokens.refresh(sent) }
            // The interceptor adds the new token.
            response.request
        } catch (_: IOException) {
            null
        } catch (_: Exception) {
            // The new tokens couldn't be kept (storage, the Keystore): the
            // refusal is the answer.
            null
        }
    }

    companion object {
        /** Whether a WWW-Authenticate value is a Bearer challenge with error="invalid_token" (RFC 6750 §3). */
        fun invalidToken(header: String): Boolean {
            val h = header.trim()
            if (!h.startsWith("Bearer", ignoreCase = true) || (h.length > 6 && !h[6].isWhitespace())) return false
            return Regex("""(^|[\s,])error\s*=\s*"?invalid_token"?(\s*,|\s*$)""", RegexOption.IGNORE_CASE)
                .containsMatchIn(h.substring(6))
        }
    }
}

/** The app's HTTP clients for signing in and for a signed-in server. */
object AuthHttp {
    /**
     * For the metadata, token and revocation endpoints: no redirects at
     * all (codes and tokens go to the address the metadata named, or
     * nowhere), no cookies, plain http to private addresses only.
     * [userAgent] is the app's (UserAgent.of).
     */
    fun endpoints(userAgent: String, base: OkHttpClient = OkHttpClient()): OkHttpClient = base.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .addInterceptor(userAgentOf(userAgent))
        .addNetworkInterceptor { chain ->
            Transport.checkConnection(chain)
            chain.proceed(chain.request())
        }
        .build()

    /**
     * For the server's API, images and streams: to the address in use
     * ([RouteInterceptor]), [tokens] on the server's own addresses only,
     * once the server there has proved itself, a refresh on an expired
     * token, redirects within a scheme but never from https down to http.
     */
    fun api(base: OkHttpClient, tokens: Tokens): OkHttpClient = base.newBuilder()
        .followRedirects(true)
        .followSslRedirects(false)
        .apply { interceptors().add(0, RouteInterceptor(tokens)) }
        .eventListenerFactory(RequestsSent)
        .addNetworkInterceptor(AuthInterceptor(tokens))
        .authenticator(TokenAuthenticator(tokens))
        .build()

    private fun userAgentOf(userAgent: String) = Interceptor { chain ->
        chain.proceed(chain.request().newBuilder().header("User-Agent", userAgent).build())
    }
}
