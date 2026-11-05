package app.pley.android.auth

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
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

/** No token answer may hold more than the app asked for: code exchange, refresh and switch (the device grant in DeviceSignInTest). */
@OptIn(ExperimentalCoroutinesApi::class)
class ScopeGuardTest {
    private val server = MockWebServer()
    private val store = TokenStore(MemoryShelf(), testVault())
    private val http = endpointsClient()
    private val issuer by lazy { issuerOf(server) }
    private val events = AuthEvents()
    private var answer: MockResponse = MockResponse().setBody(tokenAnswer("pley_at_new", "pley_rt_new"))
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
                            .put("code_challenge_methods_supported", JSONArray(listOf("S256")))
                            .put("scopes_supported", JSONArray(listOf("library", "play", "playlists", "profiles", "admin")))
                            .toString(),
                    )
                "/api/oauth/token" -> answer
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

    private fun tokens() = Tokens(issuer, store, http, events)

    private fun wider(scope: String = "library play playlists profiles admin") =
        MockResponse().setBody(tokenAnswer("pley_at_wide", "pley_rt_wide", scope = scope))

    @Test
    fun onlyNamesNotAskedForCount() {
        val answer = TokenResponse("a", 60, "r", "play library")
        assertTrue(ScopeGuard.extra(answer, "library play playlists").isEmpty())
        assertEquals(setOf("admin"), ScopeGuard.extra(answer.copy(scope = "library admin"), "library play"))
        // Less than asked is what the person allowed.
        assertTrue(ScopeGuard.extra(answer.copy(scope = "library"), "library play").isEmpty())
    }

    @Test
    fun aCodeExchangeWithMoreIsEndedAndRefused() = runBlocking {
        store.save(savedTokens(issuer, access = "pley_at_old", refresh = "pley_rt_old"))
        val signIn = SignIn(http, store, { tokens() })
        val url = signIn.start(server.url("/").toString())
        val state = java.net.URI(url).rawQuery.split('&').first { it.startsWith("state=") }.substringAfter('=')
        answer = wider()
        val callback = Callback.parse("$REDIRECT_URI?code=c&state=$state&iss=${SignIn.encode(issuer)}")!!
        assertEquals(SignInState.Failed(WIDER_SCOPE), signIn.finish(callback))
        assertEquals(listOf(mapOf("token" to "pley_rt_wide", "client_id" to CLIENT_ID)), revokes)
        // Nothing kept: the sign-in before stays.
        assertEquals("pley_rt_old", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun aRefreshWithMoreSignsOut() = runTest {
        val heard = mutableListOf<AuthEvent>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { events.events.collect { heard += it } }
        store.save(savedTokens(issuer, scope = "library play"))
        val tokens = tokens()
        answer = wider("library play playlists")
        assertThrows(WiderScopeException::class.java) { runBlocking { tokens.refresh("pley_at_one") } }
        assertEquals(listOf(mapOf("token" to "pley_rt_wide", "client_id" to CLIENT_ID)), revokes)
        assertNull(store.load(issuer))
        assertFalse(tokens.signedIn.value)
        assertEquals(listOf<AuthEvent>(AuthEvent.SignedOut(issuer, SignOutReason.WIDER_SCOPE)), heard)
    }

    @Test
    fun aSwitchWithMoreSignsOut() = runBlocking {
        store.save(savedTokens(issuer))
        val tokens = tokens()
        answer = wider()
        assertThrows(WiderScopeException::class.java) { runBlocking { tokens.switchProfile(2, null) } }
        assertEquals(listOf(mapOf("token" to "pley_rt_wide", "client_id" to CLIENT_ID)), revokes)
        assertNull(store.load(issuer))
        assertFalse(tokens.signedIn.value)
    }

    @Test
    fun theSameOrLessIsKept() = runBlocking {
        store.save(savedTokens(issuer))
        val tokens = tokens()
        answer = MockResponse().setBody(tokenAnswer("pley_at_new", "pley_rt_new", scope = "library play"))
        assertEquals("pley_at_new", tokens.refresh("pley_at_one"))
        assertEquals("library play", store.load(issuer)!!.scope)
        assertTrue(revokes.isEmpty())
    }
}
