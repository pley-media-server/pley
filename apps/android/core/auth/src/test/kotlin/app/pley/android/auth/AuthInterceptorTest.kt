package app.pley.android.auth

import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Proxy
import java.net.ProxySelector
import java.net.Socket
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import javax.net.SocketFactory
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import okhttp3.Address
import okhttp3.Call
import okhttp3.Connection
import okhttp3.ConnectionSpec
import okhttp3.Dns
import okhttp3.Handshake
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import okhttp3.tls.HandshakeCertificates
import okhttp3.tls.HeldCertificate
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AuthInterceptorTest {
    private val home = MockWebServer()
    private val other = MockWebServer()
    private val shelf = MemoryShelf()
    private val store = TokenStore(shelf, testVault())
    private val issuer by lazy { issuerOf(home) }
    private val refreshes = AtomicInteger()
    private val events = AuthEvents()

    /** What the server's API answers, by path. */
    private val api = mutableMapOf<String, (RecordedRequest) -> MockResponse>()

    /** What another server answers. */
    private var elsewhere: MockResponse = MockResponse().setBody("{}")

    init {
        other.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = elsewhere
        }
        home.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                if (request.path == "/api/oauth/token") {
                    val n = refreshes.incrementAndGet()
                    return MockResponse().setBody(tokenAnswer("pley_at_fresh$n", "pley_rt_fresh$n"))
                }
                return api[request.path]?.invoke(request) ?: MockResponse().setBody("{}")
            }
        }
    }

    @After
    fun stop() {
        home.shutdown()
        other.shutdown()
    }

    private fun client(base: OkHttpClient = OkHttpClient()): OkHttpClient =
        AuthHttp.api(endpointsClient(base), Tokens(issuer, store, endpointsClient(base)))

    private fun get(client: OkHttpClient, url: String): Response = client.newCall(Request.Builder().url(url).build()).execute()

    /** Answers 401 invalid_token to anything but [good]. */
    private fun onlyAccepts(good: String) = { request: RecordedRequest ->
        if (request.getHeader("Authorization") == "Bearer $good") {
            MockResponse().setBody("{\"ok\":true}")
        } else {
            MockResponse().setResponseCode(401).setHeader("WWW-Authenticate", "Bearer error=\"invalid_token\"")
        }
    }

    @Test
    fun bearerGoesToTheServersOwnAddressOnly() {
        store.save(savedTokens(issuer))
        val client = client()
        get(client, home.url("/api/libraries").toString()).close()
        val sent = home.takeRequest()
        assertEquals("Bearer pley_at_one", sent.getHeader("Authorization"))
        assertEquals("pleyTest/1", sent.getHeader("User-Agent"))
        assertNull("never a token in the address", sent.requestUrl?.query)

        // Another port of the same host is another server.
        get(client, other.url("/x").toString()).close()
        assertNull(other.takeRequest().getHeader("Authorization"))
        // Even when the caller adds one by hand.
        client.newCall(Request.Builder().url(other.url("/y")).header("Authorization", "Bearer pley_at_one").build()).execute().close()
        assertNull(other.takeRequest().getHeader("Authorization"))
    }

    @Test
    fun aRedirectElsewhereLeavesTheTokenBehind() {
        store.save(savedTokens(issuer))
        api["/api/media/1"] = { MockResponse().setResponseCode(302).setHeader("Location", other.url("/file").toString()) }
        get(client(), home.url("/api/media/1").toString()).close()
        assertEquals("Bearer pley_at_one", home.takeRequest().getHeader("Authorization"))
        val followed = other.takeRequest()
        assertEquals("/file", followed.path)
        assertNull(followed.getHeader("Authorization"))
    }

    @Test
    fun httpsNeverFollowsARedirectToHttp() {
        val certificate = HeldCertificate.Builder()
            .addSubjectAlternativeName("localhost")
            .addSubjectAlternativeName(InetAddress.getByName("localhost").canonicalHostName)
            .addSubjectAlternativeName("127.0.0.1")
            .build()
        val serverCerts = HandshakeCertificates.Builder().heldCertificate(certificate).build()
        val clientCerts = HandshakeCertificates.Builder().addTrustedCertificate(certificate.certificate).build()
        val secure = MockWebServer()
        secure.useHttps(serverCerts.sslSocketFactory(), false)
        try {
            secure.enqueue(MockResponse().setResponseCode(302).setHeader("Location", other.url("/plain").toString()))
            val secureIssuer = issuerOf(secure)
            assertTrue(secureIssuer.startsWith("https://"))
            store.save(savedTokens(secureIssuer))
            val base = OkHttpClient.Builder().sslSocketFactory(clientCerts.sslSocketFactory(), clientCerts.trustManager).build()
            val client = AuthHttp.api(endpointsClient(base), Tokens(secureIssuer, store, endpointsClient(base)))
            val response = get(client, secure.url("/api/items/1").toString())
            response.close()
            assertEquals(302, response.code)
            assertEquals("Bearer pley_at_one", secure.takeRequest().getHeader("Authorization"))
            assertEquals(0, other.requestCount)
        } finally {
            secure.shutdown()
        }
    }

    @Test
    fun anExpiredTokenIsRefreshedOnceAndTheRequestRetried() {
        store.save(savedTokens(issuer))
        api["/api/libraries"] = onlyAccepts("pley_at_fresh1")
        val response = get(client(), home.url("/api/libraries").toString())
        response.close()
        assertEquals(200, response.code)
        assertEquals(1, refreshes.get())
        val first = home.takeRequest()
        assertEquals("Bearer pley_at_one", first.getHeader("Authorization"))
        assertEquals("/api/oauth/token", home.takeRequest().path)
        assertEquals("Bearer pley_at_fresh1", home.takeRequest().getHeader("Authorization"))
        assertEquals("pley_rt_fresh1", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun aRedirectWithinTheServerStillGetsItsRefresh() {
        store.save(savedTokens(issuer))
        api["/api/old"] = { MockResponse().setResponseCode(302).setHeader("Location", "/api/libraries") }
        api["/api/libraries"] = onlyAccepts("pley_at_fresh1")
        val response = get(client(), home.url("/api/old").toString())
        response.close()
        assertEquals(200, response.code)
        assertEquals(1, refreshes.get())
    }

    @Test
    fun aRefreshThatDoesntHelpIsTheAnswer() {
        store.save(savedTokens(issuer))
        api["/api/libraries"] = onlyAccepts("never")
        val response = get(client(), home.url("/api/libraries").toString())
        response.close()
        assertEquals(401, response.code)
        assertEquals(1, refreshes.get())
    }

    @Test
    fun aServerThatRefusesAFreshlyRefreshedTokenEndsTheSignIn() = runTest {
        val heard = mutableListOf<AuthEvent>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { events.events.collect { heard += it } }
        store.save(savedTokens(issuer))
        api["/api/libraries"] = onlyAccepts("never")
        val tokens = Tokens(issuer, store, endpointsClient(), events)
        val client = AuthHttp.api(endpointsClient(), tokens)

        val response = get(client, home.url("/api/libraries").toString())
        response.close()
        assertEquals(401, response.code)
        assertEquals(1, refreshes.get())
        assertNull(store.load(issuer))
        assertFalse(tokens.signedIn.value)
        assertEquals(listOf<AuthEvent>(AuthEvent.SignedOut(issuer, SignOutReason.ENDED)), heard)

        // Signed out: the next request goes without a token, and nothing refreshes or ends again.
        get(client, home.url("/api/libraries").toString()).close()
        assertEquals(1, refreshes.get())
        assertEquals(1, heard.size)
        assertEquals(4, home.requestCount)
    }

    @Test
    fun aTokenJustRefreshedAheadOfItsEndThatIsRefusedEndsTheSignInWithoutAnotherRefresh() = runTest {
        val heard = mutableListOf<AuthEvent>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { events.events.collect { heard += it } }
        // About to end: the request refreshes before it goes, then the new token is refused.
        store.save(savedTokens(issuer, expiresAt = System.currentTimeMillis() + 30_000))
        api["/api/libraries"] = onlyAccepts("never")
        val tokens = Tokens(issuer, store, endpointsClient(), events)

        val response = get(AuthHttp.api(endpointsClient(), tokens), home.url("/api/libraries").toString())
        response.close()
        assertEquals(401, response.code)
        assertEquals(1, refreshes.get())
        assertNull(store.load(issuer))
        assertEquals(listOf<AuthEvent>(AuthEvent.SignedOut(issuer, SignOutReason.ENDED)), heard)
    }

    @Test
    fun aTokenRefreshedLongAgoThatIsRefusedIsRefreshedAsUsual() {
        var now = 1_000_000L
        store.save(savedTokens(issuer))
        api["/api/libraries"] = onlyAccepts("pley_at_fresh1")
        val tokens = Tokens(issuer, store, endpointsClient(), clock = { now })
        val client = AuthHttp.api(endpointsClient(), tokens)
        get(client, home.url("/api/libraries").toString()).close()
        assertEquals(1, refreshes.get())

        // Two minutes on, the server refuses that token: an ordinary refresh, and the sign-in goes on.
        now += 2 * 60_000
        api["/api/libraries"] = onlyAccepts("pley_at_fresh2")
        val response = get(client, home.url("/api/libraries").toString())
        response.close()
        assertEquals(200, response.code)
        assertEquals(2, refreshes.get())
        assertTrue(tokens.signedIn.value)
        assertEquals("pley_at_fresh2", store.load(issuer)!!.accessToken)
    }

    @Test
    fun otherRefusalsAreNotRetried() {
        store.save(savedTokens(issuer))
        api["/api/activity"] = {
            MockResponse().setResponseCode(403).setHeader("WWW-Authenticate", "Bearer error=\"insufficient_scope\", scope=\"play\"")
        }
        api["/api/other"] = { MockResponse().setResponseCode(401).setHeader("WWW-Authenticate", "Bearer realm=\"pley\"") }
        api["/api/plain"] = { MockResponse().setResponseCode(401) }
        for (path in listOf("/api/activity", "/api/other", "/api/plain")) {
            val response = get(client(), home.url(path).toString())
            response.close()
            assertTrue(path, response.code == 401 || response.code == 403)
        }
        assertEquals(0, refreshes.get())
        assertEquals(3, home.requestCount)
    }

    @Test
    fun aRefusalFromElsewhereIsntRefreshedFor() {
        store.save(savedTokens(issuer))
        elsewhere = MockResponse().setResponseCode(401).setHeader("WWW-Authenticate", "Bearer error=\"invalid_token\"")
        val response = get(client(), other.url("/x").toString())
        response.close()
        assertEquals(401, response.code)
        assertEquals(0, refreshes.get())
    }

    @Test
    fun addressesWithCharactersJavaRefusesAreStillTheServers() {
        store.save(savedTokens(issuer))
        api["/api/search?q=a|b"] = onlyAccepts("pley_at_fresh1")
        val response = get(client(), home.url("/api/search?q=a|b").toString())
        response.close()
        assertEquals(200, response.code)
        assertEquals("Bearer pley_at_one", home.takeRequest().getHeader("Authorization"))
        assertEquals(1, refreshes.get())
        home.takeRequest()
        assertEquals("Bearer pley_at_fresh1", home.takeRequest().getHeader("Authorization"))
        get(client(), home.url("/api/items/a[1]/b").toString()).close()
        assertEquals("Bearer pley_at_fresh1", home.takeRequest().getHeader("Authorization"))
    }

    @Test
    fun aKeystoreThatFailsFailsTheRequestNotTheApp() {
        // Tokens about to end: the request refreshes them, and keeping the
        // new ones fails.
        var broken = false
        val key = javax.crypto.KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        val store = TokenStore(MemoryShelf(), Vault { if (broken) throw java.security.ProviderException("Keystore") else key })
        store.save(savedTokens(issuer, expiresAt = System.currentTimeMillis() + 30_000))
        broken = true
        val client = AuthHttp.api(endpointsClient(), Tokens(issuer, store, endpointsClient()))
        val failed = assertThrows(java.io.IOException::class.java) { get(client, home.url("/api/libraries").toString()) }
        assertEquals(Vault.UNKEPT, textOf(failed))
    }

    @Test
    fun aStoreThatFailsFailsTheRequestNotTheApp() {
        // A refused token whose refresh can't be kept: the refusal is the answer.
        store.save(savedTokens(issuer))
        api["/api/libraries"] = onlyAccepts("pley_at_fresh1")
        shelf.failWrites = true
        val response = get(client(), home.url("/api/libraries").toString())
        response.close()
        assertEquals(401, response.code)
        // About to end, then: the request fails with an IOException.
        store.wipe(issuer)
        shelf.failWrites = false
        store.save(savedTokens(issuer, expiresAt = System.currentTimeMillis() + 30_000))
        shelf.failWrites = true
        assertThrows(java.io.IOException::class.java) { get(client(), home.url("/api/libraries").toString()) }
    }

    @Test
    fun invalidTokenChallengesAreRecognized() {
        assertTrue(TokenAuthenticator.invalidToken("Bearer error=\"invalid_token\""))
        assertTrue(TokenAuthenticator.invalidToken("bearer realm=\"pley\", error=\"invalid_token\", error_description=\"expired\""))
        assertFalse(TokenAuthenticator.invalidToken("Bearer error=\"insufficient_scope\""))
        assertFalse(TokenAuthenticator.invalidToken("Bearer error=\"invalid_token_x\""))
        assertFalse(TokenAuthenticator.invalidToken("Basic realm=\"pley\""))
        assertFalse(TokenAuthenticator.invalidToken("Bearerx error=\"invalid_token\""))
    }

    @Test
    fun privateAddressesAreTheHomeNetworks() {
        for (private in listOf("127.0.0.1", "10.0.0.5", "172.16.0.1", "172.31.255.255", "192.168.1.20", "169.254.1.1", "100.64.0.1", "100.127.255.255", "::1", "fd7a:115c:a1e0::1", "fe80::1")) {
            assertTrue(private, Transport.isPrivate(InetAddress.getByName(private)))
        }
        for (public in listOf("8.8.8.8", "172.32.0.1", "100.128.0.1", "192.169.0.1", "2606:4700:4700::1111", "11.0.0.1")) {
            assertFalse(public, Transport.isPrivate(InetAddress.getByName(public)))
        }
    }

    @Test
    fun plainHttpSignInsAreForTheHomeNetwork() {
        for (ok in listOf("https://pley.example.com", "http://192.168.1.20:7539", "http://nas:7539", "http://pley.local:7539", "http://box.tail1234.ts.net:7539", "http://[fd7a:115c:a1e0::1]:7539")) {
            Transport.checkAddress(ok)
        }
        for (refused in listOf("http://pley.example.com", "http://8.8.8.8:7539", "ftp://nas")) {
            assertThrows(refused, AuthException::class.java) { Transport.checkAddress(refused) }
        }
    }

    @Test
    fun plainHttpGoesOutOnlyToPrivateAddresses() {
        val publicIssuer = "http://8.8.8.8:7539"
        store.save(savedTokens(publicIssuer))
        val interceptor = AuthInterceptor(Tokens(publicIssuer, store, endpointsClient()))
        var sent = 0
        val chain = FakeChain(Request.Builder().url("$publicIssuer/api/libraries").build(), InetAddress.getByName("8.8.8.8")) { sent++ }
        assertEquals(UNENCRYPTED, assertThrows(AuthException::class.java) { interceptor.intercept(chain) }.text)
        assertEquals(0, sent)

        // The same name on the home network works.
        val home = FakeChain(Request.Builder().url("$publicIssuer/api/libraries").build(), InetAddress.getByName("192.168.1.20")) { sent++ }
        interceptor.intercept(home)
        assertEquals("Bearer pley_at_one", home.sent?.header("Authorization"))
        assertEquals(1, sent)
    }

    /**
     * A network chain whose connection goes to [to]. What the interceptor
     * doesn't use (OkHttp 5's per-call settings among it) isn't there.
     */
    private class FakeChain(private val request: Request, private val to: InetAddress, private val onSend: () -> Unit) :
        Interceptor.Chain by unsupported() {
        var sent: Request? = null

        private val connection = object : Connection {
            override fun route(): Route = Route(
                Address(
                    "pley.test", 7539, Dns.SYSTEM, SocketFactory.getDefault(), null, null, null,
                    okhttp3.Authenticator.NONE, null, listOf(Protocol.HTTP_1_1), listOf(ConnectionSpec.CLEARTEXT), ProxySelector.getDefault(),
                ),
                Proxy.NO_PROXY,
                InetSocketAddress(to, 7539),
            )

            override fun socket(): Socket = Socket()

            override fun handshake(): Handshake? = null

            override fun protocol(): Protocol = Protocol.HTTP_1_1
        }

        override fun request(): Request = request

        override fun proceed(request: Request): Response {
            onSend()
            sent = request
            return Response.Builder().request(request).protocol(Protocol.HTTP_1_1).code(200).message("OK").build()
        }

        override fun connection(): Connection = connection

        override fun call(): Call = throw UnsupportedOperationException()

        override fun connectTimeoutMillis(): Int = 0

        override fun withConnectTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this

        override fun readTimeoutMillis(): Int = 0

        override fun withReadTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this

        override fun writeTimeoutMillis(): Int = 0

        override fun withWriteTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this
    }
}

/** A chain whose every member throws: the rest of a fake one. */
private fun unsupported(): Interceptor.Chain = java.lang.reflect.Proxy.newProxyInstance(
    Interceptor.Chain::class.java.classLoader,
    arrayOf(Interceptor.Chain::class.java),
) { _, method, _ -> throw UnsupportedOperationException(method.name) } as Interceptor.Chain
