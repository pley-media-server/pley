package app.pley.android.auth

import app.pley.android.common.UiText
import java.io.IOException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** The TV's sign-in with a code (RFC 8628), on a clock the test moves. */
@OptIn(ExperimentalCoroutinesApi::class)
class DeviceSignInTest {
    private val server = MockWebServer()
    private val shelf = MemoryShelf()
    private val store = TokenStore(shelf, testVault())
    private val http = endpointsClient()
    private val issuer by lazy { issuerOf(server) }

    /** Virtual milliseconds of the running test, as the server sees them. */
    @Volatile private var now: () -> Long = { 0L }

    private var deviceEndpoint: String? = "/api/oauth/device"
    private var grantTypes = listOf("authorization_code", "refresh_token", DEVICE_CODE_GRANT)
    private var scopes = listOf("library", "play", "playlists", "profiles", "admin")
    private var codeAnswer: () -> MockResponse = {
        MockResponse().setBody(
            JSONObject()
                .put("device_code", "the-device-code")
                .put("user_code", "BCDF-GHJK")
                .put("verification_uri", "$issuer/link")
                .put("verification_uri_complete", "$issuer/link?code=BCDFGHJK")
                .put("expires_in", 600)
                .put("interval", 5)
                .toString(),
        )
    }

    /** The token endpoint's answer to a poll at a virtual time. */
    private var pollAnswer: (Long) -> MockResponse = { pending() }
    private val polls = mutableListOf<Pair<Long, Map<String, String>>>()
    private val codeRequests = mutableListOf<Map<String, String>>()
    private val revokes = mutableListOf<Map<String, String>>()

    init {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = when (request.path) {
                "/.well-known/oauth-authorization-server" -> MockResponse()
                    .setHeader("Content-Type", "application/json")
                    .setBody(
                        JSONObject()
                            .put("issuer", issuer)
                            .put("authorization_endpoint", "$issuer/oauth/authorize")
                            .put("token_endpoint", "$issuer/api/oauth/token")
                            .put("revocation_endpoint", "$issuer/api/oauth/revoke")
                            .apply { deviceEndpoint?.let { put("device_authorization_endpoint", if (it.startsWith("/")) issuer + it else it) } }
                            .put("grant_types_supported", JSONArray(grantTypes))
                            .put("code_challenge_methods_supported", JSONArray(listOf("S256")))
                            .put("scopes_supported", JSONArray(scopes))
                            .toString(),
                    )
                "/api/oauth/device" -> {
                    synchronized(codeRequests) { codeRequests += formOf(request) }
                    codeAnswer()
                }
                "/api/oauth/token" -> {
                    val at = now()
                    synchronized(polls) { polls += at to formOf(request) }
                    pollAnswer(at)
                }
                "/api/oauth/revoke" -> {
                    synchronized(revokes) { revokes += formOf(request) }
                    MockResponse()
                }
                else -> MockResponse().setResponseCode(404)
            }
        }
    }

    @After
    fun stop() = server.shutdown()

    private fun error(name: String) = MockResponse().setResponseCode(400).setBody("""{"error":"$name","error_description":"$name"}""")

    private fun pending() = error("authorization_pending")

    private fun TestScope.grant(): DeviceGrant {
        now = { testScheduler.currentTime }
        return DeviceGrant(http, { Tokens(it, store, http) { testScheduler.currentTime } }) { testScheduler.currentTime }
    }

    private fun pollTimes() = synchronized(polls) { polls.map { it.first } }

    @Test
    fun aCodeIsAskedForWithTheAppsIdAndScope() = runTest {
        val code = grant().start(server.url("/").toString())
        assertEquals(mapOf("client_id" to CLIENT_ID, "scope" to "library play playlists profiles"), codeRequests.single())
        assertEquals("BCDF-GHJK", code.userCode)
        assertEquals("the-device-code", code.deviceCode)
        assertEquals("$issuer/link", code.verificationUri)
        assertEquals("$issuer/link?code=BCDFGHJK", code.verificationUriComplete)
        assertEquals(600_000L, code.expiresAt)
        assertEquals(5, code.intervalSeconds)
        assertEquals(issuer, code.issuer)
        assertEquals("$issuer/api/oauth/token", code.tokenEndpoint)
        // Printing never shows the device code.
        assertFalse(code.toString().contains("the-device-code"))
    }

    @Test
    fun aCodeAsksForTheScopesOfRequestsWhereTheServerListsThem() = runTest {
        // Phones' and TVs' alike: both ask for movies and shows, and approve them on an administrator's account.
        scopes = listOf("library", "play", "playlists", "profiles", "requests", "approvals", "admin")
        grant().start(server.url("/").toString())
        assertEquals("library play playlists profiles requests approvals", codeRequests.single()["scope"])
    }

    @Test
    fun codesAreShownInTwoHalves() {
        assertEquals("BCDF-GHJK", DeviceGrant.shown("bcdfghjk"))
        assertEquals("BCDF-GHJK", DeviceGrant.shown("BCDF GHJK"))
        assertEquals("ABC", DeviceGrant.shown("ABC"))
    }

    @Test
    fun answersThatArentCodesAreRefused() = runTest {
        for (bad in listOf(
            "<html>",
            """{"user_code":"BCDF-GHJK","verification_uri":"http://x/link","expires_in":600}""",
            """{"device_code":"d","user_code":"BCDF-GHJK","verification_uri":"javascript:alert(1)","expires_in":600}""",
            """{"device_code":"d","user_code":"BCDF-GHJK","verification_uri":"http://x/link","expires_in":0}""",
        )) {
            codeAnswer = { MockResponse().setBody(bad) }
            val error = runCatching { grant().start(server.url("/").toString()) }.exceptionOrNull()
            assertTrue(bad, error is AuthException)
        }
        // Without an interval, the RFC's 5 seconds.
        codeAnswer = { MockResponse().setBody("""{"device_code":"d","user_code":"BCDF-GHJK","verification_uri":"http://x/link","expires_in":600}""") }
        assertEquals(5, grant().start(server.url("/").toString()).intervalSeconds)
    }

    @Test
    fun serversWithoutCodesSaySo() = runTest {
        deviceEndpoint = null
        assertEquals(NO_CODES, textOf(runCatching { grant().start(server.url("/").toString()) }.exceptionOrNull()))
        // The endpoint without the grant to collect the tokens isn't enough.
        deviceEndpoint = "/api/oauth/device"
        grantTypes = listOf("authorization_code", "refresh_token")
        assertEquals(NO_CODES, textOf(runCatching { grant().start(server.url("/").toString()) }.exceptionOrNull()))
        assertTrue(codeRequests.isEmpty())
    }

    @Test
    fun theDeviceEndpointMustBeOnTheServer() = runTest {
        deviceEndpoint = "http://elsewhere.example.com/api/oauth/device"
        val error = runCatching { grant().start(server.url("/").toString()) }.exceptionOrNull()
        assertEquals(UiText.res(R.string.auth_other_endpoint), textOf(error))
        assertTrue(codeRequests.isEmpty())
        val metadata = ServerMetadata.parse(
            """{"issuer":"http://nas:7539","authorization_endpoint":"http://nas:7539/oauth/authorize",
               "token_endpoint":"http://nas:7539/api/oauth/token","revocation_endpoint":"http://nas:7539/api/oauth/revoke",
               "code_challenge_methods_supported":["S256"]}""",
            "http://nas:7539",
        )
        assertNull(metadata.deviceAuthorizationEndpoint)
        assertFalse(metadata.signsInWithCodes)
    }

    @Test
    fun pendingThenSlowDownThenSignedIn() = runTest {
        val grant = grant()
        val code = grant.start(server.url("/").toString())
        var n = 0
        pollAnswer = {
            when (n++) {
                0 -> pending()
                1 -> error("slow_down")
                2 -> pending()
                else -> MockResponse().setBody(tokenAnswer("pley_at_tv", "pley_rt_tv"))
            }
        }
        assertEquals(listOf(DevicePoll.Waiting, DevicePoll.SignedIn(issuer)), grant.poll(code).toList())
        // Every 5 seconds, then 10 after slow_down, for this wait and every later one.
        assertEquals(listOf(5_000L, 10_000L, 20_000L, 30_000L), pollTimes())
        assertEquals(
            mapOf("grant_type" to DEVICE_CODE_GRANT, "device_code" to "the-device-code", "client_id" to CLIENT_ID),
            polls.first().second,
        )
        val saved = store.load(issuer)!!
        assertEquals("pley_at_tv", saved.accessToken)
        assertEquals("pley_rt_tv", saved.refreshToken)
        assertEquals("$issuer/api/oauth/token", saved.tokenEndpoint)
        assertTrue(revokes.isEmpty())
    }

    @Test
    fun aDeniedCodeSaysSo() = runTest {
        val grant = grant()
        val code = grant.start(server.url("/").toString())
        pollAnswer = { at -> if (at < 10_000) pending() else error("access_denied") }
        assertEquals(listOf(DevicePoll.Waiting, DevicePoll.Denied), grant.poll(code).toList())
        assertEquals(listOf(5_000L, 10_000L), pollTimes())
        assertNull(store.load(issuer))
    }

    @Test
    fun anExpiredCodeSaysSo() = runTest {
        val grant = grant()
        val code = grant.start(server.url("/").toString())
        pollAnswer = { error("expired_token") }
        assertEquals(listOf(DevicePoll.Waiting, DevicePoll.Expired), grant.poll(code).toList())

        // The app's own clock ends it too, at the moment it ends.
        polls.clear()
        pollAnswer = { pending() }
        val short = code.copy(expiresAt = 12_000 + testScheduler.currentTime)
        val started = testScheduler.currentTime
        assertEquals(listOf(DevicePoll.Waiting, DevicePoll.Expired), grant.poll(short).toList())
        assertEquals(listOf(started + 5_000, started + 10_000), pollTimes())
        assertEquals(started + 12_000, testScheduler.currentTime)

        // Coming back to a code that ended asks nothing.
        polls.clear()
        assertEquals(listOf(DevicePoll.Waiting, DevicePoll.Expired), grant.poll(short).toList())
        assertTrue(pollTimes().isEmpty())
    }

    @Test
    fun networkFailuresBackOff() = runTest {
        codeAnswer = {
            MockResponse().setBody(
                """{"device_code":"d","user_code":"BCDF-GHJK","verification_uri":"http://x/link","expires_in":600,"interval":1}""",
            )
        }
        now = { testScheduler.currentTime }
        // The network is down until 61 s.
        val tries = mutableListOf<Long>()
        val offline = endpointsClient(
            OkHttpClient.Builder().addInterceptor { chain ->
                val at = testScheduler.currentTime
                if (chain.request().url.encodedPath == "/api/oauth/token") synchronized(tries) { tries += at }
                if (at in 1..60_999) throw IOException("no network")
                chain.proceed(chain.request())
            }.build(),
        )
        val grant = DeviceGrant(offline, { Tokens(it, store, offline) }) { testScheduler.currentTime }
        val code = grant.start(server.url("/").toString())
        pollAnswer = { MockResponse().setBody(tokenAnswer("pley_at_tv", "pley_rt_tv")) }
        // Each failure says when the next try is.
        val trouble = listOf(2_000L, 4_000L, 8_000L, 16_000L, 30_000L).map { DevicePoll.Unreachable(it) }
        assertEquals(listOf(DevicePoll.Waiting) + trouble + DevicePoll.SignedIn(issuer), grant.poll(code).toList())
        // 1 s, then 2, 4, 8, 16 and 30 s after failures in a row.
        assertEquals(listOf(1_000L, 3_000L, 7_000L, 15_000L, 31_000L, 61_000L), tries)
    }

    @Test
    fun theSignInItReplacesIsEnded() = runTest {
        store.save(savedTokens(issuer, access = "pley_at_old", refresh = "pley_rt_old"))
        val grant = grant()
        val code = grant.start(server.url("/").toString())
        pollAnswer = { MockResponse().setBody(tokenAnswer("pley_at_tv", "pley_rt_tv")) }
        assertEquals(DevicePoll.SignedIn(issuer), grant.poll(code).toList().last())
        assertEquals("pley_rt_tv", store.load(issuer)!!.refreshToken)
        assertEquals(listOf(mapOf("token" to "pley_rt_old", "client_id" to CLIENT_ID)), revokes)
    }

    @Test
    fun moreThanWasAskedIsEndedAndRefused() = runTest {
        store.save(savedTokens(issuer, access = "pley_at_old", refresh = "pley_rt_old"))
        val grant = grant()
        val code = grant.start(server.url("/").toString())
        pollAnswer = { MockResponse().setBody(tokenAnswer("pley_at_tv", "pley_rt_tv", scope = "library play playlists profiles admin")) }
        assertEquals(DevicePoll.Failed(WIDER_SCOPE), grant.poll(code).toList().last())
        // The new grant is ended; the sign-in before is left as it was.
        assertEquals(listOf(mapOf("token" to "pley_rt_tv", "client_id" to CLIENT_ID)), revokes)
        assertEquals("pley_rt_old", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun otherRefusalsStopPolling() = runTest {
        val grant = grant()
        val code = grant.start(server.url("/").toString())
        pollAnswer = { error("invalid_grant") }
        assertEquals(listOf(DevicePoll.Waiting, DevicePoll.Failed(UiText.res(R.string.auth_code_used))), grant.poll(code).toList())
        polls.clear()
        pollAnswer = { MockResponse().setResponseCode(400).setBody("""{"error":"invalid_client","error_description":"only Pley's own apps"}""") }
        assertEquals(DevicePoll.Failed(UiText.raw("only Pley's own apps")), grant.poll(code).toList().last())
        assertEquals(1, polls.size)
    }

    @Test
    fun plainHttpIsForTheHomeNetworkOnly() = runTest {
        val error = assertThrows(AuthException::class.java) {
            kotlinx.coroutines.runBlocking { grant().start("http://pley.example.com:7539") }
        }
        assertEquals(UNENCRYPTED, error.text)
        assertEquals(0, server.requestCount)
    }
}
