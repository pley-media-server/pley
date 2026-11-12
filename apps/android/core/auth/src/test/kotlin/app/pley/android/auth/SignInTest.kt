package app.pley.android.auth

import app.pley.android.common.UiText
import java.net.URI
import java.net.URLDecoder
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SignInTest {
    private val server = MockWebServer()
    private val shelf = MemoryShelf()
    private val vault = testVault()
    private val store = TokenStore(shelf, vault)
    private var now = 1_000_000L
    private val http = endpointsClient()
    private val tokens = mutableMapOf<String, Tokens>()
    private val signIn = SignIn(http, store, { issuer -> tokens.getOrPut(issuer) { Tokens(issuer, store, http) { now } } }) { now }
    private val issuer by lazy { issuerOf(server) }
    private var scopes = listOf("library", "play", "playlists", "profiles")
    private val requests = mutableListOf<RecordedRequest>()
    private var tokenAnswers = ArrayDeque<MockResponse>()

    init {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                synchronized(requests) { requests += request }
                return when (request.path) {
                    "/.well-known/oauth-authorization-server" -> MockResponse()
                        .setHeader("Content-Type", "application/json")
                        .setBody(
                            JSONObject()
                                .put("issuer", issuer)
                                .put("authorization_endpoint", "$issuer/oauth/authorize")
                                .put("token_endpoint", "$issuer/api/oauth/token")
                                .put("revocation_endpoint", "$issuer/api/oauth/revoke")
                                .put("code_challenge_methods_supported", JSONArray(listOf("S256")))
                                .put("scopes_supported", JSONArray(scopes))
                                .toString(),
                        )
                    "/api/oauth/token" -> synchronized(tokenAnswers) { tokenAnswers.removeFirstOrNull() } ?: MockResponse().setResponseCode(500)
                    "/api/oauth/revoke" -> MockResponse()
                    else -> MockResponse().setResponseCode(404)
                }
            }
        }
    }

    @After
    fun stop() = server.shutdown()

    private fun query(url: String): Map<String, String> = URI(url).rawQuery.split('&').associate {
        URLDecoder.decode(it.substringBefore('='), "UTF-8") to URLDecoder.decode(it.substringAfter('='), "UTF-8")
    }

    private fun callback(vararg params: Pair<String, String>): Callback =
        Callback.parse("$REDIRECT_URI?" + params.joinToString("&") { (n, v) -> "${SignIn.encode(n)}=${SignIn.encode(v)}" })!!

    private fun posted(path: String) = synchronized(requests) { requests.filter { it.path == path } }

    @Test
    fun theAuthorizationUrlAsksForACodeWithPkce() = runBlocking {
        val url = signIn.start(server.url("/").toString())
        assertTrue(url, url.startsWith("$issuer/oauth/authorize?"))
        val q = query(url)
        assertEquals(
            setOf("response_type", "client_id", "redirect_uri", "scope", "state", "code_challenge", "code_challenge_method"),
            q.keys,
        )
        assertEquals("code", q["response_type"])
        assertEquals("https://pley-media-server.github.io/pley/oauth/android.json", q["client_id"])
        assertEquals("app.pley.android:/oauth/callback", q["redirect_uri"])
        assertEquals("library play playlists profiles", q["scope"])
        assertEquals("S256", q["code_challenge_method"])
        val flow = store.pendingFlow(issuer)!!
        assertEquals(flow.state, q["state"])
        assertEquals(Pkce.challenge(flow.verifier), q["code_challenge"])
        assertEquals(issuer, flow.issuer)
        // Everything URL-encoded: no raw ':', '/' or spaces in the query.
        val raw = URI(url).rawQuery
        assertTrue(raw, raw.contains("client_id=https%3A%2F%2Fpley-media-server.github.io%2Fpley%2Foauth%2Fandroid.json"))
        assertTrue(raw, raw.contains("redirect_uri=app.pley.android%3A%2Foauth%2Fcallback"))
        assertTrue(raw, raw.contains("scope=library%20play%20playlists%20profiles"))
        // The verifier never leaves the app with the sign-in page.
        assertTrue(!url.contains(flow.verifier))
    }

    @Test
    fun serversWithoutProfilesAreAskedForWhatTheyOffer() = runBlocking {
        scopes = listOf("library", "play", "playlists")
        assertEquals("library play playlists", query(signIn.start(server.url("/").toString()))["scope"])
    }

    @Test
    fun requestsAreAskedForOnlyWhereTheServerListsThem() = runBlocking {
        scopes = listOf("library", "play", "playlists", "profiles", "requests", "approvals", "admin")
        assertEquals("library play playlists profiles requests approvals", query(signIn.start(server.url("/").toString()))["scope"])
        // Managing the server, where the server can let apps do it.
        scopes = listOf("library", "play", "playlists", "profiles", "requests", "approvals", "activity", "server")
        assertEquals("library play playlists profiles requests approvals server", query(signIn.start(server.url("/").toString()))["scope"])
        scopes = listOf("library", "play", "playlists", "profiles", "requests")
        assertEquals("library play playlists profiles requests", query(signIn.start(server.url("/").toString()))["scope"])
    }

    @Test
    fun oneSignInWaitsPerServerAndOutlivesTheProcess() = runBlocking {
        val first = query(signIn.start(server.url("/").toString()))["state"]!!
        val second = query(signIn.start(server.url("/").toString()))["state"]!!
        // A new process: a new store over the same storage and key.
        val again = SignIn(http, TokenStore(shelf, vault), { Tokens(it, store, http) }) { now }
        assertEquals(CallbackOutcome.Ignored, again.handle(callback("code" to "c", "state" to first, "iss" to issuer)))
        val outcome = again.handle(callback("code" to "c", "state" to second, "iss" to issuer))
        assertTrue(outcome is CallbackOutcome.Exchange)
        // Sealed: the verifier and state aren't readable in storage.
        val flow = (outcome as CallbackOutcome.Exchange).flow
        assertTrue(shelf.values.values.none { it.contains(flow.verifier) || it.contains(second) })
    }

    @Test
    fun callbacksWithAnotherStateAreIgnored() = runBlocking {
        val state = query(signIn.start(server.url("/").toString()))["state"]!!
        assertEquals(CallbackOutcome.Ignored, signIn.handle(callback("code" to "c", "state" to "x$state", "iss" to issuer)))
        assertEquals(CallbackOutcome.Ignored, signIn.handle(callback("code" to "c", "iss" to issuer)))
        // The waiting sign-in is still there for its own answer.
        assertTrue(signIn.handle(callback("code" to "c", "state" to state, "iss" to issuer)) is CallbackOutcome.Exchange)
    }

    @Test
    fun aSecondCallbackIsIgnored() = runBlocking {
        val state = query(signIn.start(server.url("/").toString()))["state"]!!
        tokenAnswers += MockResponse().setBody(tokenAnswer("pley_at_1", "pley_rt_1"))
        assertTrue(signIn.finish(callback("code" to "c1", "state" to state, "iss" to issuer)) is SignInState.SignedIn)
        assertNull(signIn.finish(callback("code" to "c2", "state" to state, "iss" to issuer)))
        assertEquals(1, posted("/api/oauth/token").size)
    }

    @Test
    fun theAnswerMustComeFromTheServerSignedInTo() = runBlocking {
        for (iss in listOf("http://evil.example.com", "$issuer/", null)) {
            val state = query(signIn.start(server.url("/").toString()))["state"]!!
            val params = listOfNotNull("code" to "c", "state" to state, iss?.let { "iss" to it })
            val outcome = signIn.finish(callback(*params.toTypedArray()))
            assertTrue("$iss: $outcome", outcome is SignInState.Failed)
            // It ended that sign-in: the right answer afterwards is ignored.
            assertNull(signIn.finish(callback("code" to "c", "state" to state, "iss" to issuer)))
        }
        assertEquals(0, posted("/api/oauth/token").size)
    }

    @Test
    fun errorsFromTheServerAreShown() = runBlocking {
        var state = query(signIn.start(server.url("/").toString()))["state"]!!
        assertEquals(
            SignInState.Failed(UiText.res(R.string.auth_denied)),
            signIn.finish(callback("error" to "access_denied", "state" to state, "iss" to issuer)),
        )
        state = query(signIn.start(server.url("/").toString()))["state"]!!
        val failed = signIn.finish(
            callback("error" to "invalid_scope", "error_description" to "this app asked for access‮ Pley doesn't offer", "state" to state, "iss" to issuer),
        ) as SignInState.Failed
        assertEquals(UiText.res(R.string.auth_refused_said, "this app asked for access Pley doesn't offer"), failed.text)
        state = query(signIn.start(server.url("/").toString()))["state"]!!
        assertTrue(signIn.finish(callback("state" to state, "iss" to issuer)) is SignInState.Failed)
        assertEquals(0, posted("/api/oauth/token").size)
    }

    @Test
    fun theCodeIsExchangedWithItsVerifier() = runBlocking {
        val state = query(signIn.start(server.url("/").toString()))["state"]!!
        val verifier = store.pendingFlow(issuer)!!.verifier
        tokenAnswers += MockResponse().setBody(tokenAnswer("pley_at_1", "pley_rt_1", "library play"))
        assertEquals(SignInState.SignedIn(issuer), signIn.finish(callback("code" to "the code", "state" to state, "iss" to issuer)))
        val request = posted("/api/oauth/token").single()
        assertEquals("POST", request.method)
        assertTrue(request.getHeader("Content-Type")!!.startsWith("application/x-www-form-urlencoded"))
        assertNull(request.getHeader("Authorization"))
        assertEquals(
            mapOf(
                "grant_type" to "authorization_code",
                "code" to "the code",
                "code_verifier" to verifier,
                "client_id" to CLIENT_ID,
                "redirect_uri" to REDIRECT_URI,
            ),
            formOf(request),
        )
        val saved = store.load(issuer)!!
        assertEquals("pley_at_1", saved.accessToken)
        assertEquals("pley_rt_1", saved.refreshToken)
        assertEquals("library play", saved.scope)
        assertEquals(now + 3_600_000, saved.accessExpiresAt)
        assertTrue(tokens[issuer]!!.signedIn.value)
    }

    @Test
    fun tokensThatCantBeKeptFailTheSignIn() = runBlocking {
        // The Keystore works for the waiting sign-in (kept, then read), and
        // fails when the tokens are kept.
        val key = javax.crypto.KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        var uses = 0
        val store = TokenStore(MemoryShelf(), Vault { if (++uses > 2) throw java.security.ProviderException("Keystore") else key })
        val signIn = SignIn(http, store, { issuer -> Tokens(issuer, store, http) { now } }) { now }
        val state = query(signIn.start(server.url("/").toString()))["state"]!!
        tokenAnswers += MockResponse().setBody(tokenAnswer("pley_at_1", "pley_rt_1", "library play"))
        val finished = signIn.finish(callback("code" to "c", "state" to state, "iss" to issuer))
        assertEquals(SignInState.Failed(Vault.UNKEPT), finished)
        assertNull(store.load(issuer))
    }

    @Test
    fun aRefusedCodeSaysToTryAgain() = runBlocking {
        val state = query(signIn.start(server.url("/").toString()))["state"]!!
        tokenAnswers += MockResponse().setResponseCode(400).setBody("{\"error\":\"invalid_grant\",\"error_description\":\"the code was used already\"}")
        assertEquals(SignInState.Failed(UiText.res(R.string.auth_too_long)), signIn.finish(callback("code" to "c", "state" to state, "iss" to issuer)))
        assertNull(store.load(issuer))
    }

    @Test
    fun signingInAgainEndsTheSignInBefore() = runBlocking {
        store.save(savedTokens(issuer, access = "pley_at_old", refresh = "pley_rt_old"))
        val state = query(signIn.start(server.url("/").toString()))["state"]!!
        tokenAnswers += MockResponse().setBody(tokenAnswer("pley_at_new", "pley_rt_new"))
        assertTrue(signIn.finish(callback("code" to "c", "state" to state, "iss" to issuer)) is SignInState.SignedIn)
        assertEquals("pley_rt_new", store.load(issuer)!!.refreshToken)
        assertEquals(mapOf("token" to "pley_rt_old", "client_id" to CLIENT_ID), formOf(posted("/api/oauth/revoke").single()))
    }

    @Test
    fun waitingSignInsEnd() = runBlocking {
        val state = query(signIn.start(server.url("/").toString()))["state"]!!
        now += SignIn.FLOW_LIFETIME_MS + 1
        assertEquals(CallbackOutcome.Ignored, signIn.handle(callback("code" to "c", "state" to state, "iss" to issuer)))
        assertNull(store.pendingFlow(issuer))
    }

    @Test
    fun callbacksAreReadStrictly() {
        val good = Callback.parse("app.pley.android:/oauth/callback?code=a%2Bb&state=s+t&iss=http%3A%2F%2Fnas%3A7539")!!
        assertEquals("a+b", good.code)
        assertEquals("s t", good.state)
        assertEquals("http://nas:7539", good.iss)
        assertNotNull(Callback.parse("APP.PLEY.ANDROID:/oauth/callback?state=s"))
        for (bad in listOf(
            "app.pley.android://oauth/callback?state=s",
            "app.pley.android:/oauth/callback/x?state=s",
            "app.pley.android:/other?state=s",
            "https://pley.example.com/oauth/callback?state=s",
            "app.pley.android:/oauth/callback?state=a&state=b",
            "app.pley.android:/oauth/callback?code=a&code=b&state=s",
            "app.pley.android:/oauth/callback?state=%zz",
        )) {
            assertNull(bad, Callback.parse(bad))
        }
    }

    @Test
    fun theModuleHoldsAppTokensOnly() {
        // Unit tests run in the module's folder.
        val sources = java.io.File("src/main/kotlin/app/pley/android/auth").listFiles { f -> f.extension == "kt" }.orEmpty()
        assertTrue(sources.size >= 8)
        for (file in sources) {
            val text = file.readText()
            for (never in listOf("CookieManager", "getCookie", "pley_token", "/api/auth/login", "WebView", "password=", "\"code\" to code_")) {
                assertTrue("${file.name} uses $never", !text.contains(never))
            }
        }
    }

    @Test
    fun plainHttpIsForTheHomeNetworkOnly() {
        val refused = runCatching { runBlocking { signIn.start("http://pley.example.com:7539") } }.exceptionOrNull()
        assertEquals(UNENCRYPTED, textOf(refused))
        assertEquals(0, server.requestCount)
    }
}
