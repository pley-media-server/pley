package app.pley.android.player.music.library

import android.net.Uri
import androidx.media3.common.MediaMetadata
import androidx.media3.session.MediaConstants
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveGroup
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveProgram
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistSummary
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Live TV in the car: its tab, the tab's headings, a list's channels, search, and what a channel plays among. */
@RunWith(RobolectricTestRunner::class)
class LiveLibraryTest {
    private val source = FakeLiveSource()
    private val live = LiveLibrary(source, { Look }, { path -> Uri.parse("content://art$path") }, now = { NOW })

    @Test
    fun idsReadBackWhatTheyWrite() {
        val ids = listOf(
            MusicId.LiveTv,
            MusicId.LiveList(LiveCategory.All),
            MusicId.LiveList(LiveCategory.Favorites),
            MusicId.LiveList(LiveCategory.Group("UK| Sport @ 9:00")),
            MusicId.Channel(7),
            MusicId.Channel(7, LiveCategory.Recent),
            MusicId.Channel(7, LiveCategory.Group("News: World")),
        )
        for (id in ids) assertEquals(id, MusicId.parse(id.text))
        for (bad in listOf("live:", "live:group:", "live:everything", "channel:0", "channel:7@", "channel:7@album:3", "channel:x"))
            assertNull(bad, MusicId.parse(bad))
        assertNull(MusicId.parse("live:group:" + "x".repeat(600)))
    }

    @Test
    fun theTabShowsOnlyWhenLiveTvIsOpenToTheProfile() = runTest {
        assertEquals("live", live.tab()?.mediaId)
        assertEquals("content://icon/live", live.tab()?.mediaMetadata?.artworkUri.toString())
        source.shown = false
        assertNull(live.tab())
        source.failing = true
        assertNull(live.tab())
        assertEquals(emptyList<Any>(), live.search("news"))
    }

    @Test
    fun theTabListsFavouritesThenWatchedLatelyThenTheGroups() = runTest {
        val items = live.children(MusicId.LiveTv)!!
        assertEquals(
            listOf("channel:1@favorites", "channel:3@recent", "live:group:News", "live:group:Sport"),
            items.map { it.mediaId },
        )
        // A favourite watched lately shows once, as a favourite.
        assertEquals(listOf("Favorites", "Recently watched", "Groups", "Groups"), items.map { it.group })
        assertEquals("2 channels", items[2].mediaMetadata.subtitle.toString())
        assertTrue(items[2].mediaMetadata.isBrowsable == true)
    }

    @Test
    fun aLineupWithoutGroupsListsEveryChannel() = runTest {
        source.groups = source.groups.copy(groups = listOf(LiveGroup("", label = "", count = 3)), favorites = 0, recent = 0)
        val items = live.children(MusicId.LiveTv)!!
        assertEquals(listOf("live:all"), items.map { it.mediaId })
        assertEquals("All channels", items[0].mediaMetadata.title.toString())
        assertEquals("3 channels", items[0].mediaMetadata.subtitle.toString())
    }

    @Test
    fun aChannelShowsItsNameWhatIsOnAndItsLogo() = runTest {
        val news = live.children(MusicId.LiveList(LiveCategory.Group("News")))!!
        assertEquals(listOf("channel:1@group:News", "channel:2@group:News"), news.map { it.mediaId })
        val one = news[0].mediaMetadata
        assertEquals("News One", one.title.toString())
        assertEquals("The Morning", one.subtitle.toString())
        // The server's address, with the version that changes with the provider's picture.
        assertEquals("content://art/api/live/channels/1/logo?v=0a1b2c3d", one.artworkUri.toString())
        assertEquals(MediaMetadata.MEDIA_TYPE_RADIO_STATION, one.mediaType)
        assertTrue(one.isPlayable == true && one.isBrowsable == false)
        // No logo, nothing on: no picture, no subtitle.
        assertNull(news[1].mediaMetadata.artworkUri)
        assertNull(news[1].mediaMetadata.subtitle)
    }

    @Test
    fun aChannelPlaysAmongTheListItWasPickedFrom() = runTest {
        assertEquals(listOf(1L, 2L) to 1, live.lineup(MusicId.Channel(2, LiveCategory.Group("News")))?.let { (l, at) -> l.map { it.id } to at })
        // Picked from a search, or gone from its list: alone.
        assertEquals(listOf(3L) to 0, live.lineup(MusicId.Channel(3))?.let { (l, at) -> l.map { it.id } to at })
        assertEquals(listOf(3L) to 0, live.lineup(MusicId.Channel(3, LiveCategory.Group("News")))?.let { (l, at) -> l.map { it.id } to at })
        assertNull(live.lineup(MusicId.Channel(99)))
    }

    @Test
    fun searchAndVoiceFindChannels() = runTest {
        val found = live.search("sport")
        assertEquals(listOf("channel:3"), found.map { it.mediaId })
        assertEquals("Channels", found[0].group)
        assertEquals(3L, live.named("SPORT 24")?.id)
        // A spoken name plays its channel only when it is the channel's name.
        assertNull(live.named("sport"))
        assertEquals(3L, live.closest("sport")?.id)
    }

    @Test
    fun groupsAndTheirChannelsAreNamedAsThePhoneListsThem() = runTest {
        source.channels += listOf(
            LiveChannel(4, name = "UK| Sky Sports 1", group = "UK| Sport"),
            LiveChannel(5, name = "FR| TF1", group = "UK| Sport"),
        )
        source.groups = source.groups.copy(groups = source.groups.groups + LiveGroup("UK| Sport", "UK", "Sport", 2))
        val lists = live.children(MusicId.LiveTv)!!.filter { it.mediaId.startsWith("live:") }
        assertEquals(listOf("News", "Sport", "UK · Sport"), lists.map { it.mediaMetadata.title.toString() })
        assertEquals("UK · Sport", live.item(MusicId.LiveList(LiveCategory.Group("UK| Sport")))?.mediaMetadata?.title.toString())

        // In the group, a channel without the region the group says; one of another region keeps it.
        val sport = live.children(MusicId.LiveList(LiveCategory.Group("UK| Sport")))!!
        assertEquals(listOf("Sky Sports 1", "FR| TF1"), sport.map { it.mediaMetadata.title.toString() })
        // Elsewhere (search, the favorites, Now Playing), its whole name, as the phone shows it there.
        assertEquals(listOf("UK| Sky Sports 1"), live.search("sky").map { it.mediaMetadata.title.toString() })

        // Said as a list shows it, or as the provider names it.
        assertEquals(4L, live.named("sky sports 1")?.id)
        assertEquals(4L, live.named("UK| Sky Sports 1")?.id)
    }

    @Test
    fun theMusicLibraryHasLiveTvAmongItsTabsAndSearchesItsChannels() = runTest {
        val music = SomeMusic()
        val library = MusicLibrary(music, { MusicLook }, { _, _ -> null }, live)
        // Nothing else in the library: Live TV's tab alone.
        assertEquals(listOf("live"), library.children(MusicId.Root, 0, 50).map { it.mediaId })
        assertEquals(listOf("channel:3"), library.search("sport").map { it.mediaId })
        assertEquals("live", library.item(MusicId.LiveTv)?.mediaId)

        // Music, playlists and books too: five tabs, more than a car shows. Playlists goes first in the artists' tab,
        // named Music.
        music.full = true
        val tabs = library.children(MusicId.Root, 0, 50)
        assertEquals(listOf("recent", "artists", "books", "live"), tabs.map { it.mediaId })
        assertEquals("Music", tabs[1].mediaMetadata.title.toString())
        assertEquals(listOf("playlists", "artist:40"), library.children(MusicId.Artists, 0, 50).map { it.mediaId })
        assertEquals("playlists", library.item(MusicId.Playlists)?.mediaId)
        // Paged, Playlists and the artists are one list, and no page holds more than asked for (Media3 refuses it).
        assertEquals(listOf("playlists"), library.children(MusicId.Artists, 0, 1).map { it.mediaId })
        assertEquals(listOf("artist:40"), library.children(MusicId.Artists, 1, 1).map { it.mediaId })
        assertEquals(listOf("recent", "artists"), library.children(MusicId.Root, 0, 2).map { it.mediaId })
        assertEquals(listOf("books", "live"), library.children(MusicId.Root, 1, 2).map { it.mediaId })

        // Channels slow to come don't hold the music's results: the search goes on without them.
        source.slow = true
        val withArtist = MusicLibrary(object : MusicSource by music {
            override suspend fun search(query: String) = listOf(ItemSummary(41, 3, ItemKind.Artist, "Sport Relief"))
        }, { MusicLook }, { _, _ -> null }, live)
        assertEquals(listOf("artist:41"), withArtist.search("sport").map { it.mediaId })
        source.slow = false

        source.shown = false
        val four = library.children(MusicId.Root, 0, 50)
        assertEquals(listOf("recent", "artists", "playlists", "books"), four.map { it.mediaId })
        assertEquals("Artists", four[1].mediaMetadata.title.toString())
        assertEquals(listOf("artist:40"), library.children(MusicId.Artists, 0, 50).map { it.mediaId })
    }

    private val androidx.media3.common.MediaItem.group: String?
        get() = mediaMetadata.extras?.getString(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_GROUP_TITLE)

    private class FakeLiveSource : LiveSource {
        var shown = true
        var failing = false

        /** Searches answer after a minute, as an IPTV provider's lineup can. */
        var slow = false
        val channels = mutableListOf(
            LiveChannel(1, name = "News One", group = "News", logo = "/api/live/channels/1/logo?v=0a1b2c3d", favorite = true, now = LiveProgram(1, NOW - 60, NOW + 600, "The Morning")),
            LiveChannel(2, name = "News Two", group = "News"),
            LiveChannel(3, name = "Sport 24", group = "Sport"),
        )
        var groups = LiveGroups(total = 3, favorites = 1, recent = 2, groups = listOf(LiveGroup("News", label = "News", count = 2), LiveGroup("Sport", label = "Sport", count = 1)))
        val recent = listOf(1L, 3L)

        override suspend fun shown(): Boolean {
            if (failing) error("offline")
            return shown
        }

        override suspend fun groups(): LiveGroups = groups

        override suspend fun channels(category: LiveCategory, query: String, limit: Int): List<LiveChannel> {
            if (slow && query.isNotEmpty()) delay(60_000)
            val listed = when (category) {
                LiveCategory.All -> channels
                LiveCategory.Favorites -> channels.filter { it.favorite }
                LiveCategory.Recent -> recent.mapNotNull { id -> channels.firstOrNull { it.id == id } }
                is LiveCategory.Group -> channels.filter { it.group == category.name }
            }
            return listed.filter { query.isEmpty() || it.name.contains(query, ignoreCase = true) }.take(limit)
        }

        override suspend fun channel(id: Long): LiveChannel? = channels.firstOrNull { it.id == id }
    }

    private object Look : LiveLook {
        override val liveTv = "Live TV"
        override val favorites = "Favorites"
        override val watchedLately = "Recently watched"
        override val groups = "Groups"
        override val allChannels = "All channels"
        override val channels = "Channels"

        override fun channelCount(count: Int) = "$count channels"

        override val liveIcon: Uri = Uri.parse("content://icon/live")
    }

    private object MusicLook : MusicLibraryLook {
        override val recent = "Recent"
        override val recentlyPlayed = "Recently played"
        override val recentlyAdded = "Recently added"

        override fun recentlyAddedIn(library: String) = "Recently added in $library"
        override val artists = "Artists"
        override val playlists = "Playlists"
        override val albums = "Albums"
        override val songs = "Tracks"
        override val shuffle = "Shuffle"

        override fun tracks(count: Int) = "$count tracks"

        override fun albums(count: Int) = "$count albums"

        override fun disc(number: Int) = "Disc $number"
        override val books = "Books"
        override val continueListening = "Continue listening"
        override val allBooks = "All books"
        override val music = "Music"

        override fun timeLeft(heardMs: Long, totalMs: Long) = ""
        override val recentIcon: Uri? = null
        override val artistsIcon: Uri? = null
        override val playlistsIcon: Uri? = null
        override val booksIcon: Uri? = null
        override val shuffleIcon: Uri? = null
    }

    /** A library with nothing in it, or when [full] an artist, a music playlist and a library of books. */
    private class SomeMusic : MusicSource {
        var full = false

        override suspend fun libraries() = if (full) listOf(Library(3, "Music", LibraryKind.Music, itemCount = 1)) else emptyList()

        override suspend fun artists(libraryId: Long, offset: Int, limit: Int) =
            listOf(ItemSummary(40, 3, ItemKind.Artist, "Harbour Lights")).drop(offset).take(limit)

        override suspend fun playlists() = if (full) listOf(PlaylistSummary(7, "Night drive", PlaylistKind.Audio, itemCount = 2)) else emptyList()

        override suspend fun bookLibraries() = if (full) listOf(Library(5, "Books", LibraryKind.Audiobooks, itemCount = 1)) else emptyList()

        override suspend fun bookShelves(libraryId: Long) = BookShelves()

        override suspend fun books(libraryId: Long, limit: Int) = emptyList<ItemSummary>()

        override suspend fun parts(bookId: Long) = emptyList<ItemSummary>()

        override suspend fun onDeck(bookId: Long): ItemSummary? = null

        override suspend fun recentlyAdded(libraryId: Long) = emptyList<app.pley.android.model.ItemSummary>()

        override suspend fun recentlyPlayed(libraryId: Long) = emptyList<app.pley.android.model.ItemSummary>()

        override suspend fun children(itemId: Long) = emptyList<app.pley.android.model.ItemSummary>()

        override suspend fun item(itemId: Long): app.pley.android.model.ItemSummary? = null

        override suspend fun playlist(playlistId: Long): app.pley.android.model.PlaylistDetail = error("none")

        override suspend fun search(query: String) = emptyList<app.pley.android.model.ItemSummary>()
    }

    private companion object {
        const val NOW = 1_800_000_000L
    }
}
