package app.pley.android.player.music

import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.session.LibraryResult
import androidx.media3.session.SessionError
import app.pley.android.network.OfflineException
import com.google.common.util.concurrent.ListenableFuture
import java.io.IOException
import java.time.Duration
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/**
 * The library's answers to a car: made off the main thread, handed back to
 * Media3 on it, and failing as before (out of reach in time, an error the car
 * shows, and nothing once the service has gone).
 */
@RunWith(RobolectricTestRunner::class)
class LibraryAnswersTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val answers = LibraryAnswers(scope, { e -> SessionError(if (e is OfflineException) SessionError.ERROR_IO else SessionError.ERROR_UNKNOWN, "") }, answerMs = 300)

    @After
    fun tearDown() = scope.cancel()

    private fun onMain(): Boolean = Looper.myLooper() == Looper.getMainLooper()

    /** A row as Media3 takes it in a list: saying whether it opens and whether it plays. */
    private fun row(id: String): MediaItem =
        MediaItem.Builder().setMediaId(id).setMediaMetadata(MediaMetadata.Builder().setIsBrowsable(false).setIsPlayable(true).build()).build()

    /**
     * [future]'s result, the main thread run meanwhile, as Media3's would be, its clock going on 50 ms each turn (the
     * main thread's time stands still in a test otherwise): 3 s at most, a slow machine's.
     */
    private fun <T> done(future: ListenableFuture<T>): T {
        val until = System.currentTimeMillis() + 3_000
        while (!future.isDone && System.currentTimeMillis() < until) {
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(50))
            Thread.sleep(5)
        }
        return future.get(1, TimeUnit.SECONDS)
    }

    @Test
    fun aListIsMadeOffTheMainThreadAndHandedBackOnIt() {
        var made: Boolean? = null
        var told: Boolean? = null
        val rows = (1..500).map { row("track:$it") }
        val result = done(
            answers.answer {
                made = onMain()
                answers.onMain { told = onMain() }
                LibraryResult.ofItemList(rows, null)
            },
        )
        assertEquals(LibraryResult.RESULT_SUCCESS, result.resultCode)
        assertEquals(500, result.value!!.size)
        assertEquals(false, made)
        assertEquals(true, told)
    }

    @Test
    fun aServerThatDoesntAnswerInTimeIsOutOfReach() {
        var slow: Boolean? = null
        val never = CompletableDeferred<Unit>()
        val result = done(
            answers.answer<MediaItem>(slow = { slow = onMain() }) {
                never.await()
                LibraryResult.ofItem(row("album:50"), null)
            },
        )
        assertEquals(SessionError.ERROR_IO, result.resultCode)
        assertEquals(true, slow)
    }

    @Test
    fun aFailureIsTheErrorTheCarShows() {
        assertEquals(SessionError.ERROR_UNKNOWN, done(answers.answer<MediaItem> { throw IllegalStateException() }).resultCode)
        assertEquals(SessionError.ERROR_IO, done(answers.answer<MediaItem> { throw OfflineException(IOException()) }).resultCode)
        // An answer of its own, an error too, goes as it is.
        assertEquals(SessionError.ERROR_BAD_VALUE, done(answers.answer<MediaItem> { LibraryResult.ofError(SessionError.ERROR_BAD_VALUE) }).resultCode)
    }

    @Test
    fun anAnswerStillBeingMadeAsTheServiceGoesIsCancelled() {
        val future = answers.answer<MediaItem> { awaitCancellation() }
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(future.isDone)
        scope.cancel()
        val until = System.currentTimeMillis() + 3_000
        while (!future.isDone && System.currentTimeMillis() < until) {
            shadowOf(Looper.getMainLooper()).idle()
            Thread.sleep(5)
        }
        assertTrue(future.isCancelled)
    }
}
