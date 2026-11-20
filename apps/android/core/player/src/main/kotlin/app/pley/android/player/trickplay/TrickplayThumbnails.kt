package app.pley.android.player.trickplay

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BitmapRegionDecoder
import android.graphics.Rect
import android.os.Build
import app.pley.android.model.TrickplayInfo
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.network.PleyApi
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

/** Where a thumbnail is on its sheet, in pixels. */
data class TileRegion(val sheet: Int, val left: Int, val top: Int, val right: Int, val bottom: Int)

/** The seek previews' arithmetic (docs/API.md, GET /api/files/{id}/trickplay). */
object TrickplayTiles {
    /** Thumbnail [k]'s sheet and its place there: column `k % columns`, row `(k / columns) % rows`. */
    fun regionOf(info: TrickplayInfo, k: Int): TileRegion {
        val (column, row) = info.cellOf(k)
        val left = column * info.width
        val top = row * info.height
        return TileRegion(info.sheetOf(k), left, top, left + info.width, top + info.height)
    }
}

/** Downloads a sheet's bytes. */
fun interface SheetSource {
    suspend fun bytes(url: String): ByteArray
}

/** Sheets through the images client (NetworkGraph.images), on OkHttp's threads. */
class OkHttpSheetSource(private val client: () -> OkHttpClient, private val io: CoroutineDispatcher = Dispatchers.IO) : SheetSource {
    override suspend fun bytes(url: String): ByteArray {
        val http = withContext(io) { client() }
        return suspendCancellableCoroutine { continuation ->
            val call = http.newCall(Request.Builder().url(url).build())
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
                                Result.success(it.body.bytes())
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
}

/** Cuts thumbnails out of a sheet without decoding all of it. */
interface SheetDecoder {
    interface Sheet {
        fun decode(region: TileRegion, lowMemory: Boolean): Bitmap?

        fun close()
    }

    fun open(bytes: ByteArray): Sheet?
}

/** BitmapRegionDecoder: a 3200×1800 sheet is never decoded whole, only the tile asked for. */
object RegionSheetDecoder : SheetDecoder {
    override fun open(bytes: ByteArray): SheetDecoder.Sheet? {
        val decoder = try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                BitmapRegionDecoder.newInstance(bytes, 0, bytes.size)
            } else {
                @Suppress("DEPRECATION")
                BitmapRegionDecoder.newInstance(bytes, 0, bytes.size, false)
            }
        } catch (_: IOException) {
            null
        } ?: return null
        return object : SheetDecoder.Sheet {
            override fun decode(region: TileRegion, lowMemory: Boolean): Bitmap? {
                val options = BitmapFactory.Options().apply {
                    inPreferredConfig = if (lowMemory) Bitmap.Config.RGB_565 else Bitmap.Config.ARGB_8888
                }
                return try {
                    decoder.decodeRegion(Rect(region.left, region.top, region.right, region.bottom), options)
                } catch (_: IllegalArgumentException) {
                    null
                }
            }

            override fun close() = decoder.recycle()
        }
    }
}

/** A least-recently-used map of at most [max] entries; [onEvict] lets go of what leaves. */
internal class Lru<K, V>(private val max: Int, private val onEvict: (V) -> Unit = {}) {
    private val map = LinkedHashMap<K, V>(16, 0.75f, true)

    val size: Int get() = map.size

    operator fun get(key: K): V? = map[key]

    operator fun set(key: K, value: V) {
        map.put(key, value)?.takeIf { it !== value }?.let(onEvict)
        while (map.size > max) {
            val eldest = map.entries.iterator().next()
            map.remove(eldest.key)
            onEvict(eldest.value)
        }
    }

    fun clear() {
        map.values.forEach(onEvict)
        map.clear()
    }
}

/**
 * The seek previews of one file: GET /api/files/{id}/trickplay (asked
 * again every 20 s for up to 10 minutes while the server makes them), the
 * sheets through the images client, and thumbnails cut from them off the
 * main thread into an LRU of 40 (16 on the smallest TVs, in RGB_565 there).
 */
class TrickplayThumbnails(
    private val scope: CoroutineScope,
    private val api: PleyApi,
    private val fileId: Long,
    private val sheets: SheetSource,
    private val decoder: SheetDecoder = RegionSheetDecoder,
    private val lowMemory: Boolean = false,
    private val work: CoroutineDispatcher = Dispatchers.Default,
) {
    private val state = MutableStateFlow<TrickplayInfo?>(null)

    /** The previews; `null` until the server has them. */
    val info: StateFlow<TrickplayInfo?> = state.asStateFlow()

    private val lock = Mutex()
    private val tiles = Lru<Int, Bitmap>(if (lowMemory) TILES_LOW else TILES)
    private val openSheets = Lru<Int, SheetDecoder.Sheet>(SHEETS_OPEN) { it.close() }
    private var job: Job? = null

    /** Asks for the previews, again every 20 s while they are being made. */
    fun start() {
        if (job != null) return
        job = scope.launch {
            var waited = 0L
            while (true) {
                val found = try {
                    api.trickplay(fileId)
                } catch (e: CancellationException) {
                    throw e
                } catch (_: IOException) {
                    null
                }
                if (found != null && found.count > 0) {
                    state.value = found
                    return@launch
                }
                if (waited >= GIVE_UP_MS) return@launch
                delay(AGAIN_MS)
                waited += AGAIN_MS
            }
        }
    }

    /** The thumbnail for [positionMs]; `null` without previews or when its sheet can't be read. */
    suspend fun thumbnail(positionMs: Long): Bitmap? {
        val info = state.value ?: return null
        val k = info.thumbnailAt(positionMs)
        lock.withLock { tiles[k] }?.let { return it }
        val region = TrickplayTiles.regionOf(info, k)
        return lock.withLock {
            tiles[k] ?: run {
                val sheet = openSheets[region.sheet] ?: openSheet(info, region.sheet)?.also { openSheets[region.sheet] = it } ?: return null
                val bitmap = withContext(work) { sheet.decode(region, lowMemory) } ?: return null
                tiles[k] = bitmap
                bitmap
            }
        }
    }

    private suspend fun openSheet(info: TrickplayInfo, sheet: Int): SheetDecoder.Sheet? {
        val bytes = try {
            sheets.bytes(api.trickplaySheetUrl(fileId, sheet, info.id))
        } catch (e: CancellationException) {
            throw e
        } catch (_: IOException) {
            return null
        }
        return withContext(work) { decoder.open(bytes) }
    }

    /** Lets go of the sheets and tiles. */
    fun close() {
        job?.cancel()
        job = null
        if (lock.tryLock()) {
            try {
                openSheets.clear()
                tiles.clear()
            } finally {
                lock.unlock()
            }
        }
    }

    companion object {
        const val AGAIN_MS = 20_000L
        const val GIVE_UP_MS = 10 * 60_000L
        const val TILES = 40
        const val TILES_LOW = 16

        /** Sheets kept open: the one being scrubbed and its neighbor. */
        const val SHEETS_OPEN = 2
    }
}
