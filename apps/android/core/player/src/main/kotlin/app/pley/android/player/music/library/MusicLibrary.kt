package app.pley.android.player.music.library

import android.net.Uri
import android.os.Bundle
import android.provider.MediaStore
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaConstants
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistKind
import app.pley.android.model.format.formatClockMs
import app.pley.android.model.PlaylistSummary
import app.pley.android.player.music.MusicPlayback
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withTimeoutOrNull

/** The words and icons of the library's own nodes, in the language the app is shown in. */
interface MusicLibraryLook {
    val recent: String

    /** Recent's groups: the albums played last, then those added last ("Recently added in Music" with several libraries). */
    val recentlyPlayed: String
    val recentlyAdded: String

    fun recentlyAddedIn(library: String): String
    val artists: String
    val playlists: String

    /** Search's groups, after the artists. */
    val albums: String
    val songs: String
    val shuffle: String

    /** "12 tracks". */
    fun tracks(count: Int): String

    /** "4 albums". */
    fun albums(count: Int): String

    /** An album's disc, heading its tracks when it has several: "Disc 2". */
    fun disc(number: Int): String

    /** The audiobooks' tab, its headings (with Recently added) and search's group. */
    val books: String
    val continueListening: String
    val allBooks: String

    /** The artists' tab when Playlists goes in it: too many tabs for a car. */
    val music: String

    /** "3h 12m left" of a book heard to [heardMs] of [totalMs]. */
    fun timeLeft(heardMs: Long, totalMs: Long): String

    /** Icons of the tabs and of Shuffle: monochrome, as `android.resource` addresses. */
    val recentIcon: Uri?
    val artistsIcon: Uri?
    val playlistsIcon: Uri?
    val booksIcon: Uri?
    val shuffleIcon: Uri?
}

/**
 * What playing a node of the library plays: [tracks] from [startIndex] ([startMs] into it), shuffled or not; [title] is
 * the playlist's or the artist's name it was picked as, for the queue's name (an album's or a book's is its tracks').
 */
data class MusicSelection(
    val tracks: List<ItemSummary>,
    val startIndex: Int = 0,
    val shuffle: Boolean = false,
    val startMs: Long = 0,
    val title: String? = null,
)

/**
 * What a spoken request asks for beside what was said, as the Assistant
 * puts it in MediaStore's extras: an artist, an album or a track ([kind]),
 * and the names it heard ("play the album Thriller by Michael Jackson":
 * an album, [album] Thriller, [artist] Michael Jackson).
 */
data class SpokenFocus(val kind: ItemKind? = null, val artist: String? = null, val album: String? = null, val title: String? = null) {
    val isEmpty: Boolean get() = kind == null && artist == null && album == null && title == null

    companion object {
        /**
         * The focus [extras] give. Without one (or one for anything), the
         * names heard say it: a title is a track's, else an album an
         * album's, else an artist an artist's. A playlist's, a genre's or
         * the radio's isn't one of these.
         */
        fun of(extras: Bundle?): SpokenFocus {
            if (extras == null) return SpokenFocus()
            fun heard(key: String) = extras.getString(key)?.trim()?.take(MAX_NAME)?.takeIf { it.isNotEmpty() }
            val artist = heard(MediaStore.EXTRA_MEDIA_ARTIST)
            val album = heard(MediaStore.EXTRA_MEDIA_ALBUM)
            val title = heard(MediaStore.EXTRA_MEDIA_TITLE)
            val kind = when (extras.getString(MediaStore.EXTRA_MEDIA_FOCUS)) {
                MediaStore.Audio.Artists.ENTRY_CONTENT_TYPE -> ItemKind.Artist
                MediaStore.Audio.Albums.ENTRY_CONTENT_TYPE -> ItemKind.Album
                MediaStore.Audio.Media.ENTRY_CONTENT_TYPE -> ItemKind.Track
                null, ANY_FOCUS -> when {
                    title != null -> ItemKind.Track
                    album != null -> ItemKind.Album
                    artist != null -> ItemKind.Artist
                    else -> null
                }
                else -> null
            }
            return SpokenFocus(kind, artist, album, title)
        }

        /** The focus of a request for anything ("play some music"). */
        private const val ANY_FOCUS = "vnd.android.cursor.item/*"

        /** The longest name heard that is used, as for what was said. */
        private const val MAX_NAME = 200
    }
}

/**
 * Pley's music as cars and voice assistants browse it (Android Auto, a
 * phone's Assistant): Recent (the books being heard, the albums played
 * last, then those added last to each music library), Artists (by name;
 * their albums, with Shuffle first; an album's tracks), Playlists (music
 * ones), Books (being heard, added last, then every one) and Live TV
 * ([live]), and search. A tab with nothing in it isn't shown, and with
 * more than a car shows, Playlists goes first in the artists' tab, named
 * Music. Albums and books show as a grid of their art, artists and tracks
 * as lists. With several music libraries, Recent groups its albums by
 * library and Artists lists the libraries first.
 *
 * Pictures go as [art] addresses: the car fetches them from the app, as
 * the server's need its token.
 */
@OptIn(markerClass = [UnstableApi::class])
class MusicLibrary(
    private val source: MusicSource,
    private val look: () -> MusicLibraryLook,
    /** A picture's address, with the title of what it shows ([placeholder]'s seed, drawn when it can't be had). */
    private val art: (path: String, seed: String?) -> Uri?,
    /** Live TV's tab and channels ([LiveLibrary]); `null` without them. */
    val live: LiveLibrary? = null,
    /**
     * The picture of an artist without one of their own, by id, with their name for its placeholder
     * ([MusicArt.artistUri]): found as it is shown ([artistPicture]); `null` for their placeholder alone.
     */
    private val artistArt: (artistId: Long, seed: String) -> Uri? = { _, _ -> null },
    /** The cover for what has none, from its title ([MusicArt.placeholderUri]). */
    private val placeholder: (seed: String) -> Uri? = { null },
) {
    fun root(): MediaItem = node(MusicId.Root, title = "Pley", browsable = true, playable = false)

    /** The node [id] names, without its children; `null` for one that isn't there. */
    suspend fun item(id: MusicId): MediaItem? = when (id) {
        MusicId.Root -> root()
        MusicId.Recent, MusicId.Artists -> tabs().firstOrNull { it.mediaId == id.text }
        // Playlists in the artists' tab is no tab: still the node it is.
        MusicId.Playlists -> playlistsNode(look())
        MusicId.Books -> booksNode(look())
        is MusicId.Book -> source.item(id.id)?.takeIf { it.kind == ItemKind.Audiobook }?.let { book(it, null) }
        MusicId.LiveTv, is MusicId.LiveList, is MusicId.Channel -> live?.item(id)
        is MusicId.Artist -> source.item(id.id)?.takeIf { it.kind == ItemKind.Artist }?.let(::artist)
        is MusicId.Album -> source.item(id.id)?.takeIf { it.kind == ItemKind.Album }?.let(::album)
        is MusicId.Track -> source.item(id.id)?.takeIf { it.kind == ItemKind.Track || it.kind == ItemKind.Part }?.let { track(it, id.from, numbered = id.from is MusicId.Album) }
        is MusicId.Playlist -> musicPlaylists().firstOrNull { it.id == id.id }?.let(::playlist)
        is MusicId.LibraryArtists, is MusicId.ShuffleArtist -> null
    }

    /** [id]'s children, page [page] of [pageSize]. */
    suspend fun children(id: MusicId, page: Int, pageSize: Int): List<MediaItem> {
        val size = pageSize.coerceIn(1, MAX_PAGE)
        val all: List<MediaItem> = when (id) {
            MusicId.Root -> tabs()
            MusicId.Recent -> recent()
            MusicId.Artists -> {
                val first = if (playlistsInArtists) listOf(playlistsNode(look())) else emptyList()
                val libraries = source.libraries()
                if (libraries.size == 1) {
                    // Playlists, then the artists, paged as one list: a page holds no more than it was asked for.
                    val start = page * size
                    val head = first.drop(start).take(size)
                    val left = size - head.size
                    val artists = if (left > 0) source.artists(libraries[0].id, (start - first.size).coerceAtLeast(0), left) else emptyList()
                    return head + artists.map(::artist)
                }
                first + libraries.map { node(MusicId.LibraryArtists(it.id), it.name, browsable = true, playable = false, browsableStyle = LIST) }
            }
            MusicId.Books -> books()
            is MusicId.LibraryArtists -> return source.artists(id.libraryId, page * size, size).map(::artist)
            MusicId.Playlists -> musicPlaylists().map(::playlist)
            is MusicId.Artist -> {
                val albums = source.children(id.id).filter { it.kind == ItemKind.Album }.sortedWith(NEWEST_FIRST)
                val l = look()
                listOf(node(MusicId.ShuffleArtist(id.id), l.shuffle, browsable = false, playable = true, icon = l.shuffleIcon)) +
                    albums.map { album(it, onArtist = true) }
            }
            is MusicId.Album -> albumTracks(id)
            is MusicId.Playlist -> source.playlist(id.id).entries.map { it.item }.filter { it.kind == ItemKind.Track }.map { track(it, from = id) }
            MusicId.LiveTv, is MusicId.LiveList -> live?.children(id).orEmpty()
            is MusicId.ShuffleArtist, is MusicId.Track, is MusicId.Channel, is MusicId.Book -> emptyList()
        }
        return all.drop(page * size).take(size)
    }

    /**
     * Artists, albums, tracks and books matching [query], each a group of
     * its own, the group of the server's closest match first (then as
     * listed); then Live TV's channels, as many as are found within
     * [LIVE_SEARCH_MS]. A query of blanks gets the albums played and added
     * last, once each, as Recent has them (Media3 answers an empty one
     * itself, before it gets here).
     */
    suspend fun search(query: String): List<MediaItem> = coroutineScope {
        if (query.isBlank()) return@coroutineScope recent().distinctBy { it.mediaId }.take(SEARCH_EACH)
        // Channels not found (Live TV closed, the server's answer failing) leave the music's results as they are, and
        // so do channels slow to come: a provider's lineup is searched apart from the music, and the car waits for both.
        val channels = async {
            try {
                withTimeoutOrNull(LIVE_SEARCH_MS) { live?.search(query) }.orEmpty()
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                emptyList()
            }
        }
        val found = source.search(query)
        val l = look()
        val groups = listOf<Pair<ItemKind, (ItemSummary) -> MediaItem>>(
            ItemKind.Artist to { artist(it).grouped(l.artists) },
            ItemKind.Album to { album(it).grouped(l.albums) },
            ItemKind.Track to { track(it, from = null).grouped(l.songs) },
            ItemKind.Audiobook to { book(it, null).grouped(l.books) },
        )
        // Sorted by where each kind's first match is: the one the server found closest leads ("Thriller" finds the
        // album before an artist whose name only holds the word).
        val closest = groups.sortedBy { (kind, _) -> found.indexOfFirst { it.kind == kind }.let { if (it < 0) Int.MAX_VALUE else it } }
        closest.flatMap { (kind, row) -> found.filter { it.kind == kind }.take(SEARCH_EACH).map(row) } + channels.await()
    }

    /** What playing [id] plays; `null` for a node that doesn't play, or plays nothing. */
    suspend fun selection(id: MusicId): MusicSelection? = when (id) {
        is MusicId.Album -> tracksOf(id.id).takeIf { it.isNotEmpty() }?.let { MusicSelection(it) }
        is MusicId.Playlist -> source.playlist(id.id).let { playlist ->
            tracksIn(playlist).takeIf { it.isNotEmpty() }?.let { MusicSelection(it, title = playlist.title) }
        }
        is MusicId.ShuffleArtist -> artistTracks(id.id).takeIf { it.isNotEmpty() }?.let { MusicSelection(it, 0, shuffle = true, title = it.first().grandparentTitle) }
        is MusicId.Track -> trackSelection(id)
        is MusicId.Book -> bookSelection(id.id)
        else -> null
    }

    /**
     * What a spoken request plays ("play … on Pley"): asked for as an
     * artist, an album or a track ([focus]), the closest of that kind to
     * the name heard (by the artist heard, when there are several), an
     * artist shuffled. Else the music playlist named so ([playlist], the
     * Assistant's own reading of the name, first; then what was said, with
     * or without its word for a playlist: "my Night Drive playlist" names
     * "Night Drive"), else the closest artist, album or track found, else
     * the playlist whose name holds what was said; with nothing said, the
     * album added last. A book found goes on where the profile is in it.
     */
    suspend fun selectionFor(query: String, playlist: String? = null, focus: SpokenFocus = SpokenFocus()): MusicSelection? {
        val spoken = playlist?.trim()?.takeIf { it.isNotEmpty() }
        if (query.isBlank() && spoken == null && focus.isEmpty) {
            val album = source.libraries().firstNotNullOfOrNull { source.recentlyAdded(it.id).firstOrNull { a -> a.kind == ItemKind.Album } }
            return album?.let { selection(MusicId.Album(it.id)) }
        }
        val said = query.trim()
        // "Play the album Night Drive" is the album, before a playlist of that name or an artist whose name holds it.
        if (spoken == null) focused(focus, said)?.let { return it }
        val names = listOfNotNull(spoken, said.takeIf { it.isNotEmpty() }, playlistNamed(said)).distinctBy { it.lowercase() }
        // The profile's own playlists first: the search doesn't list them, and "my Road Trip playlist" names one exactly.
        val playlists = try {
            musicPlaylists()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            emptyList()
        }
        for (name in names) {
            playlists.firstOrNull { it.title.trim().equals(name, ignoreCase = true) }?.let { named -> selection(MusicId.Playlist(named.id))?.let { return it } }
        }
        val best = if (said.isEmpty()) null else source.search(query).firstOrNull { it.kind in SPOKEN }
        return when (best?.kind) {
            ItemKind.Artist -> selection(MusicId.ShuffleArtist(best.id))
            ItemKind.Audiobook -> selection(MusicId.Book(best.id))
            ItemKind.Album -> selection(MusicId.Album(best.id))
            ItemKind.Track -> selection(MusicId.Track(best.id))
            else -> names.firstNotNullOfOrNull { name -> playlists.firstOrNull { it.title.contains(name, ignoreCase = true) } }?.let { selection(MusicId.Playlist(it.id)) }
        }
    }

    /**
     * The closest [focus]'s kind to the name heard (what was [said] without
     * one), by the artist heard first; `null` when the request has no such
     * focus or none of that kind is found, and the request is read as
     * any other.
     */
    private suspend fun focused(focus: SpokenFocus, said: String): MusicSelection? {
        val kind = focus.kind ?: return null
        val name = when (kind) {
            ItemKind.Artist -> focus.artist
            ItemKind.Album -> focus.album
            else -> focus.title
        } ?: said.takeIf { it.isNotEmpty() } ?: return null
        val found = source.search(name).filter { it.kind == kind }
        val by = focus.artist?.takeIf { kind != ItemKind.Artist }
        val best = by?.let { artist -> found.firstOrNull { (if (kind == ItemKind.Album) it.parentTitle else it.grandparentTitle).equals(artist, ignoreCase = true) } }
            ?: found.firstOrNull()
            ?: return null
        return selection(
            when (kind) {
                ItemKind.Artist -> MusicId.ShuffleArtist(best.id)
                ItemKind.Album -> MusicId.Album(best.id)
                else -> MusicId.Track(best.id)
            },
        )
    }

    /**
     * What waits for a car's Play when nothing was kept on the phone (the
     * music was stopped, or none has played on it): whichever the profile
     * heard last, on any device, of the book it is part way through
     * (where it is in it) and the album of the track it played last (from
     * that track); else the album added last. Plays that can't be read
     * leave the album added last; books that can't be read within
     * [KNOWN_MS], the album.
     */
    suspend fun suggestion(): MusicSelection? = coroutineScope {
        val book = async { known { lastBook() } }
        val last = source.libraries().firstNotNullOfOrNull { library ->
            val played = try {
                source.recentlyPlayed(library.id)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                emptyList()
            }
            played.firstOrNull { it.kind == ItemKind.Track }
        }
        val heard = book.await()
        if (heard != null && heard.second >= (last?.userState?.lastPlayedAt ?: 0)) {
            known { bookSelection(heard.first) }?.let { return@coroutineScope it }
        }
        val album = last?.parentId?.let { tracksOf(it) }.orEmpty()
        val at = album.indexOfFirst { it.id == last?.id }
        if (at >= 0) MusicSelection(album, at) else selectionFor("")
    }

    /**
     * The book the profile heard last of those it is part way through, and
     * when (its part heard last's time, unix seconds); `null` with none.
     */
    private suspend fun lastBook(): Pair<Long, Long>? = coroutineScope {
        val listening = shelves(source.bookLibraries()).mapNotNull { it.second.listening.firstOrNull() }
        listening.map { book ->
            async { known { source.parts(book.id) }?.mapNotNull { it.userState.lastPlayedAt }?.maxOrNull()?.let { book.id to it } }
        }.awaitAll().filterNotNull().maxByOrNull { it.second }
    }

    /**
     * The picture of artist [artistId], who has none of their own: the cover
     * of their newest album that can be had ([picture] reads one), from the
     * albums the phone has kept of theirs, a few at most; `null` when none
     * can, and the car is given the artist's placeholder.
     */
    suspend fun artistPicture(artistId: Long, picture: suspend (path: String) -> ByteArray?): ByteArray? {
        val albums = source.children(artistId).filter { it.kind == ItemKind.Album }.sortedWith(NEWEST_FIRST)
        return albums.mapNotNull { it.posterUrl }.take(ARTIST_COVERS).firstNotNullOfOrNull { picture(it) }
    }

    /** A queue's [track] as the session lists it: its art, album and artist. */
    fun queueItem(track: ItemSummary, mediaId: String = MusicId.Track(track.id).text): MediaItem = track(track, from = null).buildUpon().setMediaId(mediaId).build()

    private suspend fun trackSelection(id: MusicId.Track): MusicSelection? {
        val (tracks, title) = when (val from = id.from) {
            is MusicId.Album -> tracksOf(from.id) to null
            is MusicId.Playlist -> source.playlist(from.id).let { tracksIn(it) to it.title }
            else -> null to null
        }
        if (tracks != null) {
            val at = tracks.indexOfFirst { it.id == id.id }
            if (at >= 0) return MusicSelection(tracks, at, title = title)
        }
        // A search's track, or one no longer where it was: its album plays on from it; a book's part, its book.
        val track = source.item(id.id)?.takeIf { it.kind == ItemKind.Track || it.kind == ItemKind.Part } ?: return null
        val album = track.parentId?.let { if (track.kind == ItemKind.Part) partsOf(it) else tracksOf(it) }.orEmpty()
        val at = album.indexOfFirst { it.id == track.id }
        return if (at >= 0) MusicSelection(album, at) else MusicSelection(listOf(track))
    }

    /**
     * A book's parts from where the profile is in it ([MusicSource.onDeck]): the part, and the place in it, it
     * listened to last part way, else the first not heard to the end; a book heard to its end, from its start.
     */
    private suspend fun bookSelection(bookId: Long): MusicSelection? = coroutineScope {
        val deck = async { known { source.onDeck(bookId) } }
        val parts = partsOf(bookId)
        if (parts.isEmpty()) return@coroutineScope null
        val at = deck.await()?.let { next -> parts.indexOfFirst { it.id == next.id } }?.takeIf { it >= 0 } ?: 0
        val part = parts[at]
        MusicSelection(parts, at, startMs = if (part.userState.watched) 0 else part.userState.positionMs.coerceAtLeast(0))
    }

    private suspend fun partsOf(bookId: Long): List<ItemSummary> = source.parts(bookId).filter { it.kind == ItemKind.Part }

    /** The profile's playlists of music: video ones don't play in a car. */
    private suspend fun musicPlaylists(): List<PlaylistSummary> = source.playlists().filter { it.kind == PlaylistKind.Audio }

    private suspend fun tracksOf(albumId: Long): List<ItemSummary> = source.children(albumId).filter { it.kind == ItemKind.Track }

    private fun tracksIn(playlist: PlaylistDetail): List<ItemSummary> = playlist.entries.map { it.item }.filter { it.kind == ItemKind.Track }

    private suspend fun artistTracks(artistId: Long): List<ItemSummary> = coroutineScope {
        val albums = source.children(artistId).filter { it.kind == ItemKind.Album }.sortedWith(NEWEST_FIRST)
        albums.map { async { tracksOf(it.id) } }.awaitAll().flatten()
    }

    /**
     * Recent, Artists, Playlists, Books, and Live TV when it is open to the
     * profile, each only with something in it: no music (the libraries'
     * counts), no music playlists, no books, nothing played, added or being
     * heard. What can't be read now (the server out of reach) is shown, but
     * books, which a server may not give the car at all. Cars show four
     * tabs: with five, Playlists goes first in the artists' tab, named Music.
     */
    private suspend fun tabs(): List<MediaItem> = coroutineScope {
        val l = look()
        val music = async { known { source.libraries() } }
        val playlists = async { known { musicPlaylists() } }
        val books = async { known { source.bookLibraries() }.orEmpty().filter { it.itemCount > 0 } }
        val liveTab = async { live?.tab() }
        val hasMusic = music.await()?.any { it.itemCount > 0 } ?: true
        val hasPlaylists = playlists.await()?.isNotEmpty() ?: true
        val bookLibraries = books.await()
        // Without music, Recent has only the books being heard.
        val heard = !hasMusic && bookLibraries.isNotEmpty() && shelves(bookLibraries).any { it.second.listening.isNotEmpty() }
        val all = listOfNotNull(
            if (hasMusic || heard) node(MusicId.Recent, l.recent, browsable = true, playable = false, icon = l.recentIcon, browsableStyle = GRID) else null,
            if (hasMusic) node(MusicId.Artists, l.artists, browsable = true, playable = false, icon = l.artistsIcon, browsableStyle = LIST) else null,
            if (hasPlaylists) playlistsNode(l) else null,
            if (bookLibraries.isNotEmpty()) booksNode(l) else null,
            liveTab.await(),
        )
        val fold = all.size > MAX_TABS && hasMusic && hasPlaylists
        playlistsInArtists = fold
        if (!fold) return@coroutineScope all
        all.filter { it.mediaId != MusicId.Playlists.text }.map { tab ->
            if (tab.mediaId != MusicId.Artists.text) return@map tab
            tab.buildUpon().setMediaMetadata(tab.mediaMetadata.buildUpon().setTitle(l.music).setDisplayTitle(l.music).build()).build()
        }
    }

    /** Whether the tabs given last put Playlists first in the artists' tab ([tabs]). */
    @Volatile
    private var playlistsInArtists = false

    private fun playlistsNode(l: MusicLibraryLook): MediaItem =
        node(MusicId.Playlists, l.playlists, browsable = true, playable = false, icon = l.playlistsIcon, browsableStyle = GRID)

    private fun booksNode(l: MusicLibraryLook): MediaItem =
        node(MusicId.Books, l.books, browsable = true, playable = false, icon = l.booksIcon, browsableStyle = GRID, playableStyle = GRID)

    /**
     * The books' tab: those being heard (the one heard last first), those
     * added last to each audiobook library, then every book by title, each
     * under its heading. A book shows how far the profile is in it.
     */
    private suspend fun books(): List<MediaItem> = coroutineScope {
        val l = look()
        val libraries = source.bookLibraries()
        val everyOne = libraries.map { async { source.books(it.id, MAX_PAGE) } }
        val shelved = shelves(libraries)
        val listening = shelved.flatMap { it.second.listening }.distinctBy { it.id }.take(RECENTLY_PLAYED)
        val heard = heardOf(listening)
        val several = shelved.count { it.second.added.isNotEmpty() } > 1
        val added = shelved.flatMap { (library, shelves) ->
            val group = if (several) l.recentlyAddedIn(library.name) else l.recentlyAdded
            shelves.added.map { book(it, heard[it.id]).grouped(group) }
        }
        val all = everyOne.awaitAll().flatten().sortedBy { it.sortTitle.ifEmpty { it.title }.lowercase() }.take(MAX_PAGE)
        listening.map { book(it, heard[it.id]).grouped(l.continueListening) } + added + all.map { book(it, heard[it.id]).grouped(l.allBooks) }
    }

    /** The books of [libraries] being heard (the one heard last first), as Recent shows them first. */
    private suspend fun listening(libraries: List<Library>): List<MediaItem> {
        val listening = shelves(libraries).flatMap { it.second.listening }.distinctBy { it.id }.take(RECENTLY_PLAYED)
        val heard = heardOf(listening)
        val l = look()
        return listening.map { book(it, heard[it.id]).grouped(l.continueListening) }
    }

    /** Each of [libraries]' rows of books; a library whose rows can't be read has none. */
    private suspend fun shelves(libraries: List<Library>): List<Pair<Library, BookShelves>> = coroutineScope {
        libraries.map { library -> async { known { source.bookShelves(library.id) }?.let { library to it } } }.awaitAll().filterNotNull()
    }

    /** How far the profile is in each of [books], from their parts; a book whose parts can't be read isn't said. */
    private suspend fun heardOf(books: List<ItemSummary>): Map<Long, Heard> = coroutineScope {
        books.map { book -> async { known { source.parts(book.id) }?.let(::heardIn)?.let { book.id to it } } }.awaitAll().filterNotNull().toMap()
    }

    /**
     * What [block] reads; `null` when it can't be read now, or not within [KNOWN_MS]: what it reads is a tab's or a
     * group's own, and a car waits for the rest (the server out of reach would hold it past its 8 seconds).
     */
    private suspend fun <T> known(block: suspend () -> T): T? = try {
        withTimeoutOrNull(KNOWN_MS) { block() }
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    /**
     * The books being heard, the albums played last (as YouTube Music's
     * Last played), then those added last to each music library, each under
     * its heading. Books or plays that can't be read leave their group out;
     * the added albums still show.
     */
    private suspend fun recent(): List<MediaItem> {
        val libraries = source.libraries()
        val (played, added, books) = coroutineScope {
            val books = async { known { listening(source.bookLibraries()) }.orEmpty() }
            val played = libraries.map { async { runCatching { source.recentlyPlayed(it.id) }.getOrDefault(emptyList()) } }
            val added = libraries.map { async { it to source.recentlyAdded(it.id) } }
            Triple(played.awaitAll(), added.awaitAll(), books.await())
        }
        val l = look()
        val playedAlbums = played.flatten()
            .filter { it.kind == ItemKind.Track }
            .mapNotNull(::albumOf)
            .distinctBy { it.id }
            .take(RECENTLY_PLAYED)
            .map { album(it).grouped(l.recentlyPlayed) }
        val several = added.count { it.second.isNotEmpty() } > 1
        val addedAlbums = added.flatMap { (library, items) ->
            val group = if (several) l.recentlyAddedIn(library.name) else l.recentlyAdded
            items.filter { it.kind == ItemKind.Album }.map { album(it).grouped(group) }
        }
        return books + playedAlbums + addedAlbums
    }

    /** The album [track] is on, as far as the track says: its title, artist and cover. */
    private fun albumOf(track: ItemSummary): ItemSummary? {
        val id = track.parentId ?: return null
        return ItemSummary(
            id = id,
            libraryId = track.libraryId,
            kind = ItemKind.Album,
            title = track.parentTitle ?: return null,
            parentId = track.grandparentId,
            parentTitle = track.grandparentTitle,
            posterUrl = "/api/items/$id/images/poster",
        )
    }

    /**
     * An artist's row, with their picture: their own when the server has one
     * (an artist.jpg or folder.jpg in their folder), else their newest
     * album's cover, looked for only as the car shows the row
     * ([artistPicture]). The list stays a list: a driver scans it for a name.
     */
    private fun artist(item: ItemSummary): MediaItem = node(
        MusicId.Artist(item.id),
        item.title,
        subtitle = item.childCount?.takeIf { it > 0 }?.let { look().albums(it) },
        browsable = true,
        playable = false,
        artPath = item.posterUrl,
        seed = item.title,
        picture = if (item.posterUrl == null) artistArt(item.id, item.title) else null,
        browsableStyle = GRID,
        playableStyle = LIST,
        mediaType = MediaMetadata.MEDIA_TYPE_ARTIST,
    )

    /**
     * An album opens to its tracks, as a car plays what can play rather than
     * open it; one of a single track (as far as the server counts) just plays.
     * Under it, its artist; on the artist's own page ([onArtist]), which
     * names the artist already, its year and how many tracks it has
     * ("2023 · 14 tracks"), each as far as the server knows it.
     */
    private fun album(item: ItemSummary, onArtist: Boolean = false): MediaItem = node(
        MusicId.Album(item.id),
        item.title,
        subtitle = if (onArtist) yearAndTracks(item) else item.parentTitle ?: item.year?.toString(),
        browsable = item.leafCount != 1,
        playable = item.leafCount == 1,
        artPath = item.posterUrl,
        seed = item.title,
        playableStyle = LIST,
        mediaType = MediaMetadata.MEDIA_TYPE_ALBUM,
        artist = item.parentTitle,
    )

    private fun yearAndTracks(album: ItemSummary): String? =
        listOfNotNull(album.year?.toString(), album.leafCount?.takeIf { it > 0 }?.let { look().tracks(it) }).joinToString(" · ").ifEmpty { null }

    /**
     * An album's tracks, each with its number as the album's page shows it;
     * of an album of several discs, each disc's under its heading, as the
     * album's page has them (a track without a disc is on the first).
     */
    private suspend fun albumTracks(album: MusicId.Album): List<MediaItem> {
        val tracks = tracksOf(album.id)
        val discs = tracks.mapTo(HashSet()) { it.parentIndex ?: 1 }.size > 1
        val l = look()
        return tracks.map { track ->
            val row = track(track, from = album, numbered = true)
            if (discs) row.grouped(l.disc(track.parentIndex ?: 1)) else row
        }
    }

    /** A playlist opens to its tracks as an album does; one of a single track just plays. */
    private fun playlist(item: PlaylistSummary): MediaItem = node(
        MusicId.Playlist(item.id),
        item.title,
        subtitle = look().tracks(item.itemCount),
        browsable = item.itemCount != 1,
        playable = item.itemCount == 1,
        artPath = item.artwork.firstOrNull(),
        seed = item.title,
        playableStyle = LIST,
        mediaType = MediaMetadata.MEDIA_TYPE_PLAYLIST,
    )

    /**
     * A track's row. In its album's list, each row would show the same
     * cover and artist: it shows the track's length instead, and no cover,
     * and [numbered], its number before its title ("1. Make Them Cry") when
     * it has one, as the album's page lists it. Elsewhere (a playlist,
     * search, the queue), its artist and cover, and its title alone: what
     * plays is named by its title.
     */
    private fun track(item: ItemSummary, from: MusicId?, numbered: Boolean = false): MediaItem {
        // A book's part, in the queue: under its book's title.
        val part = item.kind == ItemKind.Part
        val artist = if (part) item.parentTitle else item.grandparentTitle
        val inAlbum = from is MusicId.Album
        val subtitle = if (inAlbum) item.durationMs?.takeIf { it > 0 }?.let(::formatClockMs) else artist
        val number = item.index?.takeIf { numbered && it > 0 }
        val metadata = MediaMetadata.Builder()
            // A car shows the display title, the title kept as the track's own.
            .setTitle(item.title)
            .setDisplayTitle(if (number != null) "$number. ${item.title}" else item.title)
            .setArtist(artist)
            .setSubtitle(subtitle)
            .setAlbumTitle(item.parentTitle)
            .setTrackNumber(item.index)
            .setDiscNumber(item.parentIndex)
            .setDurationMs(item.durationMs?.takeIf { it > 0 })
            // Its own cover, else its album's; without one (or one that can't be had), its album's placeholder, as the app shows it.
            .setArtworkUri(if (inAlbum) null else cover(MusicPlayback.artPath(item), item.parentTitle ?: item.title))
            .setIsBrowsable(false)
            .setIsPlayable(true)
            .setMediaType(if (part) MediaMetadata.MEDIA_TYPE_AUDIO_BOOK_CHAPTER else MediaMetadata.MEDIA_TYPE_MUSIC)
            .build()
        return MediaItem.Builder().setMediaId(MusicId.Track(item.id, from).text).setMediaMetadata(metadata).build()
    }

    /**
     * A book: played as it is picked, where the profile is in it, never
     * opened (its parts are the queue). Its authors, and how long is left
     * of one being heard ([heard]), beside the bar under its cover: a car
     * doesn't always draw that bar.
     */
    private fun book(item: ItemSummary, heard: Heard?): MediaItem {
        val authors = item.authors.joinToString(", ").ifEmpty { null }
        val finished = item.userState.watched
        val partway = !finished && heard != null && heard.heardMs > 0 && heard.heardMs < heard.totalMs
        val left = if (partway && heard != null) look().timeLeft(heard.heardMs, heard.totalMs).ifEmpty { null } else null
        val extras = Bundle()
        val viewed = item.viewedLeafCount ?: 0
        val parts = item.leafCount ?: 0
        when {
            finished -> extras.putInt(MediaConstants.EXTRAS_KEY_COMPLETION_STATUS, MediaConstants.EXTRAS_VALUE_COMPLETION_STATUS_FULLY_PLAYED)
            partway && heard != null -> extras.partly(heard.heardMs.toDouble() / heard.totalMs)
            viewed in 1 until parts -> extras.partly(viewed.toDouble() / parts)
            else -> extras.putInt(MediaConstants.EXTRAS_KEY_COMPLETION_STATUS, MediaConstants.EXTRAS_VALUE_COMPLETION_STATUS_NOT_PLAYED)
        }
        val metadata = MediaMetadata.Builder()
            .setTitle(item.title)
            .setDisplayTitle(item.title)
            .setSubtitle(listOfNotNull(authors, left).joinToString(" · ").ifEmpty { item.year?.toString() })
            .setArtist(authors)
            .setDurationMs(item.durationMs?.takeIf { it > 0 })
            .setArtworkUri(cover(item.posterUrl, item.title))
            .setIsBrowsable(false)
            .setIsPlayable(true)
            .setMediaType(MediaMetadata.MEDIA_TYPE_AUDIO_BOOK)
            .setExtras(extras)
            .build()
        return MediaItem.Builder().setMediaId(MusicId.Book(item.id).text).setMediaMetadata(metadata).build()
    }

    /** Part way through, [fraction] of it: a car needs both. */
    private fun Bundle.partly(fraction: Double) {
        putInt(MediaConstants.EXTRAS_KEY_COMPLETION_STATUS, MediaConstants.EXTRAS_VALUE_COMPLETION_STATUS_PARTIALLY_PLAYED)
        putDouble(MediaConstants.EXTRAS_KEY_COMPLETION_PERCENTAGE, fraction.coerceIn(0.0, 1.0))
    }

    private fun node(
        id: MusicId,
        title: String,
        subtitle: String? = null,
        browsable: Boolean,
        playable: Boolean,
        artPath: String? = null,
        /** An item's title, for its placeholder when it has no picture; `null` for the tabs and groups. */
        seed: String? = null,
        /** Its picture's address when it isn't the server's at [artPath] ([artistArt]). */
        picture: Uri? = null,
        icon: Uri? = null,
        browsableStyle: Int? = null,
        playableStyle: Int? = null,
        mediaType: Int = MediaMetadata.MEDIA_TYPE_FOLDER_MIXED,
        artist: String? = null,
    ): MediaItem {
        val extras = Bundle()
        browsableStyle?.let { extras.putInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_BROWSABLE, it) }
        playableStyle?.let { extras.putInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_PLAYABLE, it) }
        val metadata = MediaMetadata.Builder()
            .setTitle(title)
            .setDisplayTitle(title)
            .setSubtitle(subtitle)
            .setArtist(artist)
            .setArtworkUri(icon ?: picture ?: if (seed != null) cover(artPath, seed) else artPath?.let { art(it, null) })
            .setIsBrowsable(browsable)
            .setIsPlayable(playable)
            .setMediaType(mediaType)
            .setExtras(extras)
            .build()
        return MediaItem.Builder().setMediaId(id.text).setMediaMetadata(metadata).build()
    }

    /** The picture at [path], else [seed]'s placeholder: also when the picture named can't be had. */
    private fun cover(path: String?, seed: String): Uri? = path?.let { art(it, seed) } ?: placeholder(seed)

    /** The item under [group]'s heading in its list (none: under the one before). */
    private fun MediaItem.grouped(group: String?): MediaItem {
        if (group == null) return this
        val extras = Bundle(mediaMetadata.extras ?: Bundle()).apply { putString(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_GROUP_TITLE, group) }
        return buildUpon().setMediaMetadata(mediaMetadata.buildUpon().setExtras(extras).build()).build()
    }

    companion object {
        private const val LIST = MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM
        private const val GRID = MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_GRID_ITEM

        /** The most children a page holds, whatever the car asks for. */
        const val MAX_PAGE = 500

        /** The most albums Recently played shows, and books Continue listening. */
        private const val RECENTLY_PLAYED = 12

        /** The most tabs a car shows (Android Auto's ROOT_CHILDREN_LIMIT). */
        private const val MAX_TABS = 4

        /** The longest what a tab has, the books being heard and how far, are waited for ([known]). */
        private const val KNOWN_MS = 3_000L

        /** What a spoken request may play: an artist (shuffled), an album, a track or a book. */
        private val SPOKEN = setOf(ItemKind.Artist, ItemKind.Album, ItemKind.Track, ItemKind.Audiobook)

        /**
         * The albums whose covers are tried for an artist's picture: the server always names an album's, which it
         * reads from its tracks when the folder has none, and a few albums without any don't leave the artist bare.
         */
        private const val ARTIST_COVERS = 3

        /** The most of each kind a search shows. */
        private const val SEARCH_EACH = 15

        /** The longest a search waits for Live TV's channels, searched beside the music. */
        private const val LIVE_SEARCH_MS = 1_500L

        /** An artist's albums: the newest first, then by title. */
        private val NEWEST_FIRST = compareByDescending<ItemSummary> { it.year ?: 0 }.thenBy { it.sortTitle.ifEmpty { it.title }.lowercase() }
    }
}

/** How far a profile is in a book: [heardMs] of its parts' [totalMs]. */
internal data class Heard(val heardMs: Long, val totalMs: Long)

/** How far the profile is in a book of [parts]: those heard to the end, and where it is in the others; `null` without lengths. */
internal fun heardIn(parts: List<ItemSummary>): Heard? {
    val total = parts.sumOf { it.durationMs?.coerceAtLeast(0) ?: 0 }
    if (total <= 0) return null
    val heard = parts.sumOf { part ->
        val length = part.durationMs?.coerceAtLeast(0) ?: 0
        if (part.userState.watched) length else part.userState.positionMs.coerceIn(0, length)
    }
    return Heard(heard, total)
}

/**
 * The name in a request that says it is a playlist, as people put it in
 * English, French and Spanish ("my Night Drive playlist", "la playlist
 * Night Drive", "mi lista Night Drive"); `null` when [said] doesn't.
 */
internal fun playlistNamed(said: String): String? {
    val found = PLAYLIST_SAID.matchEntire(said.trim()) ?: return null
    return (found.groupValues[1].ifEmpty { found.groupValues[2] }).trim().takeIf { it.isNotEmpty() }
}

/** The words for a playlist, longest first, and a possessive or article said before them. */
private const val PLAYLIST_WORDS = "playlist|liste de lecture|lista de reproducción|lista de reproduccion|lista"
private const val BEFORE_IT = "my|ma|mon|mes|mi|mis|la|le|el|the"
private val PLAYLIST_SAID = Regex(
    "^(?:(?:$BEFORE_IT) )?(?:(?:$PLAYLIST_WORDS) (.+)|(.+?) (?:$PLAYLIST_WORDS))$",
    setOf(RegexOption.IGNORE_CASE),
)
