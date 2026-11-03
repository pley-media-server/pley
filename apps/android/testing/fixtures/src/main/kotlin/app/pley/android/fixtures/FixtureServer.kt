package app.pley.android.fixtures

import app.pley.android.model.ServerEvent
import java.io.Closeable
import java.net.InetAddress
import java.net.ServerSocket
import java.util.concurrent.TimeUnit
import javax.net.ServerSocketFactory
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okio.Buffer

/**
 * A fake pley server on this machine, for tests, demos and benchmarks, so
 * none of them writes into a real server. It listens on 127.0.0.1 (by
 * address: a hosts file may give "localhost" another one; loopback is
 * outside Android 17's local-network permission) on a free port, and
 * answers docs/API.md's routes as an app's token sees them:
 *
 * - the catalogue of [FixtureIds]: libraries, listings with ETag and 304,
 *   hubs, items with their files, streams, chapters and markers, people,
 *   collections, playlists, search and history;
 * - playback: sessions answered direct or as HLS, the files with Range, the
 *   HLS stream, WebVTT subtitles, seek previews and chapter pictures, all
 *   from the committed clips (scripts/make-android-fixtures.ps1);
 * - writes (progress, watched, ratings, track choices, playlists, track
 *   defaults, My List, leaving Continue Watching), which change what is
 *   read next and go to [journal];
 * - server events, with keep-alives and a scan every 30 s;
 * - casting: devices listed while their event streams are open, sent
 *   what to play and how to steer it, and saying what they play;
 * - pictures drawn per width bucket;
 * - signing in as pley's authorization server does: discovery, the
 *   authorize page (allowing at once, as [signInAs]), PKCE, refresh
 *   rotation with reuse detection, the switch-profile grant with Sam's PIN
 *   ([FixtureIds.PIN]), the TV's device grant, revocation, and every
 *   route's token and scope check.
 *
 * ```
 * FixtureServer().use { server ->
 *     server.start()
 *     val token = server.accessToken()
 *     // GET ${server.url}/api/libraries with "Authorization: Bearer $token"
 * }
 * ```
 */
class FixtureServer(private val options: Options = Options()) : Closeable {
    /** How the server behaves. Every field has the real server's value or a sensible default. */
    data class Options(
        /** Routes need a token. Off, requests without one act as [signInAs] with every scope. */
        val requireAuth: Boolean = true,
        /** Between the event stream's keep-alive comments. */
        val keepAliveMs: Long = 15_000,
        /** Between two scans' events; 0 for none. */
        val scanEveryMs: Long = 30_000,
        /** What the device grant asks TVs to wait between polls. */
        val deviceIntervalSeconds: Int = 5,
        /** [Mode.Slow]: added before each answer. */
        val slowLatencyMs: Long = 800,
        /** [Mode.Slow]: the bodies' pace. */
        val slowBytesPerSecond: Long = 256 * 1_024,
        /** The server's clock, in ms: token lifetimes, poll intervals, dates of plays. */
        val clock: () -> Long = System::currentTimeMillis,
    )

    /** How the server answers, changed at any time. */
    enum class Mode {
        Normal,

        /** Every answer late and its body slow ([Options.slowLatencyMs], [Options.slowBytesPerSecond]). */
        Slow,

        /** Not listening: connections are refused, on the same port, until the mode changes. */
        Down,
    }

    /** How the person answers a TV's sign-in code: the first poll always waits. */
    enum class DeviceApproval { Allow, Deny, Never }

    private val state = State(options)
    private val routes: List<Route> =
        state.oauth.routes() + ExtraRoutes(state).routes() + BrowseRoutes(state).routes() + PlayRoutes(state).routes() + PlaylistRoutes(state).routes() +
            ListRoutes(state).routes() + CastRoutes(state).routes()
    private val listening = Any()
    private var server = MockWebServer()
    private var port = 0
    private var started = false

    /** Every write apps made, for tests to read. */
    val journal: Journal get() = state.journal

    /** The server's address, once [start]ed: `http://127.0.0.1:<port>`. */
    val url: String
        get() {
            check(started) { "start the FixtureServer first" }
            return "http://127.0.0.1:$port"
        }

    @Volatile
    var mode: Mode = Mode.Normal
        set(value) {
            synchronized(listening) {
                if (started) {
                    if (value == Mode.Down && field != Mode.Down) stopListening()
                    if (value != Mode.Down && field == Mode.Down) listen(port)
                }
                field = value
            }
        }

    /** The profile the authorize page and TV codes sign in as. */
    var signInAs: Long
        get() = synchronized(state.lock) { state.oauth.signInAs }
        set(value) {
            require(state.profile(value) != null) { "no profile $value" }
            synchronized(state.lock) { state.oauth.signInAs = value }
        }

    var deviceApproval: DeviceApproval
        get() = synchronized(state.lock) { state.oauth.deviceApproval }
        set(value) = synchronized(state.lock) { state.oauth.deviceApproval = value }

    /** The demo server only: tokens it never issued sign in as [signInAs] (OAuth.adoptUnknownTokens). */
    var adoptUnknownTokens: Boolean
        get() = synchronized(state.lock) { state.oauth.adoptUnknownTokens }
        set(value) = synchronized(state.lock) { state.oauth.adoptUnknownTokens = value }

    /** Every access token refused with invalid_token, refreshes still answered (OAuth.refuseAccessTokens). */
    var refuseAccessTokens: Boolean
        get() = synchronized(state.lock) { state.oauth.refuseAccessTokens }
        set(value) = synchronized(state.lock) { state.oauth.refuseAccessTokens = value }

    /** The apps play from another network than the server's: Automatic's speed test is due when nothing is known of it. */
    var networkRemote: Boolean
        get() = synchronized(state.lock) { state.network.remote }
        set(value) = synchronized(state.lock) { state.network.remote = value }

    /** Off, GET and PUT /api/playback/network answer as an older server's do (404, 405). */
    var networkRoutes: Boolean
        get() = synchronized(state.lock) { state.network.available }
        set(value) = synchronized(state.lock) { state.network.available = value }

    /** Off, subtitle search answers 503 "not set up". */
    var subtitleSearch: Boolean
        get() = state.subtitleSearch
        set(value) {
            state.subtitleSearch = value
        }

    /** Event streams open now. */
    val eventListeners: Int get() = state.events.listeners

    /** Starts listening on 127.0.0.1, on [port] or a free one. */
    fun start(port: Int = 0) {
        synchronized(listening) {
            check(!started) { "the FixtureServer started already" }
            listen(port)
            started = true
            state.events.start()
        }
    }

    /**
     * An access token for [profile] with [scopes], as if the app had signed
     * in: for tests that don't test signing in.
     */
    fun accessToken(profile: Long = FixtureIds.ALEX, scopes: Set<String> = OAuth.SCOPES.toSet()): String {
        require(state.profile(profile) != null) { "no profile $profile" }
        return synchronized(state.lock) { state.oauth.grantFor(profile, scopes) }
    }

    /** Sends [event] to every open event stream. */
    fun emit(event: ServerEvent) = state.events.emit(event)

    override fun close() {
        synchronized(listening) {
            state.events.stop()
            server.close()
        }
    }

    private fun listen(at: Int) {
        val next = MockWebServer()
        next.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = answer(request)
        }
        next.serverSocketFactory = ReusingSockets
        next.start(InetAddress.getByName("127.0.0.1"), at)
        server = next
        port = next.port
    }

    /**
     * Server sockets that always reuse their address. MockWebServer sets it
     * only when given a port, so on Linux the connections left from a first
     * start on a free port would keep a restart ([Mode.Down], [start] with
     * the old port) from binding it again.
     */
    private object ReusingSockets : ServerSocketFactory() {
        override fun createServerSocket(): ServerSocket = object : ServerSocket() {
            override fun setReuseAddress(on: Boolean) = super.setReuseAddress(true)
        }

        override fun createServerSocket(port: Int): ServerSocket = unbound()

        override fun createServerSocket(port: Int, backlog: Int): ServerSocket = unbound()

        override fun createServerSocket(port: Int, backlog: Int, address: InetAddress?): ServerSocket = unbound()

        private fun unbound(): Nothing = throw UnsupportedOperationException("MockWebServer binds the socket itself")
    }

    private fun stopListening() {
        // Open event streams end with the connections.
        state.events.drop()
        server.close()
    }

    private fun answer(request: RecordedRequest): MockResponse {
        val call = Call(request)
        var response = try {
            route(call)
        } catch (e: Exception) {
            Reply.error(500, "internal", "the FixtureServer failed: $e")
        }
        if (call.method == "HEAD") {
            // GET's headers, no body.
            val length = response.headers["Content-Length"]
            response = response.newBuilder().body(Buffer()).apply { if (length != null) setHeader("Content-Length", length) }.build()
        }
        if (mode != Mode.Slow) return response
        return response.newBuilder()
            .headersDelay(options.slowLatencyMs, TimeUnit.MILLISECONDS)
            .throttleBody(options.slowBytesPerSecond / 10, 100, TimeUnit.MILLISECONDS)
            .build()
    }

    private fun route(call: Call): MockResponse {
        val method = if (call.method == "HEAD") "GET" else call.method
        var values: List<String>? = null
        val route = routes.firstOrNull { it.method == method && call.match(it.pattern)?.also { v -> values = v } != null }
            ?: return if (routes.any { call.match(it.pattern) != null }) {
                Reply.error(405, "bad_request", "${call.method} isn't allowed here")
            } else {
                Reply.notFound("no such route: ${call.path}")
            }
        var caller: Caller? = null
        if (route.need != Need.Public) {
            val (who, refusal) = synchronized(state.lock) { state.oauth.authenticate(call) }
            if (refusal != null) return refusal
            caller = who!!
            val need = route.need
            if (need is Need.Scope && need.name !in caller.scopes) {
                val scope = if (need.name == BrowseRoutes.WEB_ONLY) "" else ", scope=\"${need.name}\""
                val message = if (scope.isEmpty()) "apps can't do that: use Pley's web app" else "this app wasn't allowed to: it needs ${need.name}"
                return Reply.error(403, "insufficient_scope", message, "Bearer error=\"insufficient_scope\"$scope")
            }
            if (method != "GET" && call.path.startsWith("/api/") && !call.path.startsWith("/api/oauth/")) {
                state.journal.add(Journal.Entry(method, call.path, call.url.encodedQuery, caller.profile.id, call.text, state.nowMs()))
            }
        }
        return if (route.free) {
            route.handle(call, caller, values!!)
        } else {
            synchronized(state.lock) { route.handle(call, caller, values!!) }
        }
    }
}
