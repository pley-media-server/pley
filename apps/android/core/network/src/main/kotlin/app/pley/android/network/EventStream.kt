package app.pley.android.network

import app.pley.android.auth.AuthException
import app.pley.android.model.ServerEvent
import java.io.IOException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.flow
import kotlinx.serialization.SerializationException
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.sse.EventSource
import okhttp3.sse.EventSourceListener
import okhttp3.sse.EventSources

/** What happens on one connection to the event stream. */
internal sealed interface SseSignal {
    /** The server took the request: events follow. */
    data object Opened : SseSignal

    /** One event's `data`. */
    data class Message(val data: String) : SseSignal

    /** The stream ended: closed by the server when [error] is `null`, else failed. Always the last. */
    data class Ended(val error: Throwable?) : SseSignal
}

/** One connection to the event stream: collecting connects, and the flow ends with [SseSignal.Ended]. */
internal fun interface SseConnection {
    fun open(): Flow<SseSignal>
}

/** GET [url] as server-sent events with okhttp-sse, on [client] (made when first connecting). */
internal class OkHttpSseConnection(private val client: () -> OkHttpClient, private val url: String) : SseConnection {
    override fun open(): Flow<SseSignal> = callbackFlow {
        val request = Request.Builder().url(url).header("Accept", "text/event-stream").build()
        val listener = object : EventSourceListener() {
            override fun onOpen(eventSource: EventSource, response: Response) {
                trySend(SseSignal.Opened)
            }

            override fun onEvent(eventSource: EventSource, id: String?, type: String?, data: String) {
                trySend(SseSignal.Message(data))
            }

            override fun onClosed(eventSource: EventSource) {
                trySend(SseSignal.Ended(null))
                channel.close()
            }

            override fun onFailure(eventSource: EventSource, t: Throwable?, response: Response?) {
                val error = when {
                    response != null && !response.isSuccessful -> ApiException.from(response)
                    t != null -> failure(t)
                    else -> IOException("that isn't an event stream (${response?.header("Content-Type")})")
                }
                trySend(SseSignal.Ended(error))
                channel.close()
            }
        }
        val source = EventSources.createFactory(client()).newEventSource(request, listener)
        awaitClose { source.cancel() }
    }.buffer(Channel.UNLIMITED)
}

/**
 * The server's events, over [connection], kept connected while collected.
 *
 * - Each event's JSON is read as a [ServerEvent]; types this app doesn't
 *   know, and data that can't be read, are skipped.
 * - [LiveEvent.Reconnected] comes on every connection, the first one
 *   included: the server replays nothing, so whatever was read before may
 *   be stale.
 * - A connection that opened says it was lost ([LiveEvent.Dropped]) when
 *   it ends, however it ended: opening it again may fail.
 * - A stream the server ended after [stableMs] or more (it ends streams
 *   when their access token expires, apps/server/src/api/activity.rs) is
 *   opened again at once: the new request carries the refreshed token.
 * - Any other end waits [firstRetryMs], doubling after each failure to
 *   [maxRetryMs]; a connection that opened starts the count again.
 * - The flow ends with the [ApiException] of a 401 or 403 (signed out,
 *   or not allowed) and the sign-in module's errors: trying again can't
 *   help.
 *
 * [now] is a clock in ms (the coroutine's virtual time in tests). A
 * stream that names a receiver connects through [asReceiver]'s connection.
 */
internal class EventStream(
    private val connection: SseConnection,
    private val now: () -> Long = { System.nanoTime() / 1_000_000 },
    private val firstRetryMs: Long = FIRST_RETRY_MS,
    private val maxRetryMs: Long = MAX_RETRY_MS,
    private val stableMs: Long = STABLE_MS,
    private val asReceiver: (EventReceiver) -> SseConnection = { connection },
) : ServerEvents {
    override fun events(): Flow<LiveEvent> = events(null)

    override fun events(receiver: EventReceiver?): Flow<LiveEvent> = flow {
        val connection = receiver?.let(asReceiver) ?: connection
        var failures = 0
        while (true) {
            var openedAt = -1L
            var ended: Throwable? = null
            connection.open().collect { signal ->
                when (signal) {
                    SseSignal.Opened -> {
                        openedAt = now()
                        failures = 0
                        emit(LiveEvent.Reconnected)
                    }
                    is SseSignal.Message -> read(signal.data)?.let { emit(LiveEvent.Event(it)) }
                    is SseSignal.Ended -> ended = signal.error
                }
            }
            val error = ended
            if (error is AuthException || (error is ApiException && (error.status == 401 || error.status == 403))) throw error
            if (openedAt >= 0) emit(LiveEvent.Dropped)
            if (error == null && openedAt >= 0 && now() - openedAt >= stableMs) continue
            failures++
            delay(retryDelay(failures))
        }
    }

    /** The wait before attempt [failures] + 1: [firstRetryMs], doubled each time, at most [maxRetryMs]. */
    fun retryDelay(failures: Int): Long {
        var wait = firstRetryMs
        repeat((failures - 1).coerceIn(0, 30)) { wait = (wait * 2).coerceAtMost(maxRetryMs) }
        return wait.coerceAtMost(maxRetryMs)
    }

    private fun read(data: String): ServerEvent? = try {
        ServerEvent.parse(data).takeIf { it !is ServerEvent.Unknown }
    } catch (_: SerializationException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }

    companion object {
        const val FIRST_RETRY_MS = 2_000L
        const val MAX_RETRY_MS = 30_000L

        /** A stream open this long and then ended by the server ended as planned (an expired token). */
        const val STABLE_MS = 30_000L
    }
}
