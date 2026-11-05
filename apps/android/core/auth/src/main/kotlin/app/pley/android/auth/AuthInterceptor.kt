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
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import okhttp3.Authenticator
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
 * Adds `Authorization: Bearer` to requests for the server's own address
 * ([Tokens.issuer]: scheme, host and port), on every hop: a redirect or a
 * playlist pointing elsewhere gets no token. A network interceptor, so it
 * also sees redirects. Tokens never go in a URL.
 */
class AuthInterceptor(private val tokens: Tokens) : Interceptor {
    private val issuer = tokens.issuer.toHttpUrlOrNull()

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        if (!onServer(request.url, issuer)) {
            // Not the server: no token, whoever added one.
            return chain.proceed(request.newBuilder().removeHeader("Authorization").build())
        }
        Transport.checkConnection(chain)
        val token = try {
            runBlocking { tokens.accessToken() }
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
 * A request to the server that gets no answer (no network, nothing at the
 * address, no answer in time): the device may be on another network now,
 * so what answers there next proves it is the server before a token goes
 * ([Tokens.doubtServer]). An application interceptor: a connection that
 * fails never reaches the network interceptors.
 */
class DoubtWhenUnanswered(private val tokens: Tokens) : Interceptor {
    private val issuer = tokens.issuer.toHttpUrlOrNull()

    override fun intercept(chain: Interceptor.Chain): Response = try {
        chain.proceed(chain.request())
    } catch (e: IOException) {
        if (onServer(chain.request().url, issuer) && !chain.call().isCanceled()) tokens.doubtServer()
        throw e
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
    private val issuer = tokens.issuer.toHttpUrlOrNull()

    override fun authenticate(route: Route?, response: Response): Request? {
        if (!onServer(response.request.url, issuer)) return null
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
     * For the server's API, images and streams: [tokens] on its own
     * address only, once the server there has proved itself, a refresh on
     * an expired token, redirects within a scheme but never from https
     * down to http.
     */
    fun api(base: OkHttpClient, tokens: Tokens): OkHttpClient = base.newBuilder()
        .followRedirects(true)
        .followSslRedirects(false)
        .addInterceptor(DoubtWhenUnanswered(tokens))
        .addNetworkInterceptor(AuthInterceptor(tokens))
        .authenticator(TokenAuthenticator(tokens))
        .build()

    private fun userAgentOf(userAgent: String) = Interceptor { chain ->
        chain.proceed(chain.request().newBuilder().header("User-Agent", userAgent).build())
    }
}
