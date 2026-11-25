package app.pley.android.player.music

import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

/**
 * A picture of the server's for music's notification and for cars, through
 * [client], a pictures' client (it carries the server's token); `null`
 * when it can't be had, or is bigger than [MAX_PICTURE_BYTES].
 */
internal suspend fun fetchPicture(client: OkHttpClient, url: String): ByteArray? = withContext(Dispatchers.IO) {
    val call = try {
        client.newCall(Request.Builder().url(url).build())
    } catch (_: IllegalArgumentException) {
        // Not an address OkHttp takes.
        return@withContext null
    }
    suspendCancellableCoroutine { continuation ->
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    continuation.resumeWith(Result.success(null))
                }

                override fun onResponse(call: Call, response: Response) {
                    val bytes = response.use { r -> if (r.isSuccessful) runCatching { bounded(r) }.getOrNull() else null }
                    continuation.resumeWith(Result.success(bytes))
                }
            },
        )
    }
}

/**
 * Pictures fetched once at a time: a second ask for an address still on its
 * way waits for the first one's answer rather than fetching it again. The
 * media session reads the covers around the track playing ahead of it
 * (SessionArt) as the car asks for the same ones through the provider
 * (MusicArtProvider), and both come in together at each change of track.
 * A fetch belongs to [scope], not to whoever asked first: one who stops
 * waiting (a car's 10 seconds) leaves it to the others, and to the cache.
 */
internal class SharedPictures(
    private val scope: CoroutineScope,
    private val fetch: suspend (client: OkHttpClient, url: String) -> ByteArray? = ::fetchPicture,
) {
    /** By client (each profile's has its own cache) and address. */
    private val fetching = ConcurrentHashMap<Pair<OkHttpClient, String>, Deferred<ByteArray?>>()

    suspend fun get(client: OkHttpClient, url: String): ByteArray? {
        val key = client to url
        val made = scope.async(start = CoroutineStart.LAZY) { fetch(client, url) }
        val shared = fetching.putIfAbsent(key, made)
        if (shared != null) {
            made.cancel()
            return shared.await()
        }
        made.invokeOnCompletion { fetching.remove(key, made) }
        made.start()
        return made.await()
    }
}

/** [response]'s body, read only as far as [MAX_PICTURE_BYTES]: `null` when it is bigger. */
private fun bounded(response: Response): ByteArray? {
    if (response.body.contentLength() > MAX_PICTURE_BYTES) return null
    val source = response.body.source()
    // True when that much and one byte more came: too big, and nothing more is read.
    if (source.request(MAX_PICTURE_BYTES + 1L)) return null
    return source.buffer.readByteArray()
}

/** Pictures bigger than this aren't shown. */
private const val MAX_PICTURE_BYTES = 2 * 1024 * 1024
