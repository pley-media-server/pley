package app.pley.android.downloads

import app.pley.android.data.DownloadState
import app.pley.android.model.ConversionState
import app.pley.android.model.ItemDetail
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PleyJson
import app.pley.android.model.StreamKind
import app.pley.android.model.caps.ClientCaps
import app.pley.android.network.ApiException
import app.pley.android.network.HttpPleyApi
import app.pley.android.network.PleyApi
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
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

/**
 * What a download needs of a server: its API, and clients for the file,
 * subtitles and pictures; and what this device plays, for a converted one.
 */
internal interface DownloadSources {
    fun api(server: String, userId: Long): PleyApi

    fun media(server: String): OkHttpClient

    fun subtitles(server: String): OkHttpClient

    val images: OkHttpClient

    /** This device's capabilities, as a playback sends them; `null` while unknown. */
    suspend fun caps(): ClientCaps? = null
}

/** The server's converted file went (it keeps them a week): it is asked for again. */
private class ConversionGone : IOException("the converted file is gone")

/**
 * Fetches downloads one at a time, oldest first: first the item's
 * details, a picture and its text subtitles as WebVTT, then the file
 * itself, picked up where it stopped (Range) after a lost network or a
 * restart. A subtitle track the server is still reading out of the file
 * (minutes for a big one on a network share) is saved as far as it goes
 * and asked for again once the file is down, and on later runs while it
 * still falls short. A download the server converts for this device is asked for,
 * then left while the server makes it (the others go on meanwhile), and
 * looked at again every few seconds until it is whole. A network
 * downloads may not use, or none, makes it wait; errors the server gives
 * fail the download until it is retried.
 */
internal class DownloadRunner(
    private val store: DownloadStore,
    private val sources: DownloadSources,
    private val gate: NetworkGate,
    /** Whether mobile data may be used now (Settings.downloadOnMobileData). */
    private val mobileAllowed: suspend () -> Boolean,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val clock: () -> Long = System::currentTimeMillis,
    /** How often a conversion the server is making is looked at. */
    private val conversionPollMs: Long = CONVERSION_POLL_MS,
    /** The wait after a lost network or server, longer each time. */
    private val backoffMs: Long = BACKOFF_MS,
    /**
     * The server and profile signed in: only their downloads run, as the
     * server's token is that profile's. `null`: nobody is, and none runs.
     */
    private val profile: () -> Pair<String, Long>?,
) {
    /** Subtitles are caught up one download at a time: the queue's, the app's opening and a run's never write the same file at once. */
    private val catchingUp = Mutex()

    @Volatile
    private var current: Pair<Key, Job>? = null

    /** Stops the download of [key] if it is the one running (it was removed). */
    fun cancel(key: Key) {
        current?.takeIf { it.first == key }?.second?.cancel()
    }

    /**
     * Tells the server that [saved], deleted, no longer wants the file it
     * was making for it, which then stops. A file made stays on the server
     * for whoever asks for the same; away from the server, the server
     * drops a waiting one itself once nobody asks about it for an hour.
     */
    suspend fun forget(saved: Saved) {
        val id = saved.conversion ?: return
        if (saved.state == DownloadState.Done || saved.mediaUrl != null) return
        try {
            sources.api(saved.server, saved.userId).cancelConversion(id)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // Away from the server, or gone already.
        }
    }

    /**
     * Runs downloads until none is left to do, or none can go on: no
     * network they may use for [waitMs]. Returns whether some still wait.
     * Subtitles that downloads kept cut short are asked for first.
     */
    suspend fun run(waitMs: Long = WAIT_MS): Boolean = coroutineScope {
        // Beside the queue: a server out of reach mustn't hold the downloads up.
        profile()?.let { launch { finishCutSubtitles(it) } }
        runQueue(waitMs)
    }

    private suspend fun runQueue(waitMs: Long): Boolean {
        var failures = 0
        while (true) {
            currentCoroutineContext().ensureActive()
            val signedIn = profile() ?: return false
            val pending = store.all.value.filter {
                (it.state == DownloadState.Queued || it.state == DownloadState.Waiting) &&
                    it.server == signedIn.first && it.userId == signedIn.second
            }
            if (pending.isEmpty()) return false
            val now = clock()
            val next = pending.firstOrNull { it.notBefore <= now }
            if (next == null) {
                // Every one left is being made by the server, or waits after a loss: look again soon.
                delay((pending.minOf { it.notBefore } - now).coerceIn(0, conversionPollMs))
                continue
            }
            val mobile = mobileAllowed()
            if (!gate.open(mobile, next.server)) {
                markWaiting()
                if (!gate.await(mobile, waitMs, next.server)) return true
                continue
            }
            when (runOne(next)) {
                Outcome.Done, Outcome.Removed, Outcome.Converting, Outcome.Failed -> failures = 0
                Outcome.Lost -> {
                    // The network went, or the server: that one waits a little longer
                    // each time, while the others have their turn.
                    failures++
                    if (failures > MAX_LOST_IN_A_ROW) return true
                    store.update(next.key, persist = false) { it.copy(notBefore = clock() + backoffMs * failures) }
                }
            }
        }
    }

    private enum class Outcome { Done, Removed, Failed, Lost, Converting }

    private suspend fun runOne(saved: Saved): Outcome {
        val key = saved.key
        // It stays Queued while the server makes its file: Downloading is for bytes coming.
        store.update(key, persist = false) { it.copy(failure = null, error = null, status = null, unreachable = false) } ?: return Outcome.Removed
        val job = Job(currentCoroutineContext()[Job])
        current = key to job
        return try {
            withContext(job) {
                var s = store.get(key) ?: return@withContext Outcome.Removed
                if (!s.extrasSaved) s = saveExtras(s)
                if (s.convert && s.mediaUrl == null) s = converted(s) ?: return@withContext Outcome.Converting
                s = keep(key) { it.copy(state = DownloadState.Downloading) }
                fetchMedia(s)
                s = store.update(key) { it.copy(state = DownloadState.Done, downloadedBytes = it.sizeBytes, failure = null, error = null, status = null) } ?: s
                // Read whole by the server meanwhile, as a rule; else when the app opens again.
                catchingUp.withLock { finishSubtitles(s) }
                Outcome.Done
            }
        } catch (e: CancellationException) {
            if (!currentCoroutineContext().isActive) {
                // Every download stopped (Android's time for the service ran out): it waits its turn again.
                store.update(key, persist = false) { if (it.state == DownloadState.Downloading) it.copy(state = DownloadState.Queued) else it }
                throw e
            }
            // Only this download was stopped: it was removed.
            Outcome.Removed
        } catch (e: DownloadFailure) {
            store.update(key) { it.copy(state = DownloadState.Failed, failure = e.failure, error = e.detail, status = null) }
            Outcome.Failed
        } catch (e: ApiException) {
            store.update(key) { it.copy(state = DownloadState.Failed).failedWith(e) }
            Outcome.Failed
        } catch (_: ConversionGone) {
            // The server forgot it (it restarted) or pruned the file: asked for again in a while.
            // The same conversion goes on with what is down already.
            store.update(key) {
                it.copy(state = DownloadState.Queued, mediaUrl = null, converting = 0f, notBefore = clock() + conversionPollMs)
            }
            Outcome.Converting
        } catch (_: IOException) {
            // Waiting for the server, unless the network it may use went.
            val away = gate.open(mobileAllowed(), saved.server)
            store.update(key) { it.copy(state = DownloadState.Waiting, unreachable = away) }
            Outcome.Lost
        } finally {
            job.complete()
            current = null
        }
    }

    private fun markWaiting() {
        store.all.value.filter { it.state == DownloadState.Queued }.forEach { s ->
            store.update(s.key) { it.copy(state = DownloadState.Waiting, unreachable = false) }
        }
    }

    /** The details (for playing and listing offline), a picture and the text subtitles, saved beside the file. */
    private suspend fun saveExtras(saved: Saved): Saved {
        val api = sources.api(saved.server, saved.userId)
        val detail = api.item(saved.itemId)
        val dir = store.dirOf(saved).apply { mkdirs() }
        withContext(io) { writeWhole(store.detailOf(saved), PleyJson.encodeToString(ItemDetail.serializer(), detail)) }
        val picture = fetchPicture(saved, api, detail)
        val file = detail.files.firstOrNull { it.id == saved.fileId }
        val tracks = file?.streams.orEmpty().filter { it.kind == StreamKind.Subtitle && it.textBased == true }
        // Asked for once each: the file needn't wait for the server to read a track out of it.
        val partial = mutableListOf<Int>()
        val subtitles = tracks.mapNotNull { track ->
            try {
                if (!saveSubtitles(saved, api, track.index)) partial += track.index
                track.index
            } catch (e: CancellationException) {
                throw e
            } catch (_: IOException) {
                null
            }
        }
        dir.mkdirs()
        return store.update(saved.key) {
            it.copy(
                extrasSaved = true,
                image = picture,
                subtitles = subtitles,
                partialSubtitles = partial,
                durationMs = file?.durationMs ?: detail.durationMs ?: it.durationMs,
            )
        } ?: throw CancellationException("removed")
    }

    /**
     * [saved]'s picture, saved as it is queued: the episodes of a season
     * queued together show their stills before their turn comes.
     */
    suspend fun savePicture(saved: Saved, detail: ItemDetail) {
        if (fetchPicture(saved, sources.api(saved.server, saved.userId), detail)) {
            withContext(io) { store.update(saved.key) { it.copy(image = true) } }
        }
    }

    /** A still, else the poster or the backdrop, into [Saved]'s picture: whether there is one. */
    private suspend fun fetchPicture(saved: Saved, api: PleyApi, detail: ItemDetail): Boolean =
        (detail.thumbUrl ?: detail.posterUrl ?: detail.backdropUrl)?.let { path ->
            try {
                fetch(sources.images, api.url(path, IMAGE_WIDTH), store.imageOf(saved))
                true
            } catch (e: CancellationException) {
                throw e
            } catch (_: IOException) {
                false
            }
        } ?: false

    /**
     * A text track as the server's WebVTT, saved as far as it goes: the
     * server may still be reading it out of the file. Whether it came whole.
     */
    private suspend fun saveSubtitles(saved: Saved, api: PleyApi, index: Int): Boolean {
        val client = sources.subtitles(saved.server)
        val url = api.url("/api/files/${saved.fileId}/subtitles/$index.vtt")
        val (text, complete) = call(client, Request.Builder().url(url).header("Accept", "text/vtt").build()) { response ->
            if (!response.isSuccessful) throw ApiException.from(response)
            response.body.string() to (response.header(HttpPleyApi.SUBTITLES_COMPLETE_HEADER)?.trim() != "0")
        }
        // Never into the folder of a download deleted meanwhile.
        if (store.get(saved.key) != null) withContext(io) { writeWhole(store.subtitleOf(saved, index), text) }
        return complete
    }

    /** The subtitle tracks of [saved] that came cut short, asked for again. */
    private suspend fun finishSubtitles(saved: Saved) {
        if (saved.partialSubtitles.isEmpty()) return
        val api = sources.api(saved.server, saved.userId)
        val whole = saved.partialSubtitles.filterTo(HashSet()) { index ->
            try {
                saveSubtitles(saved, api, index)
            } catch (e: CancellationException) {
                throw e
            } catch (_: IOException) {
                // Out of reach, or gone from the server: what came stays.
                false
            }
        }
        if (whole.isNotEmpty()) store.update(saved.key) { it.copy(partialSubtitles = it.partialSubtitles - whole) }
    }

    /**
     * Downloads of [signedIn] whose subtitles still fall short (the app
     * closed before the server had read them whole): asked for once more,
     * with their server in reach.
     */
    internal suspend fun finishCutSubtitles(signedIn: Pair<String, Long>) = catchingUp.withLock {
        val short = store.all.value.filter {
            it.state == DownloadState.Done && it.partialSubtitles.isNotEmpty() &&
                it.server == signedIn.first && it.userId == signedIn.second
        }
        for (saved in short) {
            if (gate.open(mobileAllowed(), saved.server)) finishSubtitles(saved)
        }
    }

    /**
     * [saved] with its converted file's address, once the server has made
     * it whole (or without converting, when the original plays here after
     * all); `null` while it is being made, looked at again in a while.
     */
    private suspend fun converted(saved: Saved): Saved? {
        val api = sources.api(saved.server, saved.userId)
        val request = PlaybackRequest(
            itemId = saved.itemId,
            fileId = saved.fileId,
            maxHeight = saved.maxHeight,
            maxBitrate = saved.maxBitrateKbps,
            caps = sources.caps() ?: ClientCaps(),
        )
        val info = try {
            saved.conversion?.let { id ->
                try {
                    api.conversion(id)
                } catch (e: ApiException) {
                    // The server restarted, or forgot it: asked for again.
                    if (e.status == 404) null else throw e
                }
            } ?: api.convert(request)
        } catch (e: ApiException) {
            when (e.status) {
                // Its transcoder isn't running (starting, restarting): like a server away, tried again in a while.
                503 -> throw IOException(e.message)
                // A server that can't convert (its transcoder, or the server itself, too old):
                // the original, unless a smaller file was asked for.
                501, 404 -> if (saved.maxHeight == null) {
                    return keep(saved.key) { it.copy(convert = false, converting = null) }
                } else {
                    throw DownloadFailure(Failure.NoConversions)
                }
                else -> throw e
            }
        }
        return when (info.state) {
            ConversionState.Direct -> keep(saved.key) { it.copy(convert = false, converting = null, sizeBytes = info.size ?: it.sizeBytes) }
            ConversionState.Done -> keep(saved.key) {
                it.copy(conversion = info.id, mediaUrl = info.url, converting = null, sizeBytes = info.size ?: it.sizeBytes)
            }
            ConversionState.Failed -> throw DownloadFailure(Failure.ConversionFailed, info.error)
            else -> {
                keep(saved.key) {
                    it.copy(
                        state = DownloadState.Queued,
                        conversion = info.id,
                        converting = info.progress.toFloat().coerceIn(0f, 1f),
                        notBefore = clock() + conversionPollMs,
                    )
                }
                null
            }
        }
    }

    /** [change] made to [key]'s download, which must still be there. */
    private fun keep(key: Key, change: (Saved) -> Saved): Saved = store.update(key, change = change) ?: throw CancellationException("removed")

    /** The file, from where its partial copy ends, then moved to its name. */
    private suspend fun fetchMedia(saved: Saved) {
        val part = store.partOf(saved)
        val media = store.mediaOf(saved)
        if (media.isFile && saved.sizeBytes > 0 && media.length() == saved.sizeBytes) return
        part.parentFile?.mkdirs()
        var have = if (part.isFile) part.length() else 0L
        val left = (saved.sizeBytes - have).coerceAtLeast(0)
        if (left > 0 && withContext(io) { store.freeBytes() } < left + SPACE_MARGIN) throw DownloadFailure(Failure.NoSpace)
        val api = sources.api(saved.server, saved.userId)
        val request = Request.Builder()
            .url(api.url(saved.mediaUrl ?: "/api/files/${saved.fileId}/stream"))
            .apply { if (have > 0) header("Range", "bytes=$have-") }
            .build()
        call(sources.media(saved.server), request) { response ->
            when {
                response.code == 416 && saved.sizeBytes > 0 && have >= saved.sizeBytes -> return@call
                response.code == 416 -> {
                    // Shorter than what is down: the file changed on the server. It starts again.
                    part.delete()
                    throw IOException("the file changed on the server")
                }
                // Kept: asked for again, the same conversion goes on from here.
                response.code == 404 && saved.mediaUrl != null -> throw ConversionGone()
                response.code == 206 -> Unit
                response.isSuccessful -> {
                    // The whole file again: the server didn't take the range.
                    have = 0
                    part.delete()
                }
                else -> throw ApiException.from(response)
            }
            val total = totalOf(response, have)
            if (have > 0 && saved.sizeBytes > 0 && total > 0 && total != saved.sizeBytes) {
                // Another size than the one begun: the file changed on the server. It starts again.
                part.delete()
                throw IOException("the file changed on the server")
            }
            if (total > 0 && total != saved.sizeBytes) store.update(saved.key, persist = false) { it.copy(sizeBytes = total) }
            copy(response, part, have, saved.key)
        }
        val size = store.get(saved.key)?.sizeBytes ?: saved.sizeBytes
        if (size > 0 && part.length() != size) throw IOException("the file stopped short")
        withContext(io) {
            media.delete()
            if (!part.renameTo(media)) throw IOException("the file couldn't be put in place")
            // What other conversions left begun.
            store.dirOf(saved).listFiles { f -> f.name.endsWith(".part") }?.forEach { it.delete() }
        }
    }

    /** Appends the body to [part], saying how far it is about twice a second (on [io], as [call] reads). */
    private suspend fun copy(response: Response, part: File, from: Long, key: Key) {
        var have = from
        var shownAt = 0L
        FileOutputStream(part, from > 0).use { out ->
            response.body.byteStream().use { input ->
                val buffer = ByteArray(BUFFER_BYTES)
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val read = input.read(buffer)
                    if (read < 0) break
                    out.write(buffer, 0, read)
                    have += read
                    val now = clock()
                    if (now - shownAt >= PROGRESS_MS) {
                        shownAt = now
                        val done = have
                        store.update(key, persist = false) { it.copy(downloadedBytes = done) }
                    }
                }
            }
        }
        store.update(key, persist = false) { it.copy(downloadedBytes = have) }
    }

    private suspend fun fetch(client: OkHttpClient, url: String, into: File) {
        val bytes = call(client, Request.Builder().url(url).build()) { response ->
            if (!response.isSuccessful) throw ApiException.from(response)
            response.body.bytes()
        }
        withContext(io) {
            into.parentFile?.mkdirs()
            into.writeBytes(bytes)
        }
    }

    /**
     * Sends [request], then reads its answer on [io]. Cancelling cancels
     * the call, which also ends a read waiting on the network.
     */
    private suspend fun <T> call(client: OkHttpClient, request: Request, read: suspend (Response) -> T): T {
        val running = client.newCall(request)
        val response = running.await()
        return response.use {
            coroutineScope {
                val canceller = launch {
                    try {
                        awaitCancellation()
                    } finally {
                        running.cancel()
                    }
                }
                try {
                    withContext(io) { read(response) }
                } finally {
                    canceller.cancel()
                }
            }
        }
    }

    private suspend fun Call.await(): Response = suspendCancellableCoroutine { continuation ->
        continuation.invokeOnCancellation { cancel() }
        enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    continuation.resumeWithException(e)
                }

                override fun onResponse(call: Call, response: Response) {
                    continuation.resume(response) { _, _, _ -> response.close() }
                }
            },
        )
    }

    private fun writeWhole(file: File, text: String) {
        file.parentFile?.mkdirs()
        val temp = File(file.path + ".tmp")
        temp.writeText(text)
        if (!temp.renameTo(file)) {
            file.delete()
            temp.renameTo(file)
        }
    }

    companion object {
        /** How long a run waits for a network it may use before it stops (the app starts it again). */
        const val WAIT_MS = 15 * 60 * 1000L
        const val MAX_LOST_IN_A_ROW = 5
        const val BACKOFF_MS = 5_000L
        const val PROGRESS_MS = 500L
        const val BUFFER_BYTES = 256 * 1024
        const val IMAGE_WIDTH = 640

        /** How often a conversion the server is making is looked at. */
        const val CONVERSION_POLL_MS = 5_000L

        /** Room kept free beside a download, so the device doesn't fill up. */
        const val SPACE_MARGIN = 500L * 1024 * 1024

        /** The whole file's size: from Content-Range for a range, else the body's length. */
        fun totalOf(response: Response, from: Long): Long {
            val range = response.header("Content-Range")
            if (response.code == 206 && range != null) return range.substringAfterLast('/').trim().toLongOrNull() ?: -1
            val length = response.body.contentLength()
            return if (length >= 0) from + length else -1
        }
    }
}
