package app.pley.android.player.music.library

import android.net.Uri
import android.os.Bundle
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaConstants
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistKind
import app.pley.android.model.format.formatClockMs
import app.pley.android.model.PlaylistSummary
import app.pley.android.player.music.MusicPlayback
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope

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

    /** Icons of the tabs and of Shuffle: monochrome, as `android.resource` addresses. */
    val recentIcon: Uri?
    val artistsIcon: Uri?
    val playlistsIcon: Uri?
    val shuffleIcon: Uri?
}

/** What playing a node of the library plays: [tracks] from [startIndex], shuffled or not. */
data class MusicSelection(val tracks: List<ItemSummary>, val startIndex: Int = 0, val shuffle: Boolean = false)

/**
 * Pley's music as cars and voice assistants browse it (Android Auto, a
 * phone's Assistant): three tabs, Recent (the albums played last, then
 * those added last to each music library), Artists (by name; their albums, with Shuffle first; an
 * album's tracks) and Playlists (music ones), and search; a fourth for
 * Live TV ([live]). Albums show as a
 * grid of their art, artists and tracks as lists. With several music
 * libraries, Recent groups its albums by library and Artists lists the
 * libraries first.
 *
 * Pictures go as [art] addresses: the car fetches them from the app, as
 * the server's need its token.
 */
@OptIn(markerClass = [UnstableApi::class])
class MusicLibrary(
    private val source: MusicSource,
    private val look: () -> MusicLibraryLook,
    private val art: (path: String) -> Uri?,
    /** Live TV's tab and channels ([LiveLibrary]); `null` without them. */
    val live: LiveLibrary? = null,
    /** The cover for what has none, from its title ([MusicArt.placeholderUri]). */
    private val placeholder: (seed: String) -> Uri? = { null },
) {
    fun root(): MediaItem = node(MusicId.Root, title = "Pley", browsable = true, playable = false)

    /** The node [id] names, without its children; `null` for one that isn't there. */
    suspend fun item(id: MusicId): MediaItem? = when (id) {
        MusicId.Root -> root()
        MusicId.Recent, MusicId.Artists, MusicId.Playlists -> tabs().firstOrNull { it.mediaId == id.text }
        MusicId.LiveTv, is MusicId.LiveList, is MusicId.Channel -> live?.item(id)
        is MusicId.Artist -> source.item(id.id)?.takeIf { it.kind == ItemKind.Artist }?.let(::artist)
        is MusicId.Album -> source.item(id.id)?.takeIf { it.kind == ItemKind.Album }?.let(::album)
        is MusicId.Track -> source.item(id.id)?.takeIf { it.kind == ItemKind.Track }?.let { track(it, id.from) }
        is MusicId.Playlist -> musicPlaylists().firstOrNull { it.id == id.id }?.let(::playlist)
        is MusicId.LibraryArtists, is MusicId.ShuffleArtist -> null
    }

    /** [id]'s children, page [page] of [pageSize]. */
    suspend fun children(id: MusicId, page: Int, pageSize: Int): List<MediaItem> {
        val size = pageSize.coerceIn(1, MAX_PAGE)
        val all: List<MediaItem> = when (id) {
            MusicId.Root -> return if (page == 0) tabs() else emptyList()
            MusicId.Recent -> recent()
            MusicId.Artists -> {
                val libraries = source.libraries()
                if (libraries.size == 1) return source.artists(libraries[0].id, page * size, size).map(::artist)
                libraries.map { node(MusicId.LibraryArtists(it.id), it.name, browsable = true, playable = false, browsableStyle = LIST) }
            }
            is MusicId.LibraryArtists -> return source.artists(id.libraryId, page * size, size).map(::artist)
            MusicId.Playlists -> musicPlaylists().map(::playlist)
            is MusicId.Artist -> {
                val albums = source.children(id.id).filter { it.kind == ItemKind.Album }.sortedWith(NEWEST_FIRST)
                val l = look()
                listOf(node(MusicId.ShuffleArtist(id.id), l.shuffle, browsable = false, playable = true, icon = l.shuffleIcon)) + albums.map { album(it) }
            }
            is MusicId.Album -> source.children(id.id).filter { it.kind == ItemKind.Track }.map { track(it, from = id) }
            is MusicId.Playlist -> source.playlist(id.id).entries.map { it.item }.filter { it.kind == ItemKind.Track }.map { track(it, from = id) }
            MusicId.LiveTv, is MusicId.LiveList -> live?.children(id).orEmpty()
            is MusicId.ShuffleArtist, is MusicId.Track, is MusicId.Channel -> emptyList()
        }
        return all.drop(page * size).take(size)
    }

    /**
     * Artists, then albums, then tracks matching [query]: each a group of
     * its own; then Live TV's channels. A query of blanks gets the albums
     * played and added last, once each, as Recent has them (Media3 answers
     * an empty one itself, before it gets here).
     */
    suspend fun search(query: String): List<MediaItem> = coroutineScope {
        if (query.isBlank()) return@coroutineScope recent().distinctBy { it.mediaId }.take(SEARCH_EACH)
        // Channels not found (Live TV closed, the server's answer failing) leave the music's results as they are.
        val channels = async { live?.let { runCatching { it.search(query) }.getOrNull() }.orEmpty() }
        val found = source.search(query)
        val l = look()
        found.filter { it.kind == ItemKind.Artist }.take(SEARCH_EACH).map { artist(it).grouped(l.artists) } +
            found.filter { it.kind == ItemKind.Album }.take(SEARCH_EACH).map { album(it).grouped(l.albums) } +
            found.filter { it.kind == ItemKind.Track }.take(SEARCH_EACH).map { track(it, from = null).grouped(l.songs) } +
            channels.await()
    }

    /** What playing [id] plays; `null` for a node that doesn't play, or plays nothing. */
    suspend fun selection(id: MusicId): MusicSelection? = when (id) {
        is MusicId.Album -> tracksOf(id.id).takeIf { it.isNotEmpty() }?.let { MusicSelection(it) }
        is MusicId.Playlist -> playlistTracks(id.id).takeIf { it.isNotEmpty() }?.let { MusicSelection(it) }
        is MusicId.ShuffleArtist -> artistTracks(id.id).takeIf { it.isNotEmpty() }?.let { MusicSelection(it, 0, shuffle = true) }
        is MusicId.Track -> trackSelection(id)
        else -> null
    }

    /**
     * What a spoken request plays ("play … on Pley"): the music playlist
     * named so ([playlist], the Assistant's own reading of the name, first;
     * then what was said, with or without its word for a playlist: "my
     * Night Drive playlist" names "Night Drive"), else the closest artist,
     * album or track found, an artist shuffled, else the playlist whose name
     * holds what was said; with nothing said, the album added last.
     */
    suspend fun selectionFor(query: String, playlist: String? = null): MusicSelection? {
        val spoken = playlist?.trim()?.takeIf { it.isNotEmpty() }
        if (query.isBlank() && spoken == null) {
            val album = source.libraries().firstNotNullOfOrNull { source.recentlyAdded(it.id).firstOrNull { a -> a.kind == ItemKind.Album } }
            return album?.let { selection(MusicId.Album(it.id)) }
        }
        val said = query.trim()
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
        val best = if (said.isEmpty()) null else source.search(query).firstOrNull { it.kind == ItemKind.Artist || it.kind == ItemKind.Album || it.kind == ItemKind.Track }
        return when (best?.kind) {
            ItemKind.Artist -> selection(MusicId.ShuffleArtist(best.id))
            ItemKind.Album -> selection(MusicId.Album(best.id))
            ItemKind.Track -> selection(MusicId.Track(best.id))
            else -> names.firstNotNullOfOrNull { name -> playlists.firstOrNull { it.title.contains(name, ignoreCase = true) } }?.let { selection(MusicId.Playlist(it.id)) }
        }
    }

    /** A queue's [track] as the session lists it: its art, album and artist. */
    fun queueItem(track: ItemSummary, mediaId: String = MusicId.Track(track.id).text): MediaItem = track(track, from = null).buildUpon().setMediaId(mediaId).build()

    private suspend fun trackSelection(id: MusicId.Track): MusicSelection? {
        val tracks = when (val from = id.from) {
            is MusicId.Album -> tracksOf(from.id)
            is MusicId.Playlist -> playlistTracks(from.id)
            else -> null
        }
        if (tracks != null) {
            val at = tracks.indexOfFirst { it.id == id.id }
            if (at >= 0) return MusicSelection(tracks, at)
        }
        // A search's track, or one no longer where it was: its album plays on from it.
        val track = source.item(id.id)?.takeIf { it.kind == ItemKind.Track } ?: return null
        val album = track.parentId?.let { tracksOf(it) }.orEmpty()
        val at = album.indexOfFirst { it.id == track.id }
        return if (at >= 0) MusicSelection(album, at) else MusicSelection(listOf(track))
    }

    /** The profile's playlists of music: video ones don't play in a car. */
    private suspend fun musicPlaylists(): List<PlaylistSummary> = source.playlists().filter { it.kind == PlaylistKind.Audio }

    private suspend fun tracksOf(albumId: Long): List<ItemSummary> = source.children(albumId).filter { it.kind == ItemKind.Track }

    private suspend fun playlistTracks(playlistId: Long): List<ItemSummary> =
        source.playlist(playlistId).entries.map { it.item }.filter { it.kind == ItemKind.Track }

    private suspend fun artistTracks(artistId: Long): List<ItemSummary> = coroutineScope {
        val albums = source.children(artistId).filter { it.kind == ItemKind.Album }.sortedWith(NEWEST_FIRST)
        albums.map { async { tracksOf(it.id) } }.awaitAll().flatten()
    }

    /** Recent, Artists, Playlists, and Live TV when it is open to the profile: four, as many as cars show. */
    private suspend fun tabs(): List<MediaItem> {
        val l = look()
        return listOfNotNull(
            node(MusicId.Recent, l.recent, browsable = true, playable = false, icon = l.recentIcon, browsableStyle = GRID),
            node(MusicId.Artists, l.artists, browsable = true, playable = false, icon = l.artistsIcon, browsableStyle = LIST),
            node(MusicId.Playlists, l.playlists, browsable = true, playable = false, icon = l.playlistsIcon, browsableStyle = GRID),
            live?.tab(),
        )
    }

    /**
     * The albums played last (as YouTube Music's Last played), then those
     * added last to each music library, each under its heading. Plays that
     * can't be read leave their group out; the added albums still show.
     */
    private suspend fun recent(): List<MediaItem> {
        val libraries = source.libraries()
        val (played, added) = coroutineScope {
            val played = libraries.map { async { runCatching { source.recentlyPlayed(it.id) }.getOrDefault(emptyList()) } }
            val added = libraries.map { async { it to source.recentlyAdded(it.id) } }
            played.awaitAll() to added.awaitAll()
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
        return playedAlbums + addedAlbums
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

    private fun artist(item: ItemSummary): MediaItem = node(
        MusicId.Artist(item.id),
        item.title,
        subtitle = item.childCount?.takeIf { it > 0 }?.let { look().albums(it) },
        browsable = true,
        playable = false,
        artPath = item.posterUrl,
        seed = item.title,
        browsableStyle = GRID,
        playableStyle = LIST,
        mediaType = MediaMetadata.MEDIA_TYPE_ARTIST,
    )

    private fun album(item: ItemSummary): MediaItem = node(
        MusicId.Album(item.id),
        item.title,
        subtitle = item.parentTitle ?: item.year?.toString(),
        browsable = true,
        playable = true,
        artPath = item.posterUrl,
        seed = item.title,
        playableStyle = LIST,
        mediaType = MediaMetadata.MEDIA_TYPE_ALBUM,
        artist = item.parentTitle,
    )

    private fun playlist(item: PlaylistSummary): MediaItem = node(
        MusicId.Playlist(item.id),
        item.title,
        subtitle = look().tracks(item.itemCount),
        browsable = true,
        playable = true,
        artPath = item.artwork.firstOrNull(),
        seed = item.title,
        playableStyle = LIST,
        mediaType = MediaMetadata.MEDIA_TYPE_PLAYLIST,
    )

    /**
     * A track's row. In its album's list, each row would show the same
     * cover and artist: it shows the track's length instead, and no cover.
     * Elsewhere (a playlist, search, the queue), its artist and cover.
     */
    private fun track(item: ItemSummary, from: MusicId?): MediaItem {
        val artist = item.grandparentTitle
        val inAlbum = from is MusicId.Album
        val subtitle = if (inAlbum) item.durationMs?.takeIf { it > 0 }?.let(::formatClockMs) else artist
        val metadata = MediaMetadata.Builder()
            .setTitle(item.title)
            .setDisplayTitle(item.title)
            .setArtist(artist)
            .setSubtitle(subtitle)
            .setAlbumTitle(item.parentTitle)
            .setTrackNumber(item.index)
            .setDiscNumber(item.parentIndex)
            .setDurationMs(item.durationMs?.takeIf { it > 0 })
            // Without a cover of its own, its album's placeholder, as the app shows it.
            .setArtworkUri(if (inAlbum) null else MusicPlayback.artPath(item)?.let(art) ?: placeholder(item.parentTitle ?: item.title))
            .setIsBrowsable(false)
            .setIsPlayable(true)
            .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
            .build()
        return MediaItem.Builder().setMediaId(MusicId.Track(item.id, from).text).setMediaMetadata(metadata).build()
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
            .setArtworkUri(icon ?: artPath?.let(art) ?: seed?.let(placeholder))
            .setIsBrowsable(browsable)
            .setIsPlayable(playable)
            .setMediaType(mediaType)
            .setExtras(extras)
            .build()
        return MediaItem.Builder().setMediaId(id.text).setMediaMetadata(metadata).build()
    }

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

        /** The most albums Recently played shows. */
        private const val RECENTLY_PLAYED = 12

        /** The most of each kind a search shows. */
        private const val SEARCH_EACH = 15

        /** An artist's albums: the newest first, then by title. */
        private val NEWEST_FIRST = compareByDescending<ItemSummary> { it.year ?: 0 }.thenBy { it.sortTitle.ifEmpty { it.title }.lowercase() }
    }
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
