package app.pley.android.fixtures

import app.pley.android.model.CastKind
import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
import app.pley.android.model.DeviceProfileName
import app.pley.android.model.PlayingSession
import app.pley.android.model.PleyJson
import app.pley.android.model.ProgressState
import app.pley.android.model.RemoteDevice
import app.pley.android.model.ServerEvent
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.Request
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The remote's routes (docs/API.md, "Remote control", "Casting"): the
 * profile's sessions, commands to their players, and the devices that
 * take casts.
 */
class RemoteRoutesTest {
    @Test
    fun sessionsListWhatTheirPlayersSaid() = withServer { server ->
        val robin = Client(server, server.accessToken(FixtureIds.ROBIN))
        val alex = Client(server, server.accessToken())
        val session = robin.send(
            "POST",
            "/api/playback",
            """{"item_id":${FixtureIds.SAMPLE_1080P},"start_ms":2000,"caps":{},"controls":["play","pause","seek"],"device_name":"Kitchen phone","device_kind":"phone"}""",
        ).field("session_id")!!
        val listed = PleyJson.decodeFromString(ListSerializer(PlayingSession.serializer()), String(robin.get("/api/sessions").bytes)).single()
        assertEquals(session, listed.sessionId)
        assertEquals("Kitchen phone", listed.device)
        assertEquals(DeviceKind.Phone, listed.deviceKind)
        assertTrue(listed.deviceId.startsWith("app-"))
        assertEquals(FixtureIds.SAMPLE_1080P, listed.item?.id)
        assertEquals(listOf(Control.Play, Control.Pause, Control.Seek), listed.controls)
        assertEquals(2_000L, listed.positionMs)

        assertEquals(204, robin.send("POST", "/api/items/${FixtureIds.SAMPLE_1080P}/progress", """{"position_ms":5000,"state":"paused","session_id":"$session"}""").code)
        val reported = PleyJson.decodeFromString(ListSerializer(PlayingSession.serializer()), String(robin.get("/api/sessions").bytes)).single()
        assertEquals(5_000L, reported.positionMs)
        assertEquals(ProgressState.Paused, reported.state)
        // Only the caller's own, an administrator's too.
        assertEquals("[]", String(alex.get("/api/sessions").bytes))
    }

    @Test
    fun aCommandReachesItsPlayerAndIsRefusedAsTheServerRefuses() = withServer { server ->
        val robin = Client(server, server.accessToken(FixtureIds.ROBIN))
        val alex = Client(server, server.accessToken())
        val session = robin.send("POST", "/api/playback", """{"item_id":${FixtureIds.SAMPLE_1080P},"caps":{},"controls":["pause","seek"]}""")
            .field("session_id")!!
        val path = "/api/sessions/$session/command"
        assertEquals(409, robin.send("POST", path, """{"command":"next"}""").code)
        assertEquals("not_controllable", robin.send("POST", path, """{"command":"next"}""").field("code"))
        assertEquals(400, robin.send("POST", path, """{"command":"seek"}""").code)
        assertEquals(400, robin.send("POST", path, """{"command":"seek","position_ms":-1}""").code)
        assertEquals(403, alex.send("POST", path, """{"command":"pause"}""").code)
        assertEquals(404, robin.send("POST", "/api/sessions/nope/command", """{"command":"pause"}""").code)

        val request = Request.Builder().url("${server.url}/api/events").header("Authorization", "Bearer ${robin.token}").build()
        robin.http.newCall(request).execute().use { response ->
            val source = response.body.source()
            val deadline = System.currentTimeMillis() + 5_000
            while (server.eventListeners == 0 && System.currentTimeMillis() < deadline) Thread.sleep(10)
            assertEquals(202, robin.send("POST", path, """{"command":"seek","position_ms":4000}""").code)
            var heard: ServerEvent.PlayerCommand? = null
            while (heard == null) {
                val line = source.readUtf8Line() ?: break
                if (line.startsWith("data: ")) heard = ServerEvent.parse(line.removePrefix("data: ")) as? ServerEvent.PlayerCommand
            }
            assertEquals(ServerEvent.PlayerCommand(session, Control.Seek, positionMs = 4_000, offsetMs = null), heard)
        }
        assertEquals(202, robin.send("POST", path, """{"command":"pause"}""").code)
    }

    @Test
    fun aStreamThatNamesItsDeviceMakesItAReceiverWhileItIsOpen() = withServer(FixtureServer.Options(scanEveryMs = 0, keepAliveMs = 100)) { server ->
        val robin = Client(server, server.accessToken(FixtureIds.ROBIN))
        fun devices() = robin.read("/api/remote/devices", ListSerializer(RemoteDevice.serializer()))
        assertEquals(emptyList<RemoteDevice>(), devices())
        val url = "${server.url}/api/events?receiver=android-den&device_name=Den%20TV&device_kind=tv&casts=item,live,switch,radio"
        robin.http.newCall(Request.Builder().url(url).header("Authorization", "Bearer ${robin.token}").build()).execute().use { response ->
            val source = response.body.source()
            assertEquals(": connected", source.readUtf8Line())
            val den = waitFor { devices().singleOrNull() }
            assertEquals("android-den", den.receiverId)
            assertEquals("Den TV", den.name)
            assertEquals(DeviceKind.Tv, den.kind)
            // Only the kinds the server knows.
            assertEquals(listOf(CastKind.Item, CastKind.Live, CastKind.Switch), den.casts)
            assertTrue(den.deviceId.startsWith("app-"))
            assertEquals(null, den.profile)
            // The household's other profiles see it as on Robin's when they ask for the household, an administrator too.
            val alex = Client(server, server.accessToken())
            assertEquals(emptyList<RemoteDevice>(), alex.read("/api/remote/devices", ListSerializer(RemoteDevice.serializer())))
            val alexSees = alex.read("/api/remote/devices?household=1", ListSerializer(RemoteDevice.serializer())).single()
            assertEquals(den.copy(profile = DeviceProfileName(FixtureIds.ROBIN, "Robin")), alexSees)
            // It appearing told the profile's streams.
            var heard: ServerEvent? = null
            while (heard != ServerEvent.SessionsChanged) {
                val line = source.readUtf8Line() ?: break
                if (line.startsWith("data: ")) heard = ServerEvent.parse(line.removePrefix("data: "))
            }
            assertEquals(ServerEvent.SessionsChanged, heard)
        }
        // Gone with its stream.
        assertEquals(emptyList<RemoteDevice>(), waitFor { devices().takeIf { it.isEmpty() } })
    }

    @Test
    fun aCastReachesItsReceiverAndIsRefusedAsTheServerRefuses() = withServer(FixtureServer.Options(scanEveryMs = 0, keepAliveMs = 100, clock = FakeClock())) { server ->
        val alex = Client(server, server.accessToken())
        val robin = Client(server, server.accessToken(FixtureIds.ROBIN))
        val path = "/api/remote/devices/android-den/cast"
        // No such receiver open yet.
        assertEquals(404, alex.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P}}""").code)
        val url = "${server.url}/api/events?receiver=android-den&device_name=Den%20TV&device_kind=tv&casts=item"
        alex.http.newCall(Request.Builder().url(url).header("Authorization", "Bearer ${alex.token}").build()).execute().use { response ->
            val source = response.body.source()
            assertEquals(": connected", source.readUtf8Line())
            waitFor { alex.read("/api/remote/devices", ListSerializer(RemoteDevice.serializer())).singleOrNull() }
            // Another profile's is as unknown without its profile.
            assertEquals(404, robin.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P}}""").code)
            assertEquals(404, robin.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P},"profile_id":999}""").code)
            // And with it, since it doesn't say it switches profile (`switch`): nor is it listed to them.
            assertEquals(404, robin.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P},"profile_id":${FixtureIds.ALEX}}""").code)
            assertEquals(emptyList<RemoteDevice>(), robin.read("/api/remote/devices?household=1", ListSerializer(RemoteDevice.serializer())))
            assertEquals(400, alex.send("POST", path, """{}""").code)
            assertEquals(400, alex.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P},"playlist_id":${FixtureIds.PLAYLIST_VIDEO}}""").code)
            assertEquals(404, alex.send("POST", path, """{"item_id":999999999}""").code)
            assertEquals(404, robin.send("POST", "/api/remote/devices/nope/cast", """{"playlist_id":${FixtureIds.PLAYLIST_VIDEO}}""").code)
            // A channel: this receiver doesn't take Live TV, and there is none here.
            assertEquals(404, alex.send("POST", path, """{"channel_id":1}""").code)
            assertEquals(400, alex.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P},"start_ms":-1}""").code)
            // Tracks go with a movie or an episode, and are of its files.
            assertEquals(400, alex.send("POST", path, """{"item_id":${FixtureIds.SHOW},"audio_index":1}""").code)
            assertEquals(400, alex.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_HDR10},"audio_index":7}""").code)
            assertEquals(400, alex.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_HDR10},"subtitle_index":99}""").code)
            assertEquals(400, alex.send("POST", path, """{"playlist_id":${FixtureIds.PLAYLIST_VIDEO},"subtitle_index":-1}""").code)

            assertEquals(202, alex.send("POST", path, """{"item_id":${FixtureIds.SHOW},"start_ms":60000}""").code)
            var heard: ServerEvent.Cast? = null
            while (heard == null) {
                val line = source.readUtf8Line() ?: break
                if (line.startsWith("data: ")) heard = ServerEvent.parse(line.removePrefix("data: ")) as? ServerEvent.Cast
            }
            assertEquals(ServerEvent.Cast("android-den", itemId = FixtureIds.SHOW, startMs = 60_000, from = heard?.from), heard)
            assertTrue("from the caller's User-Agent", heard?.from?.isNotEmpty() == true)
            assertEquals(202, alex.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_HDR10},"audio_index":1,"subtitle_index":-1}""").code)
            heard = null
            while (heard == null) {
                val line = source.readUtf8Line() ?: break
                if (line.startsWith("data: ")) heard = ServerEvent.parse(line.removePrefix("data: ")) as? ServerEvent.Cast
            }
            assertEquals(1 to -1, heard?.audioIndex to heard?.subtitleIndex)
            assertEquals(202, alex.send("POST", path, """{"playlist_id":${FixtureIds.PLAYLIST_VIDEO},"shuffle":true}""").code)

            // Ten a second to one receiver, as on the server: three went already.
            repeat(7) { assertEquals(202, alex.send("POST", path, """{"item_id":${FixtureIds.TRACK}}""").code) }
            assertEquals(429, alex.send("POST", path, """{"item_id":${FixtureIds.TRACK}}""").code)
        }
    }

    @Test
    fun aReceiverOfTitlesOnlyRefusesAChannelAsNotCastable() = withServer(FixtureServer.Options(scanEveryMs = 0, keepAliveMs = 100)) { server ->
        val alex = Client(server, server.accessToken())
        val url = "${server.url}/api/events?receiver=android-den&casts=live"
        alex.http.newCall(Request.Builder().url(url).header("Authorization", "Bearer ${alex.token}").build()).execute().use { response ->
            assertEquals(": connected", response.body.source().readUtf8Line())
            waitFor { alex.read("/api/remote/devices", ListSerializer(RemoteDevice.serializer())).singleOrNull() }
            val refused = alex.send("POST", "/api/remote/devices/android-den/cast", """{"item_id":${FixtureIds.SAMPLE_1080P}}""")
            assertEquals(409, refused.code)
            assertEquals("not_castable", refused.field("code"))
        }
    }

    @Test
    fun aCastToAnotherProfilesReceiverTakesTheCastersPinAndCarriesATicket() = withServer(FixtureServer.Options(scanEveryMs = 0, keepAliveMs = 100)) { server ->
        val alex = Client(server, server.accessToken())
        val sam = Client(server, server.accessToken(FixtureIds.SAM))
        val robin = Client(server, server.accessToken(FixtureIds.ROBIN))
        val path = "/api/remote/devices/android-den/cast"
        val url = "${server.url}/api/events?receiver=android-den&device_name=Den%20TV&device_kind=tv&casts=item,switch"
        alex.http.newCall(Request.Builder().url(url).header("Authorization", "Bearer ${alex.token}").build()).execute().use { response ->
            val source = response.body.source()
            assertEquals(": connected", source.readUtf8Line())
            waitFor { sam.read("/api/remote/devices?household=1", ListSerializer(RemoteDevice.serializer())).singleOrNull() }
            fun toAlex(pin: String? = null) =
                sam.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P},"profile_id":${FixtureIds.ALEX}${pin?.let { ""","pin":"$it"""" } ?: ""}}""")
            // Sam has a PIN: asked for, checked, and throttled with Sam's switches.
            assertEquals(403 to "pin_needed", toAlex().let { it.code to it.field("code") })
            assertEquals(403 to "wrong_pin", toAlex("0000").let { it.code to it.field("code") })
            assertEquals(202, toAlex(FixtureIds.PIN).code)
            var heard: ServerEvent.Cast? = null
            while (heard == null) {
                val line = source.readUtf8Line() ?: break
                if (line.startsWith("data: ")) heard = ServerEvent.parse(line.removePrefix("data: ")) as? ServerEvent.Cast
            }
            assertEquals(FixtureIds.SAM, heard?.profileId)
            assertTrue(heard?.switchTicket?.startsWith("pley_st_") == true)
            repeat(4) { assertEquals(403, toAlex("0000").code) }
            assertEquals(429, toAlex("0000").code)
            assertEquals(429, toAlex(FixtureIds.PIN).code)
            // Robin has none: nothing to ask.
            assertEquals(202, robin.send("POST", path, """{"item_id":${FixtureIds.SAMPLE_1080P},"profile_id":${FixtureIds.ALEX}}""").code)
        }
    }

    /** [read]'s first answer that isn't `null`, asked for every 20 ms for 5 s at most. */
    private fun <T : Any> waitFor(read: () -> T?): T {
        val deadline = System.currentTimeMillis() + 5_000
        while (true) {
            read()?.let { return it }
            if (System.currentTimeMillis() > deadline) throw AssertionError("not by 5 s")
            Thread.sleep(20)
        }
    }

    @Test
    fun sessionsListTheirTracksWhichCommandsAndReportsName() = withServer { server ->
        val robin = Client(server, server.accessToken(FixtureIds.ROBIN))
        val session = robin.send(
            "POST",
            "/api/playback",
            """{"item_id":${FixtureIds.SAMPLE_HDR10},"caps":{},"controls":["audio","subtitles","quality"]}""",
        ).field("session_id")!!
        fun listed() = PleyJson.decodeFromString(ListSerializer(PlayingSession.serializer()), String(robin.get("/api/sessions").bytes)).single()
        val started = listed()
        assertEquals(listOf(1), started.tracks.audio.map { it.index })
        assertEquals(listOf(FixtureIds.SAMPLE_HDR10_SUBTITLE, FixtureIds.SAMPLE_HDR10_EXTERNAL_SUBTITLE), started.tracks.subtitles.map { it.index })
        // As it started: the audio chosen, no subtitles.
        assertEquals(1, started.audioIndex)
        assertEquals(PlayingSession.SUBTITLES_OFF, started.subtitleIndex)

        val path = "/api/sessions/$session/command"
        assertEquals(400, robin.send("POST", path, """{"command":"audio","stream_index":7}""").code)
        assertEquals(400, robin.send("POST", path, """{"command":"subtitles"}""").code)
        assertEquals(202, robin.send("POST", path, """{"command":"subtitles","stream_index":-1}""").code)
        assertEquals(202, robin.send("POST", path, """{"command":"subtitles","stream_index":${FixtureIds.SAMPLE_HDR10_SUBTITLE}}""").code)

        // The player's report says what it plays now; a track not of the file changes nothing.
        val report = """{"position_ms":1000,"session_id":"$session","audio_index":1,"subtitle_index":${FixtureIds.SAMPLE_HDR10_SUBTITLE}}"""
        assertEquals(204, robin.send("POST", "/api/items/${FixtureIds.SAMPLE_HDR10}/progress", report).code)
        assertEquals(FixtureIds.SAMPLE_HDR10_SUBTITLE, listed().subtitleIndex)
        val stray = """{"position_ms":2000,"session_id":"$session","audio_index":9}"""
        assertEquals(204, robin.send("POST", "/api/items/${FixtureIds.SAMPLE_HDR10}/progress", stray).code)
        assertEquals(1, listed().audioIndex)
        assertEquals(FixtureIds.SAMPLE_HDR10_SUBTITLE, listed().subtitleIndex)

        // The quality: one its menu offers, as its player last said.
        assertEquals(listOf("original", "auto"), started.qualities.take(2).map { it.id })
        assertEquals(null, started.quality)
        assertEquals(400, robin.send("POST", path, """{"command":"quality","quality":"9m"}""").code)
        val offered = started.qualities.last().id
        assertEquals(202, robin.send("POST", path, """{"command":"quality","quality":"$offered"}""").code)
        val picked = """{"position_ms":3000,"session_id":"$session","quality":"$offered"}"""
        assertEquals(204, robin.send("POST", "/api/items/${FixtureIds.SAMPLE_HDR10}/progress", picked).code)
        assertEquals(offered, listed().quality)
    }
}
