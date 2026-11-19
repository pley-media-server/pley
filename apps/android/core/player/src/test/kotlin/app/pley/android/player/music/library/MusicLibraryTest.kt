package app.pley.android.player.music.library

import android.graphics.BitmapFactory
import android.graphics.Color
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.session.MediaConstants
import androidx.test.core.app.ApplicationProvider
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistEntry
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistSummary
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

/**
 * The music library as Android Auto browses it: its tabs, the artists,
 * albums and playlists, search, and what playing each node plays.
 */
@RunWith(RobolectricTestRunner::class)
class MusicLibraryTest {
    private val source = FakeSource()
    private val library = MusicLibrary(source, { Look }, { path -> Uri.parse("content://art$path") })

    @Test
    fun idsReadBackWhatTheyWrite() {
        val ids = listOf(
            MusicId.Root,
            MusicId.Recent,
            MusicId.Artists,
            MusicId.Playlists,
            MusicId.LibraryArtists(3),
            MusicId.Artist(40),
            MusicId.ShuffleArtist(40),
            MusicId.Album(50),
            MusicId.Playlist(7),
            MusicId.Track(51),
            MusicId.Track(51, MusicId.Album(50)),
            MusicId.Track(51, MusicId.Playlist(7)),
        )
        for (id in ids) assertEquals(id, MusicId.parse(id.text))
    }

    @Test
    fun idsFromElsewhereAreRefused() {
        for (text in listOf("", "album", "album:", "album:-1", "album:0", "album:1x", "album:99999999999999999999", "track:1@artist:2", "track:1@track:2", "movie:3", "track:1@")) {
            assertNull(text, MusicId.parse(text))
        }
    }

    @Test
    fun theRootHasThreeTabsWithIcons() = runTest {
        val tabs = library.children(MusicId.Root, 0, 10)
        assertEquals(listOf("recent", "artists", "playlists"), tabs.map { it.mediaId })
        assertEquals(listOf("Recent", "Artists", "Playlists"), tabs.map { it.mediaMetadata.title.toString() })
        assertTrue(tabs.all { it.mediaMetadata.isBrowsable == true && it.mediaMetadata.isPlayable == false && it.mediaMetadata.artworkUri != null })
        assertEquals(GRID, tabs[0].browsableStyle)
    }

    @Test
    fun recentListsAlbumsWithTheirArtAndGroupsThemByLibraryWhenThereAreSeveral() = runTest {
        val one = library.children(MusicId.Recent, 0, 50)
        assertEquals(listOf("album:50"), one.map { it.mediaId })
        assertEquals("content://art/api/items/50/images/poster?v=1", one[0].mediaMetadata.artworkUri.toString())
        assertEquals("Harbour Lights", one[0].mediaMetadata.subtitle.toString())
        assertTrue(one[0].mediaMetadata.isPlayable == true && one[0].mediaMetadata.isBrowsable == true)
        assertEquals("Recently added", one[0].group)

        source.libraries += Library(4, "Live recordings", LibraryKind.Music)
        source.recent[4] = listOf(album(60, "Live at Sea"))
        val two = library.children(MusicId.Recent, 0, 50)
        assertEquals(listOf("Recently added in Music", "Recently added in Live recordings"), two.map { it.group })
    }

    @Test
    fun recentStartsWithTheAlbumsPlayedLastOnceEach() = runTest {
        // Newest first: two tracks of Deep Water, a video, then one of Second Tide.
        val other = track(56, 55).copy(parentTitle = "Second Tide")
        source.played[3] = listOf(track(52, 50), track(51, 50), ItemSummary(9, 3, ItemKind.Movie, "A film"), other)
        val recent = library.children(MusicId.Recent, 0, 50)
        assertEquals(listOf("album:50", "album:55", "album:50"), recent.map { it.mediaId })
        assertEquals(listOf("Recently played", "Recently played", "Recently added"), recent.map { it.group })
        assertEquals("Second Tide", recent[1].mediaMetadata.title.toString())
        assertEquals("Harbour Lights", recent[1].mediaMetadata.subtitle.toString())
        assertEquals("content://art/api/items/55/images/poster", recent[1].mediaMetadata.artworkUri.toString())
    }

    @Test
    fun artistsAreListedByPagesOrByLibraryWithSeveral() = runTest {
        val page = library.children(MusicId.Artists, 1, 2)
        assertEquals(listOf(2 to 2), source.artistPages.map { it.second to it.third })
        assertEquals(listOf("artist:40"), page.map { it.mediaId })
        assertEquals("2 albums", page[0].mediaMetadata.subtitle.toString())

        source.libraries += Library(4, "Live recordings", LibraryKind.Music)
        val libraries = library.children(MusicId.Artists, 0, 50)
        assertEquals(listOf("artists:3", "artists:4"), libraries.map { it.mediaId })
        assertEquals(listOf("artist:40"), library.children(MusicId.LibraryArtists(4), 0, 50).map { it.mediaId })
    }

    @Test
    fun anArtistHasShuffleThenTheirAlbumsNewestFirst() = runTest {
        val children = library.children(MusicId.Artist(40), 0, 50)
        assertEquals(listOf("shuffle:40", "album:55", "album:50"), children.map { it.mediaId })
        assertEquals("Shuffle", children[0].mediaMetadata.title.toString())
        assertTrue(children[0].mediaMetadata.isPlayable == true)
        assertFalse(children[0].mediaMetadata.isBrowsable == true)
    }

    @Test
    fun anAlbumsTracksPlayOnThroughTheAlbum() = runTest {
        val tracks = library.children(MusicId.Album(50), 0, 50)
        assertEquals(listOf("track:51@album:50", "track:52@album:50"), tracks.map { it.mediaId })
        assertEquals("Harbour Lights", tracks[0].mediaMetadata.artist.toString())
        // In its album's list a track shows its length, under no cover; elsewhere its artist and its album's cover.
        assertEquals("3:00", tracks[0].mediaMetadata.subtitle.toString())
        assertNull(tracks[1].mediaMetadata.artworkUri)
        val inPlaylist = library.children(MusicId.Playlist(7), 0, 50)
        assertEquals("Harbour Lights", inPlaylist[0].mediaMetadata.subtitle.toString())
        assertEquals("content://art/api/items/50/images/poster", inPlaylist[0].mediaMetadata.artworkUri.toString())

        val selection = library.selection(MusicId.parse(tracks[1].mediaId)!!)!!
        assertEquals(listOf(51L, 52L), selection.tracks.map { it.id })
        assertEquals(1, selection.startIndex)
        assertFalse(selection.shuffle)
    }

    @Test
    fun playlistsAreMusicOnesAndPlayTheirTracks() = runTest {
        val playlists = library.children(MusicId.Playlists, 0, 50)
        assertEquals(listOf("playlist:7"), playlists.map { it.mediaId })
        assertEquals("2 tracks", playlists[0].mediaMetadata.subtitle.toString())
        assertEquals(listOf("track:52@playlist:7", "track:51@playlist:7"), library.children(MusicId.Playlist(7), 0, 50).map { it.mediaId })
        assertEquals(listOf(52L, 51L), library.selection(MusicId.Playlist(7))!!.tracks.map { it.id })
    }

    @Test
    fun shuffleAnArtistPlaysAllTheirTracksShuffled() = runTest {
        val selection = library.selection(MusicId.ShuffleArtist(40))!!
        assertTrue(selection.shuffle)
        assertEquals(setOf(51L, 52L, 56L), selection.tracks.map { it.id }.toSet())
    }

    @Test
    fun aTrackFoundAlonePlaysOnThroughItsAlbum() = runTest {
        val selection = library.selection(MusicId.Track(52))!!
        assertEquals(listOf(51L, 52L), selection.tracks.map { it.id })
        assertEquals(1, selection.startIndex)
    }

    @Test
    fun searchGroupsArtistsAlbumsAndTracks() = runTest {
        val found = library.search("harbour")
        assertEquals(listOf("artist:40", "album:50", "album:55", "track:51"), found.map { it.mediaId })
        assertEquals(listOf("Artists", "Albums", "Albums", "Tracks"), found.map { it.group })
    }

    @Test
    fun aSearchOfBlanksListsTheAlbumsPlayedAndAddedLastWithoutAskingTheServer() = runTest {
        // Media3 answers an empty query itself; one of blanks reaches the library.
        source.found = emptyList()
        source.played[3] = listOf(track(56, 55).copy(parentTitle = "Second Tide"), track(51, 50))
        source.recent[3] = listOf(album(50, "Deep Water"), album(60, "Live at Sea"))
        val suggested = library.search("  ")
        // Each album once, where it comes first: Deep Water was played, Live at Sea only added.
        assertEquals(listOf("album:55", "album:50", "album:60"), suggested.map { it.mediaId })
        assertEquals(listOf("Recently played", "Recently played", "Recently added"), suggested.map { it.group })
        assertEquals(emptyList<String>(), source.searched)
    }

    @Test
    fun aSpokenRequestPlaysTheBestMatchAnArtistShuffled() = runTest {
        val artist = library.selectionFor("harbour")!!
        assertTrue(artist.shuffle)

        source.found = listOf(track(51, 50))
        assertEquals(listOf(51L, 52L), library.selectionFor("first")!!.tracks.map { it.id })

        // Nothing said: the album added last.
        assertEquals(listOf(51L, 52L), library.selectionFor("")!!.tracks.map { it.id })

        source.found = emptyList()
        assertNull(library.selectionFor("nothing like it"))
        // A music playlist by its name, as said ("play my Night Drive playlist"), or by a part of it when nothing
        // else is found; never a video one.
        val named = library.selectionFor(" night DRIVE ")!!
        assertEquals(library.selection(MusicId.Playlist(7))!!.tracks.map { it.id }, named.tracks.map { it.id })
        assertEquals(named.tracks.map { it.id }, library.selectionFor("night")!!.tracks.map { it.id })
        assertNull(library.selectionFor("Trailers"))
        // Said with its word for a playlist, in each language, or named by the Assistant alone.
        for (said in listOf("night drive playlist", "my Night Drive playlist", "la playlist Night Drive", "mi lista de reproducción night drive")) {
            assertEquals(said, named.tracks.map { it.id }, library.selectionFor(said)?.tracks?.map { it.id })
        }
        assertEquals(named.tracks.map { it.id }, library.selectionFor("", playlist = "Night Drive")!!.tracks.map { it.id })
    }

    @Test
    fun aPlaylistSaidIsItsNameAlone() {
        assertEquals("Night Drive", playlistNamed("my Night Drive playlist"))
        assertEquals("Night Drive", playlistNamed("Playlist Night Drive"))
        assertEquals("road trip", playlistNamed("ma liste de lecture road trip"))
        assertEquals("road trip", playlistNamed("mis lista road trip"))
        // Not a playlist said: nothing to take from it, a name with the word inside it included.
        assertNull(playlistNamed("Night Drive"))
        assertNull(playlistNamed("Setlists"))
        assertNull(playlistNamed("playlist"))
    }

    @Test
    fun picturesAreOnlyTheServersItemPictures() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val uri = MusicArt.uri(context, "/api/items/50/images/poster?v=17")!!
        assertEquals("/api/items/50/images/poster?v=17", MusicArt.pathOf(context, uri))
        for (path in listOf("/api/auth/me", "https://elsewhere.example/api/items/1/images/poster", "/api/items/1/images/../../users", "/api/items/1/images/poster?w=1&x=/../")) {
            assertNull(path, MusicArt.uri(context, path))
        }
        assertNull(MusicArt.pathOf(context, Uri.parse("content://${MusicArt.authority(context)}/picture?p=/api/auth/me")))
        assertNull(MusicArt.pathOf(context, Uri.parse("content://elsewhere/picture?p=/api/items/1/images/poster")))
    }

    @Test
    fun whatHasNoPictureHasItsTitlesPlaceholderAndWhatHasOneKeepsIt() = runTest {
        val covered = MusicLibrary(source, { Look }, { path -> Uri.parse("content://art$path") }) { seed -> Uri.parse("content://placeholder/$seed") }
        val playlists = covered.children(MusicId.Playlists, 0, 50)
        assertEquals("content://placeholder/Night drive", playlists.single().mediaMetadata.artworkUri.toString())
        val albums = covered.children(MusicId.Artist(40), 0, 50).filter { it.mediaId.startsWith("album:") }
        assertTrue(albums.all { it.mediaMetadata.artworkUri.toString().startsWith("content://art/api/items/") })
        // The tabs keep their icons, and the library without placeholders shows none for what has no picture.
        assertTrue(covered.children(MusicId.Root, 0, 50).none { it.mediaMetadata.artworkUri.toString().startsWith("content://placeholder") })
        assertNull(library.children(MusicId.Playlists, 0, 50).single().mediaMetadata.artworkUri)
    }

    @Test
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun aCoverForWhatHasNoneIsDrawnFromItsTitlesHueAlone() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val uri = MusicArt.placeholderUri(context, "Deep Water")
        // The title isn't in the address the car is given: its hue is.
        assertFalse(uri.toString().contains("Deep"))
        assertEquals(PlaceholderCover.hue("Deep Water"), MusicArt.hueOf(context, uri))
        assertNull(MusicArt.pathOf(context, uri))
        assertNull(MusicArt.hueOf(context, MusicArt.uri(context, "/api/items/50/images/poster")!!))
        assertNull(MusicArt.hueOf(context, Uri.parse("content://${MusicArt.authority(context)}/placeholder?h=400")))
        // The web's hue for a title, which the app's placeholder draws too.
        assertEquals(0, PlaceholderCover.hue(""))
        assertEquals(97 % 360, PlaceholderCover.hue("a"))

        val png = PlaceholderCover.png(context, MusicArt.hueOf(context, uri)!!)
        val cover = BitmapFactory.decodeByteArray(png, 0, png.size)
        assertEquals(PlaceholderCover.SIZE, cover.width)
        assertEquals(PlaceholderCover.SIZE, cover.height)
        // Opaque all over: the gradient under the mark.
        val middle = cover.getPixel(PlaceholderCover.SIZE / 2, PlaceholderCover.SIZE / 2)
        assertTrue(Color.alpha(middle) == 255)
    }

    private val MediaItem.browsableStyle: Int? get() = mediaMetadata.extras?.takeIf { it.containsKey(BROWSABLE) }?.getInt(BROWSABLE)

    private val MediaItem.group: String? get() = mediaMetadata.extras?.getString(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_GROUP_TITLE)

    private object Look : MusicLibraryLook {
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

        override val recentIcon: Uri = Uri.parse("android.resource://pley/drawable/recent")
        override val artistsIcon: Uri = Uri.parse("android.resource://pley/drawable/artists")
        override val playlistsIcon: Uri = Uri.parse("android.resource://pley/drawable/playlists")
        override val shuffleIcon: Uri = Uri.parse("android.resource://pley/drawable/shuffle")
    }

    private class FakeSource : MusicSource {
        val libraries = mutableListOf(Library(3, "Music", LibraryKind.Music), Library(9, "Films", LibraryKind.Movies))
        val recent = mutableMapOf(3L to listOf(album(50, "Deep Water"), track(51, 50)))
        val artistPages = mutableListOf<Triple<Long, Int, Int>>()
        val played = mutableMapOf<Long, List<ItemSummary>>()
        private val artist = ItemSummary(40, 3, ItemKind.Artist, "Harbour Lights", posterUrl = "/api/items/40/images/poster", childCount = 2)
        private val albums = listOf(album(50, "Deep Water", year = 2019), album(55, "Second Tide", year = 2023))
        private val tracks = mapOf(50L to listOf(track(51, 50), track(52, 50)), 55L to listOf(track(56, 55)))
        var found: List<ItemSummary> = listOf(artist, albums[0], albums[1], track(51, 50))

        override suspend fun libraries() = libraries.filter { it.kind == LibraryKind.Music }

        override suspend fun recentlyAdded(libraryId: Long) = recent[libraryId].orEmpty()

        override suspend fun recentlyPlayed(libraryId: Long) = played[libraryId].orEmpty()

        override suspend fun artists(libraryId: Long, offset: Int, limit: Int): List<ItemSummary> {
            artistPages += Triple(libraryId, offset, limit)
            return listOf(artist)
        }

        override suspend fun children(itemId: Long): List<ItemSummary> = when (itemId) {
            40L -> albums
            else -> tracks[itemId].orEmpty()
        }

        override suspend fun item(itemId: Long): ItemSummary? = (listOf(artist) + albums + tracks.values.flatten()).firstOrNull { it.id == itemId }

        override suspend fun playlists() = listOf(
            PlaylistSummary(7, "Night drive", PlaylistKind.Audio, itemCount = 2),
            PlaylistSummary(8, "Trailers", PlaylistKind.Video, itemCount = 3),
        )

        override suspend fun playlist(playlistId: Long) = PlaylistDetail(
            id = playlistId,
            title = "Night drive",
            kind = PlaylistKind.Audio,
            entries = listOf(PlaylistEntry(1, track(52, 50)), PlaylistEntry(2, track(51, 50))),
        )

        val searched = mutableListOf<String>()

        override suspend fun search(query: String): List<ItemSummary> {
            searched += query
            return found
        }

        override fun pictureUrl(path: String, width: Int) = "http://server$path"
    }

    private companion object {
        val GRID = MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_GRID_ITEM
        val BROWSABLE = MediaConstants.EXTRAS_KEY_CONTENT_STYLE_BROWSABLE

        fun album(id: Long, title: String, year: Int? = null) = ItemSummary(
            id,
            3,
            ItemKind.Album,
            title,
            year = year,
            parentId = 40,
            parentTitle = "Harbour Lights",
            posterUrl = "/api/items/$id/images/poster?v=1",
        )

        fun track(id: Long, album: Long) = ItemSummary(
            id,
            3,
            ItemKind.Track,
            "Track $id",
            index = (id % 10).toInt(),
            parentId = album,
            parentTitle = "Deep Water",
            grandparentId = 40,
            grandparentTitle = "Harbour Lights",
            durationMs = 180_000,
        )
    }
}
