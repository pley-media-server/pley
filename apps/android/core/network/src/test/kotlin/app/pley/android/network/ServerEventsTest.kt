package app.pley.android.network

import app.cash.turbine.test
import app.pley.android.auth.SignedOutException
import app.pley.android.model.ScanPhase
import app.pley.android.model.ServerEvent
import app.pley.android.testing.Fixtures
import app.pley.android.testing.network.MockPley
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ServerEventsTest {
    @get:Rule val folder = TemporaryFolder()

    private lateinit var server: MockWebServer
    private lateinit var issuer: String
    private lateinit var graph: HttpNetworkGraph

    @Before
    fun setUp() {
        server = MockPley.server()
        issuer = MockPley.issuerOf(server)
        val tokens = testTokens(issuer)
        graph = testGraph(folder.root, { tokens })
    }

    @After
    fun tearDown() {
        server.close()
    }

    private fun event(name: String) = Fixtures.text("events/$name.json").replace("\n", " ").trim()

    /** The stream over the network, waiting only a moment between connections. */
    private fun stream() = EventStream(
        OkHttpSseConnection({ graph.eventsClient(issuer) }, "$issuer/api/events"),
        firstRetryMs = 10,
        maxRetryMs = 20,
    )

    @Test
    fun eventsAreReadSkippingKeepAlivesAndUnknownTypes() = runBlocking {
        server.enqueue(MockPley.events(event("scan_started"), event("unknown_type"), event("scan_progress"), "not json", event("library_changed")))
        server.enqueue(MockPley.events(event("libraries_changed")))

        val got = stream().events().take(7).toList()

        assertEquals(
            listOf(
                LiveEvent.Reconnected,
                LiveEvent.Event(ServerEvent.ScanStarted(1)),
                LiveEvent.Event(ServerEvent.ScanProgress(1, ScanPhase.Probing, 118, 412)),
                LiveEvent.Event(ServerEvent.LibraryChanged(2)),
                // The server ended the stream at once: lost, connected again, and said so.
                LiveEvent.Dropped,
                LiveEvent.Reconnected,
                LiveEvent.Event(ServerEvent.LibrariesChanged),
            ),
            got,
        )
        val request = server.takeRequest(1, TimeUnit.SECONDS)!!
        assertEquals("GET /api/events HTTP/1.1", request.requestLine)
        assertEquals("text/event-stream", request.headers["Accept"])
        assertEquals("Bearer pley_at_one", request.headers["Authorization"])
        assertEquals(TEST_AGENT, request.headers["User-Agent"])
    }

    @Test
    fun aRefusalEndsTheStream() = runBlocking {
        server.enqueue(MockPley.insufficientScope("library"))
        try {
            stream().events().toList()
            fail("the stream went on")
        } catch (e: ApiException) {
            assertEquals(403, e.status)
            assertEquals("library", e.scope)
        }
    }

    @Test
    fun theEventsClientWaitsLongerThanTheKeepAlives() {
        assertEquals(45_000, graph.eventsClient(issuer).readTimeoutMillis)
    }

    // ---------------------------------------------------------------- reconnecting, on the test's clock

    /** What a scripted connection does. */
    private sealed interface Attempt {
        /** Fails before the server takes it. */
        data class Fails(val error: Throwable = java.io.IOException("connection refused")) : Attempt

        /** Opens, sends [events], and after [lastsMs] ends (cleanly unless [error]). */
        data class Opens(val events: List<String>, val lastsMs: Long, val error: Throwable? = null) : Attempt
    }

    /** A connection playing [attempts] in turn; [starts] gets the clock at each. */
    private class Scripted(private val attempts: List<Attempt>, private val now: () -> Long) : SseConnection {
        val starts = mutableListOf<Long>()
        private var next = 0

        override fun open(): Flow<SseSignal> = flow {
            starts += now()
            when (val attempt = attempts.getOrElse(next++) { Attempt.Fails(SignedOutException()) }) {
                is Attempt.Fails -> emit(SseSignal.Ended(attempt.error))
                is Attempt.Opens -> {
                    emit(SseSignal.Opened)
                    attempt.events.forEach { emit(SseSignal.Message(it)) }
                    delay(attempt.lastsMs)
                    emit(SseSignal.Ended(attempt.error))
                }
            }
        }
    }

    @Test
    fun reconnectsBackingOffFrom2sTo30s() = runTest {
        val connection = Scripted(List(7) { Attempt.Fails() }, now = { testScheduler.currentTime })
        try {
            EventStream(connection, now = { testScheduler.currentTime }).events().toList()
            fail("the stream went on")
        } catch (_: SignedOutException) {
            // The script's end: signed out.
        }
        // Waits of 2, 4, 8, 16, 30 and 30 s.
        assertEquals(listOf(0L, 2_000, 6_000, 14_000, 30_000, 60_000, 90_000, 120_000), connection.starts)
    }

    @Test
    fun aConnectionThatOpenedStartsTheCountAgain() = runTest {
        val connection = Scripted(
            listOf(
                Attempt.Fails(),
                Attempt.Fails(),
                // Opens, then drops after a second: the next wait is the first one again.
                Attempt.Opens(listOf(event("scan_started")), lastsMs = 1_000, error = java.io.IOException("reset")),
                // Opens, and the server ends it after an hour (its token expired): at once again.
                Attempt.Opens(listOf(event("library_changed")), lastsMs = 3_600_000),
                Attempt.Opens(emptyList(), lastsMs = 5),
            ),
            now = { testScheduler.currentTime },
        )
        EventStream(connection, now = { testScheduler.currentTime }).events().test {
            assertEquals(LiveEvent.Reconnected, awaitItem())
            assertEquals(LiveEvent.Event(ServerEvent.ScanStarted(1)), awaitItem())
            // Dropped after a second: lost until it opens again.
            assertEquals(LiveEvent.Dropped, awaitItem())
            assertEquals(LiveEvent.Reconnected, awaitItem())
            assertEquals(LiveEvent.Event(ServerEvent.LibraryChanged(2)), awaitItem())
            // Ended as planned (an expired token): lost, should opening again fail, and opened again at once.
            assertEquals(LiveEvent.Dropped, awaitItem())
            assertEquals(LiveEvent.Reconnected, awaitItem())
            assertEquals(LiveEvent.Dropped, awaitItem())
            assertTrue(awaitError() is SignedOutException)
        }
        val opened = 2_000L + 4_000
        val dropped = opened + 1_000
        val reopened = dropped + 2_000
        val expired = reopened + 3_600_000
        // The 5 ms stream ended too soon to be a planned end: 2 s, then the script ends.
        assertEquals(listOf(0L, 2_000, opened, reopened, expired, expired + 5 + 2_000), connection.starts)
    }

    @Test
    fun retryDelaysDoubleUpTo30s() {
        val stream = EventStream({ error("unused") })
        assertEquals(listOf(2_000L, 4_000, 8_000, 16_000, 30_000, 30_000), (1..6).map(stream::retryDelay))
    }
}
