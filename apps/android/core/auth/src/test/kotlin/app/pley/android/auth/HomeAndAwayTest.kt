package app.pley.android.auth

import app.pley.android.common.Addresses
import java.io.IOException
import java.util.Base64
import kotlinx.coroutines.runBlocking
import okhttp3.Cache
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import okhttp3.mockwebserver.SocketPolicy
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * One server at two addresses ([a], signed in at, and [b], which it
 * lists): requests go wherever it proves itself, as from the sign-in
 * address, and its token goes nowhere else (docs/ANDROID.md, "Home and
 * away").
 */
class HomeAndAwayTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val a = MockWebServer()
    private val b = MockWebServer()
    private val store = TokenStore(MemoryShelf(), testVault())
    private val issuer by lazy { issuerOf(a) }
    private val other by lazy { issuerOf(b) }

    private val key = ByteArray(32) { 3 }
    private val proof = ServerProof(SERVER_ID, "12", Base64.getUrlEncoder().withoutPadding().encodeToString(key))

    /** What [b] proves with: the sign-in's key unless a test says otherwise. */
    private var bKey = key

    /** What each address got, besides its proofs. */
    private val atA = mutableListOf<RecordedRequest>()
    private val atB = mutableListOf<RecordedRequest>()

    /** What the API answers at each. */
    private var answerA: (RecordedRequest) -> MockResponse = { MockResponse().setBody("""{"at":"a"}""") }
    private var answerB: (RecordedRequest) -> MockResponse = { MockResponse().setBody("""{"at":"b"}""") }
    private var refreshes = 0

    /** [a] drops every connection once a request arrives, its proofs' too: it went away mid-read. */
    @Volatile
    private var aGone = false

    init {
        a.dispatcher = serving(a, { key }, atA, gone = { aGone }) { answerA(it) }
        b.dispatcher = serving(b, { bKey }, atB) { answerB(it) }
    }

    /** [server] proving itself with [key] at its own address, the rest to [answer]. */
    private fun serving(
        server: MockWebServer,
        key: () -> ByteArray,
        got: MutableList<RecordedRequest>,
        gone: () -> Boolean = { false },
        answer: (RecordedRequest) -> MockResponse,
    ) =
        object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                if (gone()) return MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AFTER_REQUEST)
                val url = request.requestUrl!!
                if (url.encodedPath == "/api/server" && url.queryParameter("challenge") != null) {
                    val address = Addresses.origin(server.url("/").toString())!!
                    val made = ServerProofs.proof(key(), SERVER_ID, address, url.queryParameter("challenge")!!)
                    return MockResponse().setBody(JSONObject().put("id", SERVER_ID).put("proof", made).toString())
                }
                synchronized(got) { got += request }
                if (url.encodedPath == "/api/oauth/token") {
                    refreshes++
                    return MockResponse().setBody(tokenAnswer("pley_at_new$refreshes", "pley_rt_new$refreshes"))
                }
                return answer(request)
            }
        }

    @After
    fun stop() {
        a.shutdown()
        b.shutdown()
    }

    private fun signedIn(expiresAt: Long = Long.MAX_VALUE / 2): Tokens {
        store.save(savedTokens(issuer, expiresAt = expiresAt).copy(proof = proof))
        return Tokens(issuer, store, endpointsClient()).also { runBlocking { it.serverAddresses(listOf(other)) } }
    }

    private fun client(tokens: Tokens): OkHttpClient = AuthHttp.api(endpointsClient(), tokens)

    private fun get(client: OkHttpClient, path: String) = client.newCall(Request.Builder().url("$issuer$path").build()).execute()

    @Test
    fun theSignInAddressIsUsedWhileItAnswers() {
        val client = client(signedIn())
        get(client, "/api/libraries").use { assertEquals("""{"at":"a"}""", it.body.string()) }
        assertEquals("Bearer pley_at_one", atA.single().getHeader("Authorization"))
        assertTrue(atB.isEmpty())
    }

    @Test
    fun requestsGoToTheAddressThatAnswersAsFromTheSignInAddress() {
        val tokens = signedIn()
        a.shutdown()
        val client = client(tokens)
        get(client, "/api/libraries?x=1").use { response ->
            assertEquals("""{"at":"b"}""", response.body.string())
            // Read as the sign-in address's: a playlist's segments stay on it.
            assertEquals("$issuer/api/libraries?x=1", response.request.url.toString())
        }
        val sent = atB.single()
        assertEquals("Bearer pley_at_one", sent.getHeader("Authorization"))
        assertEquals(other, tokens.addresses.value.inUse)
    }

    @Test
    fun anotherDeviceAtTheOtherAddressNeverGetsTheToken() {
        val tokens = signedIn()
        bKey = ByteArray(32) { 7 }
        a.shutdown()
        assertThrows(IOException::class.java) { get(client(tokens), "/api/libraries").close() }
        assertTrue(atB.isEmpty())
        assertEquals(AddressStatus.OTHER_DEVICE, tokens.addresses.value.addresses.first { it.address.url == other }.status)
    }

    @Test
    fun aReadThatGetsNoAnswerGoesOnceMoreToTheNextAddress() {
        val tokens = signedIn()
        val client = client(tokens)
        get(client, "/api/libraries").close()
        // The sign-in address goes away mid-read.
        aGone = true
        get(client, "/api/libraries").use { assertEquals("""{"at":"b"}""", it.body.string()) }
        assertEquals(other, tokens.addresses.value.inUse)
    }

    @Test
    fun aReadThatFailsWhereTheServerStillAnswersStaysThere() {
        val tokens = signedIn()
        val client = client(tokens)
        get(client, "/api/libraries").close()
        // One read fails, and the server proves itself there again: nothing moves.
        answerA = { MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AFTER_REQUEST) }
        assertThrows(IOException::class.java) { get(client, "/api/libraries").close() }
        answerA = { MockResponse().setBody("""{"at":"a"}""") }
        get(client, "/api/libraries").use { assertEquals("""{"at":"a"}""", it.body.string()) }
        assertEquals(issuer, tokens.addresses.value.inUse)
        assertTrue(atB.isEmpty())
    }

    @Test
    fun aChangeThatWasSentIsNeverSentAgain() {
        val tokens = signedIn()
        val client = client(tokens)
        get(client, "/api/libraries").close()
        aGone = true
        val post = Request.Builder().url("$issuer/api/me/progress").post("{}".toRequestBody("application/json".toMediaType())).build()
        assertThrows(IOException::class.java) { client.newCall(post).execute().close() }
        assertTrue(atB.none { it.method == "POST" })
        // The next request goes to the address that answers.
        get(client, "/api/libraries").use { assertEquals("""{"at":"b"}""", it.body.string()) }
    }

    @Test
    fun aRefreshGoesToTheAddressInUse() {
        val tokens = signedIn(expiresAt = System.currentTimeMillis() + 30_000)
        a.shutdown()
        get(client(tokens), "/api/libraries").use { assertEquals(200, it.code) }
        assertEquals("/api/oauth/token", atB.first().path)
        assertEquals("Bearer pley_at_new1", atB.last().getHeader("Authorization"))
        assertEquals("pley_rt_new1", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun signedOutRequestsGoToTheSignInAddressWithoutAToken() {
        val tokens = signedIn()
        runBlocking { tokens.signOut() }
        get(client(tokens), "/api/server").close()
        assertNull(atA.last().getHeader("Authorization"))
        assertTrue(atB.none { it.path == "/api/server" })
        // Signing out forgets the other addresses.
        assertEquals(listOf(issuer), tokens.addresses.value.addresses.map { it.address.url })
        assertEquals(null, store.addresses(issuer))
    }

    @Test
    fun whatTheCacheKeepsComesFromItWhenNoAddressAnswers() {
        val tokens = signedIn()
        val cached = AuthHttp.api(endpointsClient().newBuilder().cache(Cache(folder.newFolder(), 1_000_000)).build(), tokens)
        answerA = { MockResponse().setHeader("Cache-Control", "private, max-age=31536000, immutable").setBody("cover") }
        get(cached, "/api/pictures/1").use { assertEquals("cover", it.body.string()) }
        // Out of reach everywhere, on a new network: a car's cover still shows.
        a.shutdown()
        b.shutdown()
        ServerProofs.networkMayHaveChanged()
        get(cached, "/api/pictures/1").use { assertEquals("cover", it.body.string()) }
        // What it doesn't keep fails, and no token goes anywhere.
        assertThrows(IOException::class.java) { get(cached, "/api/libraries").close() }
    }

    @Test
    fun theAddressesAreKeptWithTheSignIn() {
        signedIn()
        val again = Tokens(issuer, store, endpointsClient())
        assertEquals(listOf(issuer, other), again.addresses.value.addresses.map { it.address.url })
    }

    companion object {
        const val SERVER_ID = "0123456789abcdef0123456789abcdef01234567"
    }
}
