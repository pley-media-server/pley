package app.pley.android.player.session

import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.ProgressReport
import app.pley.android.model.ProgressState
import app.pley.android.network.PleyApi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Sends playbacks' reports one after the other, from the app's scope: a
 * player screen that closes still gets its last position and its session's
 * end to the server, in the order they were made (progress, then
 * `stopped`, then DELETE). Reports are best effort: a failure is dropped,
 * as on the web (apps/web/src/lib/progress.ts).
 */
class SessionReporter(scope: CoroutineScope) {
    /** A report and its place in the queue; a flush's mark has none, and is never dropped. */
    private class Queued(val number: Long?, val run: suspend () -> Unit)

    private val queue = Channel<Queued>(Channel.UNLIMITED)
    private val lock = Any()

    /** Reports queued so far, and those of them [drop] dropped (the first [dropped]). */
    private var queued = 0L
    private var dropped = 0L

    /** The report going out now. */
    private var sending: Job? = null

    init {
        scope.launch {
            for (report in queue) {
                // Run in the loop itself, a job of its own to cancel it by: no extra dispatch, so a report goes
                // out as soon as it would have without one.
                val turn = synchronized(lock) {
                    if (report.number != null && report.number <= dropped) return@synchronized null
                    SupervisorJob(coroutineContext.job).also { sending = it }
                } ?: continue
                try {
                    withContext(turn) { report.run() }
                } catch (e: CancellationException) {
                    // Dropped, unless the reporter's own scope ended.
                    ensureActive()
                } catch (_: Exception) {
                    // Best effort: the next report says where the player is anyway.
                } finally {
                    turn.complete()
                }
            }
        }
    }

    /** Queues [report]. */
    fun send(report: suspend () -> Unit) {
        synchronized(lock) { queue.trySend(Queued(++queued, report)) }
    }

    /** Waits until everything queued so far went out (or was dropped). */
    suspend fun flush() {
        val done = CompletableDeferred<Unit>()
        queue.send(Queued(null) { done.complete(Unit) })
        done.await()
    }

    /**
     * Drops what was queued so far and hasn't gone out, the report going
     * out now too: a request picks the tokens in use as it goes, so after a
     * switch of profile it would go as the next profile. Reports queued
     * after this go out as usual.
     */
    fun drop() {
        synchronized(lock) {
            dropped = queued
            sending?.cancel()
        }
    }
}

/**
 * The tracks a video plays, as its progress reports say them for the
 * profile's remotes: stream indexes, [subtitle] [PlayingSession.SUBTITLES_OFF][app.pley.android.model.PlayingSession.SUBTITLES_OFF]
 * for none; [audio] `null` while it isn't known. [quality] is the Quality
 * menu's pick, by id ("auto", "original", "8m"…).
 */
data class PlayedTracks(val audio: Int?, val subtitle: Int, val quality: String? = null)

/**
 * One session on the server (POST /api/playback): its progress reports and
 * its end. Video and music both play through one. Ending it reports
 * `stopped` at the last position, then deletes it (and any transcode); it
 * ends once. Reports give the item's length as [durationMs] says it: the
 * server's, or for a file on a drive the player's, once it knows it, and
 * the tracks [tracks] says play (a video's; music's say none).
 */
class ServerPlaybackSession(
    private val api: PleyApi,
    val itemId: Long,
    val info: PlaybackInfo,
    private val reporter: SessionReporter,
    private val durationMs: () -> Long? = { info.durationMs.takeIf { it > 0 } },
    private val tracks: () -> PlayedTracks? = { null },
) {
    val id: String get() = info.sessionId

    var ended: Boolean = false
        private set

    /** POST /api/items/{id}/progress with the session, so it and its transcode stay alive. */
    fun progress(positionMs: Long, state: ProgressState) {
        if (ended) return
        val report = report(positionMs, state)
        reporter.send { api.reportProgress(itemId, report) }
    }

    /** Reports `stopped` at [positionMs], then DELETE /api/playback/{session}. */
    fun end(positionMs: Long) {
        if (ended) return
        ended = true
        val report = report(positionMs, ProgressState.Stopped)
        reporter.send { api.reportProgress(itemId, report) }
        reporter.send { api.stopPlayback(id) }
    }

    /**
     * The server stopped the session itself: `stopped` at [positionMs]
     * keeps the resume point (best effort), and nothing is deleted.
     */
    fun stoppedByServer(positionMs: Long) {
        if (ended) return
        ended = true
        val report = report(positionMs, ProgressState.Stopped)
        reporter.send { api.reportProgress(itemId, report) }
    }

    /**
     * Deletes a session that never played (asked for again another way):
     * no `stopped`, which would move the profile's resume point.
     */
    fun discard() {
        if (ended) return
        ended = true
        reporter.send { api.stopPlayback(id) }
    }

    private fun report(positionMs: Long, state: ProgressState): ProgressReport {
        val played = tracks()
        return ProgressReport(
            positionMs = positionMs.coerceAtLeast(0),
            durationMs = durationMs(),
            state = state,
            sessionId = id,
            audioIndex = played?.audio,
            subtitleIndex = played?.subtitle,
            quality = played?.quality,
        )
    }

    companion object {
        /**
         * Starts a session for [request], whose reports say the [tracks]
         * playing. The request isn't cancelled halfway: the server may have
         * made the session (and its transcode) already, so a caller
         * cancelled meanwhile gets it deleted, then the cancellation.
         */
        suspend fun start(
            api: PleyApi,
            request: PlaybackRequest,
            reporter: SessionReporter,
            tracks: () -> PlayedTracks? = { null },
        ): ServerPlaybackSession {
            val info = withContext(NonCancellable) { api.startPlayback(request) }
            try {
                currentCoroutineContext().ensureActive()
            } catch (e: CancellationException) {
                reporter.send { api.stopPlayback(info.sessionId) }
                throw e
            }
            return ServerPlaybackSession(api, request.itemId, info, reporter, tracks = tracks)
        }
    }
}
