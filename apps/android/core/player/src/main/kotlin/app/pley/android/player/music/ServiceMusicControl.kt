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
import app.pley.android.model.ItemSummary
import app.pley.android.network.NetworkGraph
import app.pley.android.player.MusicControl
import app.pley.android.player.MusicPosition
import app.pley.android.player.MusicQueueState
import app.pley.android.player.NowPlaying
import app.pley.android.player.PlayerEngine
import app.pley.android.player.session.SessionReporter
import app.pley.android.player.session.clientIdOf
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import com.google.common.util.concurrent.ListenableFuture
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Request
import okhttp3.Response

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

    override fun toggle() = onMain {
        playback.toggle()
        connect()
    }

    override fun next() = onMain { playback.next() }

    override fun previous() = onMain { playback.previous() }

    override fun seekTo(positionMs: Long) = onMain { playback.seekTo(positionMs) }

    override fun jumpTo(index: Int) = onMain { playback.jumpTo(index) }

    override fun setShuffle(shuffle: Boolean) = onMain { playback.setShuffle(shuffle) }

    override fun stop() = onMain {
        playback.stop()
        disconnect()
    }

    override fun pauseForVideo() = onMain { playback.pauseForVideo() }

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
                        fetch(network, url)
                    }
                },
                events = data.liveUpdates.events,
                playing = data.liveUpdates::playing,
                here = data.remote::here,
            )
            val playback = MusicPlayback(env, main)
            MusicHub.playback = playback
            return ServiceMusicControl(app, playback)
        }

        /** Music's player id: the device's, marked, so music and video don't end each other's sessions. */
        const val MUSIC_SUFFIX = "-music"

        /** The notification's art, through the pictures' client (it carries the server's token). */
        private suspend fun fetch(network: NetworkGraph, url: String): ByteArray? = withContext(Dispatchers.IO) {
            val call = network.images.newCall(Request.Builder().url(url).build())
            suspendCancellableCoroutine { continuation ->
                continuation.invokeOnCancellation { call.cancel() }
                call.enqueue(
                    object : Callback {
                        override fun onFailure(call: Call, e: IOException) {
                            continuation.resumeWith(Result.success(null))
                        }

                        override fun onResponse(call: Call, response: Response) {
                            val bytes = response.use { r ->
                                if (!r.isSuccessful) null else runCatching { r.body.bytes() }.getOrNull()?.takeIf { it.size <= MAX_ART_BYTES }
                            }
                            continuation.resumeWith(Result.success(bytes))
                        }
                    },
                )
            }
        }

        /** Art bigger than this isn't put in the notification. */
        private const val MAX_ART_BYTES = 2 * 1024 * 1024
    }
}
