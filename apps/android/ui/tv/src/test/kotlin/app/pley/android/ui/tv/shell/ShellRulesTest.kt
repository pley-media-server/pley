package app.pley.android.ui.tv.shell

import androidx.compose.ui.graphics.Color
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.DiscoverList
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.RequestsEntry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The start gate (tv-addendum §3.3) and what the rail lists (design spec §2.1). */
class ShellRulesTest {
    @Test
    fun startGoesToServersWithoutAServer() {
        assertEquals(PleyKey.Servers, startKey(StartFacts(server = null, signedIn = false)))
    }

    @Test
    fun startGoesToLinkWithoutTokens() {
        assertEquals(PleyKey.Link("http://192.168.1.20:7539"), startKey(StartFacts("http://192.168.1.20:7539", signedIn = false)))
    }

    @Test
    fun startAsksWhoIsWatchingInAHouseholdWithoutAlwaysStartAs() {
        assertEquals(PleyKey.Profiles(), startKey(StartFacts("http://s", signedIn = true, profiles = 3)))
        assertEquals(PleyKey.Profiles(), startKey(StartFacts("http://s", signedIn = true, profiles = 3, hasPin = true)))
        assertEquals(PleyKey.Home, startKey(StartFacts("http://s", signedIn = true, profiles = 1)))
        assertEquals(PleyKey.Home, startKey(StartFacts("http://s", signedIn = true, profiles = 1, alwaysStartAs = 1, hasPin = true)))
    }

    @Test
    fun alwaysStartAsGoesOnAsTheProfileLeftOnButNeverPastItsPin() {
        assertEquals(PleyKey.Home, startKey(StartFacts("http://s", signedIn = true, profiles = 3, alwaysStartAs = 2, hasPin = false)))
        assertEquals(PleyKey.Profiles(pin = true), startKey(StartFacts("http://s", signedIn = true, profiles = 3, alwaysStartAs = 2, hasPin = true)))
        // Who the app was left on was never heard: whether it has a PIN isn't known.
        assertEquals(PleyKey.Profiles(), startKey(StartFacts("http://s", signedIn = true, profiles = 3, alwaysStartAs = 2)))
    }

    private fun libraries(n: Int) = (1..n).map { Library(it.toLong(), "Library $it", if (it % 2 == 0) LibraryKind.Shows else LibraryKind.Movies) }

    @Test
    fun theRailListsUpToThreeLibrariesThenCollections() {
        val items = railItems(libraries(3))
        assertEquals(
            listOf("search", "home", "library:1", "library:2", "library:3", "playlists", "collections"),
            items.map { it.id },
        )
        assertEquals(PleyKey.Collections(1), items.last().key)
    }

    @Test
    fun moreThanThreeLibrariesAddLibrariesInCollectionsPlace() {
        val items = railItems(libraries(5))
        assertEquals(
            listOf("search", "home", "library:1", "library:2", "library:3", "libraries", "playlists"),
            items.map { it.id },
        )
        // Seven entries between the avatar and Settings, as the rail has room for.
        assertEquals(7, items.size)
    }

    @Test
    fun whileMusicPlaysNowPlayingComesLastWithinTheRailsSevenEntries() {
        assertEquals(
            "in Collections' place",
            listOf("search", "home", "library:1", "library:2", "library:3", "playlists", "now-playing"),
            railItems(libraries(3), nowPlaying = true).map { it.id },
        )
        assertEquals(
            "in the last library's, which Libraries lists",
            listOf("search", "home", "library:1", "library:2", "libraries", "playlists", "now-playing"),
            railItems(libraries(5), nowPlaying = true).map { it.id },
        )
        assertEquals(
            "added where there is room",
            listOf("search", "home", "library:1", "library:2", "playlists", "collections", "now-playing"),
            railItems(libraries(2), nowPlaying = true).map { it.id },
        )
        val items = railItems(libraries(3), nowPlaying = true)
        assertEquals("now-playing", railCurrent(PleyKey.NowPlaying, items))
    }

    @Test
    fun liveTvComesAfterHomeAndMakesRoomAsNowPlayingDoes() {
        assertEquals(
            "in Collections' place",
            listOf("search", "home", "live-tv", "library:1", "library:2", "library:3", "playlists"),
            railItems(libraries(3), liveTv = true).map { it.id },
        )
        assertEquals(
            "in the last library's, which Libraries lists",
            listOf("search", "home", "live-tv", "library:1", "library:2", "libraries", "playlists"),
            railItems(libraries(5), liveTv = true).map { it.id },
        )
        assertEquals(
            "with Now Playing too, two libraries make way for Libraries",
            listOf("search", "home", "live-tv", "library:1", "libraries", "playlists", "now-playing"),
            railItems(libraries(3), nowPlaying = true, liveTv = true).map { it.id },
        )
        assertEquals(
            "without libraries",
            listOf("search", "home", "live-tv", "playlists"),
            railItems(emptyList(), liveTv = true).map { it.id },
        )
        for (n in 0..6) {
            for (music in listOf(false, true)) assertTrue(railItems(libraries(n), music, liveTv = true).size <= MAX_ITEMS)
        }
        assertEquals("live-tv", railCurrent(PleyKey.LiveTv, railItems(libraries(2), liveTv = true)))
    }

    @Test
    fun theWayIntoRequestsIsDiscoverAfterHomeElseTheRequestsElseNothing() {
        val discover = requestsItem(RequestsEntry(discover = true, requests = true, badge = 2))
        assertEquals(PleyKey.Discover, discover?.key)
        assertEquals("it carries the number", 2, discover?.requests?.badge)
        // Not open to this account, with requests of its own; for managers, everyone's.
        assertEquals(PleyKey.Requests, requestsItem(RequestsEntry(requests = true))?.key)
        assertEquals("requests", requestsItem(RequestsEntry(requests = true, manage = true, badge = 3, failed = 1))?.id)
        assertNull(requestsItem(RequestsEntry.None))
        assertEquals(
            listOf("search", "home", "discover", "library:1", "library:2", "playlists", "collections"),
            railItems(libraries(2), requests = discover).map { it.id },
        )
    }

    @Test
    fun theWayIntoRequestsMakesRoomAsLiveTvDoesAndComesBeforeIt() {
        val discover = requestsItem(RequestsEntry(discover = true))
        assertEquals(
            "in Collections' place",
            listOf("search", "home", "discover", "library:1", "library:2", "library:3", "playlists"),
            railItems(libraries(3), requests = discover).map { it.id },
        )
        assertEquals(
            "in the last library's, which Libraries lists",
            listOf("search", "home", "discover", "library:1", "library:2", "libraries", "playlists"),
            railItems(libraries(5), requests = discover).map { it.id },
        )
        assertEquals(
            listOf("search", "home", "discover", "library:1", "libraries", "playlists", "now-playing"),
            railItems(libraries(3), nowPlaying = true, requests = discover).map { it.id },
        )
        assertEquals(
            listOf("search", "home", "discover", "library:1", "libraries", "playlists", "now-playing"),
            railItems(libraries(5), nowPlaying = true, requests = discover).map { it.id },
        )
        assertEquals(
            listOf("search", "home", "discover", "live-tv", "library:1", "libraries", "playlists"),
            railItems(libraries(3), liveTv = true, requests = discover).map { it.id },
        )
        for (n in 0..6) {
            for (music in listOf(false, true)) {
                for (live in listOf(false, true)) assertTrue(railItems(libraries(n), music, live, discover).size <= MAX_ITEMS)
            }
        }
    }

    @Test
    fun discoversPagesAndTheRequestsReachedFromItAreUnderItsEntry() {
        val items = railItems(libraries(2), requests = requestsItem(RequestsEntry(discover = true, requests = true))) + SettingsItem
        assertEquals("discover", railCurrent(PleyKey.Discover, items))
        assertEquals("discover", railCurrent(PleyKey.DiscoverList(DiscoverList.Trending), items))
        assertEquals("discover", railCurrent(PleyKey.DiscoverTitle(TitleKind.Movie, 1), items))
        assertEquals("discover", railCurrent(PleyKey.Requests, items))
        val own = railItems(libraries(2), requests = requestsItem(RequestsEntry(requests = true))) + SettingsItem
        assertEquals("requests", railCurrent(PleyKey.Requests, own))
        assertEquals("requests", railCurrent(PleyKey.Discover, own))
        assertNull(railCurrent(PleyKey.Discover, railItems(libraries(2)) + SettingsItem))
    }

    @Test
    fun aUsbDriveShowsAfterNowPlayingWhileOneIsPluggedIn() {
        assertTrue(railItems(libraries(1)).none { it.slot == RailSlot.Drive })
        val items = railItems(libraries(1), nowPlaying = true, drive = true)
        assertEquals(listOf("now-playing", "drive"), items.takeLast(2).map { it.id })
        assertEquals(PleyIcons.Usb, items.last().icon)
        assertEquals(
            "it makes room as Now Playing does: two libraries make way for Libraries",
            listOf("search", "home", "library:1", "libraries", "playlists", "now-playing", "drive"),
            railItems(libraries(3), nowPlaying = true, drive = true).map { it.id },
        )
        for (n in 0..6) assertTrue(railItems(libraries(n), nowPlaying = true, liveTv = true, drive = true).size <= MAX_ITEMS)
        // Its folders are its pages.
        assertEquals("drive", railCurrent(PleyKey.Drive("/storage/1A2B-3C4D/Films"), items))
    }

    @Test
    fun withEveryExtraTheDriveTakesPlaylistsPlaceWhenNoLibraryCanMakeWay() {
        val discover = requestsItem(RequestsEntry(discover = true, requests = true))
        assertEquals(
            "the libraries have all made way for Live TV, Discover and Now Playing: Playlists makes way for the drive",
            listOf("search", "home", "discover", "live-tv", "libraries", "now-playing", "drive"),
            railItems(libraries(5), nowPlaying = true, liveTv = true, requests = discover, drive = true).map { it.id },
        )
        assertEquals(
            "\"Libraries\" in the one library's place would free nothing: Playlists makes way",
            listOf("search", "home", "discover", "live-tv", "library:1", "now-playing", "drive"),
            railItems(libraries(1), nowPlaying = true, liveTv = true, requests = discover, drive = true).map { it.id },
        )
        for (n in 0..8) {
            for (music in listOf(false, true)) {
                for (live in listOf(false, true)) {
                    for (requests in listOf(null, discover)) {
                        val items = railItems(libraries(n), music, live, requests, drive = true)
                        assertTrue("$n libraries, music $music, live $live, requests ${requests != null}", items.size <= MAX_ITEMS)
                        assertTrue(items.any { it.slot == RailSlot.Drive })
                    }
                }
            }
        }
    }

    @Test
    fun aRailWithoutLibrariesHasNoCollections() {
        assertEquals(listOf("search", "home", "playlists"), railItems(emptyList()).map { it.id })
    }

    @Test
    fun theCurrentEntryFollowsTheBackStacksFirstPage() {
        val items = railItems(libraries(5)) + SettingsItem
        assertEquals("home", railCurrent(PleyKey.Home, items))
        assertEquals("search", railCurrent(PleyKey.Search("north"), items))
        assertEquals("library:2", railCurrent(PleyKey.Library(2), items))
        assertEquals("libraries", railCurrent(PleyKey.Library(5), items))
        assertEquals("libraries", railCurrent(PleyKey.Libraries, items))
        assertEquals("settings", railCurrent(PleyKey.Settings, items))
        assertEquals(PROFILE_ITEM, railCurrent(PleyKey.Profiles(), items))
        assertNull(railCurrent(PleyKey.Servers, items))
    }

    @Test
    fun profileColoursParseAsTheWebWritesThem() {
        assertEquals(Color(0xFFE07A5F), RailProfile.parseColor("#e07a5f"))
        assertNull(RailProfile.parseColor("teal"))
        assertEquals("A", RailProfile("anna").initial)
    }
}
