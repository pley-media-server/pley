package app.pley.android.network

import app.pley.android.auth.AuthInterceptor
import app.pley.android.auth.RequestsSent
import app.pley.android.auth.RouteInterceptor
import app.pley.android.auth.RoutedFrom
import app.pley.android.auth.TokenAuthenticator
import app.pley.android.auth.Tokens
import app.pley.android.common.Addresses
import java.util.concurrent.ConcurrentHashMap
import okhttp3.Authenticator
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route

/**
 * The sign-in module's rules for one server (AuthHttp.api): its requests
 * to the address in use, whichever of the server's answers
 * (RouteInterceptor); its token on its own addresses only, on every hop,
 * once the server there has proved itself (again after a request that got
 * no answer); one refresh on a 401 invalid_token; redirects within a
 * scheme, never from https down to http. The tokens are read the first
 * time a request needs them, on OkHttp's threads, so a client can be made
 * on the main thread without touching the Keystore.
 */
internal class IssuerAuth(tokens: () -> Tokens) : Interceptor, Authenticator {
    private val tokens by lazy(tokens)
    private val interceptor by lazy { AuthInterceptor(this.tokens) }
    private val authenticator by lazy { TokenAuthenticator(this.tokens) }
    private val router by lazy { RouteInterceptor(this.tokens) }

    /** The first application interceptor (RouteInterceptor). */
    val route = Interceptor { chain -> router.intercept(chain) }

    override fun intercept(chain: Interceptor.Chain): Response = interceptor.intercept(chain)

    override fun authenticate(route: Route?, response: Response): Request? = authenticator.authenticate(route, response)
}

/** [builder] with [auth]: as AuthHttp.api sets a client up. */
internal fun OkHttpClient.Builder.signedIn(auth: IssuerAuth): OkHttpClient.Builder = this
    .followRedirects(true)
    .followSslRedirects(false)
    .apply { interceptors().add(0, auth.route) }
    .eventListenerFactory(RequestsSent)
    .addNetworkInterceptor(auth)
    .authenticator(auth)

/**
 * The same rules for a client shared by every server (pictures): each
 * request gets the tokens of its own origin, when the app is signed in
 * there, or, sent on to another of a server's addresses, of the server it
 * was for ([RoutedFrom]: whose rules still check the address it goes to);
 * requests anywhere else go out without a token.
 */
internal class OriginAuth(private val tokens: (String) -> Tokens) : Interceptor, Authenticator {
    private val byOrigin = ConcurrentHashMap<String, IssuerAuth>()

    private fun of(request: Request): IssuerAuth? {
        val origin = request.tag(RoutedFrom::class.java)?.issuer ?: Addresses.origin(request.url.toString()) ?: return null
        val auth = byOrigin.getOrPut(origin) { IssuerAuth { tokens(origin) } }
        return auth.takeIf { tokens(origin).signedIn.value }
    }

    override fun intercept(chain: Interceptor.Chain): Response {
        val auth = of(chain.request())
            ?: return chain.proceed(chain.request().newBuilder().removeHeader("Authorization").build())
        return auth.intercept(chain)
    }

    override fun authenticate(route: Route?, response: Response): Request? = of(response.request)?.authenticate(route, response)

    /** [IssuerAuth.route] for the request's own origin. */
    val route = Interceptor { chain -> of(chain.request())?.route?.intercept(chain) ?: chain.proceed(chain.request()) }
}
