package app.pley.android.model.admin

import app.pley.android.model.PleyJson
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Live TV's lineup and tuners, read as apps/server/src/api/livetv.rs and livetv/mod.rs write them (docs/API.md, "Live TV"). */
class LiveTvLineupTest {
    @Test
    fun theLineupReadsAPageAndItsGroups() {
        val page = PleyJson.decodeFromString(
            LineupPage.serializer(),
            """{"total":250,"hidden":12,"offset":100,"channels":[{"id":9,"source_id":3,"name":"News HD","number":"101",
                "group":"UK| News","logo":"/api/live/channels/9/logo","guide_id":"news.uk","hidden":true,"source_name":"News",
                "source_number":"5","source_guide_id":"","matched_guide_id":"news.uk","custom_name":"News HD","custom_number":"101",
                "custom_guide_id":null,"has_guide":true,"offline":false,"offline_since":null}]}""",
        )
        assertEquals(250, page.total)
        assertEquals(12, page.hidden)
        assertEquals(100, page.offset)
        val channel = page.channels.single()
        assertEquals(9L, channel.id)
        assertEquals("News", channel.sourceName)
        assertEquals("5", channel.sourceNumber)
        assertEquals("news.uk", channel.matchedGuideId)
        assertEquals("News HD", channel.customName)
        assertNull(channel.customGuideId)
        assertTrue(channel.hidden)
        assertTrue(channel.hasGuide)

        // An AdminGroup is a Group flattened, with its hidden and offline counts.
        val groups = PleyJson.decodeFromString(
            ListSerializer(LineupGroup.serializer()),
            """[{"name":"UK| News","region":"UK","label":"News","count":40,"hidden":40,"offline":2},
               {"name":"","region":null,"label":"","count":3,"hidden":0,"offline":0}]""",
        )
        assertEquals(LineupGroup("UK| News", "UK", "News", 40, 40, 2), groups[0])
        assertNull(groups[1].region)
    }

    @Test
    fun aLineupChangeLeavesOutWhatItDoesntChange() {
        val hide = PleyJson.encodeToJsonElement(LineupPatch.serializer(), LineupPatch(groups = listOf("UK| News"), hidden = true)).jsonObject
        assertEquals(setOf("ids", "groups", "hidden"), hide.keys)
        val filtered = PleyJson.encodeToJsonElement(
            LineupPatch.serializer(),
            LineupPatch(filter = LineupFilter(state = LineupState.NoGuide, q = "news"), hidden = false),
        ).jsonObject
        val filter = filtered["filter"] as JsonObject
        assertEquals("no-guide", filter["state"]?.jsonPrimitive?.content)
        assertEquals("news", filter["q"]?.jsonPrimitive?.content)
        assertFalse("group" in filter)
        // "" gives back the source's own value, so it is sent.
        val named = PleyJson.encodeToJsonElement(LineupPatch.serializer(), LineupPatch(ids = listOf(9), name = "", number = "7", guideId = "")).jsonObject
        assertEquals("", named["name"]?.jsonPrimitive?.content)
        assertEquals("", named["guide_id"]?.jsonPrimitive?.content)
        assertFalse("hidden" in named)
        assertFalse(named["name"] is JsonNull)
    }

    @Test
    fun aTunerSaysWhoWatchesAndWhatTheTranscoderMakes() {
        val tuners = PleyJson.decodeFromString(
            ListSerializer(TunerStatus.serializer()),
            """[{"channel_id":9,"channel_name":"News HD","source_id":3,"source_name":"Provider","started_at":1700000000,
                "connected":true,"error":null,"bytes_in":1000,"kbps":4200,
                "viewers":[{"session_id":"s1","user_id":2,"username":"ann","kind":"hls","started_at":1700000000}],
                "idle":false,"plan":{"video_codec":"mpeg2video","width":1920,"height":1080,"frame_rate":25.0,"interlaced":true,
                "audio_codec":"mp2","audio_channels":2,"video":"h264_nvenc","audio":"aac"}}]""",
        )
        val tuner = tuners.single()
        assertEquals(9L, tuner.channelId)
        assertEquals(4200L, tuner.kbps)
        assertEquals(listOf("ann"), tuner.viewers.map { it.username })
        assertEquals("h264_nvenc", tuner.plan?.video)
        assertTrue(tuner.plan?.interlaced == true)
    }
}
