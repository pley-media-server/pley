package app.pley.android.player.music.library

import app.pley.android.data.Loadable
import java.io.IOException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/** What a car's list is made of: the server's answer, or what was saved when the server is slow. */
class FreshOrSavedTest {
    private val answer = MutableStateFlow<Loadable<String>>(Loading)

    /** The server answers [value] [afterMs] from now. */
    private fun TestScope.answers(value: Loadable<String>, afterMs: Long) {
        backgroundScope.launch {
            delay(afterMs)
            answer.value = value
        }
    }

    @Test
    fun somethingSavedIsShownAfterAMomentOnASlowNetwork() = runTest {
        answer.value = Loadable.Ready("saved", fresh = false)
        answers(Loadable.Ready("fresh", fresh = true), afterMs = 2_000)
        assertEquals("saved", answer.freshOrSaved())
        assertEquals(SAVED_WAIT_MS, currentTime)
    }

    @Test
    fun theServersAnswerWithinThatMomentIsShown() = runTest {
        answer.value = Loadable.Ready("saved", fresh = false)
        answers(Loadable.Ready("fresh", fresh = true), afterMs = 100)
        assertEquals("fresh", answer.freshOrSaved())
        assertEquals(100L, currentTime)
    }

    @Test
    fun withNothingSavedTheServerIsWaitedForLonger() = runTest {
        answers(Loadable.Ready("fresh", fresh = true), afterMs = 2_000)
        assertEquals("fresh", answer.freshOrSaved())
        assertEquals(2_000L, currentTime)
    }

    @Test
    fun whatWasSavedAsTheServerIsAskedComesAfterAMomentToo() = runTest {
        // Read from the device a second in, the server still asked: shown a moment after it comes.
        answers(Loadable.Ready("saved", fresh = false), afterMs = 1_000)
        answers(Loadable.Ready("fresh", fresh = true), afterMs = 4_000)
        assertEquals("saved", answer.freshOrSaved())
        assertEquals(1_000 + SAVED_WAIT_MS, currentTime)
    }

    @Test
    fun aFailureShowsWhatWasSavedElseIsThrown() = runTest {
        answer.value = Loadable.Failed(IOException("offline"), stale = "saved")
        assertEquals("saved", answer.freshOrSaved())
        answer.value = Loadable.Failed(IOException("offline"))
        assertEquals("offline", runCatching { answer.freshOrSaved() }.exceptionOrNull()?.message)
    }

    private companion object {
        val Loading: Loadable<String> = Loadable.Loading
    }
}
