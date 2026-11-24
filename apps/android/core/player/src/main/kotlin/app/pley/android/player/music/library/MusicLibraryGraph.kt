package app.pley.android.player.music.library

import android.content.ContentResolver
import android.content.Context
import android.net.Uri
import androidx.annotation.DrawableRes
import app.pley.android.common.AppLanguage
import app.pley.android.common.formatWords
import app.pley.android.common.resolve
import app.pley.android.model.format.formatTimeLeft
import app.pley.android.data.DataGraph
import app.pley.android.network.NetworkGraph
import app.pley.android.player.R
import app.pley.android.player.live.LivePlayback
import app.pley.android.player.music.LiveRadio
import app.pley.android.player.music.SharedPictures
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

/** The music library and Live TV for cars and voice assistants, their pictures, and the radio, over the app's data. */
class MusicLibraryGraph(
    context: Context,
    private val data: DataGraph,
    private val network: NetworkGraph,
    /** A playback of Live TV's sound for the radio ([LivePlayback.create] with `radio`), in a scope. */
    private val livePlayback: ((CoroutineScope) -> LivePlayback)? = null,
) {
    private val app = context.applicationContext
    private val source = RepositoryMusicSource(data, network)
    private val pictures = SharedPictures(CoroutineScope(SupervisorJob() + Dispatchers.IO))

    /** Live TV's tab and channels, when the radio can play them. */
    val live: LiveLibrary? = livePlayback?.let {
        LiveLibrary(
            RepositoryLiveSource(data),
            { liveLook() },
            { path -> MusicArt.logoUri(app, path) ?: MusicArt.uri(app, path) },
            placeholder = { seed -> MusicArt.placeholderUri(app, seed) },
        )
    }

    val library = MusicLibrary(
        source,
        { look() },
        { path, seed -> MusicArt.uri(app, path, seed) },
        live,
        artistArt = { artist -> MusicArt.artistUri(app, artist.id, artist.childCount, artist.title, artist.posterUrl) },
    ) { seed -> MusicArt.placeholderUri(app, seed) }

    /** The car's radio of Live TV's channels; `null` without Live TV here. */
    fun radio(): LiveRadio? {
        val make = livePlayback ?: return null
        val channels = live ?: return null
        return LiveRadio(make, { channels.channel(it, from = null) }) { trouble -> trouble.text.resolve(AppLanguage.resources(app)) }
    }

    /** Whether a profile is signed in now: the library has nothing to show otherwise. */
    fun signedIn(): Boolean = sessions.value != null

    /** Waits for the sign-in to be read back from the device, as the process starts ([signInRead]). */
    suspend fun signInRead() = data.signInRead()

    /**
     * Who is signed in ("server/profile"), `null` when no one is, as it
     * changes, now first: another profile has another library and Live TV.
     */
    val profileChanges: Flow<String?> get() = sessions.map { s -> s?.let { "${it.issuer}/${it.me?.id}" } }.distinctUntilChanged()

    private val sessions = data.session.session

    /**
     * The bytes of the server's picture at [path] (a [MusicArt] path); `null` when it can't be had. Through the
     * profile's own cache of pictures, the server's address and the cache read from the same sign-in; one asked for
     * again while it is on its way (the session's cover and the car's) is fetched once.
     */
    suspend fun picture(path: String): ByteArray? {
        if (!MusicArt.servable(path)) return null
        data.signInRead()
        val session = sessions.value ?: return null
        val userId = session.me?.id
        val url = network.api(session.issuer, userId).url(path, ART_WIDTH)
        return pictures.get(network.pictures(session.issuer, userId), url)
    }

    /** The picture of an artist who has none of their own ([MusicArt.artistUri]): their newest album's cover. */
    suspend fun artistPicture(artistId: Long): ByteArray? = library.artistPicture(artistId, ::picture)

    /** The words in the language shown now. */
    private fun look(): MusicLibraryLook {
        val res = AppLanguage.resources(app)
        return object : MusicLibraryLook {
            override val recent = res.getString(R.string.music_library_recent)
            override val recentlyPlayed = res.getString(R.string.music_library_recently_played)
            override val recentlyAdded = res.getString(R.string.music_library_recently_added)

            override fun recentlyAddedIn(library: String): String = res.getString(R.string.music_library_recently_added_in, library)
            override val artists = res.getString(R.string.music_library_artists)
            override val playlists = res.getString(R.string.music_library_playlists)
            override val albums = res.getString(R.string.music_library_albums)
            override val songs = res.getString(R.string.music_library_songs)
            override val shuffle = res.getString(R.string.music_library_shuffle)

            override fun tracks(count: Int): String = res.getQuantityString(R.plurals.music_library_tracks, count, count)

            override fun albums(count: Int): String = res.getQuantityString(R.plurals.music_library_albums_count, count, count)

            override fun disc(number: Int): String = res.getString(R.string.music_library_disc, number)

            override val books = res.getString(R.string.music_library_books)
            override val continueListening = res.getString(R.string.music_library_continue_listening)
            override val allBooks = res.getString(R.string.music_library_all_books)
            override val music = res.getString(R.string.music_library_music)

            override fun timeLeft(heardMs: Long, totalMs: Long): String = formatTimeLeft(heardMs, totalMs, formatWords(res))

            override val recentIcon = icon(R.drawable.music_library_recent)
            override val artistsIcon = icon(R.drawable.music_library_artists)
            override val playlistsIcon = icon(R.drawable.music_library_playlists)
            override val booksIcon = icon(R.drawable.music_library_books)
            override val shuffleIcon = icon(R.drawable.music_library_shuffle)
        }
    }

    /** Live TV's words in the language shown now. */
    private fun liveLook(): LiveLook {
        val res = AppLanguage.resources(app)
        return object : LiveLook {
            override val liveTv = res.getString(R.string.music_library_live_tv)
            override val favorites = res.getString(R.string.music_library_live_favorites)
            override val watchedLately = res.getString(R.string.music_library_live_watched_lately)
            override val groups = res.getString(R.string.music_library_live_groups)
            override val allChannels = res.getString(R.string.music_library_live_all_channels)
            override val channels = res.getString(R.string.music_library_live_channels)

            override fun channelCount(count: Int): String = res.getQuantityString(R.plurals.music_library_live_channel_count, count, count)

            override val liveIcon = icon(R.drawable.music_library_live)
        }
    }

    private fun icon(@DrawableRes id: Int): Uri = Uri.Builder()
        .scheme(ContentResolver.SCHEME_ANDROID_RESOURCE)
        .authority(app.packageName)
        .appendPath(app.resources.getResourceTypeName(id))
        .appendPath(app.resources.getResourceEntryName(id))
        .build()

    private companion object {
        /** The pictures' width for the car: its grid's tiles and the art behind Now Playing. */
        const val ART_WIDTH = 512
    }
}
