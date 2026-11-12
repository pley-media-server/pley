package app.pley.android.player.engine

import android.content.Context
import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.HttpDataSource
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.exoplayer.hls.HlsMediaSource
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.source.LoadEventInfo
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.upstream.DefaultLoadErrorHandlingPolicy
import androidx.media3.exoplayer.upstream.LoadErrorHandlingPolicy
import app.pley.android.common.Addresses
import app.pley.android.player.EngineSource
import okhttp3.Call
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException

/**
 * Where the engine's bytes come from: the media client of the server each
 * request goes to (NetworkGraph.media, which sends the Bearer token to that
 * server's own origin only), through Media3's OkHttp data source. Local
 * files and app assets (tests) are read directly.
 */
@OptIn(markerClass = [UnstableApi::class])
class MediaLoading(
    context: Context,
    /** The media client for a server's origin (`http://nas:7539`). */
    private val mediaClient: (origin: String) -> OkHttpClient,
    now: () -> Long = SystemClock::elapsedRealtime,
) {
    /** Requests go out on their own origin's client. */
    val calls: Call.Factory = Call.Factory { request: Request ->
        val url = request.url.toString()
        mediaClient(Addresses.origin(url) ?: url).newCall(request)
    }

    val http: HttpDataSource.Factory = OkHttpDataSource.Factory(calls)

    private val dataSources = DefaultDataSource.Factory(context.applicationContext, http)

    private val retries = MediaRetries(now)

    val sources: MediaSource.Factory = DefaultMediaSourceFactory(dataSources)
        .setLoadErrorHandlingPolicy(retries)

    /** HLS as [sources] makes it, but for its extractors: a Dolby Vision stream's samples come whole ([DolbyVisionSegments]). */
    private val hls: MediaSource.Factory = HlsMediaSource.Factory(dataSources)
        .setExtractorFactory(DolbyVisionSegments())
        .setLoadErrorHandlingPolicy(retries)

    /** The media source for [source], its type told by the source rather than guessed from the address. */
    fun mediaSource(source: EngineSource): MediaSource = (if (source.hls) hls else sources).createMediaSource(mediaItem(source))

    companion object {
        fun mediaItem(source: EngineSource): MediaItem = MediaItem.Builder()
            .setUri(source.url)
            .setMimeType(if (source.hls) MimeTypes.APPLICATION_M3U8 else source.mimeType)
            .build()
    }
}

/**
 * Retries for segments and files, waiting 0.5 s, then twice as long each
 * time:
 * - while the server is out of reach ([isOutOfReach]: Wi-Fi dropped, the
 *   server restarting), up to 2 s apart for up to [OUT_OF_REACH_MS], so a
 *   drop is ridden out where playback stood and it plays on soon after;
 * - an answer that is an error, up to 8 s apart, [RETRIES] times.
 * A load that reads anything starts over: a file played as it is is one
 * load from its start to its end.
 *
 * A 404 or 410 isn't retried and doesn't exclude the track: the server's
 * session is gone, and the playback session starts a new one at the same
 * position. 401 and 403 aren't either: a token the client couldn't refresh
 * won't work a second later.
 */
@OptIn(markerClass = [UnstableApi::class])
class MediaRetries(private val now: () -> Long = SystemClock::elapsedRealtime) : DefaultLoadErrorHandlingPolicy(RETRIES) {
    /** A load's failures since it last read anything. */
    private class Failures(val sinceMs: Long, val bytes: Long, val count: Int, val answers: Int)

    /** By Media3's load task id, until the load concludes. */
    private val failures = HashMap<Long, Failures>()

    override fun getRetryDelayMsFor(loadErrorInfo: LoadErrorHandlingPolicy.LoadErrorInfo): Long {
        val error = loadErrorInfo.exception
        if (isFinal(error)) return C.TIME_UNSET
        if (super.getRetryDelayMsFor(loadErrorInfo) == C.TIME_UNSET) return C.TIME_UNSET
        val outOfReach = isOutOfReach(error)
        val failed = count(loadErrorInfo.loadEventInfo, answered = !outOfReach)
        return when {
            outOfReach -> if (now() - failed.sinceMs < OUT_OF_REACH_MS) backoffMs(failed.count, OUT_OF_REACH_DELAY_MS) else C.TIME_UNSET
            else -> if (failed.answers <= RETRIES) backoffMs(failed.count) else C.TIME_UNSET
        }
    }

    /**
     * Counts a failure of [load]. Media3's own count isn't used: it carries
     * over to the first failure after a load read on.
     */
    private fun count(load: LoadEventInfo, answered: Boolean): Failures = synchronized(failures) {
        val before = failures[load.loadTaskId]?.takeIf { load.bytesLoaded <= it.bytes }
        val answers = (before?.answers ?: 0) + if (answered) 1 else 0
        Failures(before?.sinceMs ?: now(), load.bytesLoaded, (before?.count ?: 0) + 1, answers).also { failures[load.loadTaskId] = it }
    }

    override fun onLoadTaskConcluded(loadTaskId: Long) {
        synchronized(failures) { failures.remove(loadTaskId) }
    }

    override fun getFallbackSelectionFor(
        fallbackOptions: LoadErrorHandlingPolicy.FallbackOptions,
        loadErrorInfo: LoadErrorHandlingPolicy.LoadErrorInfo,
    ): LoadErrorHandlingPolicy.FallbackSelection? =
        if (isFinal(loadErrorInfo.exception)) null else super.getFallbackSelectionFor(fallbackOptions, loadErrorInfo)

    /** The retries decide when a load gives up ([getRetryDelayMsFor]), not a count. */
    override fun getMinimumLoadableRetryCount(dataType: Int): Int = Int.MAX_VALUE

    companion object {
        const val RETRIES = 5
        const val FIRST_DELAY_MS = 500L
        const val MAX_DELAY_MS = 8_000L

        /**
         * How long a load is retried while the server is out of reach: as
         * long as the server waits for a player it doesn't hear from.
         */
        const val OUT_OF_REACH_MS = 90_000L

        /**
         * The longest wait between tries while the server is out of reach:
         * a try costs nothing then, and the next one comes within 2 s of
         * the network coming back rather than 8.
         */
        const val OUT_OF_REACH_DELAY_MS = 2_000L

        /**
         * The server is out of reach: no answer came (no network, the
         * connection refused, dropped or timed out), or a gateway in front
         * of it answered that it can't reach it (502, 503, 504).
         */
        fun isOutOfReach(error: Throwable?): Boolean {
            var e = error
            while (e != null) {
                if (e is HttpDataSource.InvalidResponseCodeException) return e.responseCode in GATEWAY_CODES
                if (e is HttpDataSource.HttpDataSourceException && e.reason in NO_ANSWER) return true
                e = e.cause
            }
            return false
        }

        private val GATEWAY_CODES = setOf(502, 503, 504)
        private val NO_ANSWER = setOf(
            PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
            PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT,
        )

        /** The wait before retry [errorCount] (1 for the first), [maxMs] at most. */
        fun backoffMs(errorCount: Int, maxMs: Long = MAX_DELAY_MS): Long {
            val shift = (errorCount - 1).coerceIn(0, 10)
            return (FIRST_DELAY_MS shl shift).coerceAtMost(maxMs)
        }

        /** HTTP answers that retrying won't change. */
        fun isFinal(error: Throwable?): Boolean {
            var e = error
            while (e != null) {
                if (e is HttpDataSource.InvalidResponseCodeException) return e.responseCode in FINAL_CODES
                e = e.cause
            }
            return false
        }

        private val FINAL_CODES = setOf(401, 403, 404, 410)
    }
}

/**
 * Whether the engine is reconnecting: a load failed with the server out of
 * reach ([MediaRetries.isOutOfReach]), Media3 tries it again, and nothing
 * came in since (no load completed, the media buffered no further).
 */
class ReconnectWatcher {
    var reconnecting = false
        private set

    /** How far the media was buffered at the last failure. */
    private var bufferedMs = 0L

    /** A seek moved the buffer: where it starts settles after the seek. */
    private var sought = false

    /** A load failed with [error], the media buffered to [bufferedMs]; [gaveUp] when Media3 won't try it again. */
    fun failed(error: IOException, gaveUp: Boolean, bufferedMs: Long) {
        // An error the server itself answers means it is there.
        reconnecting = !gaveUp && MediaRetries.isOutOfReach(error)
        this.bufferedMs = bufferedMs
        sought = false
    }

    /** A load completed: the server answers again. */
    fun completed() {
        reconnecting = false
    }

    /** The media is buffered to [bufferedMs] now. */
    fun buffered(bufferedMs: Long) {
        if (sought) {
            sought = false
            this.bufferedMs = bufferedMs
        } else if (bufferedMs > this.bufferedMs) {
            reconnecting = false
        }
    }

    fun sought() {
        sought = true
    }

    fun reset() {
        reconnecting = false
        sought = false
    }
}
