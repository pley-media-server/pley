package app.pley.android.network

import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
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

    private fun api(name: String?, kind: DeviceKind? = null): PleyApi {
        val tokens = testTokens(issuer)
        return testGraph(folder.newFolder(), { tokens }, deviceName = { name }, deviceKind = { kind }).api(issuer, userId = 2)
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
    fun playbacksSayWhatKindOfDeviceThisIs() = runTest {
        for ((kind, wire) in listOf(DeviceKind.Tv to "tv", DeviceKind.Phone to "phone", DeviceKind.Tablet to "tablet")) {
            val api = api("Living room TV", kind)
            server.enqueue(MockPley.fixture("playback/info_direct.json"))
            api.startPlayback(PlaybackRequest(itemId = 101))
            assertEquals(JsonPrimitive(wire), sent()["device_kind"])

            server.enqueue(MockPley.fixture("live/watching.json"))
            api.watchChannel(1, WatchRequest("android-1"))
            assertEquals(JsonPrimitive(wire), sent()["device_kind"])

            assertEquals("$issuer/api/live/channels/1/stream.ts?client_id=android-1&device_kind=$wire", api.liveStreamUrl(1, "android-1"))
        }
    }

    @Test
    fun withoutANameOrControlsNeitherIsSent() = runTest {
        val api = api(null)
        server.enqueue(MockPley.fixture("playback/info_direct.json"))
        api.startPlayback(PlaybackRequest(itemId = 101))
        val play = sent()
        assertFalse("controls" in play)
        assertFalse("device_name" in play)
        assertFalse("device_kind" in play)
        assertEquals("$issuer/api/live/channels/1/stream.ts?client_id=android-1", api.liveStreamUrl(1, "android-1"))
    }

    @Test
    fun aTvInterfaceIsATvAndAPhoneInterfaceATabletFrom600dp() {
        assertEquals(DeviceKind.Tv, DeviceKinds.of(tv = true, smallestWidthDp = 960))
        assertEquals(DeviceKind.Tv, DeviceKinds.of(tv = true, smallestWidthDp = 360))
        assertEquals(DeviceKind.Phone, DeviceKinds.of(tv = false, smallestWidthDp = 411))
        assertEquals(DeviceKind.Phone, DeviceKinds.of(tv = false, smallestWidthDp = 599))
        assertEquals(DeviceKind.Tablet, DeviceKinds.of(tv = false, smallestWidthDp = 600))
        assertEquals(DeviceKind.Tablet, DeviceKinds.of(tv = false, smallestWidthDp = 800))
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
        assertEquals("app-12", tv.deviceId)
        assertEquals(DeviceKind.Tv, tv.deviceKind)
        assertEquals(PlayingKind.Item, tv.kind)
        assertEquals(204L, tv.item?.id)
        assertEquals(ProgressState.Playing, tv.state)
        assertEquals(4_200L, tv.ageMs)
        assertEquals(Control.Quality, tv.controls.last())
        // Its Quality menu's pick, among what the menu offers for the file.
        assertEquals("auto", tv.quality)
        assertEquals(listOf("original", "auto", "8m", "4m", "2m"), tv.qualities.map { it.id })
        assertEquals("8 Mbps 1080p", tv.qualities[2].label)
        assertEquals(PlayingKind.Live, phone.kind)
        assertEquals("app-15", phone.deviceId)
        assertEquals(DeviceKind.Phone, phone.deviceKind)
        assertEquals("The Evening News", phone.channel?.now)
        // When the programme on started and ends, and the next.
        assertEquals(1_761_388_200L, phone.channel?.nowStart)
        assertEquals(1_761_391_800L, phone.channel?.nowStop)
        assertEquals("Weather", phone.channel?.next)
        assertNull(phone.durationMs)
        assertEquals(listOf(Control.Next, Control.Previous), phone.controls)
        assertNull(phone.quality)
        assertEquals(emptyList<Any>(), phone.qualities)
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

    @Test
    fun aRefusedCastSaysWhy() = runTest {
        val api = api(null)
        suspend fun refusal(): CastRefusal = try {
            api.cast("android-den", app.pley.android.model.CastRequest(itemId = 204))
            error("expected a refusal")
        } catch (e: IOException) {
            CastRefusal.of(e)
        }
        server.enqueue(MockPley.fixture("errors/not_found.json", code = 404))
        assertEquals(CastRefusal.Gone, refusal())
        server.enqueue(MockPley.error(409, "not_castable", "this device doesn't play titles from a remote"))
        assertEquals(CastRefusal.NotCastable, refusal())
        server.enqueue(MockPley.error(429, "too_many_requests", "10 casts a second at most: wait a moment"))
        assertEquals(CastRefusal.TooMany, refusal())
        // Another conflict, and everything else, says its own sentence.
        server.enqueue(MockPley.fixture("errors/not_controllable.json", code = 409))
        assertEquals(CastRefusal.Other, refusal())
        server.enqueue(MockPley.error(403, "forbidden", "Live TV isn't open to you"))
        assertEquals(CastRefusal.Other, refusal())
        assertEquals(CastRefusal.Offline, CastRefusal.of(OfflineException(java.net.ConnectException("away"))))
    }

    @Test
    fun aCastToAnotherProfilesDeviceSendsItsProfileAndThePinAndSaysWhyItWasRefused() = runTest {
        val api = api(null)
        server.enqueue(MockPley.noContent())
        api.cast("android-den", app.pley.android.model.CastRequest(itemId = 204, profileId = 3, pin = "1234"))
        val body = sent()
        assertEquals(JsonPrimitive(3), body["profile_id"])
        assertEquals(JsonPrimitive("1234"), body["pin"])
        // To the profile's own devices, neither goes.
        server.enqueue(MockPley.noContent())
        api.cast("android-den", app.pley.android.model.CastRequest(itemId = 204))
        val own = sent()
        assertFalse("profile_id" in own || "pin" in own)

        suspend fun refusal(pin: String? = null): CastRefusal = try {
            api.cast("android-den", app.pley.android.model.CastRequest(itemId = 204, profileId = 3, pin = pin))
            error("expected a refusal")
        } catch (e: IOException) {
            CastRefusal.of(e, pinSent = pin != null)
        }
        server.enqueue(MockPley.error(403, "pin_needed", "this profile is protected by a PIN"))
        assertEquals(CastRefusal.PinNeeded, refusal())
        server.enqueue(MockPley.error(403, "wrong_pin", "wrong PIN"))
        assertEquals(CastRefusal.WrongPin, refusal("0000"))
        server.enqueue(MockPley.error(403, "pin_locked", "this profile's PIN is locked"))
        assertEquals(CastRefusal.PinLocked, refusal("0000"))
        // With a PIN, a 429 is the PIN's: checked before the casts are counted.
        server.enqueue(MockPley.error(429, "too_many_requests", "too many attempts, try again in 14 min"))
        assertEquals(CastRefusal.Throttled, refusal("0000"))
        server.enqueue(MockPley.error(429, "too_many_requests", "10 casts a second at most: wait a moment"))
        assertEquals(CastRefusal.TooMany, refusal())
        // Its PIN is never written out where the request is.
        assertFalse("1234" in app.pley.android.model.CastRequest(itemId = 204, pin = "1234").toString())
    }
}
