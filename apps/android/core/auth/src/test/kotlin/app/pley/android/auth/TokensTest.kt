package app.pley.android.auth

import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class TokensTest {
    private val server = MockWebServer()
    private val shelf = MemoryShelf()
    private val vault = testVault()
    private val store = TokenStore(shelf, vault)
    private var now = 10_000_000L
    private val issuer by lazy { issuerOf(server) }
    private val refreshes = AtomicInteger()
    private val revokes = mutableListOf<Map<String, String>>()

    /** What the token endpoint answers; by default a new pair per refresh. */
    private var answer: (Map<String, String>) -> MockResponse = { form ->
        val n = refreshes.incrementAndGet()
        Thread.sleep(150)
        if (form["refresh_token"] == "pley_rt_one" || form["refresh_token"] == "pley_rt_new${n - 1}") {
            MockResponse().setBody(tokenAnswer("pley_at_new$n", "pley_rt_new$n"))
        } else {
            MockResponse().setResponseCode(400).setBody("{\"error\":\"invalid_grant\",\"error_description\":\"spent\"}")
        }
    }

    init {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = when (request.path) {
                "/api/oauth/token" -> answer(formOf(request))
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

    private fun tokens() = Tokens(issuer, store, endpointsClient()) { now }

    @Test
    fun tokenAnswersAreRead() {
        val t = TokenResponse.parse("""{"access_token":"pley_at_a","token_type":"bearer","expires_in":3600,"refresh_token":"pley_rt_b","scope":"library play"}""")
        assertEquals(TokenResponse("pley_at_a", 3600, "pley_rt_b", "library play"), t)
        // No refresh token for apps that don't list it; no scope means what was asked.
        assertEquals(
            TokenResponse("pley_at_a", 60, null, "library"),
            TokenResponse.parse("""{"access_token":"pley_at_a","token_type":"Bearer","expires_in":60}""", "library"),
        )
        for (bad in listOf(
            "not json",
            """{"token_type":"Bearer","expires_in":60}""",
            """{"access_token":"a","token_type":"mac","expires_in":60}""",
            """{"access_token":"a","expires_in":60}""",
            """{"access_token":"a","token_type":"Bearer","expires_in":0}""",
        )) {
            assertThrows(bad, AuthException::class.java) { TokenResponse.parse(bad) }
        }
        // Printing never shows a token.
        assertFalse(t.toString().contains("pley_"))
        assertFalse(savedTokens(issuer).toString().contains("pley_"))
    }

    @Test
    fun errorsAreReadWithTheirReason() {
        assertEquals(
            OAuthError("invalid_grant", "wrong PIN", "wrong_pin"),
            OAuthError.parse("""{"error":"invalid_grant","error_description":"wrong PIN","reason":"wrong_pin"}"""),
        )
        assertEquals(OAuthError("invalid_client", null, null), OAuthError.parse("""{"error":"invalid_client"}"""))
        // Control characters never reach the screen.
        assertEquals(OAuthError("invalid_request", "ab", null), OAuthError.parse("""{"error":"invalid_request","error_description":"a\u0007b","reason":null}"""))
        assertNull(OAuthError.parse("<html>"))
        assertNull(OAuthError.parse("""{"code":"bad_request"}"""))
    }

    @Test
    fun tokensAreSealedPerServer() {
        store.save(savedTokens(issuer))
        store.save(savedTokens("http://other:7539", access = "pley_at_other", refresh = "pley_rt_other"))
        assertEquals("pley_at_one", store.load(issuer)!!.accessToken)
        assertEquals("pley_rt_other", store.load("http://other:7539")!!.refreshToken)
        assertTrue(shelf.values.values.none { it.contains("pley_") })
        // One server's tokens moved under another's name don't open.
        shelf.values["tokens:http://other:7539"] = shelf.values["tokens:$issuer"]!!
        assertNull(TokenStore(shelf, vault).load("http://other:7539"))
        assertEquals("pley_at_one", TokenStore(shelf, vault).load(issuer)!!.accessToken)
        // Nor do they after the key is gone (a Keystore reset): signed out.
        val other = TokenStore(shelf, testVault())
        assertNull(other.load(issuer))
        assertNull(shelf.get("tokens:$issuer"))
        store.save(savedTokens(issuer))
        store.wipe(issuer)
        assertNull(store.load(issuer))
    }

    @Test
    fun theAccessTokenIsUsedUntilAMinuteBeforeItEnds() = runBlocking {
        store.save(savedTokens(issuer, expiresAt = now + 62_000))
        val tokens = tokens()
        assertEquals("pley_at_one", tokens.accessToken())
        now += 1_000
        assertEquals("pley_at_one", tokens.accessToken())
        assertEquals(0, refreshes.get())
        now += 1_000
        assertEquals("pley_at_new1", tokens.accessToken())
        assertEquals(1, refreshes.get())
    }

    @Test
    fun refreshesHappenOneAtATime() = runBlocking {
        store.save(savedTokens(issuer))
        val tokens = tokens()
        val got = (1..8).map { async(Dispatchers.IO) { tokens.refresh("pley_at_one") } }.awaitAll()
        assertEquals(List(8) { "pley_at_new1" }, got)
        assertEquals(1, refreshes.get())
        val form = formOf(server.takeRequest())
        assertEquals(mapOf("grant_type" to "refresh_token", "refresh_token" to "pley_rt_one", "client_id" to CLIENT_ID), form)
        // A token refreshed already isn't refreshed again for a late 401 with the old one.
        assertEquals("pley_at_new1", tokens.refresh("pley_at_one"))
        assertEquals(1, refreshes.get())
    }

    @Test
    fun theNewRefreshTokenIsSavedBeforeTheAccessTokenIsUsed() = runBlocking {
        store.save(savedTokens(issuer))
        val tokens = tokens()
        assertEquals("pley_at_new1", tokens.refresh("pley_at_one"))
        assertEquals("pley_rt_new1", store.load(issuer)!!.refreshToken)

        // When it can't be saved, the new access token isn't used either.
        shelf.failWrites = true
        assertThrows(IllegalStateException::class.java) { runBlocking { tokens.refresh("pley_at_new1") } }
        shelf.failWrites = false
        assertEquals("pley_at_new1", store.load(issuer)!!.accessToken)
    }

    @Test
    fun invalidGrantMeansSignedOut() = runBlocking {
        store.save(savedTokens(issuer, refresh = "pley_rt_spent"))
        val tokens = tokens()
        assertTrue(tokens.signedIn.value)
        assertThrows(SignedOutException::class.java) { runBlocking { tokens.refresh("pley_at_one") } }
        assertFalse(tokens.signedIn.value)
        assertNull(store.load(issuer))
        assertNull(tokens.accessToken())
    }

    @Test
    fun otherFailuresKeepTheSignIn() = runBlocking {
        store.save(savedTokens(issuer))
        answer = { MockResponse().setResponseCode(500).setBody("{\"error\":\"server_error\"}") }
        val tokens = tokens()
        assertThrows(TokenException::class.java) { runBlocking { tokens.refresh("pley_at_one") } }
        assertTrue(tokens.signedIn.value)
        assertEquals("pley_rt_one", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun signingOutRevokesTheRefreshTokenThenForgets() = runBlocking {
        store.save(savedTokens(issuer))
        val tokens = tokens()
        tokens.signOut()
        assertEquals(listOf(mapOf("token" to "pley_rt_one", "client_id" to CLIENT_ID)), revokes)
        assertNull(store.load(issuer))
        assertFalse(tokens.signedIn.value)

        // Offline, it forgets all the same.
        store.save(savedTokens(issuer))
        server.shutdown()
        tokens.signOut()
        assertNull(store.load(issuer))
    }

    @Test
    fun aFailingKeystoreKeepsNothingAndReadsNothing() {
        // The AndroidKeyStore provider throws ProviderException, a RuntimeException.
        val broken = Vault { throw java.security.ProviderException("Keystore key generation failed") }
        val refused = assertThrows(AuthException::class.java) { broken.seal(ByteArray(3), "http://nas:7539") }
        assertEquals(Vault.UNKEPT, refused.text)
        assertNull(broken.open(ByteArray(40), "http://nas:7539"))
        // So signing in can't keep its tokens, and says so.
        assertThrows(AuthException::class.java) { TokenStore(MemoryShelf(), broken).save(savedTokens("http://nas:7539")) }
    }
}