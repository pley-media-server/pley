package app.pley.android.network

import android.content.Context
import app.pley.android.auth.AuthGraph
import app.pley.android.auth.Tokens
import app.pley.android.common.Addresses
import app.pley.android.common.DeviceProfile
import app.pley.android.common.Tier
import app.pley.android.common.sha1Hex
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import okhttp3.Cache
import okhttp3.Dispatcher
import okhttp3.OkHttpClient
import okhttp3.Protocol

/**
 * The network side of the app: one client per purpose, all derived from
 * the root so they share its connections, and the API on top. Made lazily:
 * nothing here runs before the first frame.
 */
interface NetworkGraph {
    /** The client every other derives from. */
    val root: OkHttpClient

    /** The API of [issuer] (a server's origin), as profile [userId] signed in there. */
    fun api(issuer: String, userId: Long?): PleyApi

    /** For pictures: its own limits per host, the server speaking HTTP/1.1. */
    val images: OkHttpClient

    /** For video and audio of [issuer]: long reads, as HLS segments can wait for the transcoder. */
    fun media(issuer: String): OkHttpClient

    /** For subtitles of [issuer], which the transcoder may still be writing. */
    fun subtitles(issuer: String): OkHttpClient

    /** The event stream of [issuer]. */
    fun events(issuer: String): ServerEvents

    /**
     * Lets go of [issuer]'s API clients once its HTTP caches are deleted
     * (signing out): the next ones open their caches again, where the old
     * ones would cache nothing more.
     */
    fun forget(issuer: String) {}

    val discovery: Discovery
    val sweep: NetworkSweep
    val probe: ServerProbe
    val localNetwork: LocalNetworkAccess

    companion object {
        /**
         * [root] reaching home addresses through the home network when
         * mobile data is the default ([LocalRoute]), as every client of a
         * server does: for signing in and the server's proofs, which go to
         * the same addresses as the rest.
         */
        fun homeRouted(context: Context, root: OkHttpClient): OkHttpClient {
            val route = LocalRoute(AndroidLanNetworks(context.applicationContext)::bypass)
            return root.newBuilder().apply { route.applyTo(this) }.build()
        }

        /**
         * The app's network parts over [root] (HttpRoot.create), with the
         * tokens of [auth]. HTTP caches go under [cacheDir]/http.
         */
        fun create(context: Context, root: OkHttpClient, auth: AuthGraph, profile: DeviceProfile, cacheDir: File): NetworkGraph {
            val app = context.applicationContext
            val lans = AndroidLanNetworks(app)
            return HttpNetworkGraph(
                root = root,
                tokens = auth::tokens,
                profile = profile,
                cacheDir = cacheDir,
                lans = lans,
                route = LocalRoute(lans::bypass),
                localNetworkOf = { PermissionsLocalNetworkAccess.of(app) },
            )
        }
    }
}

/**
 * [NetworkGraph] with OkHttp. Every client is [root]'s newBuilder(), with
 * [route] for connections to the home network over mobile data, and the
 * tokens of [tokens] for a server's own address only.
 */
internal class HttpNetworkGraph(
    override val root: OkHttpClient,
    private val tokens: (String) -> Tokens,
    private val profile: DeviceProfile,
    private val cacheDir: File,
    private val lans: LanNetworks,
    private val route: LocalRoute?,
    localNetworkOf: () -> LocalNetworkAccess,
) : NetworkGraph {
    private val base: OkHttpClient by lazy { root.newBuilder().apply { route?.applyTo(this) }.build() }

    private val auths = ConcurrentHashMap<String, IssuerAuth>()
    private val apis = ConcurrentHashMap<Pair<String, Long?>, HttpPleyApi>()
    private val mediaClients = ConcurrentHashMap<String, OkHttpClient>()
    private val subtitleClients = ConcurrentHashMap<String, OkHttpClient>()
    private val eventClients = ConcurrentHashMap<String, OkHttpClient>()

    private fun auth(origin: String): IssuerAuth = auths.getOrPut(origin) { IssuerAuth { tokens(origin) } }

    override fun api(issuer: String, userId: Long?): PleyApi {
        val origin = originOf(issuer)
        return apis.getOrPut(origin to userId) { HttpPleyApi(origin, { apiClient(origin, userId) }) }
    }

    /**
     * The API client of [origin] as [userId]: with an HTTP cache of its own
     * (library listings come back as 304 Not Modified when unchanged), none
     * before the profile is known.
     */
    internal fun apiClient(origin: String, userId: Long?): OkHttpClient = base.newBuilder()
        .apply { if (userId != null) cache(Cache(cacheDirOf(origin, userId), API_CACHE_BYTES)) }
        .signedIn(auth(origin))
        .build()

    /** Where the HTTP cache of [origin] as [userId] is: `http/<sha1 of the origin>/<user id>`. */
    internal fun cacheDirOf(origin: String, userId: Long): File = File(cacheDir, "http/${sha1Hex(originOf(origin))}/$userId")

    // Their caches aren't closed: a request under way may still use one, and a closed cache throws.
    override fun forget(issuer: String) {
        val origin = originOf(issuer)
        apis.keys.removeAll { it.first == origin }
    }

    override val images: OkHttpClient by lazy {
        val auth = OriginAuth(tokens)
        base.newBuilder()
            .dispatcher(
                Dispatcher().apply {
                    maxRequests = HttpRoot.MAX_REQUESTS
                    maxRequestsPerHost = if (profile.tier == Tier.T0) IMAGES_PER_HOST_T0 else IMAGES_PER_HOST
                },
            )
            // pley speaks HTTP/1.1 only: a request per connection at a time.
            .protocols(listOf(Protocol.HTTP_1_1))
            .followRedirects(true)
            .followSslRedirects(false)
            .addInterceptor(auth.unanswered)
            .addNetworkInterceptor(auth)
            .authenticator(auth)
            .build()
    }

    override fun media(issuer: String): OkHttpClient = mediaClients.getOrPut(originOf(issuer)) {
        signedInClient(originOf(issuer), MEDIA_READ_SECONDS)
    }

    override fun subtitles(issuer: String): OkHttpClient = subtitleClients.getOrPut(originOf(issuer)) {
        signedInClient(originOf(issuer), SUBTITLES_READ_SECONDS)
    }

    /** The client of [origin]'s event stream: a read gives up after 45 s, the server's keep-alives come every 15 s. */
    internal fun eventsClient(origin: String): OkHttpClient = eventClients.getOrPut(originOf(origin)) {
        signedInClient(originOf(origin), EVENTS_READ_SECONDS)
    }

    override fun events(issuer: String): ServerEvents {
        val origin = originOf(issuer)
        return EventStream(OkHttpSseConnection({ eventsClient(origin) }, "$origin/api/events"))
    }

    private fun signedInClient(origin: String, readSeconds: Long): OkHttpClient = base.newBuilder()
        .readTimeout(readSeconds, TimeUnit.SECONDS)
        .signedIn(auth(origin))
        .build()

    /** For checking addresses: no token, no redirects followed. */
    internal val probeClient: OkHttpClient by lazy {
        base.newBuilder()
            .followRedirects(false)
            .followSslRedirects(false)
            .connectTimeout(HttpServerProbe.CONNECT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(HttpServerProbe.READ_SECONDS, TimeUnit.SECONDS)
            .build()
    }

    /** The same for home addresses, which accept a connection at once or never. */
    internal val localProbeClient: OkHttpClient by lazy {
        probeClient.newBuilder().connectTimeout(HttpServerProbe.LOCAL_CONNECT_SECONDS, TimeUnit.SECONDS).build()
    }

    override val discovery: Discovery by lazy { UdpDiscovery(lans) }

    override val sweep: NetworkSweep by lazy {
        HttpNetworkSweep({ HttpNetworkSweep.targetsOf(lans.current(), probeClient) })
    }

    override val probe: ServerProbe by lazy { HttpServerProbe(probeClient, localProbeClient) }

    override val localNetwork: LocalNetworkAccess by lazy(localNetworkOf)

    private fun originOf(issuer: String): String = Addresses.origin(issuer) ?: issuer.trimEnd('/')

    companion object {
        /** The API's HTTP cache per server and profile. */
        const val API_CACHE_BYTES = 50L * 1024 * 1024

        /** Pictures at once per server, above OkHttp's 5: the server speaks HTTP/1.1. */
        const val IMAGES_PER_HOST = 10
        const val IMAGES_PER_HOST_T0 = 6

        /** HLS segments can wait up to 45 s for ffmpeg. */
        const val MEDIA_READ_SECONDS = 60L
        const val SUBTITLES_READ_SECONDS = 60L
        const val EVENTS_READ_SECONDS = 45L
    }
}
