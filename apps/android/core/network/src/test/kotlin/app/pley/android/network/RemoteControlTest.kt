package app.pley.android.network

import app.pley.android.model.Control
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingKind
import app.pley.android.model.PleyJson
import app.pley.android.model.ProgressState
import app.pley.android.model.WatchRequest
import app.pley.android.testing.network.MockPley
import app.pley.android.testing.network.bodyText
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import mockwebserver3.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** What players tell the server for remotes, and the remote's own calls (docs/API.md, "Remote control"). */
class RemoteControlTest {
    @get:Rule val folder = TemporaryFolder()

    private lateinit var server: MockWebServer
    private lateinit var issuer: String

    @Before
    fun setUp() {
        server = MockPley.server()
        issuer = MockPley.issuerOf(server)
    }

    @After
    fun tearDown() {
        server.close()
    }

    private fun api(name: String?): PleyApi {
        val tokens = testTokens(issuer)
        return testGraph(folder.newFolder(), { tokens }, deviceName = { name }).api(issuer, userId = 2)
    }

    private fun sent(): JsonObject {
        val request = server.takeRequest(5, TimeUnit.SECONDS) ?: error("no request")
        return PleyJson.parseToJsonElement(request.bodyText).jsonObject
    }

    @Test
    fun playbacksSayWhatTheyObeyAndWhatTheDeviceIsCalled() = runTest {
        val api = api("Living room TV")
        server.enqueue(MockPley.fixture("playback/info_direct.json"))
        api.startPlayback(PlaybackRequest(itemId = 101, controls = listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip)))
        val play = sent()
        assertEquals("""["play","pause","seek","skip"]""", play["controls"].toString())
        assertEquals(JsonPrimitive("Living room TV"), play["device_name"])

        server.enqueue(MockPley.fixture("live/watching.json"))
        api.watchChannel(1, WatchRequest("android-1", controls = listOf(Control.Next, Control.Previous)))
        val watch = sent()
        assertEquals("""["next","previous"]""", watch["controls"].toString())
        assertEquals(JsonPrimitive("Living room TV"), watch["device_name"])
    }

    @Test
    fun withoutANameOrControlsNeitherIsSent() = runTest {
        val api = api(null)
        server.enqueue(MockPley.fixture("playback/info_direct.json"))
        api.startPlayback(PlaybackRequest(itemId = 101))
        val play = sent()
        assertFalse("controls" in play)
        assertFalse("device_name" in play)
    }

    @Test
    fun namesAreCutToWhatTheServerKeeps() {
        assertEquals("Living room TV", DeviceNames.of("  Living room TV "))
        assertEquals(64, DeviceNames.of("x".repeat(100))!!.length)
        assertNull(DeviceNames.of("   "))
        assertNull(DeviceNames.of(null))
    }

    @Test
    fun sessionsAreRead() = runTest {
        server.enqueue(MockPley.fixture("sessions/list.json"))
        val (tv, phone) = api(null).playingSessions()
        assertEquals("Living room TV", tv.device)
        assertEquals(PlayingKind.Item, tv.kind)
        assertEquals(204L, tv.item?.id)
        assertEquals(ProgressState.Playing, tv.state)
        assertEquals(4_200L, tv.ageMs)
        assertEquals(Control.Next, tv.controls.last())
        assertEquals(PlayingKind.Live, phone.kind)
        assertEquals("The Evening News", phone.channel?.now)
        assertNull(phone.durationMs)
        assertEquals(listOf(Control.Next, Control.Previous), phone.controls)
    }

    @Test
    fun aRefusedCommandSaysWhy() = runTest {
        val api = api(null)
        suspend fun refusal(): CommandRefusal = try {
            api.sendCommand("6f1c2a9e", PlayerCommandRequest(Control.Next))
            error("expected a refusal")
        } catch (e: IOException) {
            CommandRefusal.of(e)
        }
        server.enqueue(MockPley.fixture("errors/not_found.json", code = 404))
        assertEquals(CommandRefusal.Gone, refusal())
        server.enqueue(MockPley.fixture("errors/not_controllable.json", code = 409))
        assertEquals(CommandRefusal.NotControllable, refusal())
        server.enqueue(MockPley.error(429, "too_many_requests", "too many commands"))
        assertEquals(CommandRefusal.TooMany, refusal())
        server.enqueue(MockPley.error(403, "forbidden", "not your session"))
        assertEquals(CommandRefusal.Other, refusal())
        assertEquals(CommandRefusal.Offline, CommandRefusal.of(OfflineException(java.net.ConnectException("away"))))
    }
}
