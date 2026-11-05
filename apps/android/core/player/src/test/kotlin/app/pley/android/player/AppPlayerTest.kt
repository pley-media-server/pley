@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player

import app.pley.android.data.LiveTvRepository
import app.pley.android.player.session.SessionReporter
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Everything that plays ends before a cast switches the profile: video,
 * music and Live TV end at once, their last reports go out as the profile
 * they played for, and what hasn't gone out within 5 s is dropped, never
 * sent as the next profile.
 */
class AppPlayerTest {
    /** The profile whose tokens are in use, as each request goes out. */
    private var profile = "anna"

    /** The requests that went out, and as whom. */
    private val sent = mutableListOf<String>()

    private fun sending(what: String) {
        sent += "$what as $profile"
    }

    /** Video's playbacks: closing them queues `stopped` and the session's end. */
    private inner class Services(val reporter: SessionReporter) : PlaybackServices {
        var closed = 0

        override fun prepare(request: PlayRequest): String = error("Nothing plays in these tests")

        override fun controller(key: String): PlaybackController? = null

        override fun closeAll() {
            closed++
            reporter.send { sending("stopped") }
            reporter.send { sending("delete") }
        }

        override suspend fun flush() = reporter.flush()

        override fun drop() = reporter.drop()
    }

    /** Music: stopping it queues its session's end. */
    private inner class Music(val reporter: SessionReporter) : MusicControl {
        override val nowPlaying = MutableStateFlow<NowPlaying?>(null)
        var stopped = 0

        override fun stop() {
            stopped++
            reporter.send { sending("music delete") }
        }

        override suspend fun flush() = reporter.flush()

        override fun drop() = reporter.drop()

        override fun pauseForVideo() = Unit
    }

    /** Live TV: the channel's leave goes once its window has gone. */
    private inner class Live : LiveTvRepository by LiveTvRepository.None {
        var dropped = 0

        override suspend fun left() = sending("leave")

        override fun drop() {
            dropped++
        }
    }

    private var windowsClosed = 0

    private fun TestScope.player(
        services: Services,
        live: Live,
        windowsGone: suspend () -> Unit = {},
        music: () -> MusicControl = { error("No music made") },
    ) = AppPlayer(
        engines = { _, _, _ -> error("No engines here") },
        services = services,
        liveTv = { live },
        closeWindows = { windowsClosed++ },
        windowsGone = windowsGone,
        music = music,
    )

    @Test
    fun `everything ends, its last reports go out as the profile it played for, then the switch`() = runTest {
        val services = Services(SessionReporter(backgroundScope))
        val music = Music(SessionReporter(backgroundScope))
        val live = Live()
        val player = player(services, live, music = { music })
        // Music played here: it was made.
        assertTrue(player.music === music)

        launch {
            player.endAll()
            // The switch, as a cast makes it: the tokens are the next profile's from now on.
            sent += "switch"
            profile = "sam"
        }
        advanceUntilIdle()

        assertEquals(1 to 1, services.closed to music.stopped)
        assertEquals(1, windowsClosed)
        assertEquals("switch", sent.last())
        assertEquals(
            setOf("stopped as anna", "delete as anna", "music delete as anna", "leave as anna"),
            sent.dropLast(1).toSet(),
        )
        assertTrue(sent.indexOf("stopped as anna") < sent.indexOf("delete as anna"))
        // Nothing waited for long.
        assertEquals(0L, currentTime)
    }

    @Test
    fun `music never played isn't made to end it`() = runTest {
        var made = false
        val player = player(Services(SessionReporter(backgroundScope)), Live(), music = {
            made = true
            Music(SessionReporter(backgroundScope))
        })
        player.endAll()
        assertFalse(made)
    }

    @Test
    fun `what hasn't gone out after 5 s is dropped, never sent as the next profile`() = runTest {
        val reporter = SessionReporter(backgroundScope)
        val services = Services(reporter)
        val live = Live()
        // A progress report the server takes its time with, ahead of the playback's end.
        val slow = CompletableDeferred<Unit>()
        reporter.send {
            slow.await()
            sending("progress")
        }
        // The window goes a second in, its channel's leave then: waited for though the reports stall.
        val player = player(services, live, windowsGone = { delay(1_000) })

        var switchedAt: Long? = null
        launch {
            player.endAll()
            switchedAt = currentTime
            sent += "switch"
            profile = "sam"
        }
        advanceTimeBy(4_999)
        runCurrent()
        assertNull(switchedAt)
        assertEquals(listOf("leave as anna"), sent)

        advanceUntilIdle()
        assertEquals(5_000L, switchedAt)
        assertEquals(1, live.dropped)
        // The server answers at last: the report going out then was dropped with the rest. (The reporter runs in
        // the background scope, which runCurrent runs and advanceUntilIdle doesn't once the test's own are done.)
        slow.complete(Unit)
        runCurrent()
        assertEquals(listOf("leave as anna", "switch"), sent)

        // The next profile's playback reports as it.
        reporter.send { sending("progress") }
        runCurrent()
        assertEquals("progress as sam", sent.last())
    }

    @Test
    fun `a flush waited for elsewhere returns though what it waited for was dropped`() = runTest {
        val reporter = SessionReporter(backgroundScope)
        reporter.send { CompletableDeferred<Unit>().await() }
        reporter.send { sending("stopped") }
        var flushed = false
        launch {
            reporter.flush()
            flushed = true
        }
        runCurrent()
        assertFalse(flushed)
        reporter.drop()
        runCurrent()
        assertTrue(flushed)
        assertTrue(sent.isEmpty())
    }
}
