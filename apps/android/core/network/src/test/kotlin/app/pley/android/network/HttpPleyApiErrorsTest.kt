package app.pley.android.network

import app.pley.android.auth.R as AuthR
import app.pley.android.common.R as CommonR
import app.pley.android.common.UiText
import app.pley.android.common.sha1Hex
import app.pley.android.testing.network.MockPley
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeout
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** Refusals, failures, the HTTP cache and cancellation. */
class HttpPleyApiErrorsTest {
    @get:Rule val folder = TemporaryFolder()

    private lateinit var server: MockWebServer
    private lateinit var issuer: String
    private lateinit var graph: HttpNetworkGraph
    private lateinit var api: PleyApi

    @Before
    fun setUp() {
        server = MockPley.server()
        issuer = MockPley.issuerOf(server)
        val tokens = testTokens(issuer)
        graph = testGraph(folder.root, { tokens })
        api = graph.api(issuer, userId = 2)
    }

    @After
    fun tearDown() {
        server.close()
    }

    private suspend inline fun <reified E : Throwable> failing(noinline call: suspend () -> Any?): E {
        try {
            call()
        } catch (e: Throwable) {
            if (e is E) return e
            throw AssertionError("expected ${E::class.simpleName}, got $e", e)
        }
        fail("expected ${E::class.simpleName}, got an answer")
        throw IllegalStateException()
    }

    @Test
    fun pleysRefusalsKeepTheirCodeAndMessage() = runTest {
        server.enqueue(MockPley.fixture("errors/not_found.json", code = 404))
        val e = failing<ApiException> { api.item(42) }
        assertEquals(404, e.status)
        assertEquals("not_found", e.code)
        assertEquals("item 42 not found", e.message)
        assertNull(e.bearerError)
    }

    @Test
    fun everyConnectionInUseSaysWhichChannelsHoldThem() = runTest {
        server.enqueue(MockPley.fixture("live/busy.json", code = 409))
        val e = failing<ApiException> { api.watchChannel(1, app.pley.android.model.WatchRequest("android-1")) }
        assertEquals(409, e.status)
        assertEquals("conflict", e.code)
        assertTrue(e.message!!.contains("Sport 1080i"))
        // What holds each connection, and which may be taken over.
        val held = e.connections.single()
        assertEquals(2L, held.channelId)
        assertEquals("Sport 1080i", held.channelName)
        assertTrue(held.yours && held.canTakeOver)
    }

    @Test
    fun insufficientScopeKeepsTheScope() = runTest {
        server.enqueue(MockPley.insufficientScope("play"))
        val e = failing<ApiException> { api.startPlayback(app.pley.android.model.PlaybackRequest(itemId = 101)) }
        assertEquals(403, e.status)
        assertTrue(e.needsScope)
        assertEquals("insufficient_scope", e.bearerError)
        assertEquals("play", e.scope)

        server.enqueue(MockPley.insufficientScope(null))
        val none = failing<ApiException> { api.me() }
        assertTrue(none.needsScope)
        assertNull(none.scope)
    }

    @Test
    fun axumsPlainTextRefusalsAreKeptAsTheirMessage() = runTest {
        for (status in listOf(400, 415, 422)) {
            server.enqueue(MockPley.plainText(status, "Failed to deserialize the JSON body into the target type: missing field `item_id`\n"))
            val e = failing<ApiException> { api.reportProgress(101, app.pley.android.model.ProgressReport(positionMs = 1)) }
            assertEquals(status, e.status)
            assertNull(e.code)
            assertEquals("Failed to deserialize the JSON body into the target type: missing field `item_id`", e.message)
            // The server's words, as it wrote them: none of the app's.
            assertNull(e.text)
        }
    }

    @Test
    fun bodiesWithoutASentenceSayTheStatus() = runTest {
        // In the app's words (NetworkWordsTest reads them): "the server refused (HTTP 500)".
        server.enqueue(MockResponse.Builder().code(500).build())
        assertEquals(UiText.res(AuthR.string.auth_refused_status, 500), failing<ApiException> { api.me() }.text)

        server.enqueue(MockResponse.Builder().code(502).setHeader("Content-Type", "text/html").body("<html><body>Bad gateway</body></html>").build())
        assertEquals(UiText.res(AuthR.string.auth_refused_status, 502), failing<ApiException> { api.me() }.text)
    }

    @Test
    fun anExpiredTokenIsRefreshedOnceAndTheRequestSentAgain() = runTest {
        server.enqueue(MockPley.invalidToken())
        server.enqueue(MockPley.json("""{"access_token":"pley_at_two","token_type":"Bearer","expires_in":3600,"refresh_token":"pley_rt_two","scope":"library play playlists profiles"}"""))
        server.enqueue(MockPley.fixture("me/app_token.json"))

        api.me()

        assertEquals("Bearer pley_at_one", server.takeRequest().headers["Authorization"])
        val refresh = server.takeRequest()
        assertEquals("POST /api/oauth/token HTTP/1.1", refresh.requestLine)
        val again = server.takeRequest()
        assertEquals("GET /api/auth/me HTTP/1.1", again.requestLine)
        assertEquals("Bearer pley_at_two", again.headers["Authorization"])
    }

    @Test
    fun aTokenTheServerRefusesAfterARefreshMeansSignedOut() = runTest {
        server.enqueue(MockPley.invalidToken())
        server.enqueue(MockPley.json("""{"error":"invalid_grant","error_description":"this sign-in has ended"}""", code = 400))
        val e = failing<ApiException> { api.me() }
        assertTrue(e.signedOut)
        assertEquals("invalid_token", e.bearerError)
    }

    @Test
    fun noServerThereIsOffline() = runTest {
        val gone = MockPley.server()
        val address = MockPley.issuerOf(gone)
        gone.close()
        val tokens = testTokens(address)
        val e = failing<OfflineException> { testGraph(folder.newFolder(), { tokens }).api(address, 2).libraries() }
        assertFalse(e.blocked)
        // "nothing answers there" or "no answer in time", in the language shown.
        val said = e.text
        assertTrue("$said", said == UiText.res(CommonR.string.error_nothing_answers) || said == UiText.res(CommonR.string.error_no_answer))
    }

    @Test
    fun anAnswerThatIsntPleysIsUnreadable() = runTest {
        server.enqueue(MockResponse.Builder().setHeader("Content-Type", "text/html").body("<html>router login</html>").build())
        val e = failing<UnreadableAnswerException> { api.server() }
        assertEquals(200, e.status)
        assertEquals(UNREADABLE_ANSWER, e.text)
    }

    @Test
    fun anUnchangedListingComesFromTheCache() = runTest {
        val etag = "W/\"0123456789abcdef01234567\""
        server.enqueue(
            MockPley.fixture("libraries/items.json").newBuilder()
                .setHeader("ETag", etag)
                .setHeader("Cache-Control", "private, no-cache")
                .build(),
        )
        server.enqueue(MockResponse.Builder().code(304).setHeader("ETag", etag).setHeader("Cache-Control", "private, no-cache").build())

        val first = api.libraryItems(1, limit = 200)
        val second = api.libraryItems(1, limit = 200)

        assertEquals(first, second)
        assertNull(server.takeRequest().headers["If-None-Match"])
        val revalidation = server.takeRequest()
        assertEquals(etag, revalidation.headers["If-None-Match"])
        assertEquals("Bearer pley_at_one", revalidation.headers["Authorization"])
        assertTrue(graph.cacheDirOf(issuer, 2).isDirectory)
    }

    @Test
    fun eachServerAndProfileHasItsOwnCache() {
        val dir = graph.cacheDirOf(issuer, 2)
        assertEquals(folder.root.resolve("http/${sha1Hex(issuer)}/2"), dir)
        assertEquals(dir, graph.cacheDirOf("$issuer/", 2))
        assertTrue(graph.cacheDirOf(issuer, 3) != dir)
        // One instance per server and profile, so one cache per folder.
        assertTrue(graph.api(issuer, 2) === graph.api("$issuer/", 2))
        assertTrue(graph.api(issuer, 2) !== graph.api(issuer, 3))
        assertNull(graph.apiClient(issuer, null).cache)
        assertEquals(dir, graph.apiClient(issuer, 2).cache?.directory)
    }

    @Test
    fun aServerForgottenGetsNewClientsOthersKeepTheirs() {
        val before = graph.api(issuer, 2)
        val other = graph.api("http://10.0.0.9:7539", 2)
        // Signing out deleted its cache folder: the next client opens a cache there again.
        graph.forget("$issuer/")
        assertTrue(graph.api(issuer, 2) !== before)
        assertTrue(graph.api("http://10.0.0.9:7539", 2) === other)
    }

    @Test
    fun cancellingTheCoroutineCancelsTheCall() = runBlocking {
        server.enqueue(MockPley.delayed(MockPley.fixture("hubs/home.json"), 10_000))
        val call = async(Dispatchers.Default, start = CoroutineStart.UNDISPATCHED) { api.homeHubs() }
        assertTrue(server.takeRequest(5, TimeUnit.SECONDS) != null)
        assertEquals(1, graph.root.dispatcher.runningCallsCount())

        call.cancel()
        withTimeout(2_000) {
            while (graph.root.dispatcher.runningCallsCount() > 0) kotlinx.coroutines.delay(10)
        }
        assertTrue(call.isCancelled)
    }
}
