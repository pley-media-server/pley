package app.pley.android.network

import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import okhttp3.ConnectionPool
import okhttp3.Dispatcher
import okhttp3.Interceptor
import okhttp3.OkHttpClient

/**
 * The app's one OkHttp client, which every other derives from
 * (newBuilder), so they all share its connections and threads.
 */
object HttpRoot {
    /** Idle connections kept: enough for the API, pictures and a stream at once. */
    const val IDLE_CONNECTIONS = 10

    /**
     * How long an idle connection is kept, in seconds: under the 30 s after
     * which the server closes one (apps/server/src/http.rs, HEADER_TIMEOUT),
     * so a request never goes out on a connection the server is closing.
     */
    const val KEEP_ALIVE_SECONDS = 25L

    /**
     * How long a server at a home address or name (192.168.…, `nas.local`)
     * has to accept a connection, in seconds: at home one accepts at once
     * or not at all, and away from home its address leads nowhere, which
     * OkHttp's 10 s (kept for names on the internet) only says later.
     */
    const val HOME_CONNECT_SECONDS = 3L

    /** Requests at once, for the whole app. */
    const val MAX_REQUESTS = 64

    /** Requests at once to one server (pictures have their own limit, see NetworkGraph.images). */
    const val MAX_REQUESTS_PER_HOST = 8

    /**
     * The root client, introducing itself as [userAgent] (UserAgent.of: the
     * server names the device after it) and asking for [language] (the
     * language shown, "fr-CA"; `null`: none), in which the server writes
     * the words it writes itself. Redirects never go from http to https or
     * back on their own; a connection that failed is tried again on another
     * route. [localRoute], when given, sends connections to the home
     * network over Wi-Fi or Ethernet while mobile data is the default
     * network (LocalRoute).
     */
    fun create(userAgent: String, localRoute: LocalRoute? = null, language: () -> String? = { null }): OkHttpClient = OkHttpClient.Builder()
        .connectionPool(ConnectionPool(IDLE_CONNECTIONS, KEEP_ALIVE_SECONDS, TimeUnit.SECONDS))
        .dispatcher(
            Dispatcher().apply {
                maxRequests = MAX_REQUESTS
                maxRequestsPerHost = MAX_REQUESTS_PER_HOST
            },
        )
        .followSslRedirects(false)
        .retryOnConnectionFailure(true)
        .addInterceptor(userAgent(userAgent))
        .addInterceptor(acceptLanguage(language))
        .addInterceptor(homeConnectTimeout())
        .apply { localRoute?.applyTo(this) }
        .build()

    /** Gives a connection to a home host [HOME_CONNECT_SECONDS] at most to be accepted. */
    fun homeConnectTimeout(): Interceptor {
        val limitMs = TimeUnit.SECONDS.toMillis(HOME_CONNECT_SECONDS).toInt()
        val home = ConcurrentHashMap<String, Boolean>()
        return Interceptor { chain ->
            val request = chain.request()
            val host = request.url.host
            val current = chain.connectTimeoutMillis()
            if ((current == 0 || current > limitMs) && home.getOrPut(host) { LocalRoute.isHomeHost(host) }) {
                chain.withConnectTimeout(limitMs, TimeUnit.MILLISECONDS).proceed(request)
            } else {
                chain.proceed(request)
            }
        }
    }

    /** Sets `Accept-Language` to [language] on every request, while there is one. */
    fun acceptLanguage(language: () -> String?): Interceptor = Interceptor { chain ->
        val tag = language()
        val request = chain.request()
        chain.proceed(if (tag.isNullOrEmpty()) request else request.newBuilder().header("Accept-Language", tag).build())
    }

    /** Sets `User-Agent` on every request. */
    fun userAgent(value: String): Interceptor = Interceptor { chain ->
        chain.proceed(chain.request().newBuilder().header("User-Agent", value).build())
    }
}
