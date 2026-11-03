package app.pley.android.fixtures

import app.pley.android.model.CastAction
import app.pley.android.model.CastDevice
import app.pley.android.model.CastPlatform
import app.pley.android.model.CastPlayState
import app.pley.android.model.ServerEvent
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.Request
import okio.BufferedSource
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Casting as the server does it: a TV listed while its stream is open, sent titles and commands, saying what it plays. */
class CastTest {
    private val tv = "0123456789abcdef0123456789abcdef"
    private val devices = ListSerializer(CastDevice.serializer())

    /** The next event on [source], skipping comments. */
    private fun next(source: BufferedSource): ServerEvent {
        var line = source.readUtf8Line()
        while (line != null && !line.startsWith("data: ")) line = source.readUtf8Line()
        return ServerEvent.parse(line!!.removePrefix("data: "))
    }

    @Test
    // Keep-alives often: a stream the app closed is noticed at the next one.
    fun aTvIsListedWhileItsStreamIsOpenAndHearsWhatItIsSent() = withServer(FixtureServer.Options(scanEveryMs = 0, keepAliveMs = 50)) { server ->
        val phone = Client(server, server.accessToken())
        val token = server.accessToken()
        val request = Request.Builder()
            .url("${server.url}/api/events?cast_device=$tv&cast_name=Living%20room&cast_platform=android_tv")
            .header("Authorization", "Bearer $token")
            .build()
        phone.http.newCall(request).execute().use { response ->
            val source = response.body.source()
            assertEquals(": connected", source.readUtf8Line())
            // Its own coming is told to the profile's streams, its own included.
            assertEquals(ServerEvent.CastDevicesChanged, next(source))
            val listed = phone.read("/api/cast/devices", devices).single()
            assertEquals(CastDevice(tv, "Living room", CastPlatform.AndroidTv, null), listed)

            assertEquals(204, phone.send("POST", "/api/cast/devices/$tv/play", """{"item_id":${FixtureIds.SAMPLE_1080P},"position_ms":5000}""").code)
            assertEquals(ServerEvent.CastPlay(tv, FixtureIds.SAMPLE_1080P, null, 5_000), next(source))
            assertEquals(204, phone.send("POST", "/api/cast/devices/$tv/control", """{"action":"pause"}""").code)
            assertEquals(ServerEvent.CastControl(tv, CastAction.Pause), next(source))

            val tvApp = Client(server, token)
            assertEquals(204, tvApp.send("PUT", "/api/cast/devices/$tv/state", """{"item_id":${FixtureIds.SAMPLE_1080P},"position_ms":5000,"state":"paused"}""").code)
            val heard = next(source) as ServerEvent.CastStateChanged
            assertEquals(CastPlayState.Paused, heard.state?.state)
            assertEquals(FixtureIds.SAMPLE_1080P, phone.read("/api/cast/devices", devices).single().state?.item?.id)
            assertEquals(204, tvApp.send("PUT", "/api/cast/devices/$tv/state", "{}").code)
            assertNull((next(source) as ServerEvent.CastStateChanged).state)
        }
        // Its stream closed, it left the list.
        var left = false
        repeat(100) {
            if (!left) {
                left = phone.read("/api/cast/devices", devices).isEmpty()
                if (!left) Thread.sleep(20)
            }
        }
        assertTrue(left)
    }

    @Test
    fun whatTheServerRefuses() = withServer { server ->
        val app = Client(server, server.accessToken())
        assertEquals(400, app.get("/api/events?cast_device=$tv&cast_platform=android_tv").code)
        assertEquals(400, app.get("/api/events?cast_device=short&cast_name=TV&cast_platform=android_tv").code)
        assertEquals(400, app.get("/api/events?cast_device=$tv&cast_name=TV&cast_platform=fridge").code)
        val libraryOnly = Client(server, server.accessToken(scopes = setOf("library")))
        assertEquals(403, libraryOnly.get("/api/events?cast_device=$tv&cast_name=TV&cast_platform=android_tv").code)
        // Not connected: 404, whatever is asked.
        assertEquals(404, app.send("POST", "/api/cast/devices/$tv/play", """{"item_id":${FixtureIds.SAMPLE_1080P}}""").code)
        assertEquals(404, app.send("PUT", "/api/cast/devices/$tv/state", "{}").code)
        assertTrue(app.read("/api/cast/devices", devices).isEmpty())
    }
}
