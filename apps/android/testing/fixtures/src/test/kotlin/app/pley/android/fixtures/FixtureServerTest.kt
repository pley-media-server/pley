package app.pley.android.fixtures

import app.pley.android.model.HistoryPage
import app.pley.android.model.ItemDetail
import app.pley.android.model.Me
import app.pley.android.model.ServerEvent
import app.pley.android.model.SubtitleMode
import java.io.IOException
import java.util.ServiceLoader
import java.util.concurrent.Callable
import kotlin.concurrent.thread
import mockwebserver3.RecordedRequest
import okhttp3.Headers.Companion.headersOf
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.Request
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class FixtureServerTest {
    @Test
    fun itRunsOnThisMachineOnly() = withServer { server ->
        assertTrue(server.url, server.url.startsWith("http://127.0.0.1:"))
        assertEquals(200, Client(server).get("/api/health").code)
    }

    @Test
    fun writesGoToTheJournalAndChangeWhatIsRead() = withServer { server ->
        val app = Client(server, server.accessToken())
        val movie = FixtureIds.SAMPLE_1080P
        assertEquals(204, app.send("POST", "/api/items/$movie/progress", """{"position_ms":5000,"state":"playing","session_id":"s"}""").code)
        assertEquals(5_000L, app.read("/api/items/$movie", ItemDetail.serializer()).userState.positionMs)
        // At 90 % it is watched, its position cleared, and the play recorded.
        val before = app.read("/api/history", HistoryPage.serializer()).total
        app.send("POST", "/api/items/$movie/progress", """{"position_ms":19000,"state":"playing"}""")
        app.send("POST", "/api/items/$movie/progress", """{"position_ms":20000,"state":"stopped"}""")
        val watched = app.read("/api/items/$movie", ItemDetail.serializer()).userState
        assertTrue(watched.watched)
        assertEquals(0L, watched.positionMs)
        assertEquals(1, watched.playCount)
        assertEquals(before + 1, app.read("/api/history", HistoryPage.serializer()).total)

        app.send("PUT", "/api/items/$movie/rating", """{"rating":7.3}""")
        assertEquals(7.5, app.read("/api/items/$movie", ItemDetail.serializer()).userState.rating!!, 0.0)
        assertEquals(400, app.send("PUT", "/api/items/$movie/rating", """{"rating":11}""").code)

        app.send("POST", "/api/items/${FixtureIds.SHOW}/watched")
        assertEquals(6, app.read("/api/items/${FixtureIds.SHOW}", ItemDetail.serializer()).viewedLeafCount)
        app.send("POST", "/api/items/${FixtureIds.SHOW_SEASON_2}/unwatched")
        assertEquals(3, app.read("/api/items/${FixtureIds.SHOW}", ItemDetail.serializer()).viewedLeafCount)

        // A track choice holds for the whole show.
        app.send("PUT", "/api/items/${FixtureIds.SHOW_S1E1}/streams", """{"audio_index":1,"subtitle_index":2}""")
        val episode = app.read("/api/items/${FixtureIds.SHOW_S1E2}", ItemDetail.serializer()).files.single()
        assertEquals(1, episode.selectedAudio)
        assertTrue(episode.audioPicked)
        assertEquals(2, episode.selectedSubtitle)
        // Automatic: the pick goes, and the episode's audio is the default's again, not a pick.
        app.send("PUT", "/api/items/${FixtureIds.SHOW_S1E1}/streams", """{"audio_index":null,"subtitle_index":2}""")
        val automatic = app.read("/api/items/${FixtureIds.SHOW_S1E2}", ItemDetail.serializer()).files.single()
        assertFalse(automatic.audioPicked)
        assertEquals(2, automatic.selectedSubtitle)

        val defaults = app.send("PUT", "/api/me/track-defaults", """{"audio_language":"fre","subtitle_language":"eng","subtitle_mode":"always"}""")
        assertEquals(200, defaults.code)
        assertEquals(SubtitleMode.Always, defaults.decode(Me.serializer()).subtitleMode)
        assertEquals("fre", app.read("/api/auth/me", Me.serializer()).audioLanguage)
        assertEquals(400, app.send("PUT", "/api/me/track-defaults", """{"audio_language":"French","subtitle_mode":"off"}""").code)

        val writes = server.journal.entries().map { "${it.method} ${it.path}" }
        assertEquals(
            listOf(
                "POST /api/items/$movie/progress",
                "POST /api/items/$movie/progress",
                "POST /api/items/$movie/progress",
                "PUT /api/items/$movie/rating",
                "PUT /api/items/$movie/rating",
                "POST /api/items/${FixtureIds.SHOW}/watched",
                "POST /api/items/${FixtureIds.SHOW_SEASON_2}/unwatched",
                "PUT /api/items/${FixtureIds.SHOW_S1E1}/streams",
                "PUT /api/items/${FixtureIds.SHOW_S1E1}/streams",
                "PUT /api/me/track-defaults",
                "PUT /api/me/track-defaults",
            ),
            writes,
        )
        val first = server.journal.writes("POST", "/api/items/$movie/progress").first()
        assertEquals(FixtureIds.ALEX, first.profileId)
        assertTrue("\"session_id\":\"s\"" in first.body)
    }

    @Test
    fun slowAnswersLate() = withServer(FixtureServer.Options(scanEveryMs = 0, slowLatencyMs = 300)) { server ->
        val app = Client(server)
        server.mode = FixtureServer.Mode.Slow
        val start = System.nanoTime()
        assertEquals(200, app.get("/api/health").code)
        assertTrue((System.nanoTime() - start) / 1_000_000 >= 300)
        server.mode = FixtureServer.Mode.Normal
        assertEquals(200, app.get("/api/health").code)
    }

    @Test
    fun downRefusesConnectionsOnTheSamePort() = withServer { server ->
        val app = Client(server, server.accessToken())
        val url = server.url
        assertEquals(200, app.get("/api/libraries").code)
        server.mode = FixtureServer.Mode.Down
        try {
            app.get("/api/libraries")
            fail("the server should refuse connections")
        } catch (_: IOException) {
            // Refused.
        }
        server.mode = FixtureServer.Mode.Normal
        assertEquals(url, server.url)
        // Same data, same tokens.
        assertEquals(200, app.get("/api/libraries").code)
    }

    @Test
    fun eventsWithKeepAlivesUntilTheTokenEnds() {
        val clock = FakeClock()
        withServer(FixtureServer.Options(scanEveryMs = 0, keepAliveMs = 100, clock = clock)) { server ->
            val app = Client(server)
            val request = Request.Builder().url("${server.url}/api/events").header("Authorization", "Bearer ${server.accessToken()}").build()
            app.http.newCall(request).execute().use { response ->
                assertEquals("text/event-stream", response.header("Content-Type"))
                val source = response.body.source()
                assertEquals(": connected", source.readUtf8Line())
                assertEquals("", source.readUtf8Line())
                assertEquals(": keep-alive", source.readUtf8Line())
                assertEquals("", source.readUtf8Line())
                server.emit(ServerEvent.LibraryChanged(FixtureIds.MOVIES))
                var line = source.readUtf8Line()
                while (line != null && !line.startsWith("data:")) line = source.readUtf8Line()
                assertEquals(ServerEvent.LibraryChanged(FixtureIds.MOVIES), ServerEvent.parse(line!!.removePrefix("data: ")))
                // The stream ends with its access token.
                clock.now += 3_600_000
                while (line != null) line = source.readUtf8Line()
                assertTrue(source.exhausted())
            }
        }
    }

    @Test
    fun aScanEveryFewSeconds() = withServer(FixtureServer.Options(scanEveryMs = 200, keepAliveMs = 5_000)) { server ->
        val app = Client(server)
        val request = Request.Builder().url("${server.url}/api/events").header("Authorization", "Bearer ${server.accessToken()}").build()
        app.http.newCall(request).execute().use { response ->
            val source = response.body.source()
            val events = mutableListOf<ServerEvent>()
            while (events.none { it is ServerEvent.LibraryChanged }) {
                val line = source.readUtf8Line() ?: break
                if (line.startsWith("data: ")) events += ServerEvent.parse(line.removePrefix("data: "))
            }
            assertEquals(
                listOf("scan_started", "scan_progress", "scan_progress", "scan_progress", "scan_finished", "library_changed"),
                events.map { it.type },
            )
        }
    }

    @Test
    fun aStreamAskedForAsTheServerStopsListeningEndsAtOnce() {
        val state = State(FixtureServer.Options(scanEveryMs = 0))
        val alex = Caller(state.profile(FixtureIds.ALEX)!!, OAuth.SCOPES.toSet(), grant = null, expiresAtMs = null)
        val request = RecordedRequest(
            connectionIndex = 0,
            exchangeIndex = 0,
            handshake = null,
            handshakeServerNames = emptyList(),
            method = "GET",
            target = "/api/events",
            version = "HTTP/1.1",
            url = "http://127.0.0.1/api/events".toHttpUrl(),
            headers = headersOf(),
            body = null,
            bodySize = 0,
            chunkSizes = null,
            failure = null,
        )
        // Answered just before the server ends its streams, written just after: none of them yet when they ended.
        val stream = state.events.response(Call(request), alex).body!!
        state.events.drop()
        val sent = Buffer()
        val writing = thread(isDaemon = true) { stream.writeTo(sent) }
        writing.join(5_000)
        // Still open, it would keep MockWebServer from closing: it waits 5 s for what it is writing, then gives up.
        assertFalse("the stream still waits for events", writing.isAlive)
        assertTrue("ended as the server ends one", sent.readUtf8().endsWith("0\r\n\r\n"))
    }

    @Test
    fun streamsStayOpenOnceTheServerListensAgain() = withServer(FixtureServer.Options(scanEveryMs = 0)) { server ->
        server.mode = FixtureServer.Mode.Down
        server.mode = FixtureServer.Mode.Normal
        val request = Request.Builder().url("${server.url}/api/events").header("Authorization", "Bearer ${server.accessToken()}").build()
        Client(server).http.newCall(request).execute().use { response ->
            val source = response.body.source()
            assertEquals(": connected", source.readUtf8Line())
            server.emit(ServerEvent.LibraryChanged(FixtureIds.MOVIES))
            var line = source.readUtf8Line()
            while (line != null && !line.startsWith("data:")) line = source.readUtf8Line()
            assertNotNull("the stream ended before its event", line)
            assertEquals(ServerEvent.LibraryChanged(FixtureIds.MOVIES), ServerEvent.parse(line!!.removePrefix("data: ")))
        }
    }

    @Test
    fun debugBuildsFindTheDemoServer() {
        val factory = ServiceLoader.load(DemoServerFactory::class.java).single()
        val url = factory.start()
        assertTrue(url.startsWith("http://127.0.0.1:"))
        // Once per process; and callable by those who can't see this module.
        assertEquals(url, (factory as Callable<*>).call())
        val type = Class.forName("app.pley.android.fixtures.DemoServerFactory")
        assertFalse(ServiceLoader.load(type).none())
    }
}
