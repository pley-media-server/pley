@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.music

import app.pley.android.model.Control
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
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
import app.pley.android.player.session.SessionReporter
import app.pley.android.testing.network.FakePleyApi
import java.io.IOException
import kotlin.random.Random
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
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
        private var wants = false
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

        override fun stop() {
            playing = false
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
