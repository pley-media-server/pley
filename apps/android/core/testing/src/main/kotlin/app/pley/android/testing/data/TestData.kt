package app.pley.android.testing.data

import app.pley.android.auth.StoredTokens
import app.pley.android.common.DeviceProfile
import app.pley.android.common.FormFactor
import app.pley.android.common.SavedServer
import app.pley.android.common.Tier
import app.pley.android.data.AppDataGraph
import app.pley.android.data.DataEnvironment
import app.pley.android.data.LegacyPreferences
import app.pley.android.data.TrackDefaultsRoute
import app.pley.android.model.CastIdentity
import app.pley.android.model.Me
import app.pley.android.model.TrackDefaults
import app.pley.android.network.ApiException
import app.pley.android.network.LiveEvent
import app.pley.android.network.PleyApi
import app.pley.android.network.ServerEvents
import app.pley.android.testing.Fixtures
import app.pley.android.testing.auth.FakeAuthGraph
import app.pley.android.testing.network.FakePleyApi
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody

/**
 * The real data side ([AppDataGraph]) for a test, without a network: the
 * API is [api] (the fixtures' household), sign-ins live in [auth] (in
 * memory), files under [dir], and everything runs on [test]'s scheduler
 * with its virtual clock. [foreground] and [events] stand for the app's
 * lifecycle and the server's event stream.
 *
 * ```
 * @Test fun home() = runTest {
 *     val data = TestData(this, folder.newFolder())
 *     data.signIn()
 *     data.useServer()
 *     data.graph.hubs.home().test { … }
 * }
 * ```
 *
 * Saving tokens needs org.json on the test's classpath (libs.json).
 */
class TestData(
    private val test: TestScope,
    val dir: File,
    val issuer: String = ISSUER,
    val api: FakePleyApi = FakePleyApi(issuer),
    val auth: FakeAuthGraph = FakeAuthGraph(offlineHttp()),
    val legacy: MemoryLegacyPreferences = MemoryLegacyPreferences(),
    val profile: DeviceProfile = PHONE,
    debuggable: Boolean = true,
    /**
     * Where the graph's work runs: the test's background scope. A test
     * that starts the app twice on the same [dir] gives the first a scope
     * it cancels before the second (a DataStore file has one owner at a time).
     */
    scope: CoroutineScope = test.backgroundScope,
    /** What the device is called where it is cast to: a TV's; `null`, a phone, which isn't cast to. */
    castName: (() -> String)? = null,
) {
    val foreground = MutableStateFlow(true)
    val events = FakeServerEvents()

    /** How each event stream opened listed this device to be cast to, in order (`null`: not at all). */
    val streams: MutableList<CastIdentity?> = java.util.Collections.synchronizedList(mutableListOf())
    val trackDefaults = FakeTrackDefaultsRoute()

    /** The APIs asked for: the issuer, and the profile (`null` before GET /api/auth/me answered). */
    val apis: MutableList<Pair<String, Long?>> = java.util.Collections.synchronizedList(mutableListOf())

    /** The API each profile gets: [api] unless a test gives one. */
    val apiOf: MutableMap<Long, PleyApi> = ConcurrentHashMap()

    /** The servers whose APIs the graph let go of (their HTTP caches deleted). */
    val forgottenApis: MutableList<String> = java.util.Collections.synchronizedList(mutableListOf())

    val dispatcher = StandardTestDispatcher(test.testScheduler)

    val graph: AppDataGraph = AppDataGraph(
        DataEnvironment(
            filesDir = { File(dir, "files") },
            cacheDir = { File(dir, "cache") },
            auth = auth,
            api = { i, user ->
                apis += i to user
                user?.let { apiOf[it] } ?: api
            },
            forgetApis = { forgottenApis += it },
            events = { _, cast ->
                streams += cast
                events
            },
            trackDefaults = trackDefaults,
            profile = profile,
            castName = castName,
            legacy = legacy,
            foreground = foreground,
            debuggable = debuggable,
            scope = scope,
            io = dispatcher,
            compute = dispatcher,
            clock = { test.testScheduler.currentTime },
            wallClock = { WALL_CLOCK_START + test.testScheduler.currentTime },
        ),
    )

    /** Signs in to [issuer] (tokens in memory), allowed [scope]. Before the graph reads the sign-in. */
    fun signIn(issuer: String = this.issuer, scope: String = "library play playlists profiles") {
        auth.store.save(
            StoredTokens(
                issuer = issuer,
                tokenEndpoint = "$issuer/oauth/token",
                revocationEndpoint = "$issuer/oauth/revoke",
                accessToken = "access",
                accessExpiresAt = Long.MAX_VALUE,
                refreshToken = "refresh",
                scope = scope,
            ),
        )
    }

    /** Opens [issuer] as the active server. */
    suspend fun useServer(issuer: String = this.issuer, name: String = "Home") {
        graph.servers.use(SavedServer(issuer, name, null, 0))
    }

    companion object {
        /** A server address that never reaches a network: [api] answers. */
        const val ISSUER = "http://192.168.1.20:7539"

        const val WALL_CLOCK_START = 1_800_000_000_000L

        /** A mid-range phone. */
        val PHONE = DeviceProfile(FormFactor.Phone, fireTv = false, tier = Tier.T2, model = "Pixel 7a", totalRamBytes = 8L shl 30, lowRam = false)

        /** A 1 GB Fire TV stick. */
        val FIRE_TV_T0 = DeviceProfile(FormFactor.Tv, fireTv = true, tier = Tier.T0, model = "AFTSS", totalRamBytes = 1L shl 30, lowRam = true)

        /**
         * An HTTP client that answers every request itself, so the
         * sign-in module never reaches a network: the token endpoint gives
         * a new pair (refreshes, profile switches), the rest `200 {}`
         * (revocations).
         */
        fun offlineHttp(): OkHttpClient = OkHttpClient.Builder()
            .addInterceptor { chain ->
                val body = if (chain.request().url.encodedPath.endsWith("/oauth/token")) TOKENS else "{}"
                Response.Builder()
                    .request(chain.request())
                    .protocol(Protocol.HTTP_1_1)
                    .code(200)
                    .message("OK")
                    .body(body.toResponseBody("application/json".toMediaType()))
                    .build()
            }
            .build()

        private const val TOKENS =
            """{"access_token":"access-2","token_type":"Bearer","expires_in":3600,"refresh_token":"refresh-2","scope":"library play playlists profiles"}"""

        /** The profile the fixtures' app acts as (Alex, 2). */
        fun me(): Me = Fixtures.decode("me/app_token.json")
    }
}

/** The web view app's preferences, in memory. */
class MemoryLegacyPreferences(values: Map<Pair<String, String>, String> = emptyMap()) : LegacyPreferences {
    val values: MutableMap<Pair<String, String>, String> = ConcurrentHashMap(values)

    override fun get(file: String, key: String): String? = values[file to key]

    override fun put(file: String, key: String, value: String) {
        values[file to key] = value
    }
}

/**
 * The server's event stream as a test drives it: [send] events to whoever
 * is connected, [refuse] ends the stream as the server would (401, 403).
 * [connections] counts the times the app connected; [connected] is how
 * many are now.
 */
class FakeServerEvents : ServerEvents {
    private sealed interface Signal {
        data class Send(val event: LiveEvent) : Signal

        data class End(val error: Throwable) : Signal
    }

    private val live = MutableSharedFlow<Signal>(extraBufferCapacity = 64)

    @Volatile
    var connections: Int = 0
        private set

    val connected: StateFlow<Int> = live.subscriptionCount

    override fun events(): Flow<LiveEvent> = flow {
        connections++
        live.collect { signal ->
            when (signal) {
                is Signal.Send -> emit(signal.event)
                is Signal.End -> throw signal.error
            }
        }
    }

    /** Sends [event] to the connected app; false when nobody is connected. */
    fun send(event: LiveEvent): Boolean = connected.value > 0 && live.tryEmit(Signal.Send(event))

    /** Ends the stream with the refusal the server would send. */
    fun refuse(status: Int = 401) {
        live.tryEmit(Signal.End(ApiException(status, if (status == 401) "unauthorized" else "forbidden", "refused")))
    }
}

/** PUT /api/me/track-defaults as a test answers it: [answer], or [failure]. */
class FakeTrackDefaultsRoute : TrackDefaultsRoute {
    val calls: MutableList<Pair<String, TrackDefaults>> = java.util.Collections.synchronizedList(mutableListOf())

    @Volatile
    var failure: Throwable? = null

    @Volatile
    var answer: (TrackDefaults) -> Me = { d ->
        TestData.me().copy(audioLanguage = d.audioLanguage, subtitleLanguage = d.subtitleLanguage, subtitleMode = d.subtitleMode)
    }

    override suspend fun put(issuer: String, defaults: TrackDefaults): Me {
        calls += issuer to defaults
        failure?.let { throw it }
        return answer(defaults)
    }
}
