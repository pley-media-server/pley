package app.pley.android.player.music

import android.os.Bundle
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.MediaMetadata
import androidx.media3.common.ParserException
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.HttpDataSource
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * [MusicOutput] on the engine's audio-only player ([PlayerEngine.Factory.create]
 * with `music`): no video decoder, audio focus handled by Media3, offload
 * with the screen off. Each track's title, artist, album and art go on the
 * player's item, which the media session shows in the notification and on
 * the lock screen; the art is fetched by [artwork] (the server's pictures
 * need its token) and added once it arrives.
 */
@OptIn(markerClass = [UnstableApi::class])
class EngineMusicOutput(
    private val engine: PlayerEngine,
    private val scope: CoroutineScope,
    private val artwork: suspend (url: String) -> ByteArray?,
) : MusicOutput {
    val player: Player get() = engine.player

    private var listener: MusicOutput.Listener? = null
    private var artJob: Job? = null
    private var buffering = false

    init {
        engine.player.addListener(
            object : Player.Listener {
                override fun onIsPlayingChanged(isPlaying: Boolean) {
                    listener?.onPlayingChanged(isPlaying)
                }

                override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
                    listener?.onPlayWhenReadyChanged(playWhenReady)
                }

                override fun onPlaybackStateChanged(playbackState: Int) {
                    val now = playbackState == Player.STATE_BUFFERING
                    if (now != buffering) {
                        buffering = now
                        listener?.onBuffering(now)
                    }
                    if (playbackState == Player.STATE_ENDED) listener?.onEnded()
                }

                override fun onPlayerError(error: PlaybackException) {
                    // The engine prepares again after a failed audio output itself.
                    if (error.errorCode in ENGINE_RESTARTS) return
                    // Release builds log no stack traces: why a track didn't play, without its address.
                    Log.w(TAG, "track failed: ${describeFailure(error)}")
                    listener?.onError(kindOf(error))
                }
            },
        )
    }

    override fun load(source: EngineSource, startMs: Long, play: Boolean, info: TrackInfo) {
        artJob?.cancel()
        engine.prepare(source, startMs, play)
        describe(info, art = null)
        val url = info.artUrl ?: return
        artJob = scope.launch {
            val bytes = try {
                artwork(url)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                null
            }
            if (bytes != null) describe(info, bytes)
        }
    }

    /** Puts [info] on the item playing; the same address and type, so the player only updates what it shows. */
    private fun describe(info: TrackInfo, art: ByteArray?) {
        val p = engine.player
        val item = p.currentMediaItem ?: return
        val metadata = MediaMetadata.Builder()
            .setTitle(info.title)
            .setArtist(info.artist)
            .setAlbumTitle(info.album)
            .setAlbumArtist(info.artist)
            .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
            .setIsPlayable(true)
            .setIsBrowsable(false)
            .apply { if (art != null) setArtworkData(art, MediaMetadata.PICTURE_TYPE_FRONT_COVER) }
            .apply { info.id?.let { setExtras(Bundle().apply { putLong(TRACK_ID, it) }) } }
            .build()
        p.replaceMediaItem(p.currentMediaItemIndex, item.buildUpon().setMediaMetadata(metadata).build())
    }

    override fun play() {
        val p = engine.player
        if (p.playbackState == Player.STATE_IDLE && p.currentMediaItem != null) p.prepare()
        p.play()
    }

    override fun pause() = engine.player.pause()

    override fun seekTo(positionMs: Long) = engine.player.seekTo(positionMs)

    override fun setSpeed(speed: Float) {
        if (engine.player.playbackParameters.speed != speed) engine.player.setPlaybackSpeed(speed)
    }

    override fun stop() {
        artJob?.cancel()
        engine.player.stop()
    }

    override fun release() {
        artJob?.cancel()
        listener = null
        engine.release()
    }

    override val positionMs: Long get() = engine.player.currentPosition

    override val durationMs: Long get() = engine.player.duration.takeIf { it > 0 } ?: 0

    override val bufferedMs: Long get() = engine.player.bufferedPosition

    override val playWhenReady: Boolean get() = engine.player.playWhenReady

    override fun setListener(listener: MusicOutput.Listener?) {
        this.listener = listener
    }

    companion object {
        private const val TAG = "pley.music"

        /** The most causes, and characters of a parser's message, a failure's log line holds. */
        private const val MAX_CAUSES = 6
        private const val MAX_MESSAGE = 120

        /** An address in a parser's message (`https://…`, a relative one's scheme-less part stays): left out of the log. */
        private val ADDRESS = Regex("""[A-Za-z][A-Za-z0-9+.-]*://\S+""")

        /** The extra of the player's item's metadata that holds the track's id. */
        const val TRACK_ID = "pley.track"

        /** Errors the engine recovers from by preparing again (AudioRestarts). */
        private val ENGINE_RESTARTS = setOf(
            PlaybackException.ERROR_CODE_AUDIO_TRACK_INIT_FAILED,
            PlaybackException.ERROR_CODE_AUDIO_TRACK_WRITE_FAILED,
        )

        /** [error]'s code and causes, for the log: their kinds, an HTTP status, a parser's words; never an address. */
        internal fun describeFailure(error: PlaybackException): String {
            val causes = generateSequence(error.cause) { it.cause }.take(MAX_CAUSES).map { cause ->
                when (cause) {
                    is HttpDataSource.InvalidResponseCodeException -> "HTTP ${cause.responseCode}"
                    // A parser quotes the line it couldn't read, which can be a playlist's address.
                    is ParserException -> "${cause.javaClass.simpleName}: ${cause.message?.replace(ADDRESS, "<address>")?.take(MAX_MESSAGE)}"
                    else -> cause.javaClass.simpleName
                }
            }
            return (sequenceOf(error.errorCodeName) + causes).joinToString(" < ")
        }

        /** What [error] means for the queue. */
        fun kindOf(error: PlaybackException): OutputError {
            var cause: Throwable? = error.cause
            while (cause != null) {
                if (cause is HttpDataSource.InvalidResponseCodeException) {
                    return when (cause.responseCode) {
                        404 -> OutputError.Gone
                        410 -> OutputError.Stopped
                        else -> OutputError.Other
                    }
                }
                cause = cause.cause
            }
            return when (error.errorCode) {
                PlaybackException.ERROR_CODE_DECODER_INIT_FAILED,
                PlaybackException.ERROR_CODE_DECODER_QUERY_FAILED,
                PlaybackException.ERROR_CODE_DECODING_FAILED,
                PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES,
                PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED,
                PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED,
                PlaybackException.ERROR_CODE_PARSING_CONTAINER_MALFORMED,
                PlaybackException.ERROR_CODE_PARSING_MANIFEST_MALFORMED,
                PlaybackException.ERROR_CODE_AUDIO_TRACK_OFFLOAD_INIT_FAILED,
                PlaybackException.ERROR_CODE_AUDIO_TRACK_OFFLOAD_WRITE_FAILED,
                -> OutputError.Decode
                PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
                PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT,
                PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
                PlaybackException.ERROR_CODE_TIMEOUT,
                -> OutputError.Network
                PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND -> OutputError.Gone
                else -> OutputError.Other
            }
        }
    }
}
