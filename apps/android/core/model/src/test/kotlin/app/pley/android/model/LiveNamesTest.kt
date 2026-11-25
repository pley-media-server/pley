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
    fun `a row names a channel without its region or its tags`() {
        fun lines(category: LiveCategory?, vararg names: String) = LiveNames.channelLines(names.map { LiveChannel(1, name = it) }, category)
        // The region goes under the name, the tags in superscript or small capitals go; a plain HD and the case stay.
        assertEquals(
            listOf(LiveNames.ChannelLine("ABC HD", "US"), LiveNames.ChannelLine("ABC DETROIT NEWS (WXYZ)", "PRIME"), LiveNames.ChannelLine("Fox", null)),
            lines(LiveCategory.All, "US| ABC HD", "PRIME| ABC DETROIT NEWS (WXYZ) ᴿᴬᵂ", "Fox ᶠᴴᴰ"),
        )
        assertEquals("TF1", LiveNames.withoutTags("TF1 ᴴᴰ ⁵⁰ᶠᵖˢ"))
        assertEquals("Canal+", LiveNames.withoutTags("Canal+ ʜᴅ"))
        assertEquals("ESPN HD", LiveNames.withoutTags("ESPN HD"))
        assertEquals("ᴴᴰ", LiveNames.withoutTags("ᴴᴰ"))
        assertEquals("Sport 1080i", LiveNames.withoutTags("Sport 1080i"))

        // A region every row has, or the group's, isn't said; one of another region is.
        assertEquals(listOf(null, null), lines(LiveCategory.Favorites, "UK| BBC One", "[UK] ITV").map { it.region })
        assertEquals(listOf(null, "FR"), lines(LiveCategory.Group("UK| Sport"), "UK| Sky Sports 1", "FR| TF1").map { it.region })
        // Alone (playing, a search's one result), it is.
        assertEquals(listOf("UK"), lines(null, "UK| BBC One").map { it.region })

        // Rows that would read the same keep the provider's names.
        assertEquals(
            listOf(LiveNames.ChannelLine("US| ABC ᴴᴰ", null), LiveNames.ChannelLine("US| ABC ᴿᴬᵂ", null), LiveNames.ChannelLine("ABC", "CA")),
            lines(LiveCategory.All, "US| ABC ᴴᴰ", "US| ABC ᴿᴬᵂ", "CA| ABC"),
        )
    }

    @Test
    fun `a channel goes by its names with and without its region and tags`() {
        assertEquals(setOf("UK| BBC One ᴴᴰ", "BBC One ᴴᴰ", "UK| BBC One", "BBC One"), LiveNames.namesOf(LiveChannel(1, name = " UK| BBC One ᴴᴰ")))
    }

    @Test
    fun `a group's region comes with it`() {
        val g = LiveGroup("UK| News", "UK", "News", 21)
        assertEquals("UK · News", LiveNames.groupName(g, g.name))
        assertEquals("News", LiveNames.groupName(LiveGroup("News"), "News"))
        assertEquals("Gone", LiveNames.groupName(null, "Gone"))
    }
}
