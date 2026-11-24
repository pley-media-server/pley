package app.pley.android.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Live TV's names as the phone, the TV and the car show them: a group's region, and a channel's in its group. */
class LiveNamesTest {
    @Test
    fun `names leave out the region their group already says`() {
        assertEquals("UK" to "Sport", LiveNames.splitRegion("UK| Sport"))
        assertEquals("UK" to "Sport", LiveNames.splitRegion("[UK] Sport"))
        assertEquals("UK" to "Sport", LiveNames.splitRegion("UK: Sport"))
        assertEquals("BEE" to "SOLO SPORT F1 HEVC", LiveNames.splitRegion("|BEE| SOLO SPORT F1 HEVC"))
        assertNull(LiveNames.splitRegion("Sport 1080i"))
        assertNull(LiveNames.splitRegion("Sky: News at Ten"))
        val sky = LiveChannel(1, name = "UK| Sky Sports 1", group = "UK| Sport")
        assertEquals("Sky Sports 1", LiveNames.channelName(sky, LiveCategory.Group("UK| Sport")))
        assertEquals("UK| Sky Sports 1", LiveNames.channelName(sky, LiveCategory.All))
        assertEquals("FR| TF1", LiveNames.channelName(LiveChannel(2, name = "FR| TF1", group = "UK| Sport"), LiveCategory.Group("UK| Sport")))
    }

    @Test
    fun `a group's region comes with it`() {
        val g = LiveGroup("UK| News", "UK", "News", 21)
        assertEquals("UK · News", LiveNames.groupName(g, g.name))
        assertEquals("News", LiveNames.groupName(LiveGroup("News"), "News"))
        assertEquals("Gone", LiveNames.groupName(null, "Gone"))
    }
}
