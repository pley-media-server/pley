@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.music

import app.pley.android.model.Chapter
import app.pley.android.model.Control
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaFile
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.ProgressReport
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.model.caps.ClientCaps
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.player.EngineSource
import app.pley.android.player.MusicError
import app.pley.android.player.MusicRepeat
import app.pley.android.player.session.SessionReporter
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.testing.network.FakePleyApi
import java.io.IOException
import kotlin.random.Random
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The music queue against the server: sessions per track, the reports'
 * cadence, the hand-off from one track to the next, Previous, what
 * happens when a track fails, and a video taking over.
 */
class MusicPlaybackTest {
    private val album = (1..3L).map { track(it) }
    private val api = FakePleyApi()
    private val output = FakeOutput()
    private val events = MutableSharedFlow<ServerEvent>()

    /** What the music told the app, in order (LiveUpdates.playing). */
    private val held = mutableListOf<Boolean>()

    /** The sessions it said it plays, in order (RemoteRepository.here). */
    private val here = mutableListOf<String?>()

    private fun TestScope.music(): MusicPlayback {
        val env = MusicEnvironment(
            api = { api },
            caps = { ClientCaps(containers = listOf("mp4"), audioCodecs = listOf("aac")) },
            clientId = { "android-test-music" },
            reporter = SessionReporter(backgroundScope),
            output = { output },
            events = events,
            playing = { _, playing -> held += playing },
            here = { _, id -> here += id },
        )
        return MusicPlayback(env, backgroundScope, Random(1))
    }

    /**
     * Runs what is due and a second more: the music's work runs in the
     * test's background scope, which advanceUntilIdle doesn't wait for.
     */
    private fun TestScope.settle() {
        runCurrent()
        advanceTimeBy(1_000)
        runCurrent()
    }

    private fun starts(): List<PlaybackRequest> = api.calls("startPlayback").map { it.args[0] as PlaybackRequest }

    private fun reports(): List<ProgressReport> = api.calls("reportProgress").map { it.args[1] as ProgressReport }

    @Test
    fun anAlbumPlaysFromTheTrackChosenWithItsOwnSession() = runTest {
        val music = music()
        music.play(album, startIndex = 1)
        runCurrent()

        val request = starts().single()
        assertEquals(2L, request.itemId)
        assertEquals("android-test-music", request.clientId)
        assertEquals(PlaybackMode.Auto, request.mode)
        assertEquals(listOf("aac"), request.caps.audioCodecs)
        assertEquals("http://127.0.0.1:7539/api/files/601/stream", output.loaded.single().url)
        assertEquals("Track 2", output.info?.title)
        assertEquals("Harbour Lights", output.info?.artist)
        assertTrue(output.info?.artUrl.orEmpty().contains("/api/items/50/images/poster"))

        output.startPlaying()
        val now = music.nowPlaying.value!!
        assertEquals(2L, now.itemId)
        assertTrue(now.playing)
        assertEquals(1, now.index)
        assertEquals(3, now.count)
        assertEquals("Harbour Lights · Deep Water", now.byline)
    }

    @Test
    fun aBookPlaysAtItsSpeedAndMusicAsRecorded() = runTest {
        val music = music()
        music.setBookSpeed(1.5f)
        val parts = listOf(track(1).copy(kind = ItemKind.Part), track(2).copy(kind = ItemKind.Part))
        music.play(parts, startIndex = 1, startMs = 600_000)
        runCurrent()
        assertEquals(1.5f, output.playsAt)
        assertEquals(600_000L, output.startPositions.single())

        music.setBookSpeed(2f)
        assertEquals(2f, output.playsAt)

        // An album after it: as recorded, the book's speed kept for the next book.
        music.play(album)
        runCurrent()
        assertEquals(1f, output.playsAt)
        music.setBookSpeed(1.2f)
        assertEquals(1f, output.playsAt)
        assertEquals(1.2f, music.bookSpeed.value)
    }

    @Test
    fun previousAndNextGoThroughTheChaptersOfABookInOneFile() = runTest {
        val music = music()
        val chapters = listOf(Chapter(0, 60_000, "Opening"), Chapter(60_000, 120_000, null), Chapter(120_000, 180_000, "End"))
        api.answer("item", ItemDetail(id = 1, libraryId = 1, kind = ItemKind.Part, title = "Book", files = listOf(MediaFile(id = 601, path = "book.m4b", chapters = chapters))))
        val parts = listOf(track(1).copy(kind = ItemKind.Part), track(2).copy(kind = ItemKind.Part))
        music.play(parts, startMs = 70_000)
        runCurrent()
        output.position = 70_000
        output.startPlaying()
        val now = music.nowPlaying.value!!
        assertTrue(now.book)
        assertEquals(chapters, now.chapters)
        assertEquals(1, now.chapter)

        // Next: the next chapter of the same file, not the next part.
        music.next()
        assertEquals(120_000L, output.seeks.last())
        assertEquals(2, music.nowPlaying.value!!.chapter)
        assertEquals(1, starts().size)
        // Previous more than 3 seconds in: the chapter's start; within them, the chapter before.
        output.position = 130_000
        music.previous()
        assertEquals(120_000L, output.seeks.last())
        music.previous()
        assertEquals(60_000L, output.seeks.last())
        assertEquals(1, music.nowPlaying.value!!.chapter)

        // The chapter playing is said as the next one starts, at the book's speed.
        music.setBookSpeed(2f)
        advanceTimeBy(5_000)
        output.position = 120_500
        advanceTimeBy(30_000)
        runCurrent()
        assertEquals(2, music.nowPlaying.value!!.chapter)

        // Next from the last chapter: the next part, which has none of its own here.
        api.answer("item", ItemDetail(id = 2, libraryId = 1, kind = ItemKind.Part, title = "Book"))
        music.next()
        runCurrent()
        assertEquals(2L, starts().last().itemId)
        assertEquals(emptyList<Chapter>(), music.nowPlaying.value!!.chapters)
    }

    @Test
    fun beforeTheFirstChapterAndAfterAVideoChaptersStillGoWherePressed() = runTest {
        val music = music()
        // The first mark comes after an intro.
        val chapters = listOf(Chapter(30_000, 90_000, "One"), Chapter(90_000, 180_000, "Two"))
        api.answer("item", ItemDetail(id = 1, libraryId = 1, kind = ItemKind.Part, title = "Book", files = listOf(MediaFile(id = 601, path = "book.m4b", chapters = chapters))))
        music.play(listOf(track(1).copy(kind = ItemKind.Part), track(2).copy(kind = ItemKind.Part)))
        runCurrent()
        output.startPlaying()
        output.position = 5_000
        assertEquals(-1, music.nowPlaying.value!!.chapter)
        assertTrue(music.nowPlaying.value!!.hasNextChapter)
        // Previous in the intro: the part's own rule (its start), no chapter to step back to.
        music.previous()
        assertEquals(0L, output.seeks.last())
        // Next: the first chapter.
        music.next()
        assertEquals(30_000L, output.seeks.last())

        // A video took the session: Next moves the place, and Play starts the part there.
        music.pauseForVideo()
        music.next()
        assertEquals(90_000L, music.position().positionMs)
        music.play()
        runCurrent()
        assertEquals(90_000L, starts().last().startMs)
        assertEquals(1L, starts().last().itemId)
    }

    @Test
    fun seekingByStepsStaysWithinTheTrack() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 10_000
        music.seekBy(-15_000)
        assertEquals(0L, output.seeks.last())
        output.position = 170_000
        music.seekBy(30_000)
        assertEquals(180_000L, output.seeks.last())
    }

    @Test
    fun pauseAndPlayWhileATrackIsAskedForDecideWhetherItPlaysOnceLoaded() = runTest {
        val music = music()
        api.hold()
        music.play(album)
        runCurrent()
        // A car's Pause comes while the server is still asked: the old track isn't played, the new one waits.
        music.pause()
        api.release()
        settle()
        assertEquals(1, output.loaded.size)
        assertFalse(output.wants)

        api.hold()
        music.next()
        runCurrent()
        music.pause()
        music.play()
        api.release()
        settle()
        assertEquals(2, output.loaded.size)
        assertTrue(output.wants)

        api.hold()
        music.next()
        runCurrent()
        music.toggle()
        api.release()
        settle()
        assertFalse(output.wants)
    }

    @Test
    fun progressGoesOutEveryTenSecondsWhilePlayingOnPauseAndEveryThirtyPaused() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 12_000
        advanceTimeBy(25_000)
        runCurrent()
        assertEquals(listOf(ProgressState.Playing, ProgressState.Playing), reports().map { it.state })
        assertEquals("0b6d5e9a2c4f4a7e8d1c3b5a7f9e2d4c", reports().first().sessionId)

        output.position = 31_000
        music.pause()
        runCurrent()
        assertEquals(ProgressState.Paused, reports().last().state)
        assertEquals(31_000, reports().last().positionMs)

        // Paused: every 30 s, so the server keeps the session (it lets one go after 90 s unheard).
        val count = reports().size
        advanceTimeBy(20_000)
        runCurrent()
        assertEquals(count, reports().size)
        advanceTimeBy(10_000)
        runCurrent()
        assertEquals(count + 1, reports().size)
        assertEquals(ProgressState.Paused, reports().last().state)
        assertEquals(31_000, reports().last().positionMs)
    }

    @Test
    fun aVideoEndsTheMusicsSessionAndPlayStartsANewOneWhereItWas() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 30_000
        music.pauseForVideo()
        settle()
        assertEquals(1, api.calls("stopPlayback").size)
        assertEquals(ProgressState.Stopped, reports().last().state)
        assertEquals(30_000, reports().last().positionMs)
        val now = music.nowPlaying.value!!
        assertFalse(now.playing)
        assertEquals(1L, now.itemId)

        // Nothing keeps a session through the film.
        val asked = api.calls.size
        advanceTimeBy(600_000)
        runCurrent()
        assertEquals(asked, api.calls.size)

        music.toggle()
        settle()
        assertEquals(listOf(1L, 1L), starts().map { it.itemId })
        assertEquals(30_000L, starts().last().startMs)
        assertEquals(30_000L, output.startPositions.last())
        assertTrue(output.playWhenReady)
    }

    @Test
    fun leavingTheTvAppPausesMusicThatPlaysAndItPlaysOnWhereItWas() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 30_000
        music.pauseForLeave(playOnReturn = true)
        settle()
        assertEquals(1, api.calls("stopPlayback").size)
        assertFalse(music.nowPlaying.value!!.playing)

        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(listOf(1L, 1L), starts().map { it.itemId })
        assertEquals(30_000L, starts().last().startMs)
        assertTrue(output.playWhenReady)
    }

    @Test
    fun aTrackStillAskedForWhenTheTvAppIsLeftPlaysOnWhenItIsBack() = runTest {
        val music = music()
        music.play(album)
        // Left before the server answered: nothing was asked for yet.
        music.pauseForLeave(playOnReturn = true)
        settle()
        assertEquals(emptyList<PlaybackRequest>(), starts())

        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(listOf(1L), starts().map { it.itemId })
        assertTrue(output.playWhenReady)
    }

    @Test
    fun musicPausedWhileItBuffersStaysPausedAfterTheTvAppIsLeft() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        music.pause()
        output.buffer()
        assertTrue(music.nowPlaying.value!!.loading)

        music.pauseForLeave(playOnReturn = true)
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(0, api.calls("stopPlayback").size)
        assertEquals(1, starts().size)
        assertFalse(output.playWhenReady)
    }

    @Test
    fun theSleepTimersNextTrackWaitingPausedStaysPausedAfterTheTvAppIsLeft() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        music.setSleep(SleepChoice.EndOfItem)
        output.end()
        // The next track is being asked for, to wait paused.
        music.pauseForLeave(playOnReturn = true)
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(listOf(1L, 2L), starts().map { it.itemId })
        assertFalse(output.playWhenReady)
    }

    @Test
    fun musicPlayedAndPausedAgainWhileAwayStaysPaused() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        music.pauseForLeave(playOnReturn = true)
        settle()
        // A media key on the TV's home screen plays it, then pauses it.
        music.play()
        settle()
        output.startPlaying()
        music.pause()

        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(2, starts().size)
        assertFalse(output.playWhenReady)
    }

    @Test
    fun leavingWhileTheNextTrackIsAskedForPlaysItFromItsStart() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 83_000
        music.next()
        // Left before the server answered for track 2: the output is still at track 1's place.
        music.pauseForLeave(playOnReturn = true)
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(listOf(1L, 2L), starts().map { it.itemId })
        assertNull(starts().last().startMs)
        assertEquals(0L, output.startPositions.last())
        assertTrue(output.playWhenReady)
    }

    @Test
    fun anotherTrackChosenWhileAwayIsLeftAsItIs() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        music.pauseForLeave(playOnReturn = true)
        settle()
        // The remote's Next, through the media session, on the TV's home screen: it plays.
        music.next()
        settle()
        output.startPlaying()
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(listOf(1L, 2L), starts().map { it.itemId })
        assertTrue(output.playWhenReady)
    }

    @Test
    fun theOutputFailingWhileAwayStartsNoSessionAndTheMusicStillPlaysOn() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 30_000
        music.pauseForLeave(playOnReturn = true)
        settle()
        // Its last fetch, from the session that just ended.
        output.fail(OutputError.Network)
        settle()
        assertEquals(1, starts().size)
        assertNull(music.nowPlaying.value!!.error)

        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(listOf(1L, 1L), starts().map { it.itemId })
        assertEquals(30_000L, starts().last().startMs)
        assertTrue(output.playWhenReady)
    }

    @Test
    fun pausePressedWhileAwayKeepsTheMusicPaused() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        music.pauseForLeave(playOnReturn = true)
        // The remote's Pause, through the media session, on the TV's home screen.
        music.pause()
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(1, starts().size)
    }

    @Test
    fun theLastBackAVideoOrAWatchNextProgramLeaveTheMusicPaused() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        music.pauseForLeave(playOnReturn = false)
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(1, starts().size)

        music.play()
        settle()
        output.startPlaying()
        music.pauseForLeave(playOnReturn = true)
        music.pauseForVideo()
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(2, starts().size)

        music.play()
        settle()
        output.startPlaying()
        music.pauseForLeave(playOnReturn = true)
        music.returnAfterLeave(playOn = false)
        // Forgotten: coming back again later doesn't play it either.
        music.returnAfterLeave(playOn = true)
        settle()
        assertEquals(3, starts().size)
        assertFalse(output.playWhenReady)
    }

    @Test
    fun aTrackEndsWithStoppedThenTheNextOneStartsItsOwnSession() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 3_427_000
        output.end()
        settle()

        val names = api.calls.map { it.method }
        val stopped = names.indexOf("stopPlayback")
        val second = names.lastIndexOf("startPlayback")
        assertTrue("the first session ends before the second starts: $names", stopped in 0 until second)
        val end = reports().last { it.state == ProgressState.Stopped }
        assertEquals(3_427_000, end.positionMs)
        assertEquals(listOf(1L, 2L), starts().map { it.itemId })
        assertEquals(2, output.loaded.size)
        assertEquals(1, music.nowPlaying.value?.index)
    }

    @Test
    fun theLastTrackEndingStopsAndPlayStartsItAgain() = runTest {
        val music = music()
        music.play(album, startIndex = 2)
        runCurrent()
        output.startPlaying()
        output.end()
        settle()
        val now = music.nowPlaying.value!!
        assertTrue(now.ended)
        assertFalse(now.playing)
        assertEquals(1, starts().size)

        music.toggle()
        settle()
        assertEquals(listOf(3L, 3L), starts().map { it.itemId })
        assertFalse(music.nowPlaying.value!!.ended)
    }

    @Test
    fun previousRestartsAfterThreeSecondsElseGoesBack() = runTest {
        val music = music()
        music.play(album, startIndex = 1)
        runCurrent()
        output.startPlaying()
        output.position = 45_000
        music.previous()
        runCurrent()
        assertEquals(0L, output.seeks.last())
        assertEquals(1, starts().size)

        output.position = 2_000
        music.previous()
        settle()
        assertEquals(listOf(2L, 1L), starts().map { it.itemId })
        // The track left ends where it was.
        assertEquals(2_000, reports().last { it.state == ProgressState.Stopped }.positionMs)
    }

    @Test
    fun aTrackTheDeviceCantDecodeIsAskedForAgainAsATranscode() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 20_000
        output.fail(OutputError.Decode)
        settle()

        val again = starts().last()
        assertEquals(PlaybackMode.Transcode, again.mode)
        assertEquals(20_000L, again.startMs)
        assertEquals(20_000L, output.startPositions.last())

        // A transcode failing too is the end of it.
        api.answer("startPlayback", transcodeInfo())
        output.fail(OutputError.Decode)
        settle()
        assertEquals(MusicError.Unplayable, music.nowPlaying.value?.error)
    }

    @Test
    fun aTrackThatCantBePlayedSaysSoThenTheNextOnePlaysByItself() = runTest {
        val music = music()
        api.fail("startPlayback", RuntimeException("the server can't open the file"))
        music.play(album)
        runCurrent()
        assertEquals(MusicError.Unplayable, music.nowPlaying.value?.error)
        assertEquals(MusicPlayback.SKIP_AFTER_S, music.nowPlaying.value?.nextIn)
        advanceTimeBy(2_000)
        runCurrent()
        assertEquals(MusicPlayback.SKIP_AFTER_S - 2, music.nowPlaying.value?.nextIn)
        assertEquals(listOf(1L), starts().map { it.itemId })

        // The server can open the next one (reset forgets the calls before with the failure).
        api.reset()
        advanceTimeBy(3_000)
        runCurrent()
        val now = music.nowPlaying.value!!
        assertEquals(2L, now.itemId)
        assertNull(now.error)
        assertNull(now.nextIn)
        assertEquals(listOf(2L), starts().map { it.itemId })
    }

    @Test
    fun pausedTheTrackThatCantBePlayedWaitsAndPlayTriesItAgain() = runTest {
        val music = music()
        api.fail("startPlayback", RuntimeException("the server can't open the file"))
        music.play(album)
        runCurrent()
        music.pause()
        assertNull(music.nowPlaying.value?.nextIn)
        advanceTimeBy(10_000)
        runCurrent()
        assertEquals(1L, music.nowPlaying.value?.itemId)
        assertEquals(MusicError.Unplayable, music.nowPlaying.value?.error)

        assertEquals(listOf(1L), starts().map { it.itemId })

        api.reset()
        music.play()
        settle()
        assertEquals(listOf(1L), starts().map { it.itemId })
        assertNull(music.nowPlaying.value?.error)
    }

    @Test
    fun noTrackPlaysByItselfOutOfTheServersReachAtTheQueuesEndOrRoundAQueueNoneOfPlays() = runTest {
        val music = music()
        // Out of reach, the next track would fail the same way.
        api.fail("startPlayback", OfflineException(IOException("refused")))
        music.play(album)
        runCurrent()
        assertEquals(MusicError.Offline, music.nowPlaying.value?.error)
        assertNull(music.nowPlaying.value?.nextIn)

        // The queue's last track: nothing comes after it.
        api.fail("startPlayback", RuntimeException("the server can't open the file"))
        music.play(album, startIndex = 2)
        runCurrent()
        assertEquals(MusicError.Unplayable, music.nowPlaying.value?.error)
        assertNull(music.nowPlaying.value?.nextIn)

        // A queue that repeats, none of whose tracks plays: each is tried once, then it stops there.
        music.setRepeat(MusicRepeat.All)
        music.play(album)
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals(listOf(1L, 2L, 3L), starts().takeLast(3).map { it.itemId })
        assertEquals(3L, music.nowPlaying.value?.itemId)
        assertNull(music.nowPlaying.value?.nextIn)
    }

    @Test
    fun anExpiredSessionIsStartedAgainWhereItWasThreeTimesAtMost() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 90_000
        repeat(3) {
            output.fail(OutputError.Gone)
            settle()
        }
        assertEquals(4, starts().size)
        assertTrue(starts().drop(1).all { it.startMs == 90_000L && it.mode == PlaybackMode.Auto })
        assertNull(music.nowPlaying.value?.error)

        output.fail(OutputError.Gone)
        settle()
        assertEquals(4, starts().size)
        assertEquals(MusicError.Unplayable, music.nowPlaying.value?.error)
    }

    @Test
    fun aFailureThePlayerCantNameAsksTheServer() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 15_000
        // The session goes on: the track is at fault, asked for once more as a transcode.
        output.fail(OutputError.Other)
        settle()
        assertEquals(1, api.calls("playbackStatus").size)
        assertEquals(PlaybackMode.Transcode, starts().last().mode)
        assertEquals(15_000L, starts().last().startMs)

        // The server out of reach: a new session where it was, three times in a row, then it says so.
        api.answer("startPlayback", transcodeInfo())
        api.fail("playbackStatus", OfflineException(IOException("refused")))
        repeat(MusicPlayback.MAX_RECREATES) {
            output.fail(OutputError.Other)
            settle()
        }
        assertEquals(5, starts().size)
        assertNull(music.nowPlaying.value?.error)
        output.fail(OutputError.Other)
        settle()
        assertEquals(5, starts().size)
        assertEquals(MusicError.Offline, music.nowPlaying.value?.error)
    }

    @Test
    fun aFailureThePlayerCantNameIsTheServersStopWhenItSaysSo() = runTest {
        api.fail("playbackStatus", ApiException(410, "session_stopped", "Stopped by Ann."))
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 8_000
        output.fail(OutputError.Other)
        settle()
        assertEquals(MusicError.Stopped, music.nowPlaying.value?.error)
        assertEquals(1, starts().size)
        assertTrue(api.calls("stopPlayback").isEmpty())
        // Where it was is kept, and nothing is deleted.
        assertEquals(ProgressState.Stopped, reports().last().state)
        assertEquals(8_000, reports().last().positionMs)
    }

    @Test
    fun aSessionStoppedByTheServerStaysStopped() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.fail(OutputError.Stopped)
        settle()
        assertEquals(1, starts().size)
        assertEquals(MusicError.Stopped, music.nowPlaying.value?.error)
        assertTrue(api.calls("stopPlayback").isEmpty())

        // Play starts it again, with a new session; then the event stops that one.
        music.play()
        settle()
        assertEquals(2, starts().size)
        assertNull(music.nowPlaying.value?.error)
        events.emit(ServerEvent.SessionStopped(sessionId = DIRECT_SESSION, byAdmin = true))
        runCurrent()
        assertEquals(MusicError.Stopped, music.nowPlaying.value?.error)
        assertEquals(2, starts().size)
    }

    @Test
    fun musicObeysEveryCommandOfTheRemote() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        assertEquals(MusicPlayback.CONTROLS, starts().single().controls)
        output.startPlaying()
        output.position = 20_000

        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Pause))
        runCurrent()
        assertFalse(output.playWhenReady)
        assertEquals(ProgressState.Paused, reports().last().state)

        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Play))
        runCurrent()
        assertTrue(output.playWhenReady)
        assertEquals(ProgressState.Playing, reports().last().state)

        // A seek stays within the track, a skip goes from where it is; each is reported straight away.
        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Seek, positionMs = 400_000))
        runCurrent()
        assertEquals(180_000L, output.seeks.last())
        assertEquals(180_000L, reports().last().positionMs)
        output.position = 60_000
        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Skip, offsetMs = -10_000))
        runCurrent()
        assertEquals(50_000L, output.seeks.last())
        assertEquals(50_000L, reports().last().positionMs)
        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Skip, offsetMs = -600_000))
        runCurrent()
        assertEquals(0L, output.seeks.last())

        // Previous: more than 3 s in, the start of the same track.
        output.position = 30_000
        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Previous))
        runCurrent()
        assertEquals(0L, output.seeks.last())
        assertEquals(0L, reports().last().positionMs)
        assertEquals(1, starts().size)

        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Next))
        settle()
        assertEquals(listOf(1L, 2L), starts().map { it.itemId })
        // Previous at the start of a track goes to the one before.
        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Previous))
        settle()
        assertEquals(listOf(1L, 2L, 1L), starts().map { it.itemId })
    }

    @Test
    fun playAndPauseThatChangeNothingAreSaidAgainSoARemoteALittleBehindSettles() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        val before = reports().size
        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Play))
        runCurrent()
        assertTrue(output.playWhenReady)
        assertEquals(before + 1, reports().size)
        assertEquals(ProgressState.Playing, reports().last().state)

        music.pause()
        runCurrent()
        val paused = reports().size
        events.emit(ServerEvent.PlayerCommand(DIRECT_SESSION, Control.Pause))
        runCurrent()
        assertFalse(output.playWhenReady)
        assertEquals(paused + 1, reports().size)
        assertEquals(ProgressState.Paused, reports().last().state)
    }

    @Test
    fun anotherSessionsCommandsChangeNothing() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        val count = reports().size
        events.emit(ServerEvent.PlayerCommand("someone-else", Control.Pause))
        events.emit(ServerEvent.PlayerCommand("someone-else", Control.Next))
        settle()
        assertTrue(output.playWhenReady)
        assertEquals(1, starts().size)
        assertEquals(count, reports().size)
        assertTrue(music.nowPlaying.value!!.playing)
    }

    @Test
    fun theEventStreamIsHeldWhileMusicIsUnderWayBetweenTracksToo() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        assertEquals(true, held.last())
        output.end()
        settle()
        // The next track's session starts: never let go in between.
        assertTrue(held.all { it })
        music.pauseForVideo()
        assertEquals(false, held.last())
        music.play()
        settle()
        assertEquals(true, held.last())
        music.stop()
        assertEquals(false, held.last())
    }

    @Test
    fun musicPausedForAWhileLetsTheEventStreamGoUntilItPlaysAgain() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        music.pause()
        runCurrent()
        advanceTimeBy(MusicPlayback.PAUSED_HOLD_MS - 1_000)
        runCurrent()
        assertEquals(true, held.last())
        advanceTimeBy(1_000)
        runCurrent()
        assertEquals(false, held.last())
        // The session is kept all the same: the paused reports go on.
        val count = reports().size
        advanceTimeBy(MusicPlayback.PAUSED_EVERY_MS)
        runCurrent()
        assertEquals(count + 1, reports().size)

        music.play()
        runCurrent()
        assertEquals(true, held.last())
        // Paused again, the wait starts over.
        music.pause()
        advanceTimeBy(MusicPlayback.PAUSED_HOLD_MS / 2)
        music.play()
        advanceTimeBy(MusicPlayback.PAUSED_HOLD_MS)
        runCurrent()
        assertEquals(true, held.last())
    }

    @Test
    fun theRemoteIsToldWhichTracksSessionPlaysHere() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        assertEquals(DIRECT_SESSION, here.last())
        music.stop()
        assertNull(here.last())
    }

    @Test
    fun theServerOutOfReachShowsAsOffline() = runTest {
        api.fail("startPlayback", OfflineException(IOException("refused")))
        val music = music()
        music.play(album)
        settle()
        assertEquals(MusicError.Offline, music.nowPlaying.value?.error)
        assertTrue(output.loaded.isEmpty())
    }

    @Test
    fun stoppingEndsTheSessionAndForgetsTheQueue() = runTest {
        val music = music()
        var stopped = false
        music.onStopped = { stopped = true }
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 7_000
        music.stop()
        settle()
        assertNull(music.nowPlaying.value)
        assertTrue(music.queueState.value.tracks.isEmpty())
        assertTrue(output.released)
        assertTrue(stopped)
        assertEquals(ProgressState.Stopped, reports().last().state)
        assertEquals(1, api.calls("stopPlayback").size)
    }

    @Test
    fun shufflingKeepsTheTrackPlaying() = runTest {
        val music = music()
        music.play((1..10L).map { track(it) }, startIndex = 4)
        runCurrent()
        music.setShuffle(true)
        val queue = music.queueState.value
        assertTrue(queue.shuffle)
        assertEquals(5L, queue.current?.id)
        assertEquals(1, starts().size)
    }

    @Test
    fun repeatingTheQueuePlaysItAgainFromItsStartAndNextAndPreviousGoRoundItsEnds() = runTest {
        val music = music()
        music.play(album, startIndex = 2)
        runCurrent()
        music.setRepeat(MusicRepeat.All)
        assertEquals(MusicRepeat.All, music.queueState.value.repeat)
        // The last track has a next one now: the first.
        assertTrue(music.nowPlaying.value!!.hasNext)
        output.startPlaying()
        output.end()
        settle()
        assertEquals(listOf(3L, 1L), starts().map { it.itemId })
        assertFalse(music.nowPlaying.value!!.ended)
        assertEquals(0, music.nowPlaying.value?.index)

        // Previous on the first track, just started, is the last; Next there, the first again.
        music.previous()
        settle()
        music.next()
        settle()
        assertEquals(listOf(3L, 1L, 3L, 1L), starts().map { it.itemId })
    }

    @Test
    fun repeatingTheTrackPlaysItAgainWithASessionOfItsOwnAndNextStillMovesOn() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        music.setRepeat(MusicRepeat.One)
        output.startPlaying()
        output.position = 180_000
        output.end()
        settle()
        // The same track, from its start, in a new session: the one before ended, as any track's does.
        assertEquals(listOf(1L, 1L), starts().map { it.itemId })
        assertEquals(1, api.calls("stopPlayback").size)
        assertEquals(0L, output.startPositions.last())
        assertEquals(0, music.nowPlaying.value?.index)

        music.next()
        settle()
        assertEquals(listOf(1L, 1L, 2L), starts().map { it.itemId })
    }

    @Test
    fun repeatIsForgottenWithTheQueueAndKeptAsAnotherAlbumPlays() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        music.setRepeat(MusicRepeat.All)
        music.play(album, startIndex = 1)
        runCurrent()
        assertEquals(MusicRepeat.All, music.nowPlaying.value?.repeat)

        music.stop()
        settle()
        music.play(album)
        runCurrent()
        assertEquals(MusicRepeat.Off, music.nowPlaying.value?.repeat)
    }

    @Test
    fun betweenTracksThePositionIsTheNextTracksStartNotWhereTheLastOneWas() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 120_000
        // The server is still asked for the next track: the output holds the last one, two minutes in.
        api.hold()
        music.next()
        runCurrent()
        assertEquals(0L, music.position().positionMs)
        assertEquals(0L, music.position().bufferedMs)
        assertEquals(180_000L, music.position().durationMs)

        // Previous pressed now goes back to the track left: the one coming hasn't played a second.
        music.previous()
        api.release()
        settle()
        assertEquals(0, music.nowPlaying.value?.index)
        assertEquals(1L, starts().last().itemId)
        assertEquals(0L, output.startPositions.last())
    }

    @Test
    fun aTrackThatFailedToStartPlaysFromItsStartNotFromWhereTheLastOneWas() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 120_000
        api.fail("startPlayback", OfflineException(IOException("refused")))
        music.next()
        settle()
        assertEquals(MusicError.Offline, music.nowPlaying.value?.error)

        api.reset()
        music.play()
        settle()
        val again = starts().single()
        assertEquals(2L, again.itemId)
        assertNull(again.startMs)
        assertEquals(0L, output.startPositions.last())
    }

    @Test
    fun whatWasPlayingIsBroughtBackWhereItWas() = runTest {
        val music = music()
        music.setRepeat(MusicRepeat.One)
        music.play(album, startIndex = 2, startMs = 61_000)
        runCurrent()
        val request = starts().single()
        assertEquals(3L, request.itemId)
        assertEquals(61_000L, request.startMs)
        assertEquals(61_000L, output.startPositions.single())
        assertTrue(output.wants)
        assertEquals(MusicRepeat.One, music.nowPlaying.value?.repeat)
    }

    @Test
    fun aQueueMadeReadyWaitsWhereItWasAndPlayStartsItThere() = runTest {
        val music = music()
        music.cue(album, startIndex = 2, startMs = 61_000)
        settle()
        // Nothing is asked of the server and nothing sounds: it waits, paused, where it will start.
        assertEquals(emptyList<PlaybackRequest>(), starts())
        assertEquals(0, output.loaded.size)
        val waiting = music.nowPlaying.value!!
        assertEquals(3L, waiting.itemId)
        assertFalse(waiting.wantsPlay)
        assertFalse(waiting.loading)
        assertEquals(61_000L, music.position().positionMs)
        assertTrue(music.active)
        assertFalse(held.lastOrNull() ?: false)

        // Made ready again as it waits (the Assistant asks to prepare, then to play): it stays as it is.
        music.cue(album, startIndex = 2, startMs = 0)
        assertEquals(61_000L, music.position().positionMs)

        music.play()
        runCurrent()
        val request = starts().single()
        assertEquals(3L, request.itemId)
        assertEquals(61_000L, request.startMs)
        assertEquals(61_000L, output.startPositions.single())
        assertTrue(output.wants)
    }

    @Test
    fun anotherQueueIsNeverShownWithTheTrackBeforesPlace() = runTest {
        val music = music()
        music.play(album)
        runCurrent()
        output.startPlaying()
        output.position = 150_000
        output.stopTells = true
        // Read as the queue changes, there and then (what keeps it for Play does so on the main thread at once).
        val seen = mutableListOf<Pair<Long, Long>>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            music.queueState.collect { queue -> seen += queue.current!!.id to music.position().positionMs }
        }
        music.play(album.reversed())
        runCurrent()
        // The new queue's first track, at its start, never where the last track was as the output stopped it.
        assertEquals(listOf(3L to 0L), seen.drop(1).distinct())
    }

    @Test
    fun whatWasPlayingShuffledWaitsShuffledAsItWas() = runTest {
        val music = music()
        // As it played: 3, 1, 2, the album being 1, 2, 3.
        val saved = SavedMusic(listOf(album[2], album[0], album[1]), index = 1, positionMs = 30_000, repeat = MusicRepeat.All, given = listOf(2, 0, 1))
        music.cue(saved)
        settle()
        val queue = music.queueState.value
        assertTrue(queue.shuffle)
        assertEquals(listOf(3L, 1L, 2L), queue.tracks.map { it.id })
        assertEquals(1L, queue.current?.id)
        assertEquals(MusicRepeat.All, queue.repeat)
        assertEquals(30_000L, music.position().positionMs)
        assertEquals(emptyList<PlaybackRequest>(), starts())

        // Brought back again as it waits: it stays as it is.
        music.cue(saved.copy(positionMs = 0))
        assertEquals(30_000L, music.position().positionMs)

        music.setShuffle(false)
        assertEquals(listOf(1L, 2L, 3L), music.queueState.value.tracks.map { it.id })
        assertEquals(1L, music.queueState.value.current?.id)
    }

    @Test
    fun aQueueThatWaitsStartsWhereItIsSoughtToAndNextPlaysTheNextTrack() = runTest {
        val music = music()
        music.cue(album, startIndex = 0, startMs = 5_000)
        music.seekTo(90_000)
        assertEquals(90_000L, music.position().positionMs)
        assertEquals(1, music.nowPlaying.value?.seeks)
        music.play()
        runCurrent()
        assertEquals(90_000L, starts().single().startMs)

        // Another queue made ready while this one plays takes its place, and plays nothing itself.
        output.startPlaying()
        music.cue(album.reversed(), startIndex = 1)
        settle()
        assertEquals(1, starts().size)
        assertEquals(2L, music.nowPlaying.value?.itemId)
        assertFalse(music.nowPlaying.value!!.wantsPlay)
        assertEquals(0L, music.position().positionMs)
        music.next()
        runCurrent()
        assertEquals(listOf(1L, 1L), starts().map { it.itemId })
        assertEquals(null, starts().last().startMs)
        assertTrue(output.wants)
    }

    @Test
    fun previousOnAQueueThatWaitsGoesToTheTracksStartAndWaitsOn() = runTest {
        val music = music()
        music.play(album, startIndex = 0)
        runCurrent()
        output.startPlaying()
        // Made ready over music that played: the queue waits all the same, nothing asked of the server.
        music.cue(album, startIndex = 1, startMs = 90_000)
        settle()
        music.previous()
        runCurrent()
        assertEquals(1, starts().size)
        assertEquals(0L, music.position().positionMs)
        assertEquals(2L, music.nowPlaying.value?.itemId)
        assertFalse(music.nowPlaying.value!!.wantsPlay)
        music.play()
        runCurrent()
        assertEquals(listOf(1L, 2L), starts().map { it.itemId })
        assertEquals(null, starts().last().startMs)
    }

    @Test
    fun aSeekFromAnywhereIsToldSoWhateverShowsThePositionReadsItAgainPausedToo() = runTest {
        val music = music()
        music.play(album, startIndex = 1)
        runCurrent()
        output.startPlaying()
        music.pause()
        val before = music.nowPlaying.value!!.seeks

        // A car's scrubber, the notification's or the remote's: paused, no reading of the position is due.
        music.seekTo(4_000)
        assertEquals(before + 1, music.nowPlaying.value!!.seeks)
        assertEquals(4_000L, music.position().positionMs)

        // Previous back to the track's start is a seek too.
        music.previous()
        assertEquals(before + 2, music.nowPlaying.value!!.seeks)
        assertEquals(0L, music.position().positionMs)
    }

    @Test
    fun playPauseShowsWhatItWillDoWhileATrackStartsAndThroughAPausedSeek() = runTest {
        val music = music()
        api.hold()
        music.play(album)
        runCurrent()
        // Starting: it will play, so the button is Pause; pressed, the track will wait.
        assertTrue(music.nowPlaying.value!!.wantsPlay)
        music.pause()
        assertFalse(music.nowPlaying.value!!.wantsPlay)
        music.play()
        assertTrue(music.nowPlaying.value!!.wantsPlay)
        api.release()
        settle()
        output.startPlaying()
        assertTrue(music.nowPlaying.value!!.wantsPlay)

        music.pause()
        assertFalse(music.nowPlaying.value!!.wantsPlay)
        // Sought while paused, the output loads there: still Play, not Pause, while it does and after.
        music.seekTo(60_000)
        output.buffer(true)
        assertTrue(music.nowPlaying.value!!.loading)
        assertFalse(music.nowPlaying.value!!.wantsPlay)
        output.buffer(false)
        assertFalse(music.nowPlaying.value!!.wantsPlay)
        // After the queue's end too: Play starts the last track again.
        music.play()
        output.startPlaying()
        music.jumpTo(2)
        settle()
        output.startPlaying()
        output.end()
        settle()
        assertFalse(music.nowPlaying.value!!.wantsPlay)
    }

    private fun transcodeInfo() = PlaybackInfo(sessionId = "t1", method = PlaybackMethod.Transcode, url = "/api/playback/t1/index.m3u8", hls = true, fileId = 601, durationMs = 180_000)

    private fun track(id: Long) = ItemSummary(
        id = id,
        libraryId = 3,
        kind = ItemKind.Track,
        title = "Track $id",
        index = id.toInt(),
        parentId = 50,
        parentTitle = "Deep Water",
        grandparentId = 40,
        grandparentTitle = "Harbour Lights",
        durationMs = 180_000,
    )

    private companion object {
        /** The session of a direct play (fixtures/playback/info_direct.json). */
        const val DIRECT_SESSION = "0b6d5e9a2c4f4a7e8d1c3b5a7f9e2d4c"
    }

    /** An output whose sound the test plays, ends and fails. */
    private class FakeOutput : MusicOutput {
        val loaded = mutableListOf<EngineSource>()
        val startPositions = mutableListOf<Long>()
        val seeks = mutableListOf<Long>()
        var info: TrackInfo? = null
        var released = false
        var position = 0L
        private var listener: MusicOutput.Listener? = null

        /** Whether it plays once ready (playWhenReady). */
        var wants = false
            private set
        private var playing = false

        override fun load(source: EngineSource, startMs: Long, play: Boolean, info: TrackInfo) {
            loaded += source
            startPositions += startMs
            this.info = info
            position = startMs
            wants = play
        }

        fun startPlaying() {
            playing = true
            listener?.onPlayingChanged(true)
        }

        fun buffer() {
            listener?.onBuffering(true)
        }

        fun end() {
            playing = false
            listener?.onPlayingChanged(false)
            listener?.onEnded()
        }

        fun fail(error: OutputError) {
            playing = false
            listener?.onPlayingChanged(false)
            listener?.onError(error)
        }

        /** Waits for data, or has it again, as after a seek. */
        fun buffer(waiting: Boolean) {
            listener?.onBuffering(waiting)
        }

        override fun play() {
            wants = true
            listener?.onPlayWhenReadyChanged(true)
        }

        override fun pause() {
            wants = false
            listener?.onPlayWhenReadyChanged(false)
            if (playing) {
                playing = false
                listener?.onPlayingChanged(false)
            }
        }

        override fun seekTo(positionMs: Long) {
            seeks += positionMs
            position = positionMs
        }

        var playsAt = 1f

        override fun setSpeed(speed: Float) {
            playsAt = speed
        }

        /** Whether [stop] tells the music it no longer plays, there and then, as ExoPlayer's listeners are. */
        var stopTells = false

        override fun stop() {
            val was = playing
            playing = false
            if (was && stopTells) listener?.onPlayingChanged(false)
        }

        override fun release() {
            released = true
        }

        override val positionMs: Long get() = position
        override val durationMs: Long get() = 180_000
        override val bufferedMs: Long get() = position
        override val playWhenReady: Boolean get() = wants

        override fun setListener(listener: MusicOutput.Listener?) {
            this.listener = listener
        }
    }
}
