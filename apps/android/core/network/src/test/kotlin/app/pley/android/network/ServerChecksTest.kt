package app.pley.android.network

import app.pley.android.auth.NOT_AN_ADDRESS
import app.pley.android.common.FoundServer
import app.pley.android.common.Tier
import app.pley.android.model.ServerInfo
import app.pley.android.testing.network.MockPley
import java.io.IOException
import java.net.ConnectException
import java.net.Inet4Address
import java.net.InetAddress
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** "Search this network" and checking a typed address. */
class ServerChecksTest {
    @get:Rule val folder = TemporaryFolder()

    private val servers = mutableListOf<MockWebServer>()
    private val root = HttpRoot.create(TEST_AGENT)

    private fun server(dispatch: (String) -> MockResponse): MockWebServer = MockPley.server().also { s ->
        servers += s
        s.dispatcher = object : mockwebserver3.Dispatcher() {
            override fun dispatch(request: mockwebserver3.RecordedRequest): MockResponse = dispatch(request.target)
        }
    }

    private fun pley(name: String = "Living Room") = server { path ->
        if (path == "/api/server") MockPley.json("""{"name":"$name","version":"0.42.0","setup_required":false}""") else MockResponse.Builder().code(404).build()
    }

    private fun notPley() = server { MockResponse.Builder().setHeader("Content-Type", "text/html").body("<html>router</html>").build() }

    private fun closedPort(): String {
        val s = MockPley.server()
        val origin = MockPley.issuerOf(s)
        s.close()
        return origin
    }

    @Before
    fun setUp() {
        servers.clear()
    }

    @After
    fun tearDown() {
        servers.forEach { it.close() }
    }

    // ---------------------------------------------------------------- the sweep

    @Test
    fun theSweepFindsPleyAmongTheAddresses() = runBlocking {
        val home = pley()
        val router = notPley()
        val targets = listOf(closedPort(), MockPley.issuerOf(router), MockPley.issuerOf(home)).map { SweepTarget(it, root) }
        val started = System.nanoTime()

        val found = HttpNetworkSweep({ targets }).sweep().toList()

        assertEquals(listOf(FoundServer(MockPley.issuerOf(home), "Living Room", id = null, version = "0.42.0")), found)
        assertTrue("took ${(System.nanoTime() - started) / 1_000_000} ms", System.nanoTime() - started < 5_000_000_000)
        assertEquals(TEST_AGENT, home.takeRequest().headers["User-Agent"])
    }

    @Test
    fun theSweepEndsWithinItsBudget() = runBlocking {
        val slow = server { MockPley.delayed(MockPley.json("""{"name":"Slow","version":"0.42.0"}"""), 10_000) }
        val started = System.nanoTime()

        val found = HttpNetworkSweep({ listOf(SweepTarget(MockPley.issuerOf(slow), root)) }, budgetMs = 300).sweep().toList()

        assertEquals(emptyList<FoundServer>(), found)
        assertTrue(System.nanoTime() - started < 3_000_000_000)
    }

    @Test
    fun theSweepAsksAtMost32AtOnce() = runBlocking {
        var running = 0
        var most = 0
        val counting = root.newBuilder().addInterceptor { chain ->
            synchronized(this@ServerChecksTest) { most = maxOf(most, ++running) }
            Thread.sleep(20)
            synchronized(this@ServerChecksTest) { running-- }
            throw ConnectException("connection refused")
        }.build()
        val targets = (1..100).map { SweepTarget("http://192.168.1.$it:7539", counting) }

        HttpNetworkSweep({ targets }).sweep().toList()

        assertTrue("$most at once", most in 2..32)
    }

    @Test
    fun aSweepTheSystemRefusesSaysBlocked() = runBlocking {
        val refused = root.newBuilder().addInterceptor { throw IOException("connect failed: EPERM (Operation not permitted)") }.build()
        try {
            HttpNetworkSweep({ listOf(SweepTarget("http://192.168.1.5:7539", refused)) }).sweep().toList()
            fail("the sweep went on")
        } catch (e: OfflineException) {
            assertTrue(e.blocked)
        }
    }

    @Test
    fun theSweepCoversTheDevicesSlash24() {
        val address = LanAddress(InetAddress.getByName("192.168.1.20") as Inet4Address, 24)
        val hosts = address.neighbours().map { it.hostAddress }
        assertEquals(253, hosts.size)
        assertEquals("192.168.1.1", hosts.first())
        assertEquals("192.168.1.254", hosts.last())
        assertFalse("192.168.1.20" in hosts)

        // A bigger network: its /24 only. A smaller one: itself.
        val wide = LanAddress(InetAddress.getByName("10.0.7.9") as Inet4Address, 16).neighbours().map { it.hostAddress }
        assertEquals(listOf("10.0.7.1", "10.0.7.254"), listOf(wide.first(), wide.last()))
        val small = LanAddress(InetAddress.getByName("192.168.1.9") as Inet4Address, 29).neighbours().map { it.hostAddress }
        assertEquals((10..14).map { "192.168.1.$it" }, small)

        val lan = Lan(listOf(address, address))
        val targets = HttpNetworkSweep.targetsOf(listOf(lan), root)
        assertEquals(253, targets.size)
        assertEquals("http://192.168.1.1:7539", targets.first().origin)
        assertEquals(400, targets.first().client.connectTimeoutMillis)
    }

    // ---------------------------------------------------------------- the probe

    /** A client that answers from [answers] by address, recording the addresses asked. */
    private class Scripted(private val answers: (String) -> Any) {
        val asked = mutableListOf<String>()

        val client: OkHttpClient = OkHttpClient.Builder().addInterceptor(
            Interceptor { chain ->
                val url = chain.request().url.toString()
                asked += url
                when (val answer = answers(url)) {
                    is IOException -> throw answer
                    is Response.Builder -> answer.request(chain.request()).build()
                    else -> error("no answer for $url")
                }
            },
        ).build()
    }

    private fun ok(body: String, code: Int = 200, location: String? = null) = Response.Builder()
        .protocol(Protocol.HTTP_1_1)
        .code(code)
        .message("OK")
        .apply { if (location != null) header("Location", location) }
        .body(body.toResponseBody("application/json".toMediaType()))

    private val pleyAnswer = """{"name":"Living Room","version":"0.42.0"}"""

    @Test
    fun aNameOnTheInternetIsTriedWithHttpsFirst() = runBlocking {
        val internet = Scripted { url -> if (url.startsWith("https://")) ConnectException("connection refused") else ok(pleyAnswer) }
        val home = Scripted { error("not a home address") }

        val result = HttpServerProbe(internet.client, home.client).probe("pley.example.com")

        assertEquals(ProbeResult.Pley("http://pley.example.com:7539", ServerInfo("Living Room", "0.42.0")), result)
        assertEquals(listOf("https://pley.example.com/api/server", "http://pley.example.com:7539/api/server"), internet.asked)
    }

    @Test
    fun httpsAnsweringStopsThere() = runBlocking {
        val internet = Scripted { ok(pleyAnswer) }
        val result = HttpServerProbe(internet.client, internet.client).probe("pley.example.com")
        assertEquals(ProbeResult.Pley("https://pley.example.com", ServerInfo("Living Room", "0.42.0")), result)
        assertEquals(1, internet.asked.size)
    }

    @Test
    fun homeAddressesUseTheQuickClient() = runBlocking {
        val internet = Scripted { error("not on the internet") }
        val home = Scripted { ok(pleyAnswer) }
        assertTrue(HttpServerProbe(internet.client, home.client).probe("nas") is ProbeResult.Pley)
        assertEquals(listOf("http://nas:7539/api/server"), home.asked)
    }

    @Test
    fun theProbeSaysWhetherTheServerRequiresTwoStepSignIn() = runBlocking {
        suspend fun probe(answer: String): ProbeResult {
            val home = Scripted { ok(answer) }
            return HttpServerProbe(home.client, home.client).probe("nas")
        }
        assertEquals(
            ProbeResult.Pley("http://nas:7539", ServerInfo("Living Room", "0.42.0", twoStepRequired = true)),
            probe("""{"name":"Living Room","version":"0.42.0","two_step_required":true}"""),
        )
        assertEquals(ProbeResult.Pley("http://nas:7539", ServerInfo("Living Room", "0.42.0")), probe(pleyAnswer))
        // An odd value doesn't make it something other than Pley.
        assertEquals(
            ProbeResult.Pley("http://nas:7539", ServerInfo("Living Room", "0.42.0")),
            probe("""{"name":"Living Room","version":"0.42.0","two_step_required":"yes"}"""),
        )
    }

    @Test
    fun aRedirectToHttpsOnTheSameHostIsFollowedByHand() = runBlocking {
        val internet = Scripted { url ->
            if (url.startsWith("http://")) ok("", code = 301, location = "https://pley.example.com/api/server") else ok(pleyAnswer)
        }
        val result = HttpServerProbe(internet.client, internet.client).probe("http://pley.example.com")
        assertEquals(ProbeResult.Pley("https://pley.example.com", ServerInfo("Living Room", "0.42.0")), result)
    }

    @Test
    fun whatAnsweredAndIsntPleyIsNotPley() = runBlocking {
        val internet = Scripted { url ->
            if (url.startsWith("https://")) ok("<html>shop</html>") else ConnectException("connection refused")
        }
        assertEquals(ProbeResult.NotPley("https://shop.example.com"), HttpServerProbe(internet.client, internet.client).probe("shop.example.com"))
    }

    @Test
    fun aRefusalByTheSystemIsBlocked() = runBlocking {
        val home = Scripted { IOException("connect failed: EPERM (Operation not permitted)") }
        assertEquals(ProbeResult.Blocked, HttpServerProbe(home.client, home.client).probe("192.168.1.5"))
    }

    @Test
    fun overTheNetwork() = runBlocking {
        val graph = testGraph(folder.root, { error("no tokens for a probe") }, profile = testProfile(Tier.T0))
        val home = pley()
        val typed = "127.0.0.1:${home.port}"
        assertEquals(ProbeResult.Pley(MockPley.issuerOf(home), ServerInfo("Living Room", "0.42.0")), graph.probe.probe(typed))
        val asked = home.takeRequest()
        assertEquals(null, asked.headers["Authorization"])
        assertEquals(TEST_AGENT, asked.headers["User-Agent"])

        val router = notPley()
        assertEquals(ProbeResult.NotPley(MockPley.issuerOf(router)), graph.probe.probe("127.0.0.1:${router.port}"))

        // Refused, or (Windows retries refused loopback connections) no answer in time.
        assertTrue(graph.probe.probe(closedPort()) is ProbeResult.Unreachable)
        assertEquals(ProbeResult.Unreachable(NOT_AN_ADDRESS), graph.probe.probe("not an address"))
    }

    @Test
    fun theProbesClientsGiveUpSoonerAtHome() {
        val graph = testGraph(folder.root, { error("no tokens") })
        assertEquals(3_000, graph.localProbeClient.connectTimeoutMillis)
        assertEquals(10_000, graph.probeClient.connectTimeoutMillis)
        assertFalse(graph.probeClient.followRedirects)
    }
}
