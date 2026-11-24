package app.pley.android.player.music.library

import android.graphics.BitmapFactory
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.provider.MediaStore
import androidx.core.graphics.ColorUtils
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
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
import app.pley.android.model.UserState
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
    private val library = MusicLibrary(source, { Look }, { path, _ -> Uri.parse("content://art$path") })

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
        // What opens is a grid by the root's default: only the artists' tab says it lists them.
        assertNull(tabs[0].browsableStyle)
        assertEquals(MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM, tabs[1].browsableStyle)
    }

    @Test
    fun everyListShowsItsRowsAsPinnedHere() = runTest {
        // Something of every kind: books, an album and a playlist of one track (they play rather than open).
        source.bookLibraries = listOf(BOOKS)
        source.recent[3] = listOf(album(50, "Deep Water"), album(60, "One Song").copy(leafCount = 1))
        source.albums = source.albums + album(61, "A Single", year = 2024).copy(leafCount = 1)
        source.audioPlaylists = listOf(PlaylistSummary(7, "Night drive", PlaylistKind.Audio, itemCount = 2), PlaylistSummary(9, "One", PlaylistKind.Audio, itemCount = 1))
        source.found = listOf(ARTIST, album(50, "Deep Water"), track(51, 50), LONG_BOOK)

        val shown = linkedMapOf<String, String>()
        fun kind(row: MediaItem): String {
            val kind = row.mediaId.substringBefore(':')
            return if (kind in setOf("album", "playlist") && row.mediaMetadata.isPlayable == true) "single $kind" else kind
        }
        fun note(where: String, parent: MediaItem?, rows: List<MediaItem>) {
            for (row in rows) shown["$where > ${kind(row)}"] = styleName(shownAs(parent, row))
        }
        val tabs = library.children(MusicId.Root, 0, 10).associateBy { it.mediaId }
        note("recent", tabs["recent"], library.children(MusicId.Recent, 0, 50))
        val artists = library.children(MusicId.Artists, 0, 50)
        note("artists", tabs["artists"], artists)
        val artist = artists.first { it.mediaId == "artist:40" }
        note("artist", artist, library.children(MusicId.Artist(40), 0, 50))
        note("album", library.children(MusicId.Artist(40), 0, 50).first { it.mediaId == "album:50" }, library.children(MusicId.Album(50), 0, 50))
        val playlists = library.children(MusicId.Playlists, 0, 50)
        note("playlists", tabs["playlists"], playlists)
        note("playlist", playlists.first(), library.children(MusicId.Playlist(7), 0, 50))
        note("books", tabs["books"], library.children(MusicId.Books, 0, 50))
        note("search", null, library.search("harbour"))
        source.libraries += Library(4, "Live recordings", LibraryKind.Music)
        val libraries = library.children(MusicId.Artists, 0, 50)
        note("libraries", library.children(MusicId.Root, 0, 10).first { it.mediaId == "artists" }, libraries)
        note("library", libraries.first(), library.children(MusicId.LibraryArtists(3), 0, 50))

        assertEquals(
            mapOf(
                "recent > album" to "grid",
                "recent > single album" to "grid",
                "recent > book" to "grid",
                "artists > artist" to "list",
                "artist > shuffle" to "grid",
                "artist > album" to "grid",
                "artist > single album" to "grid",
                "album > track" to "list",
                "playlists > playlist" to "grid",
                "playlists > single playlist" to "grid",
                "playlist > track" to "list",
                "books > book" to "grid",
                "search > artist" to "list",
                "search > album" to "list",
                "search > track" to "list",
                "search > book" to "list",
                "libraries > artists" to "list",
                "library > artist" to "list",
            ),
            shown,
        )
    }

    @Test
    fun tabsWithNothingInThemAreLeftOut() = runTest {
        // No music in the library, no music playlist, no books: no tab at all.
        source.libraries[0] = Library(3, "Music", LibraryKind.Music, itemCount = 0)
        source.audioPlaylists = emptyList()
        assertEquals(emptyList<String>(), library.children(MusicId.Root, 0, 10).map { it.mediaId })

        // Books alone: their tab, and Recent with the one being heard; with none being heard, no Recent.
        source.bookLibraries = listOf(BOOKS, Library(6, "Empty", LibraryKind.Audiobooks, itemCount = 0))
        assertEquals(listOf("recent", "books"), library.children(MusicId.Root, 0, 10).map { it.mediaId })
        source.shelves = BookShelves(added = listOf(SHORT_BOOK))
        assertEquals(listOf("books"), library.children(MusicId.Root, 0, 10).map { it.mediaId })

        // What can't be read now is shown, as the server out of reach would be: not books, which it may not give.
        val failing = MusicLibrary(object : MusicSource by source {
            override suspend fun libraries(): List<Library> = error("Out of reach")
            override suspend fun playlists(): List<PlaylistSummary> = error("Out of reach")
            override suspend fun bookLibraries(): List<Library> = error("Out of reach")
        }, { Look }, { _, _ -> null })
        assertEquals(listOf("recent", "artists", "playlists"), failing.children(MusicId.Root, 0, 10).map { it.mediaId })
    }

    @Test
    fun booksHaveTheirTabAndGoOnWhereTheProfileIsInThem() = runTest {
        source.bookLibraries = listOf(BOOKS)
        assertEquals(listOf("recent", "artists", "playlists", "books"), library.children(MusicId.Root, 0, 10).map { it.mediaId })

        // Being heard, added last, then every one by title.
        val books = library.children(MusicId.Books, 0, 50)
        assertEquals(listOf("book:70", "book:80", "book:70", "book:70", "book:80"), books.map { it.mediaId })
        assertEquals(listOf("Continue listening", "Recently added", "Recently added", "All books", "All books"), books.map { it.group })
        // A book plays as it is picked, never opens, and says how far the profile is: an hour and a half of two.
        val heard = books[0].mediaMetadata
        assertTrue(heard.isPlayable == true && heard.isBrowsable == false)
        assertEquals(MediaMetadata.MEDIA_TYPE_AUDIO_BOOK, heard.mediaType)
        // Under Continue listening, how long is left alone; elsewhere its authors beside it.
        assertEquals("30 min left", heard.subtitle.toString())
        assertEquals("A Writer · 30 min left", books[2].mediaMetadata.subtitle.toString())
        assertEquals(MediaConstants.EXTRAS_VALUE_COMPLETION_STATUS_PARTIALLY_PLAYED, heard.extras!!.getInt(MediaConstants.EXTRAS_KEY_COMPLETION_STATUS))
        assertEquals(0.75, heard.extras!!.getDouble(MediaConstants.EXTRAS_KEY_COMPLETION_PERCENTAGE), 0.001)
        // One not started says nothing of it: the car would mark it.
        val fresh = books[1].mediaMetadata
        assertEquals("A Writer, Another", fresh.subtitle.toString())
        assertFalse(fresh.extras?.containsKey(MediaConstants.EXTRAS_KEY_COMPLETION_STATUS) == true)
        assertEquals(listOf("book:70", "album:50"), library.children(MusicId.Recent, 0, 50).map { it.mediaId })

        // Played: from the part it was in, where it was.
        val selection = library.selection(MusicId.Book(70))!!
        assertEquals(listOf(71L, 72L), selection.tracks.map { it.id })
        assertEquals(1, selection.startIndex)
        assertEquals(1_800_000L, selection.startMs)
        assertFalse(selection.shuffle)
        // A part in the queue is under its book's title.
        val queued = library.queueItem(selection.tracks[1]).mediaMetadata
        assertEquals("Long Book", queued.artist.toString())
        assertEquals(MediaMetadata.MEDIA_TYPE_AUDIO_BOOK_CHAPTER, queued.mediaType)
        assertEquals("track:72", library.item(MusicId.Track(72))?.mediaId)

        // Found by search, and said: it goes on where it was.
        source.found = listOf(LONG_BOOK)
        val found = library.search("long")
        assertEquals(listOf("book:70"), found.map { it.mediaId })
        assertEquals("Books", found[0].group)
        assertEquals(1_800_000L, library.selectionFor("long book")!!.startMs)
    }

    @Test
    fun howFarABookIsHeardComesFromItsParts() {
        val parts = listOf(part(71, 70, positionMs = 0, watched = true), part(72, 70, positionMs = 900_000), part(73, 70, positionMs = 0))
        assertEquals(Heard(4_500_000, 10_800_000), heardIn(parts))
        // A place past a part's end counts as its end; parts without lengths say nothing.
        assertEquals(Heard(3_600_000, 3_600_000), heardIn(listOf(part(72, 70, positionMs = 9_000_000))))
        assertNull(heardIn(listOf(part(74, 70, positionMs = 0).copy(durationMs = null))))
    }

    @Test
    fun recentListsAlbumsWithTheirArtAndGroupsThemByLibraryWhenThereAreSeveral() = runTest {
        val one = library.children(MusicId.Recent, 0, 50)
        assertEquals(listOf("album:50"), one.map { it.mediaId })
        assertEquals("content://art/api/items/50/images/poster?v=1", one[0].mediaMetadata.artworkUri.toString())
        assertEquals("Harbour Lights", one[0].mediaMetadata.subtitle.toString())
        assertTrue(one[0].mediaMetadata.isPlayable == false && one[0].mediaMetadata.isBrowsable == true)
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
        // Deep Water, added last too, isn't listed again under Recently added.
        assertEquals(listOf("album:50", "album:55"), recent.map { it.mediaId })
        assertEquals(listOf("Recently played", "Recently played"), recent.map { it.group })
        assertEquals("Second Tide", recent[1].mediaMetadata.title.toString())
        assertEquals("Harbour Lights", recent[1].mediaMetadata.subtitle.toString())
        assertEquals("content://art/api/items/55/images/poster", recent[1].mediaMetadata.artworkUri.toString())
    }

    @Test
    fun recentIsAGridOfAFewBooksAndTheAlbumsAddedLast() = runTest {
        // What plays as it is picked (a book, an album of one track) is a tile of Recent's and Playlists' grids too.
        val tabs = library.children(MusicId.Root, 0, 10)
        assertEquals(GRID, tabs.first { it.mediaId == "recent" }.playableStyle)
        assertEquals(GRID, tabs.first { it.mediaId == "playlists" }.playableStyle)

        // Five books being heard: Recent shows three, the Books tab all of them.
        val many = (1..5L).map { LONG_BOOK.copy(id = 100 + it) }
        source.bookLibraries = listOf(BOOKS)
        source.shelves = BookShelves(listening = many)
        assertEquals(3, library.children(MusicId.Recent, 0, 50).count { it.mediaId.startsWith("book:") })
        assertEquals(5, library.children(MusicId.Books, 0, 50).count { it.group == "Continue listening" })

        // Twenty albums added: twelve.
        source.recent[3] = (1..20L).map { album(200 + it, "Album $it") }
        assertEquals(12, library.children(MusicId.Recent, 0, 50).count { it.group == "Recently added" })
    }

    @Test
    fun recentPutsTheMusicFirstWhenATrackWasHeardAfterTheBooks() = runTest {
        source.bookLibraries = listOf(BOOKS)
        // The book's part heard last was heard at 2 000: before the track, the book leads; after it, the album.
        source.played[3] = listOf(track(52, 50).copy(userState = UserState(lastPlayedAt = 1_500)))
        assertEquals(listOf("book:70", "album:50"), library.children(MusicId.Recent, 0, 50).map { it.mediaId })
        source.played[3] = listOf(track(52, 50).copy(userState = UserState(lastPlayedAt = 3_000)))
        assertEquals(listOf("album:50", "book:70"), library.children(MusicId.Recent, 0, 50).map { it.mediaId })
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
        // The page names the artist: each album's tile says its year alone, a tile's line having room for little.
        assertEquals(listOf("2023", "2019"), children.drop(1).map { it.mediaMetadata.subtitle.toString() })
        // Shuffle says how many tracks it plays once every album's are counted; it is a tile of the albums' grid.
        assertNull(children[0].mediaMetadata.subtitle)
        assertEquals(GRID, children[0].mediaMetadata.extras!!.getInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_SINGLE_ITEM))
        assertEquals("android.resource://pley/drawable/shuffle", children[0].mediaMetadata.artworkUri.toString())
        source.albums = source.albums.map { it.copy(leafCount = 7) }
        val tiled = MusicLibrary(source, { Look }, { path, _ -> Uri.parse("content://art$path") }, shuffleArt = { Uri.parse("content://shuffle/$it") })
        val shuffle = tiled.children(MusicId.Artist(40), 0, 50)[0].mediaMetadata
        assertEquals("14 tracks", shuffle.subtitle.toString())
        // Its tile is of the artist's hue, from their name.
        assertEquals("content://shuffle/Harbour Lights", shuffle.artworkUri.toString())
        // Elsewhere an album says its artist, and in search its year too.
        assertEquals("Harbour Lights", library.children(MusicId.Recent, 0, 50).first { it.mediaId == "album:50" }.mediaMetadata.subtitle.toString())
        assertEquals("Harbour Lights · 2019", library.search("deep").first { it.mediaId == "album:50" }.mediaMetadata.subtitle.toString())
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
        // Numbered in its album's list, as the album's page lists it; by its title alone elsewhere and in the queue.
        assertEquals(listOf("1. Track 51", "2. Track 52"), tracks.map { it.mediaMetadata.displayTitle.toString() })
        assertEquals("Track 51", tracks[0].mediaMetadata.title.toString())
        assertEquals("1. Track 51", library.item(MusicId.Track(51, MusicId.Album(50)))?.mediaMetadata?.displayTitle.toString())
        assertEquals("Track 52", inPlaylist[0].mediaMetadata.displayTitle.toString())
        assertEquals("Track 51", library.queueItem(track(51, 50)).mediaMetadata.displayTitle.toString())
        // One disc: no headings.
        assertTrue(tracks.all { it.group == null })

        val selection = library.selection(MusicId.parse(tracks[1].mediaId)!!)!!
        assertEquals(listOf(51L, 52L), selection.tracks.map { it.id })
        assertEquals(1, selection.startIndex)
        assertFalse(selection.shuffle)
    }

    @Test
    fun anAlbumOfSeveralDiscsListsEachUnderItsHeading() = runTest {
        val tracks = library.children(MusicId.Album(57), 0, 50)
        assertEquals(listOf("Disc 1", "Disc 1", "Disc 2"), tracks.map { it.group })
        // Numbered on each disc, as the album's page numbers them; one without a number has its title alone.
        assertEquals(listOf("1. Track 58", "Track 59", "1. Track 60"), tracks.map { it.mediaMetadata.displayTitle.toString() })
    }

    @Test
    fun albumsAndPlaylistsOpenToTheirTracksUnlessTheyHaveJustOne() = runTest {
        // A car plays what can play rather than open it: an album or playlist of several tracks only opens.
        val opens = library.children(MusicId.Artist(40), 0, 50).filter { it.mediaId.startsWith("album:") } +
            library.children(MusicId.Playlists, 0, 50)
        assertTrue(opens.all { it.mediaMetadata.isBrowsable == true && it.mediaMetadata.isPlayable == false })

        // One of a single track plays as it is picked; one whose tracks aren't counted opens.
        source.recent[3] = listOf(album(60, "One Song").copy(leafCount = 1), album(61, "Uncounted"))
        val recent = library.children(MusicId.Recent, 0, 50)
        assertTrue(recent[0].mediaMetadata.isPlayable == true && recent[0].mediaMetadata.isBrowsable == false)
        assertTrue(recent[1].mediaMetadata.isPlayable == false && recent[1].mediaMetadata.isBrowsable == true)
        source.audioPlaylists = listOf(PlaylistSummary(7, "Night drive", PlaylistKind.Audio, itemCount = 1))
        val single = library.children(MusicId.Playlists, 0, 50).single()
        assertTrue(single.mediaMetadata.isPlayable == true && single.mediaMetadata.isBrowsable == false)
    }

    @Test
    fun playlistsAreMusicOnesAndPlayTheirTracks() = runTest {
        val playlists = library.children(MusicId.Playlists, 0, 50)
        assertEquals(listOf("playlist:7"), playlists.map { it.mediaId })
        assertEquals("2 tracks", playlists[0].mediaMetadata.subtitle.toString())
        assertEquals(listOf("track:52@playlist:7", "track:51@playlist:7"), library.children(MusicId.Playlist(7), 0, 50).map { it.mediaId })
        assertEquals(listOf(52L, 51L), library.selection(MusicId.Playlist(7))!!.tracks.map { it.id })
        // The queue is named for the playlist, picked whole or from one of its tracks.
        assertEquals("Night drive", library.selection(MusicId.Playlist(7))!!.title)
        assertEquals("Night drive", library.selection(MusicId.Track(51, MusicId.Playlist(7)))!!.title)
        assertNull(library.selection(MusicId.Track(51, MusicId.Album(50)))!!.title)
    }

    @Test
    fun shuffleAnArtistPlaysAllTheirTracksShuffled() = runTest {
        val selection = library.selection(MusicId.ShuffleArtist(40))!!
        assertTrue(selection.shuffle)
        assertEquals("Harbour Lights", selection.title)
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
    fun searchPutsTheGroupOfTheClosestMatchFirst() = runTest {
        // The server found a track closest, then an album, then the artist: the tracks lead, then the albums.
        source.found = listOf(track(51, 50), album(55, "Second Tide"), ItemSummary(40, 3, ItemKind.Artist, "Harbour Lights"), track(52, 50))
        val found = library.search("tide")
        assertEquals(listOf("track:51", "track:52", "album:55", "artist:40"), found.map { it.mediaId })
        assertEquals(listOf("Tracks", "Tracks", "Albums", "Artists"), found.map { it.group })
    }

    @Test
    fun searchShowsAFewOfEachKindItsResultsReadAlone() = runTest {
        // Ten tracks found closest, then ten albums: six tracks, four albums.
        source.found = (1..10L).map { track(100 + it, 50) } + (1..10L).map { album(200 + it, "Album $it", year = 2023) }
        val found = library.search("deep")
        assertEquals(6, found.count { it.mediaId.startsWith("track:") })
        assertEquals(4, found.count { it.mediaId.startsWith("album:") })
        // A track says its artist and album, an album its artist and year; the queue's track its artist alone.
        assertEquals("Harbour Lights · Deep Water", found.first().mediaMetadata.subtitle.toString())
        assertEquals("Harbour Lights · 2023", found.first { it.mediaId.startsWith("album:") }.mediaMetadata.subtitle.toString())
        assertEquals("Harbour Lights", library.queueItem(track(101, 50)).mediaMetadata.subtitle.toString())
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
    fun aSpokenRequestForAnArtistAnAlbumOrATrackPlaysOneOfThatKind() = runTest {
        // The server's closest match is an album; then a track, then the artist.
        source.found = listOf(album(55, "Second Tide"), track(52, 50), ARTIST)
        // Said alone: the closest, the album.
        assertEquals(listOf(56L), library.selectionFor("harbour")!!.tracks.map { it.id })
        // Asked for as an artist: the artist, shuffled, by the name heard.
        val artist = library.selectionFor("harbour", focus = SpokenFocus(ItemKind.Artist, artist = "Harbour Lights"))!!
        assertTrue(artist.shuffle)
        assertEquals("Harbour Lights", source.searched.last())
        // As a track: its album from it.
        val song = library.selectionFor("track 52", focus = SpokenFocus(ItemKind.Track, title = "Track 52"))!!
        assertEquals(listOf(51L, 52L), song.tracks.map { it.id })
        assertEquals(1, song.startIndex)
        assertEquals("Track 52", source.searched.last())

        // As an album, before a playlist of that name ("play the album Night Drive").
        source.found = listOf(track(52, 50), album(50, "Night Drive"))
        assertEquals(listOf(51L, 52L), library.selectionFor("night drive", focus = SpokenFocus(ItemKind.Album, album = "Night Drive"))!!.tracks.map { it.id })
        assertEquals(0, library.selectionFor("night drive", focus = SpokenFocus(ItemKind.Album))!!.startIndex)
        assertEquals(library.selection(MusicId.Playlist(7))!!.tracks.map { it.id }, library.selectionFor("night drive")!!.tracks.map { it.id })

        // By the artist heard, when albums of several artists have the name.
        source.found = listOf(album(60, "Greatest Hits").copy(parentTitle = "Someone Else"), album(55, "Greatest Hits"))
        val hits = library.selectionFor("greatest hits by harbour lights", focus = SpokenFocus(ItemKind.Album, artist = "harbour lights", album = "Greatest Hits"))!!
        assertEquals(listOf(56L), hits.tracks.map { it.id })

        // None of that kind: read as any other request.
        source.found = listOf(track(52, 50))
        assertEquals(listOf(51L, 52L), library.selectionFor("anything", focus = SpokenFocus(ItemKind.Album, album = "Missing"))!!.tracks.map { it.id })
    }

    @Test
    fun theAssistantsExtrasSayWhatIsAskedFor() {
        fun of(vararg extras: Pair<String, String>) = SpokenFocus.of(Bundle().apply { for ((k, v) in extras) putString(k, v) })
        assertEquals(
            SpokenFocus(ItemKind.Album, artist = "Harbour Lights", album = "Deep Water"),
            of(MediaStore.EXTRA_MEDIA_FOCUS to MediaStore.Audio.Albums.ENTRY_CONTENT_TYPE, MediaStore.EXTRA_MEDIA_ALBUM to " Deep Water ", MediaStore.EXTRA_MEDIA_ARTIST to "Harbour Lights"),
        )
        assertEquals(ItemKind.Artist, of(MediaStore.EXTRA_MEDIA_FOCUS to MediaStore.Audio.Artists.ENTRY_CONTENT_TYPE).kind)
        assertEquals(ItemKind.Track, of(MediaStore.EXTRA_MEDIA_FOCUS to MediaStore.Audio.Media.ENTRY_CONTENT_TYPE, MediaStore.EXTRA_MEDIA_TITLE to "First Light").kind)
        // Without a focus, or one for anything, the names heard say it.
        assertEquals(ItemKind.Track, of(MediaStore.EXTRA_MEDIA_TITLE to "First Light", MediaStore.EXTRA_MEDIA_ARTIST to "Harbour Lights").kind)
        assertEquals(ItemKind.Album, of(MediaStore.EXTRA_MEDIA_FOCUS to "vnd.android.cursor.item/*", MediaStore.EXTRA_MEDIA_ALBUM to "Deep Water").kind)
        // A playlist's is the playlist's, a genre's nothing of these; nothing heard, nothing asked.
        assertNull(of(MediaStore.EXTRA_MEDIA_FOCUS to MediaStore.Audio.Playlists.ENTRY_CONTENT_TYPE, MediaStore.EXTRA_MEDIA_TITLE to "Night Drive").kind)
        assertNull(of(MediaStore.EXTRA_MEDIA_FOCUS to "vnd.android.cursor.item/genre", MediaStore.EXTRA_MEDIA_ARTIST to " ").kind)
        assertTrue(SpokenFocus.of(null).isEmpty)
    }

    @Test
    fun aCarIsSuggestedTheAlbumPlayedLastFromItsTrackElseTheOneAddedLast() = runTest {
        // Nothing played: the album added last, from its start.
        val added = library.suggestion()!!
        assertEquals(listOf(51L, 52L), added.tracks.map { it.id })
        assertEquals(0, added.startIndex)

        // Newest first: a film, then the second track of Deep Water.
        source.played[3] = listOf(ItemSummary(9, 3, ItemKind.Movie, "A film"), track(52, 50), track(56, 55))
        val played = library.suggestion()!!
        assertEquals(listOf(51L, 52L), played.tracks.map { it.id })
        assertEquals(1, played.startIndex)
        assertFalse(played.shuffle)

        // Plays that can't be read: the album added last.
        val failing = MusicLibrary(object : MusicSource by source {
            override suspend fun recentlyPlayed(libraryId: Long): List<ItemSummary> = error("Out of reach")
        }, { Look }, { _, _ -> null })
        assertEquals(0, failing.suggestion()!!.startIndex)
    }

    @Test
    fun aCarIsSuggestedTheBookHeardLastWhenItWasHeardAfterTheMusic() = runTest {
        source.bookLibraries = listOf(Library(5, "Books", LibraryKind.Audiobooks, itemCount = 2))
        // The book's part heard last was heard after the track: the book, where the profile is in it.
        source.played[3] = listOf(track(52, 50).copy(userState = UserState(lastPlayedAt = 1_500)))
        val book = library.suggestion()!!
        assertEquals(listOf(71L, 72L), book.tracks.map { it.id })
        assertEquals(1, book.startIndex)
        assertEquals(1_800_000L, book.startMs)

        // Music played since: its album, from the track.
        source.played[3] = listOf(track(52, 50).copy(userState = UserState(lastPlayedAt = 3_000)))
        val album = library.suggestion()!!
        assertEquals(listOf(51L, 52L), album.tracks.map { it.id })
        assertEquals(1, album.startIndex)
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
    fun aPicturesAddressIsShortAndReadBackStrictly() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val authority = MusicArt.authority(context)
        // The server's usual forms go as a few segments, read back to the same path.
        val forms = mapOf(
            "/api/items/50/images/poster?v=1763856000" to "i/50/poster/1763856000",
            "/api/items/50/images/backdrop" to "i/50/backdrop",
            "/api/collections/7/images/poster?v=-3" to "c/7/poster/-3",
            "/api/live/channels/12/logo" to "l/12",
            "/api/live/channels/12/logo?v=0a1b2c3d" to "l/12/0a1b2c3d",
        )
        for ((path, short) in forms) {
            val uri = MusicArt.uri(context, path, "Deep Water")!!
            assertEquals("content://$authority/$short?h=${PlaceholderCover.hue("Deep Water")}", uri.toString())
            assertEquals(path, MusicArt.pathOf(context, uri))
        }
        // Another query than the version still reads back, its path carried whole.
        val unusual = MusicArt.uri(context, "/api/items/50/images/poster?v=1&w=2")!!
        assertEquals("/api/items/50/images/poster?v=1&w=2", MusicArt.pathOf(context, unusual))
        // Anything else is no picture of the server's.
        for (text in listOf("i/50", "i/50/poster/1/2", "i/5x/poster", "i/50/Poster", "i/50/../poster", "i/%2F50/poster", "i/50/poster/1.5", "x/50/poster", "l/12/logo", "l/12/0A1B", "l/12/0a/1", "l/-1", "c/7", "i/50/poster/v1")) {
            assertNull(text, MusicArt.pathOf(context, Uri.parse("content://$authority/$text")))
        }
        assertNull(MusicArt.pathOf(context, Uri.parse("content://elsewhere/i/50/poster")))
        assertNull(MusicArt.pathOf(context, Uri.parse("file://$authority/i/50/poster")))
    }

    @Test
    fun anArtistsAddressNamesTheArtistAloneByDigits() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val uri = MusicArt.artistUri(context, 40, 3, "Harbour Lights")
        assertEquals(null to 40L, MusicArt.roundOf(context, uri))
        // The name isn't in the address, its hue is: the placeholder when no cover can be had.
        assertFalse(uri.toString().contains("Harbour"))
        assertEquals(PlaceholderCover.hue("Harbour Lights"), MusicArt.hueIn(uri))
        // It is no server path: nothing of the API goes through it.
        assertNull(MusicArt.pathOf(context, uri))
        assertNull(MusicArt.hueOf(context, uri))
        val authority = MusicArt.authority(context)
        val hue = PlaceholderCover.hue("Harbour Lights")
        // A circle, by the letter of its drawing: Android Auto kept the square pictures by their addresses ("a/40/3").
        assertEquals("content://$authority/n/40/3?h=$hue", uri.toString())
        assertNull(MusicArt.artistOf(context, uri))
        assertEquals(40L, MusicArt.artistOf(context, Uri.parse("content://$authority/a/40/3")))
        for (text in listOf("0", "-1", "4x", "+4", "99999999999999999999", "40/extra", "40/3/1", "", "%2E%2E")) {
            assertNull(text, MusicArt.artistOf(context, Uri.parse("content://$authority/a/$text")))
            assertNull(text, MusicArt.roundOf(context, Uri.parse("content://$authority/n/$text")))
        }
        assertNull(MusicArt.roundOf(context, Uri.parse("content://$authority/artist/40")))
        assertNull(MusicArt.artistOf(context, Uri.parse("content://elsewhere/a/40")))
        assertNull(MusicArt.roundOf(context, Uri.parse("content://elsewhere/n/40")))

        // Their own picture, by the server's short form, read back as strictly; one of an unusual form is passed over.
        val own = MusicArt.artistUri(context, 40, 3, "Harbour Lights", "/api/items/40/images/poster?v=17")
        assertEquals("content://$authority/o/40/poster/17?h=$hue", own.toString())
        assertEquals("/api/items/40/images/poster?v=17" to null, MusicArt.roundOf(context, own))
        assertNull(MusicArt.pathOf(context, own))
        assertEquals(uri, MusicArt.artistUri(context, 40, 3, "Harbour Lights", "/api/items/40/images/poster?v=1&w=2"))
        assertEquals(uri, MusicArt.artistUri(context, 40, 3, "Harbour Lights", "/api/collections/40/images/poster?v=1"))
        assertEquals(uri, MusicArt.artistUri(context, 40, 3, "Harbour Lights", "/api/auth/me"))
        for (text in listOf("o", "o/40", "o/4x/poster", "o/40/Poster", "o/40/poster/1/2", "o/40/poster/v1", "o/i/40/poster", "o/12/0a1b", "n/n/40", "r/1/a/40")) {
            assertNull(text, MusicArt.roundOf(context, Uri.parse("content://$authority/$text")))
        }
    }

    @Test
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun anArtistsPictureIsACircleItsCornersTransparent() {
        val square = android.graphics.Bitmap.createBitmap(400, 300, android.graphics.Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }
        val png = java.io.ByteArrayOutputStream().also { square.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
        val round = RoundPicture.png(png)!!.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
        assertEquals(RoundPicture.SIZE, round.width)
        assertEquals(RoundPicture.SIZE, round.height)
        assertEquals(0, Color.alpha(round.getPixel(2, 2)))
        assertEquals(Color.RED, round.getPixel(RoundPicture.SIZE / 2, RoundPicture.SIZE / 2))
        assertEquals(Color.RED, round.getPixel(RoundPicture.SIZE / 2, 4))
        assertNull(RoundPicture.png(byteArrayOf(1, 2)))

        // Their placeholder too.
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val placeholder = RoundPicture.placeholder(context, 200).let { BitmapFactory.decodeByteArray(it, 0, it.size) }
        assertEquals(0, Color.alpha(placeholder.getPixel(2, RoundPicture.SIZE - 3)))
        assertEquals(255, Color.alpha(placeholder.getPixel(RoundPicture.SIZE / 2, RoundPicture.SIZE / 2)))
    }

    @Test
    fun anArtistWithoutAPictureOfTheirOwnHasTheirNewestAlbumsCover() = runTest {
        val pictured = MusicLibrary(source, { Look }, { path, _ -> Uri.parse("content://art$path") }, artistArt = { Uri.parse("content://artist/${it.id}/${it.title}/${it.posterUrl}") })
        // The server has one of theirs: the artist's picture address is made with it (cut to a circle).
        assertEquals("content://artist/40/Harbour Lights//api/items/40/images/poster", pictured.children(MusicId.Artists, 0, 50).single().mediaMetadata.artworkUri.toString())
        // Without the artists' addresses, the server's picture as it is.
        assertEquals("content://art/api/items/40/images/poster", library.children(MusicId.Artists, 0, 50).single().mediaMetadata.artworkUri.toString())

        // None: the address of their picture, looked for only as the car asks for it.
        source.artist = ARTIST
        source.found = listOf(ARTIST)
        assertEquals("content://artist/40/Harbour Lights/null", pictured.children(MusicId.Artists, 0, 50).single().mediaMetadata.artworkUri.toString())
        assertEquals("content://artist/40/Harbour Lights/null", pictured.search("harbour").first { it.mediaId == "artist:40" }.mediaMetadata.artworkUri.toString())
        // Asked for, the cover of their newest album that can be had.
        val asked = mutableListOf<String>()
        val bytes = pictured.artistPicture(40) { path ->
            asked += path
            if (path.startsWith("/api/items/55/")) null else byteArrayOf(5, 0)
        }
        assertEquals(listOf<Byte>(5, 0), bytes?.toList())
        assertEquals(listOf("/api/items/55/images/poster?v=1", "/api/items/50/images/poster?v=1"), asked)
        // No cover to be had: none, and the car is given their placeholder.
        assertNull(pictured.artistPicture(40) { null })
        assertNull(pictured.artistPicture(41) { byteArrayOf(1) })
    }

    @Test
    fun whatHasNoPictureHasItsTitlesPlaceholderAndWhatHasOneKeepsIt() = runTest {
        val covered = MusicLibrary(source, { Look }, { path, _ -> Uri.parse("content://art$path") }) { seed -> Uri.parse("content://placeholder/$seed") }
        val playlists = covered.children(MusicId.Playlists, 0, 50)
        assertEquals("content://placeholder/Night drive", playlists.single().mediaMetadata.artworkUri.toString())
        val albums = covered.children(MusicId.Artist(40), 0, 50).filter { it.mediaId.startsWith("album:") }
        assertTrue(albums.all { it.mediaMetadata.artworkUri.toString().startsWith("content://art/api/items/") })
        // A playlist with its entries' covers: their mosaic, else the first alone.
        source.audioPlaylists = listOf(PlaylistSummary(7, "Night drive", PlaylistKind.Audio, itemCount = 2, artwork = listOf("/a", "/b")))
        val mosaic = MusicLibrary(source, { Look }, { path, _ -> Uri.parse("content://art$path") }, mosaicArt = { covers, seed -> Uri.parse("content://mosaic/${covers.size}/$seed") })
        assertEquals("content://mosaic/2/Night drive", mosaic.children(MusicId.Playlists, 0, 50).single().mediaMetadata.artworkUri.toString())
        assertEquals("content://art/a", covered.children(MusicId.Playlists, 0, 50).single().mediaMetadata.artworkUri.toString())
        source.audioPlaylists = listOf(PlaylistSummary(7, "Night drive", PlaylistKind.Audio, itemCount = 2))
        // The tabs keep their icons, and the library without placeholders shows none for what has no picture.
        assertTrue(covered.children(MusicId.Root, 0, 50).none { it.mediaMetadata.artworkUri.toString().startsWith("content://placeholder") })
        assertNull(library.children(MusicId.Playlists, 0, 50).single().mediaMetadata.artworkUri)
    }

    @Test
    fun aPictureCarriesItsPlaceholderForWhenItCantBeHad() = runTest {
        val seeds = mutableListOf<Pair<String, String?>>()
        val covered = MusicLibrary(source, { Look }, { path, seed -> seeds += path to seed; Uri.parse("content://art$path") })
        covered.queueItem(track(51, 50))
        // A track without a cover of its own is given its album's, which may have none either: its album's placeholder.
        assertEquals("/api/items/50/images/poster" to "Deep Water", seeds.last())

        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val uri = MusicArt.uri(context, "/api/items/50/images/poster", "First Light")!!
        assertEquals("/api/items/50/images/poster", MusicArt.pathOf(context, uri))
        assertFalse(uri.toString().contains("First"))
        val standIn = MusicArt.standIn(context, uri)!!
        assertEquals(PlaceholderCover.hue("First Light"), MusicArt.hueOf(context, standIn))
        assertNull(MusicArt.hueOf(context, uri))
        assertNull(MusicArt.standIn(context, MusicArt.uri(context, "/api/items/50/images/poster")!!))
        assertNull(MusicArt.standIn(context, standIn))
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
        for (text in listOf("h/2/400", "h/2/-1", "h/2/1x", "h", "h/12", "h/2", "h/2/12/3", "placeholder?h=12")) {
            assertNull(text, MusicArt.hueOf(context, Uri.parse("content://${MusicArt.authority(context)}/$text")))
        }
        // The address names the drawing, so a car that kept another drawing by its address asks for this one; another
        // drawing's address isn't drawn.
        val hue = PlaceholderCover.hue("Deep Water")
        assertEquals("content://${MusicArt.authority(context)}/h/${PlaceholderCover.DRAWING}/$hue", uri.toString())
        assertEquals(2, PlaceholderCover.DRAWING)
        for (other in listOf(1, 3)) assertNull(MusicArt.hueOf(context, Uri.parse("content://${MusicArt.authority(context)}/h/$other/$hue")))
        // The web's hue for a title, which the app's placeholder draws too.
        assertEquals(0, PlaceholderCover.hue(""))
        assertEquals(97 % 360, PlaceholderCover.hue("a"))

        // An artist's Shuffle tile: of their hue, named strictly as a placeholder is, the shuffle glyph on its gradient.
        val shuffle = MusicArt.shuffleUri(context, "Deep Water")
        assertEquals("content://${MusicArt.authority(context)}/s/${PlaceholderCover.DRAWING}/$hue", shuffle.toString())
        assertEquals(hue, MusicArt.shuffleHueOf(context, shuffle))
        assertNull(MusicArt.hueOf(context, shuffle))
        assertNull(MusicArt.shuffleHueOf(context, uri))
        for (text in listOf("s/2/400", "s/1/12", "s/2", "s/2/12/3", "s/2/1x")) {
            assertNull(text, MusicArt.shuffleHueOf(context, Uri.parse("content://${MusicArt.authority(context)}/$text")))
        }
        val tile = PlaceholderCover.png(context, hue, PlaceholderCover.Glyph.Shuffle).let { BitmapFactory.decodeByteArray(it, 0, it.size) }
        assertEquals(PlaceholderCover.SIZE, tile.width)
        assertFalse(PlaceholderCover.png(context, hue, PlaceholderCover.Glyph.Shuffle).contentEquals(PlaceholderCover.png(context, hue)))

        val png = PlaceholderCover.png(context, MusicArt.hueOf(context, uri)!!)
        val cover = BitmapFactory.decodeByteArray(png, 0, png.size)
        assertEquals(PlaceholderCover.SIZE, cover.width)
        assertEquals(PlaceholderCover.SIZE, cover.height)
        // Opaque all over: the gradient under the mark.
        val middle = cover.getPixel(PlaceholderCover.SIZE / 2, PlaceholderCover.SIZE / 2)
        assertTrue(Color.alpha(middle) == 255)
        // Light enough to stand out on a car's dark gray list: its top left corner, the gradient's lightest end.
        val corner = FloatArray(3).also { ColorUtils.colorToHSL(cover.getPixel(1, 1), it) }
        assertTrue(corner[2] > 0.35f)
    }

    private val MediaItem.browsableStyle: Int? get() = mediaMetadata.extras?.takeIf { it.containsKey(BROWSABLE) }?.getInt(BROWSABLE)

    private val MediaItem.playableStyle: Int? get() = mediaMetadata.extras?.takeIf { it.containsKey(PLAYABLE) }?.getInt(PLAYABLE)

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

        override fun disc(number: Int) = "Disc $number"

        override val books = "Books"
        override val continueListening = "Continue listening"
        override val allBooks = "All books"
        override val music = "Music"

        override fun timeLeft(heardMs: Long, totalMs: Long) = "${(totalMs - heardMs) / 60_000} min left"

        override val recentIcon: Uri = Uri.parse("android.resource://pley/drawable/recent")
        override val artistsIcon: Uri = Uri.parse("android.resource://pley/drawable/artists")
        override val playlistsIcon: Uri = Uri.parse("android.resource://pley/drawable/playlists")
        override val booksIcon: Uri = Uri.parse("android.resource://pley/drawable/books")
        override val shuffleIcon: Uri = Uri.parse("android.resource://pley/drawable/shuffle")
    }

    private class FakeSource : MusicSource {
        val libraries = mutableListOf(Library(3, "Music", LibraryKind.Music, itemCount = 1), Library(9, "Films", LibraryKind.Movies))

        // A book heard to the middle of its second part (Continue listening), and one not started.
        var bookLibraries = emptyList<Library>()
        var shelves = BookShelves(listening = listOf(LONG_BOOK), added = listOf(SHORT_BOOK, LONG_BOOK))
        private val parts = mapOf(
            70L to listOf(part(71, 70, positionMs = 0, watched = true, playedAt = 1_000), part(72, 70, positionMs = 1_800_000, playedAt = 2_000)),
            80L to listOf(part(81, 80, positionMs = 0)),
        )

        override suspend fun bookLibraries() = bookLibraries

        override suspend fun bookShelves(libraryId: Long) = shelves

        override suspend fun books(libraryId: Long, limit: Int) = listOf(SHORT_BOOK, LONG_BOOK)

        override suspend fun parts(bookId: Long) = parts[bookId].orEmpty()

        override suspend fun onDeck(bookId: Long): ItemSummary? = parts[bookId]?.firstOrNull { !it.userState.watched }
        val recent = mutableMapOf(3L to listOf(album(50, "Deep Water"), track(51, 50)))
        val artistPages = mutableListOf<Triple<Long, Int, Int>>()
        val played = mutableMapOf<Long, List<ItemSummary>>()
        var artist = ItemSummary(40, 3, ItemKind.Artist, "Harbour Lights", posterUrl = "/api/items/40/images/poster", childCount = 2)
        var albums = listOf(album(50, "Deep Water", year = 2019).copy(leafCount = 2), album(55, "Second Tide", year = 2023))

        // Album 57, of no artist's here, is on two discs; a track of it without a disc is on the first.
        private val tracks = mapOf(
            50L to listOf(track(51, 50), track(52, 50)),
            55L to listOf(track(56, 55)),
            57L to listOf(track(58, 57).copy(index = 1), track(59, 57).copy(index = null, parentIndex = 1), track(60, 57).copy(index = 1, parentIndex = 2)),
        )
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

        override suspend fun item(itemId: Long): ItemSummary? =
            (listOf(artist, LONG_BOOK, SHORT_BOOK) + albums + tracks.values.flatten() + parts.values.flatten()).firstOrNull { it.id == itemId }

        var audioPlaylists = listOf(PlaylistSummary(7, "Night drive", PlaylistKind.Audio, itemCount = 2))

        override suspend fun playlists() = audioPlaylists + PlaylistSummary(8, "Trailers", PlaylistKind.Video, itemCount = 3)

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
    }

    private companion object {
        val GRID = MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_GRID_ITEM
        val ARTIST = ItemSummary(40, 3, ItemKind.Artist, "Harbour Lights", childCount = 2)
        val BROWSABLE = MediaConstants.EXTRAS_KEY_CONTENT_STYLE_BROWSABLE
        val PLAYABLE = MediaConstants.EXTRAS_KEY_CONTENT_STYLE_PLAYABLE

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

        val BOOKS = Library(5, "Books", LibraryKind.Audiobooks, itemCount = 2)
        val LONG_BOOK = ItemSummary(70, 5, ItemKind.Audiobook, "Long Book", posterUrl = "/api/items/70/images/poster", leafCount = 2, authors = listOf("A Writer"))
        val SHORT_BOOK = ItemSummary(80, 5, ItemKind.Audiobook, "Short Book", posterUrl = "/api/items/80/images/poster", leafCount = 1, authors = listOf("A Writer", "Another"))

        /** An hour-long part of [book]. */
        fun part(id: Long, book: Long, positionMs: Long, watched: Boolean = false, playedAt: Long? = null) = ItemSummary(
            id,
            5,
            ItemKind.Part,
            "Part $id",
            parentId = book,
            parentTitle = if (book == 70L) "Long Book" else "Short Book",
            durationMs = 3_600_000,
            posterUrl = "/api/items/$book/images/poster",
            userState = UserState(positionMs = positionMs, watched = watched, lastPlayedAt = playedAt),
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
