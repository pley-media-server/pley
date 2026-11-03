@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.session

import app.pley.android.caps.DefaultPlaybackCapsPolicy
import app.pley.android.data.drive.Drive
import app.pley.android.data.drive.DriveFiles
import app.pley.android.data.drive.DriveKept
import app.pley.android.data.drive.DriveResume
import app.pley.android.data.drive.Drives
import app.pley.android.data.drive.FileDriveFiles
import app.pley.android.model.Chapter
import app.pley.android.model.DynamicRange
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import app.pley.android.testing.caps.FakeDeviceCapabilities
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import app.pley.android.testing.network.FakePleyApi
import app.pley.android.testing.player.session.FakeEngineLink
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestCoroutineScheduler
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment

/**
 * A video on a drive in Pley's player, with no server (the Samsung app's
 * LocalTests): the file as it is with the subtitle files beside it, its
 * tracks and chapters as Media3 reads them, where it stopped kept on the
 * device, and what it says when it can't play.
 */
@RunWith(RobolectricTestRunner::class)
class DrivePlaybackTest {
    private val dir = Files.createTempDirectory("pley-drive").toFile()

    /** The drive's folder, as Android paths go (forward slashes, here too). */
    private val root = dir.path.replace('\\', '/')
    private val film = "$root/Mad Max.mkv".also { File(it).writeBytes(ByteArray(64)) }
    private val scheduler = TestCoroutineScheduler()
    private val dispatcher = StandardTestDispatcher(scheduler)
    private val engine = FakeEngineLink { scheduler.currentTime }
    private val api = FakePleyApi()
    private val texts = ResourcePlayerTexts(RuntimeEnvironment.getApplication())
    private val drives = object : Drives {
        override val drive: StateFlow<Drive?> = MutableStateFlow(Drive(root))
        override val files: DriveFiles = FileDriveFiles()
        override val resume: DriveResume = DriveResume.memory()
    }
    private val controllers = mutableListOf<SessionPlaybackController>()

    @After
    fun close() {
        controllers.forEach { it.close() }
        scheduler.runCurrent()
        dir.deleteRecursively()
    }

    private fun play(request: PlayRequest = PlayRequest.drive(film)): SessionPlaybackController {
        val video = DriveVideo(film, drives, dispatcher)
        val env = PlaybackEnvironment(
            api = api,
            capabilities = FakeDeviceCapabilities(probe = capsOf(TestDevices.sdrPhone)),
            policy = DefaultPlaybackCapsPolicy,
            details = ItemDetails { video.detail(it) },
            engines = { engine },
            texts = texts,
            clientId = { "android-test" },
            reporter = SessionReporter(CoroutineScope(dispatcher + SupervisorJob())),
            subtitles = DriveSubtitleFetcher(dispatcher),
            now = { scheduler.currentTime },
            work = dispatcher,
            local = video,
            drive = true,
        )
        val controller = SessionPlaybackController(request, env, CoroutineScope(dispatcher + SupervisorJob()))
        controllers += controller
        controller.start()
        scheduler.runCurrent()
        return controller
    }

    private fun advance(ms: Long) {
        scheduler.advanceTimeBy(ms)
        scheduler.runCurrent()
    }

    /** The Samsung test's film: HEVC, TrueHD with an AC-3 beside it, Russian AAC, English text, two chapters. */
    private fun read(playingAudio: Int = 1) = engine.tracks(
        EngineTracks(
            video = listOf(EngineTrack(null, "video/hevc", null, supported = true, width = 1920, height = 800, range = DynamicRange.Sdr, selected = true)),
            audio = listOf(
                EngineTrack("en", "audio/true-hd", 8, supported = true, label = "Atmos", default = true, selected = playingAudio == 0),
                EngineTrack("en", "audio/ac3", 6, supported = true, label = "Compatibility", selected = playingAudio == 1),
                EngineTrack("ru", "audio/mp4a-latm", 2, supported = true, selected = playingAudio == 2),
            ),
            text = listOf(EngineTrack("en", "application/x-subrip", null, supported = true)),
            chapters = listOf(Chapter(0, 0, "Opening"), Chapter(600_000, 0, "The Road")),
        ),
    )

    @Test
    fun aFilmOnADrivePlaysAsItIsWithTheSubtitleFilesBesideIt() {
        File("$root/Mad Max.en.srt").writeText("1\n00:00:01,000 --> 00:00:02,000\nWitness me\n")
        File("$root/Mad Max.notes.txt").writeText("not subtitles")
        val c = play()
        val source = engine.source!!
        assertEquals(File(film).toURI().toString(), source.url)
        assertFalse(source.hls)
        assertEquals("video/x-matroska", source.mimeType)
        assertTrue(source.profile7BaseLayer)
        val s = c.state.value
        assertEquals("Mad Max", s.title)
        assertEquals(texts.method(SessionTestInfo.direct), s.method)
        // Nothing converts it, nor searches for subtitles for it.
        assertTrue(s.qualities.isEmpty())
        assertFalse(s.canFindSubtitles)
        assertEquals(listOf(null, DriveStreams.SIDECAR_FROM.toLong()), s.subtitles.map { it.index })

        // The file beside it, through the app's WebVTT, and Sync moves it.
        c.selectSubtitle(DriveStreams.SIDECAR_FROM.toLong())
        engine.ready()
        advance(1_500)
        assertEquals("Witness me", c.cues.value.single().text.toString())
        assertTrue(c.state.value.subtitleSync)
        // Nothing was asked of a server.
        assertTrue(api.calls.toString(), api.calls.isEmpty())
    }

    @Test
    fun itsTracksAndChaptersComeFromMedia3AsItLoads() {
        val c = play()
        engine.durationMs = 7_380_000
        read(playingAudio = 1)
        scheduler.runCurrent()
        val s = c.state.value
        assertEquals(listOf(1L, 2L, 3L), s.audio.map { it.index })
        // TrueHD is the file's default, but Media3 plays the AC-3: it shows as chosen.
        assertEquals(2L, s.audio.single { it.selected }.index)
        assertEquals(listOf(null, 4L), s.subtitles.map { it.index })
        assertEquals(listOf(0L to 600_000L, 600_000L to 7_380_000L), s.chapters.map { it.startMs to it.endMs })
        assertEquals("The Road", s.chapters[1].title)

        // Its own tracks, chosen on the engine by their order; Sync is for the files beside it.
        c.selectAudio(3)
        assertEquals(2 to "ru", engine.audioChoices.last())
        c.selectSubtitle(4)
        assertEquals(0 to "en", engine.textChoices.last())
        assertFalse(c.state.value.subtitleSync)
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun whereItStoppedIsKeptOnTheDeviceUntilItWasWatched() {
        val first = play()
        engine.durationMs = 100_000
        engine.ready()
        advance(30_000)
        first.close()
        controllers.remove(first)
        scheduler.runCurrent()
        assertEquals(DriveKept(30_000, 100_000), drives.resume.kept(film))

        // Played again, it starts there; to its end, it was watched.
        play()
        assertEquals(30_000L, engine.prepared.last().startMs)
        engine.ready()
        engine.end()
        scheduler.runCurrent()
        assertEquals(DriveKept(), drives.resume.kept(film))
    }

    @Test
    fun whatOnlyAConversionWouldPlaySaysSo() {
        val c = play()
        engine.ready()
        engine.fail(EngineError.Unsupported(audio = false, container = false, detail = "no decoder takes it"))
        scheduler.runCurrent()
        val error = c.state.value.phase as PlayerPhase.Error
        assertEquals(texts.driveUnplayable, error.message)
        assertFalse(error.canRetry)
        assertEquals(1, engine.prepared.size)
    }

    @Test
    fun aPictureThatNeverStartsSaysSoAfterTwelveSeconds() {
        // Loading and loading, its picture never showing.
        val c = play()
        advance(13_000)
        val error = c.state.value.phase as PlayerPhase.Error
        assertEquals(texts.driveStuck, error.message)
        assertTrue(error.canRetry)
        c.retry()
        scheduler.runCurrent()
        assertEquals(2, engine.prepared.size)
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun aPictureThatShowedThenStandsStillSaysTheDriveMayHaveBeenPulledOut() {
        // It played, then waits for data that doesn't come: the file is one this TV plays, so not
        // "try another copy" (as on LG TVs).
        val c = play()
        engine.ready()
        advance(2_000)
        engine.starve()
        advance(13_000)
        val error = c.state.value.phase as PlayerPhase.Error
        assertEquals(texts.driveUnreadable, error.message)
        assertTrue(error.canRetry)
        assertTrue(api.calls.isEmpty())

        // Shown and standing still with data: the same, once it has shown.
        c.retry()
        scheduler.runCurrent()
        engine.ready()
        engine.stall()
        advance(13_000)
        assertEquals(texts.driveUnreadable, (c.state.value.phase as PlayerPhase.Error).message)
        assertEquals(2, engine.prepared.size)
    }

    @Test
    fun aFileThatCantBeReadAsItPlaysSaysTheDriveMayHaveBeenPulledOut() {
        val c = play()
        engine.ready()
        // Read again where it was, a few times, then it says what went wrong: no server to blame.
        repeat(Recovery.MAX_NETWORK + 1) {
            engine.fail(EngineError.Network("ERROR_CODE_IO_UNSPECIFIED"))
            scheduler.runCurrent()
        }
        val error = c.state.value.phase as PlayerPhase.Error
        assertEquals(texts.driveUnreadable, error.message)
        assertTrue(error.canRetry)
        assertFalse(error.offline)
        assertEquals(Recovery.MAX_NETWORK + 1, engine.prepared.size)
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun aFileThatIsntThereAnymoreSaysTheDriveMayHaveBeenPulledOut() {
        File(film).delete()
        val c = play()
        assertEquals(texts.driveUnreadable, (c.state.value.phase as PlayerPhase.Error).message)
    }

    @Test
    fun itComesBackWithItsTracksWhenItsWindowDoes() {
        File("$root/Mad Max.fr.vtt").writeText("WEBVTT\n\n00:01.000 --> 00:02.000\nBonjour\n")
        val first = play()
        read()
        scheduler.runCurrent()
        first.selectAudio(3)
        val saved = SavedPlaybackStateBundle.roundTrip(first.saveState())
        assertEquals(film, saved.drivePath)
        first.close()
        controllers.remove(first)
        scheduler.runCurrent()

        // The window came back: the same file, the Russian track once Media3 has read it.
        val again = play(saved.toPlayRequest())
        assertEquals(film, engine.prepared.last().source.url.let { File(java.net.URI(it)).path.replace('\\', '/') })
        read()
        scheduler.runCurrent()
        assertEquals(3L, again.state.value.audio.single { it.selected }.index)
        assertEquals(2 to "ru", engine.audioChoices.last())
    }
}

/** A direct play's answer, for the method the player shows. */
private object SessionTestInfo {
    val direct = app.pley.android.model.PlaybackInfo(sessionId = "x", method = app.pley.android.model.PlaybackMethod.Direct, url = "file:/x", fileId = 1)
}

/** A saved state through the Bundle the player screen keeps, as after the process went. */
private object SavedPlaybackStateBundle {
    fun roundTrip(state: SavedPlaybackState): SavedPlaybackState = SavedPlaybackState.fromBundle(state.toBundle())!!
}
