package app.pley.android.screens.admin.livetv

import app.pley.android.model.admin.LineupFilter
import app.pley.android.model.admin.LineupGroup
import app.pley.android.model.admin.LineupState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Live TV's pages and the rules they share with the web's (apps/web/src/pages/settings/livetv). */
class LiveTvAdminTest {
    @Test
    fun aPagesIdNamesItAndComesBack() {
        val pages = listOf(LiveTvAdminPage.Front, LiveTvAdminPage.Groups, LiveTvAdminPage.Channels)
        for (page in pages) assertEquals(page, LiveTvAdminPage.of(page.id))
        // Pages that were the web app's alone are the first page.
        assertEquals(LiveTvAdminPage.Front, LiveTvAdminPage.of("source:12"))
        assertEquals(LiveTvAdminPage.Front, LiveTvAdminPage.of("options"))
    }

    @Test
    fun groupsGoTogetherByRegionAtTheirFirstPlace() {
        val groups = listOf(
            LineupGroup("UK| News", "UK", "News", count = 10),
            LineupGroup("Sport", null, "Sport", count = 5, hidden = 5),
            LineupGroup("FR| Films", "FR", "Films", count = 4),
            LineupGroup("UK| Films", "UK", "Films", count = 8, hidden = 3),
        )
        val sections = groupSections(groups)
        assertEquals(listOf("UK", null, null), sections.map { it.region })
        assertEquals(listOf("UK| News", "UK| Films"), sections[0].groups.map { it.name })
        assertEquals(18, sections[0].channels)
        // A region of one group is listed as the group alone.
        assertEquals("FR| Films", sections[2].groups.single().name)
        assertEquals(false, sections[0].allHidden)
        assertEquals(false, sections[0].allShown)

        assertEquals(listOf("Sport"), groupSections(groups, shown = GroupsShown.Hidden).flatMap { it.groups }.map { it.name })
        assertEquals(3, groupSections(groups, shown = GroupsShown.Shown).flatMap { it.groups }.size)
        // Found by name, a region with one group found is that group alone.
        val found = groupSections(groups, query = " films")
        assertEquals(listOf(null, null), found.map { it.region })
        assertEquals(listOf("FR| Films", "UK| Films"), found.map { it.groups.single().name })
    }

    @Test
    fun codecsAndEncodersAreNamedAsPeopleKnowThem() {
        assertEquals("MPEG-2", codecName("mpeg2video"))
        assertEquals("Dolby Digital Plus", codecName("eac3"))
        assertEquals("vp9", codecName("vp9"))
        assertEquals("", codecName(null))
        assertNull(encoderName("libx264"))
        assertEquals("NVIDIA", encoderName("h264_nvenc"))
        assertEquals("h264_vaapi", encoderName("h264_vaapi"))
    }

    @Test
    fun aPageOfTheLineupIsAskedForWithItsFilter() {
        assertEquals(
            listOf("group" to null, "state" to "all", "q" to null, "offset" to "0", "limit" to "100"),
            lineupQuery(LineupFilter(), 0),
        )
        assertEquals(
            listOf("group" to "UK| News", "state" to "no-guide", "q" to "bbc", "offset" to "200", "limit" to "100"),
            lineupQuery(LineupFilter(group = "UK| News", state = LineupState.NoGuide, q = " bbc "), 2),
        )
        assertEquals("q" to null, lineupQuery(LineupFilter(q = "  "), 0)[2])
    }
}
