package app.pley.android.player.session

import app.pley.android.common.Addresses
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import java.io.IOException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okio.blackholeSink

/**
 * Gets a new session's stream ready while the last one plays on (a quality,
 * track or Automatic change): it is asked for as the engine will ask for
 * it, so the transcoder makes the segment the player starts with before
 * the player moves there (apps/web/src/player/prepare.ts).
 */
fun interface StreamWarmer {
    /** Asks for the HLS stream at [url] as a player starting at [startMs] would; returns once its segment has come. */
    suspend fun warm(url: String, startMs: Long)

    companion object {
        /** Asks for nothing: the player moves over at once. */
        val NONE = StreamWarmer { _, _ -> }
    }
}

/** What a warm-up reads of HLS playlists. */
object HlsWarmup {
    /**
     * The sizes a multivariant playlist lists, in its order; none for a
     * media playlist. Media3 loads the first one listed to prepare the
     * stream, and the server starts making that one first.
     */
    fun variants(playlist: String, base: HttpUrl): List<HttpUrl> {
        val lines = playlist.lineSequence().map { it.trim() }.toList()
        return lines.indices.filter { lines[it].startsWith("#EXT-X-STREAM-INF") }.mapNotNull { i ->
            lines.drop(i + 1).firstOrNull { it.isNotEmpty() && !it.startsWith("#") }?.let(base::resolve)
        }
    }

    /** The segment of a media playlist that holds [positionMs]: the last one for a time past its end. */
    fun segmentAt(playlist: String, base: HttpUrl, positionMs: Long): HttpUrl? {
        val seconds = positionMs / 1000.0
        var at = 0.0
        var duration: Double? = null
        var last: HttpUrl? = null
        for (raw in playlist.lineSequence()) {
            val line = raw.trim()
            if (line.startsWith("#EXTINF:")) {
                duration = line.removePrefix("#EXTINF:").substringBefore(',').trim().toDoubleOrNull()
            } else if (line.isNotEmpty() && !line.startsWith("#") && duration != null) {
                last = base.resolve(line) ?: continue
                if (seconds < at + duration) return last
                at += duration
                duration = null
            }
        }
        return last
    }
}

/**
 * [StreamWarmer] over the media client of the server each request goes to
 * (the engine's: NetworkGraph.media, which sends the Bearer token to that
 * server's own origin only). It asks for the playlist, the first size
 * listed of a stream in several, and the segment that holds the start.
 */
class OkHttpStreamWarmer(
    private val mediaClient: (origin: String) -> OkHttpClient,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : StreamWarmer {
    override suspend fun warm(url: String, startMs: Long) {
        var playlistUrl = url.toHttpUrlOrNull() ?: return
        val http = withContext(io) { mediaClient(Addresses.origin(url) ?: url) }
        var playlist = http.read(playlistUrl) { it.body.string() }
        HlsWarmup.variants(playlist, playlistUrl).firstOrNull()?.let { variant ->
            playlistUrl = variant
            playlist = http.read(variant) { it.body.string() }
        }
        val segment = HlsWarmup.segmentAt(playlist, playlistUrl, startMs) ?: return
        // Read to its end: the segment is whole once the transcoder made it.
        http.read(segment) { it.body.source().readAll(blackholeSink()) }
    }

    /** Asks for [url] and reads its answer with [read], on OkHttp's threads; canceling cancels the call. */
    private suspend fun <T> OkHttpClient.read(url: HttpUrl, read: (Response) -> T): T = suspendCancellableCoroutine { continuation ->
        val call = newCall(Request.Builder().url(url).build())
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    continuation.resumeWith(Result.failure(OfflineException(e)))
                }

                override fun onResponse(call: Call, response: Response) {
                    val result = try {
                        response.use {
                            if (!it.isSuccessful) throw ApiException.from(it)
                            Result.success(read(it))
                        }
                    } catch (e: IOException) {
                        Result.failure(e)
                    }
                    continuation.resumeWith(result)
                }
            },
        )
    }
}
