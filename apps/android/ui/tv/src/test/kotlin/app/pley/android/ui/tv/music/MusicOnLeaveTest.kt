package app.pley.android.ui.tv.music

import app.pley.android.player.MusicControl
import app.pley.android.player.NowPlaying
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * When the TV app tells the music the person left and came back: Home or
 * another app, the last Back, the screensaver, windows over the app and a
 * Watch Next program. What the music does then is MusicPlaybackTest's.
 */
class MusicOnLeaveTest {
    private val music = Music()
    private var clock = 1_000L

    /** The blocks waiting for their delay, as [MusicOnLeave] left them. */
    private val waiting = mutableListOf<() -> Unit>()
    private val later: (Long, () -> Unit) -> () -> Unit = { _, block ->
        waiting += block
        ({ waiting.remove(block) })
    }
    private val leave = MusicOnLeave({ clock }, later) { music }

    /** Their delay passes. */
    private fun waitOut() {
        val due = waiting.toList()
        waiting.clear()
        due.forEach { it() }
    }

    @Test
    fun homePausesTheMusicAndItPlaysOnWhenTheAppIsBack() {
        leave.leaving()
        clock += 400
        leave.stopped(finishing = false)
        assertEquals(listOf("pause, play on return"), music.calls)

        leave.resumed(videoNext = false)
        assertEquals(listOf("pause, play on return", "return, play on"), music.calls)
    }

    @Test
    fun theLastBackPausesItForGood() {
        leave.stopped(finishing = true)
        assertEquals(listOf("pause for good"), music.calls)
    }

    @Test
    fun theScreensaverLeavesItPlaying() {
        // The screensaver stops the activity without onUserLeaveHint.
        leave.stopped(finishing = false)
        assertEquals(emptyList<String>(), music.calls)
    }

    @Test
    fun aWindowOverTheAppLeavesItPlaying() {
        // The TV's settings panel: the hint, then the app comes back without stopping.
        leave.leaving()
        leave.resumed(videoNext = false)
        leave.stopped(finishing = false)
        assertEquals(listOf("return, play on"), music.calls)
    }

    @Test
    fun theScreensaverUnderAWindowLeftOpenLeavesItPlaying() {
        // The settings panel stays open, and the screensaver stops the activity minutes later.
        leave.leaving()
        clock += 5 * 60_000
        leave.stopped(finishing = false)
        leave.dreaming(on = true)
        waitOut()
        assertEquals(emptyList<String>(), music.calls)
    }

    @Test
    fun theScreensaverAlreadyOnWhenTheAppStopsLeavesItPlaying() {
        leave.leaving()
        clock += 5 * 60_000
        leave.dreaming(on = true)
        leave.stopped(finishing = false)
        waitOut()
        assertEquals(emptyList<String>(), music.calls)
    }

    @Test
    fun homeFromAWindowLeftOpenPausesItOnceNoScreensaverStarted() {
        // The settings panel gave the hint as it opened; Home from it gives the app none.
        leave.leaving()
        clock += 5 * 60_000
        leave.stopped(finishing = false)
        assertEquals(emptyList<String>(), music.calls)
        waitOut()
        assertEquals(listOf("pause, play on return"), music.calls)
    }

    @Test
    fun comingBackBeforeTheWaitIsOverLeavesItPlaying() {
        leave.leaving()
        clock += 5 * 60_000
        leave.stopped(finishing = false)
        leave.resumed(videoNext = false)
        waitOut()
        assertEquals(listOf("return, play on"), music.calls)
    }

    @Test
    fun aStopUpToFifteenSecondsAfterTheHintPausesAtOnce() {
        leave.leaving()
        clock += MusicOnLeave.HINT_MS
        leave.stopped(finishing = false)
        assertEquals(listOf("pause, play on return"), music.calls)
        assertEquals(0, waiting.size)

        leave.leaving()
        clock += MusicOnLeave.HINT_MS + 1
        leave.stopped(finishing = false)
        assertEquals(1, waiting.size)
    }

    @Test
    fun aWatchNextProgramPlaysItsVideoNotTheMusic() {
        leave.leaving()
        leave.stopped(finishing = false)
        leave.resumed(videoNext = true)
        assertEquals(listOf("pause, play on return", "return, stay paused"), music.calls)
    }

    @Test
    fun anActivityMadeAnewWhileAwayStillPlaysTheMusicOn() {
        leave.leaving()
        leave.stopped(finishing = false)
        // A language or display mode change made the activity anew: the music remembers.
        val again = MusicOnLeave({ clock }, later) { music }
        again.resumed(videoNext = false)
        assertEquals(listOf("pause, play on return", "return, play on"), music.calls)
    }

    @Test
    fun nothingHappensBeforeTheMusicIsMade() {
        val none = MusicOnLeave({ clock }, later) { null }
        none.leaving()
        none.stopped(finishing = false)
        none.resumed(videoNext = false)
    }

    private class Music : MusicControl {
        override val nowPlaying = MutableStateFlow<NowPlaying?>(null)
        val calls = mutableListOf<String>()

        override fun pauseForVideo() {
            calls += "pauseForVideo"
        }

        override fun pauseForLeave(playOnReturn: Boolean) {
            calls += if (playOnReturn) "pause, play on return" else "pause for good"
        }

        override fun returnAfterLeave(playOn: Boolean) {
            calls += if (playOn) "return, play on" else "return, stay paused"
        }
    }
}
