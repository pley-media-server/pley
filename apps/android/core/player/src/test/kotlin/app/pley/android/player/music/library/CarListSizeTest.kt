package app.pley.android.player.music.library

import android.content.Context
import android.net.Uri
import android.os.Bundle
import androidx.media3.common.MediaItem
import androidx.media3.session.CAR_LIST_BYTES
import androidx.media3.session.MediaConstants
import androidx.media3.session.keptForCar
import androidx.media3.session.parcelledForCar
import androidx.media3.session.shownToCar
import androidx.test.core.app.ApplicationProvider
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveProgram
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistSummary
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The longest lists a car is given, 500 rows, as Android Auto is sent
 * them: Media3 sends a car its list in one parcel, cut where it passes
 * 256 KiB (MediaLibraryServiceLegacyStub), and the rows after the cut are
 * never shown. Media3 itself takes some 200 bytes of each row (the row's
 * class, its flags, the folder's kind), and the row's address, its
 * picture's and its hints take much of the rest: the rows are sent with
 * nothing the car doesn't read ([forOlderBrowser]), and their pictures'
 * addresses short ([MusicArt]).
 */
@RunWith(RobolectricTestRunner::class)
class CarListSizeTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val source = LongSource()
    private val library = MusicLibrary(
        source,
        { Look },
        { path, seed -> MusicArt.uri(context, path, seed) },
        artistArt = { id, seed -> MusicArt.artistUri(context, id, seed) },
    ) { seed -> MusicArt.placeholderUri(context, seed) }
    private val live = LiveLibrary(LongLiveSource(), { LiveWords }, { path -> MusicArt.uri(context, path) }, now = { NOW })

    @Test
    fun anAlbumOfFiveHundredLongTracksFits() = runTest {
        // A box set: five discs, each track numbered and under its disc's heading, its title long.
        assertEquals(ROWS, sent("album tracks", library.children(MusicId.Album(ALBUM), 0, ROWS)))
    }

    @Test
    fun fiveHundredChannelsFit() = runTest {
        // Each with its logo and what is on, picked from a provider's group: the group's name is in each row's id.
        assertEquals(ROWS, sent("channels", live.children(MusicId.LiveList(LiveCategory.Group(GROUP)))!!))
    }

    @Test
    fun halfAsManyArtistsAgainAreSent() = runTest {
        // Each with a picture of their own on the server, the longest address. 500 don't fit: what Media3 takes, the
        // picture's address and the hint that an artist's albums are a grid leave a row some 660 bytes before its name.
        val pictured = library.children(MusicId.Artists, 0, ROWS)
        val kept = sent("artists with pictures", pictured)
        assertTrue("$kept artists", kept >= 360)
        val before = keptForCar(pictured.map { it.asBefore() })
        assertTrue("$before before", 2 * kept >= 3 * before)
        source.pictured = false
        assertTrue(sent("artists without", library.children(MusicId.Artists, 0, ROWS)) >= 360)
    }

    /** How many of [rows] a car is sent; each is shown as it was, only what the car doesn't read left out. */
    private fun sent(what: String, rows: List<MediaItem>): Int {
        assertEquals(ROWS, rows.size)
        val slim = rows.map { it.forOlderBrowser() }
        for ((row, sent) in rows.zip(slim)) assertEquals(shownToCar(row), shownToCar(sent))
        val bytes = parcelledForCar(slim)
        val kept = keptForCar(slim)
        println("$what: $bytes bytes, ${bytes / ROWS} a row, $kept sent")
        if (kept == ROWS) assertTrue(bytes <= CAR_LIST_BYTES)
        return kept
    }

    /** [this] row as it was sent before: whole, with the hint for its playable children that the root gives, and its address as a query. */
    private fun MediaItem.asBefore(): MediaItem {
        val extras = Bundle(mediaMetadata.extras ?: Bundle()).apply {
            putInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_PLAYABLE, MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM)
        }
        val path = mediaMetadata.artworkUri?.let { MusicArt.pathOf(context, it) }
        val art = path?.let { Uri.parse("content://${MusicArt.authority(context)}/picture").buildUpon().appendQueryParameter("p", it).appendQueryParameter("h", "123").build() }
        return buildUpon().setMediaMetadata(mediaMetadata.buildUpon().setExtras(extras).setArtworkUri(art ?: mediaMetadata.artworkUri).build()).build()
    }

    /** A library of the longest names and the largest ids a big server gives. */
    private class LongSource : MusicSource {
        var pictured = true

        override suspend fun libraries() = listOf(Library(LIBRARY, "Music", LibraryKind.Music, itemCount = 1))

        override suspend fun artists(libraryId: Long, offset: Int, limit: Int) = (0 until limit).map { i ->
            val id = ITEM + i
            ItemSummary(
                id,
                LIBRARY,
                ItemKind.Artist,
                long("Artist $i", ARTIST_NAME),
                childCount = 999,
                posterUrl = if (pictured) "/api/items/$id/images/poster?v=$VERSION" else null,
            )
        }

        override suspend fun children(itemId: Long) = (0 until ROWS).map { i ->
            ItemSummary(
                ITEM + i,
                LIBRARY,
                ItemKind.Track,
                long("Track $i", TRACK_TITLE),
                index = 100 + i,
                parentIndex = 1 + i / 100,
                parentId = itemId,
                parentTitle = long("Album"),
                grandparentId = ITEM,
                grandparentTitle = long("Artist"),
                durationMs = 5_999_000,
                posterUrl = "/api/items/$itemId/images/poster?v=$VERSION",
            )
        }

        override suspend fun recentlyAdded(libraryId: Long) = emptyList<ItemSummary>()

        override suspend fun recentlyPlayed(libraryId: Long) = emptyList<ItemSummary>()

        override suspend fun item(itemId: Long): ItemSummary? = null

        override suspend fun playlists() = emptyList<PlaylistSummary>()

        override suspend fun playlist(playlistId: Long): PlaylistDetail = error("None")

        override suspend fun search(query: String) = emptyList<ItemSummary>()

        override suspend fun bookLibraries() = emptyList<Library>()

        override suspend fun bookShelves(libraryId: Long) = BookShelves()

        override suspend fun books(libraryId: Long, limit: Int) = emptyList<ItemSummary>()

        override suspend fun parts(bookId: Long) = emptyList<ItemSummary>()

        override suspend fun onDeck(bookId: Long): ItemSummary? = null
    }

    /** A provider's group of channels with long names, each with a logo and a long program on. */
    private class LongLiveSource : LiveSource {
        override suspend fun shown() = true

        override suspend fun groups() = LiveGroups()

        override suspend fun channels(category: LiveCategory, query: String, limit: Int) = (0 until minOf(limit, ROWS)).map { i ->
            val id = ITEM + i
            LiveChannel(
                id,
                name = "UK| " + long("Channel $i", CHANNEL_NAME),
                group = GROUP,
                logo = "/api/live/channels/$id/logo",
                now = LiveProgram(id, NOW - 60, NOW + 600, long("Program $i", PROGRAM)),
            )
        }

        override suspend fun channel(id: Long): LiveChannel? = null
    }

    private object Look : MusicLibraryLook {
        override val recent = "Recent"
        override val recentlyPlayed = "Recently played"
        override val recentlyAdded = "Recently added"

        override fun recentlyAddedIn(library: String) = "Recently added in $library"
        override val artists = "Artists"
        override val playlists = "Playlists"
        override val albums = "Albums"
        override val songs = "Songs"
        override val shuffle = "Shuffle"

        override fun tracks(count: Int) = "$count tracks"

        override fun albums(count: Int) = "$count albums"

        override fun disc(number: Int) = "Disque $number"

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

    private object LiveWords : LiveLook {
        override val liveTv = "Live TV"
        override val favorites = "Favorites"
        override val watchedLately = "Recently watched"
        override val groups = "Groups"
        override val allChannels = "All channels"
        override val channels = "Channels"

        override fun channelCount(count: Int) = "$count channels"

        override val liveIcon: Uri? = null
    }

    private companion object {
        /** As many rows as a car's list holds ([MusicLibrary.MAX_PAGE], [LiveLibrary.MAX_CHANNELS]). */
        const val ROWS = 500

        /** Ids of seven digits: a server that has scanned millions of items. */
        const val LIBRARY = 1_234_567L
        const val ITEM = 9_000_000L
        const val ALBUM = 9_876_543L

        /** The server's picture versions are times in seconds. */
        const val VERSION = 1_763_856_000L

        /**
         * Names longer than most, each row's: an artist's (most are under 16 characters), a classical work's movement,
         * an album's and its performers' (not sent in the album's list), a channel's, what is on, a provider's group.
         */
        const val ARTIST_NAME = 24
        const val TRACK_TITLE = 48
        const val NAME = 80
        const val CHANNEL_NAME = 16
        const val PROGRAM = 24
        const val GROUP = "UK| Sport & News"

        const val NOW = 1_763_856_000L

        fun long(start: String, length: Int = NAME) = (start + " ").padEnd(length, 'w')
    }
}
