package app.pley.android.player.music

import android.os.SystemClock
import app.pley.android.auth.AuthException
import app.pley.android.model.Control
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.model.caps.ClientCaps
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.network.PleyApi
import app.pley.android.player.EngineSource
import app.pley.android.player.MusicError
import app.pley.android.player.MusicPosition
import app.pley.android.player.MusicQueueState
import app.pley.android.player.NowPlaying
import app.pley.android.player.session.ServerPlaybackSession
import app.pley.android.player.session.SessionReporter
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.sleep.SleepTimer
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** What music plays with: the server, what the device plays, the output. */
class MusicEnvironment(
    /** The API of the server and profile signed in to now; throws when there is none. */
    val api: () -> PleyApi,
    /** What this device plays, for the server's choice of direct play or a conversion. */
    val caps: suspend () -> ClientCaps,
    /** Music's own player id: a video starting doesn't end the music's session, nor the other way round. */
    val clientId: () -> String,
    val reporter: SessionReporter,
    /** Makes the output, on the main thread, when music first plays. */
    val output: () -> MusicOutput,
    /** A monotonic clock in ms, for the sleep timer. */
    val now: () -> Long = SystemClock::elapsedRealtime,
    /** The server's events, to hear of the track's session being stopped, and the remote's commands. */
    val events: Flow<ServerEvent> = emptyFlow(),
    /**
     * Told whether music is under way (LiveUpdates.playing): the event
     * stream stays open in the background meanwhile, for the remote.
     */
    val playing: (player: Any, playing: Boolean) -> Unit = { _, _ -> },
    /** Told which track's session plays now, `null` once none (RemoteRepository.here): no remote shows it here. */
    val here: (player: Any, sessionId: String?) -> Unit = { _, _ -> },
)

/**
 * Music's queue and the server's side of it (apps/web/src/audio/AudioPlayer.tsx):
 *
 * - one server session per track (POST /api/playback), the one before
 *   ended first, so a transcode never waits on the last one;
 * - progress every 10 s while playing, on pause, and every 30 s paused (the
 *   server lets a session it hasn't heard from go, its transcode with it);
 *   at the end of a track `stopped` at its length and DELETE, then the next
 *   track by itself;
 * - a track the device can't decode is asked for again as a transcode, and
 *   a session that expired is started again where it was, up to three
 *   times in a row; a failure the player can't name asks the server
 *   whether the session goes on first;
 * - Previous restarts a track that has played more than 3 s;
 * - a sleep timer pauses after the minutes chosen, or at the end of the
 *   track (the next one waits, paused);
 * - a video or a channel ends the music's session ([pauseForVideo]): the
 *   queue and the place stay, and Play starts a new session there;
 * - the remote, on another of the profile's devices, plays and pauses,
 *   seeks, skips, and goes to the next or previous track ([obey]); the
 *   event stream stays open while music is under way, the screen off too.
 *
 * It lives as long as the app's process, so it plays on with no screen
 * open; the [MusicService] shows it in the notification and on the lock
 * screen. Main thread only.
 */
class MusicPlayback(
    private val env: MusicEnvironment,
    private val scope: CoroutineScope,
    random: Random = Random.Default,
) {
    private val queue = MusicQueue(random)
    private val now = MutableStateFlow<NowPlaying?>(null)
    private val order = MutableStateFlow(MusicQueueState())

    val nowPlaying: StateFlow<NowPlaying?> = now.asStateFlow()
    val queueState: StateFlow<MusicQueueState> = order.asStateFlow()

    private var out: MusicOutput? = null
    private var session: ServerPlaybackSession? = null
        set(value) {
            field = value
            env.here(this, value?.id)
        }
    private var api: PleyApi? = null
    private var loadJob: Job? = null
    private var ticker: Job? = null

    /** The mode to ask for after direct play failed to decode the current track. */
    private var fallback: PlaybackMode? = null

    /** New sessions asked for the current track since it last played. */
    private var recreates = 0
    private var playing = false
    private var loading = false
    private var ended = false

    /** A video or a channel took over ([pauseForVideo]): the session went, the place stays. */
    private var yielded = false
    private var error: MusicError? = null

    private val timer = SleepTimer(scope, env.now, ::trackLeftMs, { queue.current?.id }) { pause() }

    /** The sleep timer. */
    val sleep: StateFlow<SleepState> get() = timer.state

    init {
        scope.launch {
            env.events.collect { event ->
                when {
                    event is ServerEvent.SessionStopped && event.sessionId == session?.id -> stoppedByServer()
                    event is ServerEvent.PlayerCommand && event.sessionId == session?.id -> obey(event)
                }
            }
        }
    }

    /**
     * A command of the profile's remote for the track playing, as the
     * web's music obeys it: a seek stays within the track, a skip goes from
     * where it is now, next and previous as the buttons here. What changed
     * is reported at once, so the remote sees it (a pause reports by
     * itself, and another track with its new session).
     */
    private fun obey(command: ServerEvent.PlayerCommand) {
        val s = session ?: return
        val o = out ?: return
        when (command.command) {
            Control.Play -> if (!o.playWhenReady) {
                play()
                s.progress(lastPosition(), ProgressState.Playing)
            }
            Control.Pause -> if (o.playWhenReady) pause()
            Control.Seek -> command.positionMs?.let { seekNow(s, o, it) }
            Control.Skip -> command.offsetMs?.let { seekNow(s, o, lastPosition() + it) }
            Control.Next -> next()
            Control.Previous -> {
                previous()
                // Back to the start of the same track: the same session, moved.
                if (session === s) s.progress(0, if (o.playWhenReady) ProgressState.Playing else ProgressState.Paused)
            }
            Control.Unknown -> Unit
        }
    }

    /** Seeks to [positionMs], within the track, and reports where it went. */
    private fun seekNow(s: ServerPlaybackSession, o: MusicOutput, positionMs: Long) {
        val duration = o.durationMs.takeIf { it > 0 } ?: s.info.durationMs.takeIf { it > 0 } ?: queue.current?.durationMs
        val target = (if (duration != null) positionMs.coerceAtMost(duration) else positionMs).coerceAtLeast(0)
        seekTo(target)
        s.progress(target, if (o.playWhenReady) ProgressState.Playing else ProgressState.Paused)
    }

    /**
     * The server ended the track's session (an administrator, or the profile
     * elsewhere): the music stops and says so, asking for no new session.
     * Play starts the track again.
     */
    private fun stoppedByServer() {
        val s = session
        session = null
        // Where it was is kept (best effort); nothing is deleted.
        s?.stoppedByServer(lastPosition())
        loadJob?.cancel()
        timer.cancel()
        out?.stop()
        fail(MusicError.Stopped)
    }

    fun setSleep(choice: SleepChoice) {
        if (queue.isEmpty && choice != SleepChoice.Off) return
        timer.set(choice)
    }

    /** "Keep listening": the same time again. */
    fun keepListening() = timer.extend()

    /** Told when the queue is forgotten ([stop]): the service lets go of its session. */
    internal var onStopped: (() -> Unit)? = null

    /** The output, made the first time music plays (the media session plays through it). */
    fun output(): MusicOutput = out ?: env.output().also {
        it.setListener(listener)
        out = it
    }

    /** Whether there is anything to play: the service runs while there is. */
    val active: Boolean get() = !queue.isEmpty

    fun position(): MusicPosition {
        val o = out ?: return MusicPosition()
        val duration = o.durationMs.takeIf { it > 0 } ?: queue.current?.durationMs ?: 0
        val position = if (ended) duration else o.positionMs
        return MusicPosition(position.coerceAtLeast(0), duration, o.bufferedMs.coerceAtLeast(0))
    }

    fun play(tracks: List<ItemSummary>, startIndex: Int = 0, shuffle: Boolean = false) {
        if (tracks.isEmpty()) return
        queue.start(tracks, startIndex, shuffle)
        startTrack(0)
    }

    /** Plays, or after the queue's end, a failure or a video, starts the current track again. */
    fun play() {
        if (queue.isEmpty) return
        when {
            ended -> startTrack(0)
            error != null || yielded -> startTrack(lastPosition(), sameTrack = true, play = true)
            else -> output().play()
        }
    }

    fun pause() {
        out?.pause()
    }

    fun toggle() {
        val o = out
        if (o != null && o.playWhenReady && !ended && error == null && !yielded) pause() else play()
    }

    fun next() {
        if (queue.next()) startTrack(0)
    }

    fun previous() {
        val o = out
        when (queue.previous(o?.positionMs ?: 0)) {
            MusicQueue.Previous.Moved -> startTrack(0)
            MusicQueue.Previous.Restart -> if (o == null || session == null || ended || error != null) {
                startTrack(0, sameTrack = !ended)
            } else {
                o.seekTo(0)
            }
        }
    }

    fun seekTo(positionMs: Long) {
        if (ended || session == null) return
        out?.seekTo(positionMs.coerceAtLeast(0))
    }

    fun jumpTo(index: Int) {
        if (queue.jumpTo(index)) startTrack(0)
    }

    fun setShuffle(shuffle: Boolean) {
        queue.setShuffle(shuffle)
        publish()
    }

    /**
     * A video or a channel starts: the music pauses and lets go of its
     * session, which its paused reports would otherwise keep through a
     * whole film. The queue and the place stay, and Play starts a new
     * session there. A track still being asked for isn't loaded.
     */
    fun pauseForVideo() {
        if (queue.isEmpty) return
        loadJob?.cancel()
        yielded = true
        stopTicker()
        endSession()
        out?.pause()
        playing = false
        loading = false
        publish()
    }

    /** Ends the session, stops the output and forgets the queue. */
    fun stop() {
        loadJob?.cancel()
        stopTicker()
        timer.cancel()
        endSession()
        out?.release()
        out = null
        queue.clear()
        fallback = null
        recreates = 0
        playing = false
        loading = false
        ended = false
        yielded = false
        error = null
        now.value = null
        order.value = MusicQueueState()
        env.playing(this, false)
        onStopped?.invoke()
    }

    /**
     * Plays the current track from [atMs] with a new session, ending the one
     * before. [sameTrack] keeps the transcode fallback and the count of
     * sessions asked for: it is the same track, asked again.
     */
    private fun startTrack(atMs: Long, sameTrack: Boolean = false, play: Boolean? = null) {
        val track = queue.current ?: return
        if (!sameTrack) {
            fallback = null
            recreates = 0
        }
        val wantPlay = play ?: (!sameTrack || error != null || (out?.playWhenReady ?: true))
        loadJob?.cancel()
        stopTicker()
        endSession()
        val o = output()
        if (!sameTrack) o.stop()
        ended = false
        yielded = false
        error = null
        loading = true
        playing = false
        publish()
        loadJob = scope.launch {
            // The session before goes first (stopped, then DELETE): the server's transcode limit.
            withTimeoutOrNull(FLUSH_WAIT_MS) { env.reporter.flush() }
            val started = try {
                val server = env.api()
                api = server
                val request = PlaybackRequest(
                    itemId = track.id,
                    startMs = atMs.takeIf { it > 0 },
                    mode = fallback ?: PlaybackMode.Auto,
                    clientId = env.clientId(),
                    caps = capsOrNone(),
                    controls = CONTROLS,
                )
                ServerPlaybackSession.start(server, request, env.reporter)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                fail(errorOf(e))
                return@launch
            }
            session = started
            val server = api ?: return@launch
            val info = started.info
            val source = EngineSource(url = server.url(info.url), hls = info.hls, durationMs = info.durationMs.takeIf { it > 0 })
            o.load(source, atMs, wantPlay, trackInfo(track, server))
            // From now on, also while it waits paused (the sleep timer's next track).
            startTicker()
            publish()
        }
    }

    private suspend fun capsOrNone(): ClientCaps = try {
        env.caps()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        ClientCaps()
    }

    private fun lastPosition(): Long = out?.positionMs?.coerceAtLeast(0) ?: 0

    /** How long the track has left, for the sleep timer; `null` while it isn't playing. */
    private fun trackLeftMs(): Long? {
        if (session == null || ended || error != null) return null
        val position = position()
        return if (position.durationMs > 0) position.durationMs - position.positionMs else null
    }

    /** Ends the current track's session at the output's position. */
    private fun endSession() {
        val s = session ?: return
        session = null
        s.end(lastPosition())
    }

    private fun fail(why: MusicError) {
        loading = false
        playing = false
        error = why
        stopTicker()
        publish()
    }

    /**
     * Progress every 10 s while the track plays (or waits for data to), and
     * every 30 s while it is paused: the server keeps a session, and its
     * transcode, as long as it hears from it.
     */
    private fun startTicker() {
        if (ticker?.isActive == true) return
        ticker = scope.launch {
            var quiet = 0L
            while (isActive) {
                delay(PROGRESS_EVERY_MS)
                val s = session ?: continue
                val wants = out?.playWhenReady == true
                quiet += PROGRESS_EVERY_MS
                if (wants || quiet >= PAUSED_EVERY_MS) {
                    quiet = 0
                    s.progress(lastPosition(), if (wants) ProgressState.Playing else ProgressState.Paused)
                }
            }
        }
    }

    private fun stopTicker() {
        ticker?.cancel()
        ticker = null
    }

    private val listener = object : MusicOutput.Listener {
        override fun onPlayingChanged(playing: Boolean) {
            this@MusicPlayback.playing = playing
            if (playing) {
                loading = false
                recreates = 0
            }
            publish()
        }

        override fun onBuffering(buffering: Boolean) {
            if (loading == buffering) return
            loading = buffering
            publish()
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean) {
            if (playWhenReady) timer.resumed()
            if (!playWhenReady && !ended) session?.progress(lastPosition(), ProgressState.Paused)
            publish()
        }

        override fun onEnded() {
            val s = session
            session = null
            stopTicker()
            s?.end(s.info.durationMs.takeIf { it > 0 } ?: out?.durationMs ?: lastPosition())
            // The sleep timer ends with this track: the next one waits, paused.
            val sleeping = timer.itemEnded(queue.current?.id)
            if (queue.next()) {
                startTrack(0, play = !sleeping)
            } else {
                ended = true
                playing = false
                loading = false
                publish()
            }
        }

        override fun onError(error: OutputError) {
            val s = session
            val direct = s?.info?.method == PlaybackMethod.Direct
            when {
                error == OutputError.Stopped -> stoppedByServer()
                error == OutputError.Decode && direct && fallback == null -> decode()
                error == OutputError.Gone -> again(MusicError.Unplayable)
                error == OutputError.Network -> again(MusicError.Offline)
                error == OutputError.Other && s != null -> askServer(s, direct)
                else -> {
                    endSession()
                    fail(MusicError.Unplayable)
                }
            }
        }
    }

    /** Played as it is, the device couldn't decode it: once more as a transcode. */
    private fun decode() {
        fallback = PlaybackMode.Transcode
        startTrack(lastPosition(), sameTrack = true)
    }

    /** A new session where it was, three times in a row at most; then the track fails as [otherwise]. */
    private fun again(otherwise: MusicError) {
        if (recreates < MAX_RECREATES) {
            recreates++
            startTrack(lastPosition(), sameTrack = true)
        } else {
            endSession()
            fail(otherwise)
        }
    }

    /**
     * The player stopped for a reason it can't name (the server answered
     * 500, say), which the server can: it is asked whether [s] goes on.
     * It does: the track is at fault, asked for once more as a transcode,
     * else in a new session. Stopped from elsewhere (410): the music says
     * so. Gone, or the server out of reach: a new session where it was, as
     * for a session that expired. A track started meanwhile drops the answer.
     */
    private fun askServer(s: ServerPlaybackSession, direct: Boolean) {
        val server = api ?: return again(MusicError.Unplayable)
        loadJob?.cancel()
        loadJob = scope.launch {
            val failure = try {
                server.playbackStatus(s.id)
                null
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                e
            }
            if (session !== s) return@launch
            when {
                failure == null -> if (direct && fallback == null) decode() else again(MusicError.Unplayable)
                failure is ApiException && failure.status == 410 -> stoppedByServer()
                failure is OfflineException -> again(MusicError.Offline)
                else -> again(MusicError.Unplayable)
            }
        }
    }

    private fun publish() {
        // Under way, between tracks too, so the stream doesn't drop and open again at each: not after the end, a
        // failure or a video, which let the session go.
        env.playing(this, !queue.isEmpty && !ended && !yielded && error == null)
        val track = queue.current
        order.value = queue.state
        now.value = track?.let {
            NowPlaying(
                itemId = it.id,
                title = it.title,
                artist = it.grandparentTitle,
                playing = playing,
                album = it.parentTitle,
                albumId = it.parentId,
                artistId = it.grandparentId,
                artUrl = artOf(it, api ?: currentApi()),
                durationMs = it.durationMs ?: 0,
                index = queue.index,
                count = queue.size,
                shuffle = queue.shuffle,
                loading = loading && error == null,
                ended = ended,
                error = error,
            )
        }
    }

    private fun currentApi(): PleyApi? = try {
        env.api().also { api = it }
    } catch (_: Exception) {
        null
    }

    private fun trackInfo(track: ItemSummary, server: PleyApi) = TrackInfo(
        title = track.title,
        artist = track.grandparentTitle,
        album = track.parentTitle,
        artUrl = artPath(track)?.let { server.url(it, NOTIFICATION_ART_PX) },
    )

    companion object {
        /** Progress reports while playing, as on the web. */
        const val PROGRESS_EVERY_MS = 10_000L

        /** While paused: the server forgets a session it hasn't heard from for 90 s, and its transcode with it. */
        const val PAUSED_EVERY_MS = 30_000L

        /** New sessions asked in a row for one track before it counts as failed. */
        const val MAX_RECREATES = 3

        /** The longest a new track waits for the last one's reports to go out. */
        const val FLUSH_WAIT_MS = 3_000L

        /** What music obeys from the remote: everything (the web's). */
        val CONTROLS = listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip, Control.Next, Control.Previous)

        /** The notification's art: the server's 480 bucket. */
        const val NOTIFICATION_ART_PX = 480

        /** The album art's path: the track's own picture, else its album's. */
        fun artPath(track: ItemSummary): String? =
            track.posterUrl?.takeIf { it.isNotEmpty() } ?: track.parentId?.let { "/api/items/$it/images/poster" }

        private fun artOf(track: ItemSummary, api: PleyApi?): String? {
            val path = artPath(track) ?: return null
            return api?.url(path)
        }

        /** Why starting a track failed, as music shows it. */
        fun errorOf(e: Throwable): MusicError = when {
            e is OfflineException -> MusicError.Offline
            e is ApiException && e.signedOut -> MusicError.SignedOut
            e is AuthException -> MusicError.SignedOut
            e is IllegalStateException -> MusicError.SignedOut
            else -> MusicError.Unplayable
        }
    }
}
