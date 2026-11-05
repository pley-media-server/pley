package app.pley.android.data

import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.common.UserFacing
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistSummary
import app.pley.android.model.TrackDefaults
import kotlinx.coroutines.flow.Flow

/**
 * This profile's playlists, and changes to them: shown at once, undone
 * if the server refuses. Smart playlists can't be edited here.
 */
interface PlaylistsRepository {
    fun playlists(): Flow<Loadable<List<PlaylistSummary>>>

    fun playlist(playlistId: Long): Flow<Loadable<PlaylistDetail>>

    /** A new playlist with [itemIds]; its id. */
    suspend fun create(title: String, itemIds: List<Long> = emptyList()): Long = create(title, null, itemIds)

    /**
     * A new playlist of [kind] (an empty one needs it: video or music)
     * with [itemIds]; its id. `null`: the first item decides.
     */
    suspend fun create(title: String, kind: PlaylistKind?, itemIds: List<Long> = emptyList()): Long

    /** A smart playlist of a library's [query] (the web's filter string). */
    suspend fun createSmart(title: String, libraryId: Long, query: String): Long

    suspend fun rename(playlistId: Long, title: String)

    suspend fun delete(playlistId: Long)

    suspend fun add(playlistId: Long, itemIds: List<Long>)

    suspend fun remove(playlistId: Long, entryIds: List<Long>)

    /** The entries in their new order. */
    suspend fun reorder(playlistId: Long, entryIds: List<Long>)
}

/**
 * Watched marks, ratings and track choices: every list and page showing the
 * item changes at once, then everything is fetched again from the server.
 */
interface WatchStateRepository {
    suspend fun setWatched(itemId: Long, watched: Boolean)

    /** Stars out of 10 (halves allowed), or none when [rating] is `null`. */
    suspend fun rate(itemId: Long, rating: Double?)

    /**
     * The version and tracks to play [itemId] with next time. [audioIndex]
     * only for a track the person picked: `null` keeps the audio chosen
     * before (subtitles alone were picked), unless [automaticAudio], which
     * forgets it so each device plays its best track of the language.
     * [subtitleIndex] `null` is off.
     */
    suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean = false)

    /**
     * The player saved these tracks for [itemId] itself: its page, and
     * playing it again, start with them; the show's other episodes are
     * asked for again, since the choice is the whole show's.
     */
    fun streamsChosen(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?) = Unit

    /**
     * Takes [itemId] out of Continue Watching, Home's and the libraries'
     * (an episode: its whole show), until it is played again. Its progress
     * and watched mark stay.
     */
    suspend fun hideFromContinueWatching(itemId: Long)
}

/**
 * The profile's My List: movies and shows, newest added first. Adding and
 * removing show at once in the list, Home's My List and the item's page,
 * and are undone if the server refuses.
 */
interface WatchlistRepository {
    fun watchlist(): Flow<Loadable<List<ItemSummary>>>

    suspend fun add(itemId: Long)

    suspend fun remove(itemId: Long)

    suspend fun refresh()
}

/** The profile's audio and subtitle defaults; older servers don't offer them to apps. */
interface TrackDefaultsRepository {
    fun defaults(): Flow<Loadable<TrackDefaults>>

    /** Saves [defaults]; throws [TrackDefaultsUnsupported] where the server doesn't let apps change them. */
    suspend fun save(defaults: TrackDefaults)
}

/**
 * The server doesn't let this app change the profile's track defaults (it
 * predates PUT /api/me/track-defaults, or the sign-in wasn't allowed to
 * play): show them read-only, to change on the web.
 */
class TrackDefaultsUnsupported(cause: Throwable? = null) : UnsupportedOperationException(cause), UserFacing {
    override val text: UiText = UiText.res(R.string.data_track_defaults_on_web)

    override val message: String get() = AppLanguage.wordsOf(text)
}
