package app.pley.android.player.music

import android.content.ComponentName
import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import app.pley.android.caps.CapsGraph
import app.pley.android.common.DeviceProfile
import app.pley.android.data.DataGraph
import app.pley.android.data.SessionState
import app.pley.android.model.ItemSummary
import app.pley.android.network.NetworkGraph
import app.pley.android.player.BOOK_SPEEDS
import app.pley.android.player.MusicControl
import app.pley.android.player.MusicPosition
import app.pley.android.player.MusicQueueState
import app.pley.android.player.MusicRepeat
import app.pley.android.player.NowPlaying
import app.pley.android.player.PlayerEngine
import app.pley.android.player.music.library.signInRead
import app.pley.android.player.session.SessionReporter
import app.pley.android.player.session.clientIdOf
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * The app's [MusicControl]: the process's [MusicPlayback], with a
 * [MediaController] connected to the [MusicService] while there is music,
 * which makes the service run (and go to the foreground while it plays) and
 * lets go of it when the music stops. Main thread only.
 */
class ServiceMusicControl(
    private val context: Context,
    private val playback: MusicPlayback,
) : MusicControl {
    private var controller: ListenableFuture<MediaController>? = null

    override val nowPlaying: StateFlow<NowPlaying?> get() = playback.nowPlaying

    override val queue: StateFlow<MusicQueueState> get() = playback.queueState

    override fun position(): MusicPosition = playback.position()

    override fun play(tracks: List<ItemSummary>, startIndex: Int, shuffle: Boolean) {
        onMain {
            playback.play(tracks, startIndex, shuffle)
            connect()
        }
    }

    override fun playFrom(tracks: List<ItemSummary>, startIndex: Int, startMs: Long) {
        onMain {
            playback.play(tracks, startIndex, startMs = startMs)
            connect()
        }
    }

    override fun seekBy(offsetMs: Long) = onMain { playback.seekBy(offsetMs) }

    override val bookSpeed: StateFlow<Float> get() = playback.bookSpeed

    override fun setBookSpeed(speed: Float) = onMain { playback.setBookSpeed(speed) }

    override fun toggle() = onMain {
        playback.toggle()
        connect()
    }

    override fun next() = onMain { playback.next() }

    override fun previous() = onMain { playback.previous() }

    override fun seekTo(positionMs: Long) = onMain { playback.seekTo(positionMs) }

    override fun jumpTo(index: Int) = onMain { playback.jumpTo(index) }

    override fun setShuffle(shuffle: Boolean) = onMain { playback.setShuffle(shuffle) }

    override fun setRepeat(repeat: MusicRepeat) = onMain { playback.setRepeat(repeat) }

    override fun stop() = onMain {
        // Signed out, another profile, or the app ending what plays: the car's radio stops for good too, before the
        // music, so the service ends the session or shows the music again (MusicService.stopped).
        MusicHub.radio?.close()
        playback.stop()
        disconnect()
    }

    override suspend fun flush() = playback.flush()

    override fun drop() = playback.drop()

    override fun pauseForVideo() = onMain {
        playback.pauseForVideo()
        MusicHub.radio?.pause()
    }

    override fun pauseForLeave(playOnReturn: Boolean) = onMain { playback.pauseForLeave(playOnReturn) }

    override fun returnAfterLeave(playOn: Boolean) = onMain {
        playback.returnAfterLeave(playOn)
        connect()
    }

    override val sleep: StateFlow<SleepState> get() = playback.sleep

    override fun setSleep(choice: SleepChoice) = onMain { playback.setSleep(choice) }

    override fun keepListening() = onMain { playback.keepListening() }

    /**
     * Binds to the service, which then shows the session; once per run of
     * music. Music stopped from the notification or a remote ends the
     * service's session, and this connection with it: the next music
     * connects again.
     */
    private fun connect() {
        if (!playback.active) return
        val current = controller
        if (current != null && (!current.isDone || current.connected())) return
        disconnect()
        val token = SessionToken(context, ComponentName(context, MusicService::class.java))
        controller = MediaController.Builder(context, token).buildAsync()
    }

    /** Whether the connection made still holds the service (a failed one doesn't). */
    private fun ListenableFuture<MediaController>.connected(): Boolean =
        !isCancelled && runCatching { get().isConnected }.getOrDefault(false)

    private fun disconnect() {
        controller?.let(MediaController::releaseFuture)
        controller = null
    }

    private val main = Handler(Looper.getMainLooper())

    private fun onMain(block: () -> Unit) {
        if (Looper.myLooper() == Looper.getMainLooper()) block() else main.post(block)
    }

    companion object {
        /**
         * The app's music: the engine's audio-only players ([engines]), the
         * server and profile signed in to now, this device's caps, reports
         * from the app's scope. Its [MusicPlayback] goes to [MusicHub] for
         * the service.
         */
        fun create(
            context: Context,
            data: DataGraph,
            network: NetworkGraph,
            caps: CapsGraph,
            profile: DeviceProfile,
            engines: PlayerEngine.Factory,
        ): ServiceMusicControl {
            val app = context.applicationContext
            val main = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
            val reporter = SessionReporter(CoroutineScope(SupervisorJob() + Dispatchers.Default))
            val clientId = clientIdOf(app)
            val env = MusicEnvironment(
                api = {
                    val session = data.session.session.value ?: error("Sign in to play")
                    network.api(session.issuer, session.me?.id)
                },
                caps = { caps.capabilities.warmUp().report },
                clientId = { clientId() + MUSIC_SUFFIX },
                reporter = reporter,
                output = {
                    EngineMusicOutput(engines.create(app, profile, music = true), main) { url ->
                        fetchPicture(network, url)
                    }
                },
                events = data.liveUpdates.events,
                playing = data.liveUpdates::playing,
                here = data.remote::here,
            )
            val playback = MusicPlayback(env, main)
            keepingSpeed(app, playback, main)
            MusicHub.playback = playback
            MusicHub.resume = remembering(playback, data, main)
            return ServiceMusicControl(app, playback)
        }

        /**
         * [playback]'s queue and place kept as they change, for Play to
         * bring back once the app has left memory ([MusicResume]): the queue
         * when it changes, the place at each track, pause and seek, and
         * every [PLACE_EVERY_MS] while it plays. The empty queue a process
         * starts with forgets nothing; one stopped does. Signed out, what
         * was written went with the sign-in ([MusicResume.signedOut]).
         */
        private fun remembering(playback: MusicPlayback, data: DataGraph, main: CoroutineScope): MusicResume {
            val snapshots = data.snapshots
            val resume = MusicResume(
                profile = { snapshots.active },
                read = { whose, key -> snapshots.read(whose, key) },
                write = { whose, key, bytes ->
                    // Only for a server still signed in, and taken back if it was signed out of as it was written: signing
                    // out deletes what was saved from the server, which a write on its way would leave behind.
                    if (snapshots.active?.server == whose.server) {
                        snapshots.write(whose, key, bytes)
                        if (snapshots.active?.server != whose.server) snapshots.delete(whose, key)
                    }
                },
                scope = main,
                signInRead = { data.signInRead() },
            )
            // A suggestion waiting for a car's Play isn't what played last: the next car is offered what's heard last then.
            val keep = { if (!playback.suggested) resume.keep(playback.queueState.value, playback.position().positionMs) }
            main.launch { playback.queueState.drop(1).collect { keep() } }
            main.launch {
                data.session.state.map { it is SessionState.SignedOut || it is SessionState.NoServer }.distinctUntilChanged().collect { out ->
                    if (out) resume.signedOut()
                }
            }
            main.launch {
                playback.nowPlaying.map { now -> now?.let { Triple(it.wantsPlay, it.seeks, it.ended) } }.distinctUntilChanged().drop(1).collect { keep() }
            }
            main.launch {
                playback.nowPlaying.map { it?.playing == true }.distinctUntilChanged().collectLatest { playing ->
                    while (playing) {
                        delay(PLACE_EVERY_MS)
                        keep()
                    }
                }
            }
            return resume
        }

        /** The speed books played at last on this device, and each new one kept as it is chosen, wherever. */
        private fun keepingSpeed(context: Context, playback: MusicPlayback, main: CoroutineScope) {
            val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
            preferences.getFloat(BOOK_SPEED, 1f).takeIf { it in BOOK_SPEEDS }?.let(playback::setBookSpeed)
            main.launch { playback.bookSpeed.drop(1).collect { preferences.edit().putFloat(BOOK_SPEED, it).apply() } }
        }

        /** Where the books' speed is kept on the device. */
        private const val PREFERENCES = "music_session"
        private const val BOOK_SPEED = "book_speed"

        /** How often the place in a playing track is kept: what Play brings back is at most this far behind. */
        private const val PLACE_EVERY_MS = 15_000L

        /** Music's player id: the device's, marked, so music and video don't end each other's sessions. */
        const val MUSIC_SUFFIX = "-music"
    }
}
