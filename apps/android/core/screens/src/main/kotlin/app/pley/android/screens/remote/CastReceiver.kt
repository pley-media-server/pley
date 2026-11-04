package app.pley.android.screens.remote

import app.pley.android.data.DataGraph
import app.pley.android.data.Loadable
import app.pley.android.data.PlayTarget
import app.pley.android.data.valueOrNull
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistKind
import app.pley.android.model.ServerEvent
import app.pley.android.player.CastFrom
import app.pley.android.player.MusicControl
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerWindows
import app.pley.android.screens.PlayerLauncher
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** Music a remote cast here started: where it came from. */
data class CastMusic(val from: CastFrom)

/**
 * What the profile's remotes cast to this device (docs/API.md, "Casting").
 * The shells start it after their first frame; what plays says where it
 * came from: a video or a channel in its player, music through
 * [music] (the phone's shell says it, the TV's opens Now Playing).
 */
interface CastReceiver {
    /** Starts playing what is cast here, from now on; calling it again does nothing. */
    fun start() {}

    /** Each time music cast here starts. */
    val music: Flow<CastMusic> get() = emptyFlow()

    companion object {
        /** Takes nothing: graphs made without one (tests of other screens). */
        val None: CastReceiver = object : CastReceiver {}

        /**
         * The app's: casts heard on [data]'s event stream play through
         * [launcher] and [music], on [scope] (the main thread's).
         */
        fun of(data: DataGraph, launcher: PlayerLauncher, music: () -> MusicControl, scope: CoroutineScope): CastReceiver =
            ServerCastReceiver(data, launcher, music, scope)
    }
}

/**
 * [CastReceiver] over the server's events. A `cast` plays here only when
 * it names the receiver the event stream is open as now: in the
 * foreground, signed in (LiveUpdates.receiver). One for another of the
 * profile's devices, or heard as this one went to the background, does
 * nothing.
 *
 * It plays as the app's own Play on that page would, what plays now
 * making way for it as pressing Play there would: a movie or an episode
 * in the player, from `start_ms` when given, else where Play resumes; a
 * show or a season its next episode; a track, an album or an artist in
 * the music player, shuffled with `shuffle`; a playlist in order from its
 * first entry (a random one with `shuffle`), video as a queue in the
 * player, music in the music player; a channel in the live player. A
 * newer cast replaces one still being looked up.
 */
internal class ServerCastReceiver(
    private val data: DataGraph,
    private val launcher: PlayerLauncher,
    private val musicControl: () -> MusicControl,
    private val scope: CoroutineScope,
    private val random: Random = Random.Default,
    /** Closes the player windows open, so music cast here plays alone. */
    private val closePlayers: () -> Unit = PlayerWindows::closeAll,
) : CastReceiver {
    private val started = AtomicBoolean(false)
    private val heard = MutableSharedFlow<CastMusic>(extraBufferCapacity = 4)

    override val music: Flow<CastMusic> = heard.asSharedFlow()

    override fun start() {
        if (!started.compareAndSet(false, true)) return
        scope.launch {
            val live = data.liveUpdates
            live.events.filterIsInstance<ServerEvent.Cast>().collectLatest { cast ->
                if (cast.receiverId != live.receiver.value) return@collectLatest
                try {
                    play(cast)
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Nothing found to play (gone, or the server out of reach): the remote sees nothing start.
                }
            }
        }
    }

    private suspend fun play(cast: ServerEvent.Cast) {
        val from = CastFrom(cast.from)
        val channelId = cast.channelId
        val playlistId = cast.playlistId
        val itemId = cast.itemId
        when {
            channelId != null -> launcher.watchLive(channelId, castFrom = from)
            playlistId != null -> playlist(playlistId, cast, from)
            itemId != null -> item(itemId, cast, from)
        }
    }

    private suspend fun item(itemId: Long, cast: ServerEvent.Cast, from: CastFrom) {
        val item = itemOf(itemId) ?: return
        when (val target = data.playItem.resolve(item, fromStart = false)) {
            is PlayTarget.Video -> {
                // Where the title starts, when the remote says: a show's or a season's episode resumes as Play would.
                val start = cast.startMs?.takeIf { target.itemId == itemId } ?: target.startMs
                launcher.play(PlayRequest(itemId = target.itemId, startMs = start, castFrom = from))
            }
            is PlayTarget.Music -> {
                val start = if (cast.shuffle) random.nextInt(target.tracks.size) else target.startIndex
                playMusic(target.tracks, start, cast.shuffle, from)
                if (item.kind == ItemKind.Track) cast.startMs?.let { musicControl().seekTo(it) }
            }
            PlayTarget.NothingToPlay -> Unit
        }
    }

    private suspend fun playlist(playlistId: Long, cast: ServerEvent.Cast, from: CastFrom) {
        val playlist = data.playlists.playlist(playlistId).first { it !is Loadable.Loading }.valueOrNull ?: return
        val items = playlist.entries.map { it.item }
        if (items.isEmpty()) return
        val start = if (cast.shuffle) random.nextInt(items.size) else 0
        if (playlist.kind == PlaylistKind.Audio) {
            playMusic(items, start, cast.shuffle, from)
        } else {
            launcher.play(PlayRequest(itemId = items[start].id, queue = PlayQueue.Playlist(playlistId), castFrom = from))
        }
    }

    private fun playMusic(tracks: List<ItemSummary>, start: Int, shuffle: Boolean, from: CastFrom) {
        // A video or a channel playing here makes way, as it would for the music player's own Play.
        closePlayers()
        musicControl().play(tracks, start, shuffle)
        heard.tryEmit(CastMusic(from))
    }

    /**
     * The item, as the server has it now when it answers within
     * [FRESH_WAIT_MS] (its resume point is where Play starts), else as it
     * was saved.
     */
    private suspend fun itemOf(itemId: Long): ItemSummary? {
        val detail = data.items.detail(itemId)
        val load = withTimeoutOrNull(FRESH_WAIT_MS) { detail.first { (it is Loadable.Ready && it.fresh) || it is Loadable.Failed } }
            ?: detail.first { it !is Loadable.Loading }
        return load.valueOrNull?.toSummary()
    }

    companion object {
        /** How long a cast waits for the server's word on what it plays before using what was saved. */
        const val FRESH_WAIT_MS = 3_000L
    }
}
