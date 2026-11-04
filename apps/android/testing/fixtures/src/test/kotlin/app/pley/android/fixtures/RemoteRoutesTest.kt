package app.pley.android.fixtures

import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.PleyJson
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.Request
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The remote's routes (docs/API.md, "Remote control"): the profile's sessions, and commands to their players. */
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
}
