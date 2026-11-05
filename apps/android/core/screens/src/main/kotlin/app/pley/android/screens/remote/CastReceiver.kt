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
import app.pley.android.screens.onboarding.ProfileGate
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** Music a remote cast here started: where it came from. */
data class CastMusic(val from: CastFrom)

/** A cast from another profile's remote that couldn't switch this device to that profile: where it came from. */
data class CastRefused(val from: CastFrom)

/**
 * What the profile's remotes cast to this device (docs/API.md, "Casting").
 * The shells start it after their first frame; what plays says where it
 * came from: a video or a channel in its player, music through
 * [music] (the phone's shell says it, the TV's opens Now Playing). A cast
 * from another profile of the household switches this device to that
 * profile first; one that couldn't plays nothing ([refused]).
 */
interface CastReceiver {
    /** Starts playing what is cast here, from now on; calling it again does nothing. */
    fun start() {}

    /** Each time music cast here starts. */
    val music: Flow<CastMusic> get() = emptyFlow()

    /** Each time a cast couldn't switch this device to the remote's profile: nothing plays, and the shells say so. */
    val refused: Flow<CastRefused> get() = emptyFlow()

    companion object {
        /** Takes nothing: graphs made without one (tests of other screens). */
        val None: CastReceiver = object : CastReceiver {}

        /**
         * The app's: casts heard on [data]'s event stream play through
         * [launcher] and [music], on [scope] (the main thread's); what
         * plays ends through [endPlaying] (PlayerGraph.endAll) before a
         * cast switches profiles.
         */
        fun of(
            data: DataGraph,
            launcher: PlayerLauncher,
            music: () -> MusicControl,
            scope: CoroutineScope,
            endPlaying: suspend () -> Unit,
        ): CastReceiver = ServerCastReceiver(data, launcher, music, scope, endPlaying = endPlaying)
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
 * in the player, from `start_ms` when given, else where Play resumes, with
 * the audio and subtitles the remote picked when it says (for this play
 * only: the profile's saved choices stay as they are); a
 * show or a season its next episode; a track, an album or an artist in
 * the music player, shuffled with `shuffle`; a playlist in order from its
 * first entry (a random one with `shuffle`), video as a queue in the
 * player, music in the music player; a channel in the live player. A
 * newer cast replaces one still being looked up.
 *
 * A cast from another profile of the household (`profile_id` other than
 * the profile in use) ends what plays, then switches to that profile, with
 * its `switch_ticket`, as a profile switch does (what shows and the event
 * stream follow it), then plays once. One that can't (a ticket spent or
 * expired, the server out of reach) plays nothing and says so; one without
 * a ticket is ignored, as one for another device. No cast plays while
 * "Who's watching?" shows, its PIN pad too ([ProfileGate.showing]).
 */
internal class ServerCastReceiver(
    private val data: DataGraph,
    private val launcher: PlayerLauncher,
    private val musicControl: () -> MusicControl,
    private val scope: CoroutineScope,
    private val random: Random = Random.Default,
    /** Closes the player windows open, so music cast here plays alone. */
    private val closePlayers: () -> Unit = PlayerWindows::closeAll,
    /** Where a switch keeps its tokens: off the main thread, as the profile pickers' switches. */
    private val io: CoroutineDispatcher = Dispatchers.IO,
    /** Ends what plays here, and waits until its last reports went out: before a switch. */
    private val endPlaying: suspend () -> Unit = {},
    /** Whether "Who's watching?" shows: no cast plays over it. */
    private val asking: () -> Boolean = { ProfileGate.showing },
) : CastReceiver {
    private val started = AtomicBoolean(false)
    private val heard = MutableSharedFlow<CastMusic>(extraBufferCapacity = 4)
    private val refusals = MutableSharedFlow<CastRefused>(extraBufferCapacity = 4)

    override val music: Flow<CastMusic> = heard.asSharedFlow()

    override val refused: Flow<CastRefused> = refusals.asSharedFlow()

    /** A cast's switch is under way: what plays is ending, or the tokens are changing. */
    @Volatile
    private var switching = false

    override fun start() {
        if (!started.compareAndSet(false, true)) return
        scope.launch {
            val live = data.liveUpdates
            live.events.filterIsInstance<ServerEvent.Cast>()
                // One for the profile being left, heard as it switches, isn't for the profile it plays as: dropped
                // as it is heard, so it doesn't replace the cast that switches.
                .filter { it.profileId != null || !switching }
                .collectLatest { cast ->
                    if (cast.receiverId != live.receiver.value) return@collectLatest
                    // Whoever is at the device is choosing who's watching: a cast would choose for them.
                    if (asking()) return@collectLatest
                    if (!asItsProfile(cast)) return@collectLatest
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

    /**
     * Whether [cast] plays here, as the profile of the remote that sent it:
     * this profile's own does; another's switches to it first with its
     * ticket. What plays ends before the switch, and its last reports
     * (`stopped`, the session's end) go out first: they carry the tokens in
     * use as they go, and would land on the caster's profile after it. A
     * switch once begun is let finish, even when a newer cast comes
     * meanwhile: the server rotates the tokens either way. Meanwhile a cast
     * for the profile being left is dropped ([switching]).
     */
    private suspend fun asItsProfile(cast: ServerEvent.Cast): Boolean {
        val profile = cast.profileId ?: return true
        if (profile == data.session.session.value?.me?.id) return true
        val ticket = cast.switchTicket ?: return false
        try {
            withContext(NonCancellable) {
                switching = true
                try {
                    endPlaying()
                    withContext(io) { data.session.switchForCast(profile, ticket) }
                } finally {
                    switching = false
                }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // Spent, expired, refused or out of reach: still the profile it was on.
            refusals.tryEmit(CastRefused(CastFrom(cast.from)))
            return false
        }
        return true
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
                // Where the title starts and its tracks, when the remote says: a show's or a season's episode
                // resumes as Play would, with the tracks saved for it.
                val own = target.itemId == itemId
                launcher.play(
                    PlayRequest(
                        itemId = target.itemId,
                        startMs = cast.startMs?.takeIf { own } ?: target.startMs,
                        audioIndex = cast.audioIndex?.takeIf { own }?.toLong(),
                        subtitleIndex = cast.subtitleIndex?.takeIf { own }?.toLong(),
                        castFrom = from,
                    ),
                )
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
