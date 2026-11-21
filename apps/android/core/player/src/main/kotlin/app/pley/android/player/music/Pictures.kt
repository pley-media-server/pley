package app.pley.android.player.music

import app.pley.android.network.NetworkGraph
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Request
import okhttp3.Response

/**
 * A picture of the server's for music's notification and for cars, through
 * the pictures' client (it carries the server's token); `null` when it
 * can't be had, or is bigger than [MAX_PICTURE_BYTES].
 */
internal suspend fun fetchPicture(network: NetworkGraph, url: String): ByteArray? = withContext(Dispatchers.IO) {
    val call = try {
        network.images.newCall(Request.Builder().url(url).build())
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
