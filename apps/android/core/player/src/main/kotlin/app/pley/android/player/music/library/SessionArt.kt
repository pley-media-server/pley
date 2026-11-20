package app.pley.android.player.music.library

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.util.LruCache
import androidx.annotation.OptIn
import androidx.media3.common.util.BitmapLoader
import androidx.media3.common.util.UnstableApi
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import java.io.IOException
import java.util.concurrent.Executor
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The covers the media session shows for what plays (the notification,
 * the lock screen, the system's media controls and a car's Now Playing),
 * kept once read. The session says what plays the moment the track
 * changes, with its cover only if it has it then; otherwise the cover
 * follows in a second update, and a car draws Now Playing without one in
 * between. A cover kept here is there at once: the next track of an album
 * has its album's, and [warm] reads the covers of the tracks around the
 * one playing ahead of them.
 *
 * It reads the library's pictures ([MusicArt]'s addresses) straight from
 * the server, as the app is allowed to; anything else (a placeholder the
 * provider draws) goes to [other], kept the same way. Main thread only.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class SessionArt(
    private val scope: CoroutineScope,
    /** The server path an address names, `null` when it isn't one of the library's ([MusicArt.pathOf]). */
    private val pathOf: (Uri) -> String?,
    /** The picture's bytes ([MusicLibraryGraph.picture]). */
    private val picture: suspend (path: String) -> ByteArray?,
    private val other: BitmapLoader,
    private val decode: (ByteArray) -> Bitmap? = ::decodeCover,
    /** Whether an address is a cover the app draws itself ([MusicArt.placeholderUri]): read ahead as the library's. */
    private val drawn: (Uri) -> Boolean = { false },
) : BitmapLoader {
    private val kept = LruCache<Uri, Bitmap>(KEPT)
    private val reading = HashMap<Uri, ListenableFuture<Bitmap>>()

    /** Counts [clear]s: a cover read for the profile before is neither kept nor shown. */
    private var generation = 0

    override fun supportsMimeType(mimeType: String): Boolean = other.supportsMimeType(mimeType)

    override fun decodeBitmap(data: ByteArray): ListenableFuture<Bitmap> = other.decodeBitmap(data)

    override fun loadBitmap(uri: Uri): ListenableFuture<Bitmap> {
        kept.get(uri)?.let { return Futures.immediateFuture(it) }
        reading[uri]?.let { return it }
        val path = pathOf(uri) ?: return keep(uri, other.loadBitmap(uri))
        val future = SettableFuture.create<Bitmap>()
        val mine = generation
        reading[uri] = future
        scope.launch {
            val bitmap = try {
                picture(path)?.let { bytes -> withContext(Dispatchers.Default) { decode(bytes) } }
            } catch (e: CancellationException) {
                reading.remove(uri, future)
                future.cancel(false)
                throw e
            } catch (_: Exception) {
                null
            }
            reading.remove(uri, future)
            if (mine != generation) {
                future.setException(IOException("Another profile's cover"))
            } else if (bitmap != null) {
                kept.put(uri, bitmap)
                future.set(bitmap)
            } else {
                // Not kept: asked again the next time it is shown.
                future.setException(IOException("No cover"))
            }
        }
        return future
    }

    /** [future], [other]'s reading of [uri], kept once it is in as the library's pictures are. */
    private fun keep(uri: Uri, future: ListenableFuture<Bitmap>): ListenableFuture<Bitmap> {
        val mine = generation
        reading[uri] = future
        future.addListener({
            reading.remove(uri, future)
            val bitmap = runCatching { Futures.getDone(future) }.getOrNull()
            if (bitmap != null && mine == generation) kept.put(uri, bitmap)
        }, MAIN)
        return future
    }

    /**
     * Reads [uris]' covers ahead of their tracks, the library's and those the
     * app draws; another address isn't fetched for a track that may never play.
     * Those kept or on their way already cost nothing.
     */
    fun warm(uris: List<Uri?>) {
        for (uri in uris) if (uri != null && (pathOf(uri) != null || drawn(uri))) loadBitmap(uri)
    }

    /** Forgets every cover, and those still on their way: another profile's are not this one's. */
    fun clear() {
        generation++
        kept.evictAll()
        reading.clear()
    }

    private companion object {
        /** The covers kept: the one playing, those around it, and a few albums back. */
        const val KEPT = 8

        /** Where [keep] hears that a reading is done: the main thread, as everything here. */
        private val MAIN = Executor { Handler(Looper.getMainLooper()).post(it) }
    }
}

/** A cover's picture, no larger than a car's Now Playing shows it; `null` when it isn't a picture. */
private fun decodeCover(bytes: ByteArray): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= MAX_COVER_PX) sample *= 2
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
}

/** The server sends covers 512 px wide (MusicLibraryGraph); a larger one is halved until under twice that. */
private const val MAX_COVER_PX = 512
