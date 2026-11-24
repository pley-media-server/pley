package app.pley.android.player.music

import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.LibraryResult
import androidx.media3.session.SessionError
import app.pley.android.network.OfflineException
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import java.net.SocketTimeoutException
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The music service's answers to what a car or the Assistant asks of the
 * library: each made on [work], off the main thread, and handed to Media3
 * from [scope]'s, the main thread, as its futures are. A car's list is up
 * to 500 rows, each made from the server's answer, and the main thread
 * meanwhile runs the music and the app's screens.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class LibraryAnswers(
    /** The service's scope, on the main thread. */
    private val scope: CoroutineScope,
    /** A failure as the car shows it: signed out, offline, or anything else. */
    private val errorOf: (Exception) -> SessionError,
    private val work: CoroutineContext = Dispatchers.Default,
    private val answerMs: Long = ANSWER_MS,
) {
    /**
     * [block]'s result, a failure as the car shows it. A server that hasn't
     * answered within [answerMs] is out of reach to the car, which is to
     * show what it asked for within 10 s (Google's car app quality
     * criterion DR-3); [slow] is told so first, on the main thread.
     */
    fun <T : Any> answer(slow: () -> Unit = {}, block: suspend () -> LibraryResult<T>): ListenableFuture<LibraryResult<T>> {
        val future = SettableFuture.create<LibraryResult<T>>()
        scope.launch {
            val result = try {
                withTimeoutOrNull(answerMs) { withContext(work) { block() } }
                    ?: LibraryResult.ofError<T>(errorOf(OfflineException(SocketTimeoutException()))).also { slow() }
            } catch (e: CancellationException) {
                future.cancel(false)
                throw e
            } catch (e: Exception) {
                LibraryResult.ofError(errorOf(e))
            }
            future.set(result)
        }
        return future
    }

    /** [block] on the main thread, from an answer's work: what the session, and what it was told, are kept on. */
    suspend fun <R> onMain(block: () -> R): R = withContext(scope.coroutineContext.minusKey(Job)) { block() }

    private companion object {
        /** How long a car's request for the library or a search waits for the server before it is told it is out of reach. */
        const val ANSWER_MS = 8_000L
    }
}
