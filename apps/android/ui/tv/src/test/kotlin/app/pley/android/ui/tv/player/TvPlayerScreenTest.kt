package app.pley.android.ui.tv.player

import android.os.SystemClock
import android.view.KeyEvent as AndroidKeyEvent
import android.view.View
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.getBoundsInRoot
import app.pley.android.player.subtitles.CaptionLook
import kotlin.math.roundToInt
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performKeyPress
import androidx.compose.ui.test.pressKey
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind
import app.pley.android.model.MarkerSource
import app.pley.android.player.ChapterMark
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.QualityChoice
import app.pley.android.player.TrackChoice
import app.pley.android.player.UpNextUi
import app.pley.android.testing.player.engine.FakePlayerEngine
import app.pley.android.testing.player.session.FakePlaybackController
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The TV player with the remote's keys: Centre, Left and Right with the
 * controls hidden, held keys scrubbing and seeking once, Up and Down
 * showing the controls, Back walking out, Skip and Up Next taking focus,
 * the media keys, the panels, the refresh-rate switch and errors.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
@OptIn(ExperimentalTestApi::class)
class TvPlayerScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var exits = 0

    /** An episode 10 minutes in, an hour long, with subtitles, two audio tracks and chapters. */
    private fun playing(change: (PlayerUiState) -> PlayerUiState = { it }) = FakePlaybackController(
        change(
            PlayerUiState(
                phase = PlayerPhase.Playing,
                itemId = 20_001,
                title = "Fixture Show",
                subtitle = "S1 E1 — Pilot",
                positionMs = 600_000,
                durationMs = 3_600_000,
                bufferedMs = 660_000,
                audio = listOf(TrackChoice(1, "English (AAC Stereo)", true), TrackChoice(2, "French (EAC3 5.1)", false)),
                subtitles = listOf(TrackChoice(null, "Off", true), TrackChoice(3, "English (SRT)", false)),
                chapters = listOf(
                    ChapterMark("Opening", 0, 300_000, null),
                    ChapterMark("The Records", 300_000, 1_800_000, null),
                    ChapterMark("Credits", 1_800_000, 3_600_000, null),
                ),
            ),
        ),
    )

    private fun show(controller: FakePlaybackController) {
        compose.setContent {
            TvTheme {
                TvPlayerScreen(controller, onExit = { exits++ }, now = { 0L })
            }
        }
        compose.waitForIdle()
    }

    private fun press(key: Key, times: Int = 1) {
        repeat(times) {
            compose.onRoot().performKeyInput { pressKey(key) }
            compose.waitForIdle()
        }
    }

    /** Sends one native key event, as a remote's held key does (repeats included). */
    private fun send(action: Int, code: Int, repeat: Int, at: Long) {
        compose.onRoot().performKeyPress(KeyEvent(AndroidKeyEvent(at, at, action, code, repeat)))
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    private fun focusedOn(tag: String) = compose.onNodeWithTag(tag, useUnmergedTree = true).assertIsFocused()

    /** Down shows the controls on Play/Pause; Right goes on past Forward 10 and Next to Subtitles (then Audio, Info). */
    private fun toSubtitles() {
        press(Key.DirectionDown)
        focusedOn(TAG_PLAY_PAUSE)
        press(Key.DirectionRight, times = 3)
        focusedOn(TAG_SUBTITLES)
    }

    @Test
    fun `Centre with the controls hidden shows them on the scrubber, where it pauses and plays`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionCenter)
        assertTrue("no pause out of sight of the controls", controller.calls.isEmpty())
        focusedOn(TAG_SCRUBBER)
        press(Key.DirectionCenter)
        assertEquals(listOf("pause"), controller.calls.map { it.name })
        controller.update { it.copy(phase = PlayerPhase.Paused) }
        compose.waitForIdle()
        press(Key.DirectionCenter)
        assertEquals("play", controller.calls.last().name)
    }

    @Test
    fun `Centre with the controls hidden goes to a fast forward on its way, and they stay hidden`() {
        val controller = playing()
        show(controller)
        press(Key.MediaFastForward)
        press(Key.DirectionCenter)
        assertEquals(listOf(630_000L), controller.calls("seekTo").map { it.args.single() })
        compose.onNodeWithTag(TAG_SCRUBBER).assertDoesNotExist()
        compose.mainClock.advanceTimeBy(TvSeeker.SETTLE_MS + 100)
        compose.waitForIdle()
        assertEquals(1, controller.calls("seekTo").size)
    }

    @Test
    fun `Left shows the controls on the scrubber with the preview, and seeks 5 s after the last press`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionLeft, times = 3)
        focusedOn(TAG_SCRUBBER)
        compose.onNodeWithTag(TAG_PREVIEW).assertExists()
        // The preview says where it goes: the badge is for the controls hidden.
        compose.onNodeWithTag(TAG_BADGE).assertDoesNotExist()
        compose.mainClock.advanceTimeBy(TvSeeker.SETTLE_MS + 100)
        compose.waitForIdle()
        assertTrue("it waits for OK", controller.calls("seekTo").isEmpty())
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS)
        compose.waitForIdle()
        assertEquals(listOf(570_000L), controller.calls("seekTo").map { it.args.single() })
        compose.onNodeWithTag(TAG_PREVIEW).assertDoesNotExist()
        compose.onNodeWithTag(TAG_SCRUBBER).assertExists()
    }

    @Test
    fun `Centre during or just after a seek goes there and plays on, instead of pausing`() {
        val controller = playing()
        show(controller)
        // A seek still to come is made now.
        press(Key.DirectionRight)
        press(Key.DirectionCenter)
        assertEquals(listOf("seekTo"), controller.calls.map { it.name })
        assertEquals(610_000L, controller.calls("seekTo").single().args.single())
        // A seek made a moment ago is confirmed, not paused.
        press(Key.DirectionRight)
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS + 100)
        compose.waitForIdle()
        press(Key.DirectionCenter)
        assertEquals(listOf("seekTo", "seekTo"), controller.calls.map { it.name })
        // Long after the seek, Centre pauses again.
        press(Key.DirectionRight)
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS + TvSeeker.CONFIRM_MS + 100)
        compose.waitForIdle()
        press(Key.DirectionCenter)
        assertEquals("pause", controller.calls.last().name)
    }

    @Test
    fun `holding Right scrubs through the previews and, let go, waits for OK there`() {
        val controller = playing()
        show(controller)
        val start = SystemClock.uptimeMillis()
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_RIGHT, 0, start)
        for (i in 1..40) send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_DPAD_RIGHT, i, start + 400 + i * 50L)
        // The scrubber shows with the preview, and nothing seeks while the key is held, not even after the presses' wait.
        compose.onNodeWithTag(TAG_PREVIEW).assertExists()
        focusedOn(TAG_SCRUBBER)
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS + 100)
        compose.waitForIdle()
        assertTrue(controller.calls("seekTo").isEmpty())
        // Let go: it waits for OK where it got to, the wait starting again.
        send(AndroidKeyEvent.ACTION_UP, AndroidKeyEvent.KEYCODE_DPAD_RIGHT, 0, start + 2_500)
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS - 500)
        compose.waitForIdle()
        assertTrue(controller.calls("seekTo").isEmpty())
        compose.onNodeWithTag(TAG_PREVIEW).assertExists()
        press(Key.DirectionCenter)
        val seeks = controller.calls("seekTo").map { it.args.single() as Long }
        assertEquals(1, seeks.size)
        assertTrue("held 2 s goes past one step: ${seeks.single()}", seeks.single() > 610_000L + 30_000L)
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS)
        compose.waitForIdle()
        assertEquals(1, controller.calls("seekTo").size)
    }

    @Test
    fun `Back lets a seek waiting go, and the controls stay`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionRight, times = 2)
        compose.onNodeWithTag(TAG_PREVIEW).assertExists()
        back()
        compose.onNodeWithTag(TAG_PREVIEW).assertDoesNotExist()
        focusedOn(TAG_SCRUBBER)
        compose.onNodeWithText("10:00").assertExists()
        assertEquals(0, exits)
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS + 100)
        compose.waitForIdle()
        assertTrue(controller.calls("seekTo").isEmpty())
    }

    @Test
    fun `Next and Previous take the place of a seek waiting`() {
        // A film with chapters: Next and Previous go by them.
        val controller = playing { it.copy(subtitle = null) }
        show(controller)
        press(Key.DirectionRight)
        press(Key.MediaNext)
        assertEquals(listOf(1_800_000L), controller.calls("seekTo").map { it.args.single() })
        press(Key.DirectionLeft, times = 3)
        press(Key.MediaPrevious)
        assertEquals(listOf(1_800_000L, 300_000L), controller.calls("seekTo").map { it.args.single() })
        compose.mainClock.advanceTimeBy(TvSeeker.WAIT_MS + 100)
        compose.waitForIdle()
        assertEquals("the seeks waiting went", 2, controller.calls("seekTo").size)
    }

    @Test
    fun `the row's Forward 10 seeks 400 ms after the last press, without OK`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionUp)
        press(Key.DirectionDown)
        focusedOn(TAG_PLAY_PAUSE)
        press(Key.DirectionRight)
        press(Key.DirectionCenter, times = 2)
        assertTrue(controller.calls("seekTo").isEmpty())
        compose.mainClock.advanceTimeBy(TvSeeker.SETTLE_MS + 100)
        compose.waitForIdle()
        assertEquals(listOf(620_000L), controller.calls("seekTo").map { it.args.single() })
    }

    @Test
    fun `the scrubber's times stand where a seek coming goes`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionUp)
        compose.onNodeWithText("10:00").assertExists()
        compose.onNodeWithText("−50:00").assertExists()
        press(Key.DirectionRight, times = 3)
        compose.onNodeWithText("−49:30").assertExists()
        compose.onNodeWithText("10:00").assertDoesNotExist()
        assertTrue(controller.calls("seekTo").isEmpty())
    }

    @Test
    fun `a seek's preview names its chapter, numbered when its title is only where it starts`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionUp)
        press(Key.DirectionRight)
        compose.onNodeWithText("Chapter 2 · The Records").assertExists()
        controller.update { s -> s.copy(chapters = s.chapters.map { if (it.startMs == 300_000L) it.copy(title = "00:05:00.000") else it }) }
        compose.waitForIdle()
        compose.onNodeWithText("Chapter 2").assertExists()
        compose.onNodeWithText("00:05:00.000", substring = true).assertDoesNotExist()
    }

    @Test
    fun `Up shows the controls on the scrubber, Down on Play-Pause`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionUp)
        focusedOn(TAG_SCRUBBER)
        press(Key.DirectionDown)
        focusedOn(TAG_PLAY_PAUSE)
        back()
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_SCRUBBER).assertDoesNotExist()
        press(Key.DirectionDown)
        focusedOn(TAG_PLAY_PAUSE)
        assertTrue(controller.calls.isEmpty())
    }

    /** Media3's SubtitleViews on screen: text first, then pictures. */
    private fun subtitleViews(): List<View> {
        val found = mutableListOf<View>()
        fun walk(view: View) {
            if (view.javaClass.name == "androidx.media3.ui.SubtitleView") found += view
            if (view is ViewGroup) (0 until view.childCount).forEach { walk(view.getChildAt(it)) }
        }
        walk(compose.activity.window.decorView)
        return found
    }

    @Test
    fun `text subtitles stay inside the safe area, and glide above the controls while they show, at the same size`() {
        show(playing())
        val density = compose.activity.resources.displayMetrics.density
        fun px(dp: androidx.compose.ui.unit.Dp) = (dp.value * density).roundToInt()
        val (text, pictures) = subtitleViews()
        assertEquals(px(SUBTITLES_BOTTOM), text.paddingBottom)
        assertEquals(px(app.pley.android.designsystem.TvSafeArea.Vertical), text.paddingTop)
        assertEquals(px(app.pley.android.designsystem.TvSafeArea.Horizontal), text.paddingLeft)
        assertEquals(px(app.pley.android.designsystem.TvSafeArea.Horizontal), text.paddingRight)
        // Pictures (PGS) keep their frame's places over the whole screen.
        assertEquals(0, pictures.paddingBottom)
        val resting = compose.onNodeWithTag(TAG_SUBTITLES_TEXT).getBoundsInRoot().top
        press(Key.DirectionUp)
        compose.mainClock.advanceTimeBy(1_000)
        // Their lines stay as they were (the same padding), only drawn higher, by the controls' height.
        assertEquals(px(SUBTITLES_BOTTOM), subtitleViews().first().paddingBottom)
        val lifted = compose.onNodeWithTag(TAG_SUBTITLES_TEXT).getBoundsInRoot().top
        assertEquals((SUBTITLES_LIFTED - SUBTITLES_BOTTOM).value, (resting - lifted).value, 0.5f)
        assertTrue(SUBTITLES_LIFTED > app.pley.android.designsystem.PleyTv.CanvasHeight - TIMES_TOP)
    }

    @Test
    fun `Playback details show over the video while nothing else does, asking for the details`() {
        val controller = playing { it.copy(details = FakePlaybackController.everything().details) }
        compose.setContent {
            TvTheme {
                TvPlayerScreen(controller, onExit = { exits++ }, showDetails = true, now = { 0L })
            }
        }
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_DETAILS).assertExists()
        compose.onNodeWithText("Transcoding · CPU").assertExists()
        assertEquals(true, controller.calls("setDetailsOpen").last().args.single())
        press(Key.DirectionUp)
        compose.onNodeWithTag(TAG_DETAILS).assertDoesNotExist()
        assertEquals(false, controller.calls("setDetailsOpen").last().args.single())
    }

    @Test
    fun `the Subtitles panel sets their size and background, and stays open`() {
        val controller = playing()
        val chosen = mutableListOf<CaptionLook>()
        compose.setContent {
            TvTheme {
                TvPlayerScreen(controller, onExit = { exits++ }, onSubtitleStyle = { chosen += it }, now = { 0L })
            }
        }
        compose.waitForIdle()
        toSubtitles()
        press(Key.DirectionCenter)
        compose.onNodeWithTag(TAG_PANEL).assertExists()
        // Off, English (SRT), then Small, Normal and Large.
        press(Key.DirectionDown, times = 4)
        compose.onNodeWithText("Large").assertIsFocused()
        press(Key.DirectionCenter)
        assertEquals(listOf(CaptionLook(textFraction = 0.06f)), chosen)
        assertTrue(controller.calls("selectSubtitle").isEmpty())
        compose.onNodeWithTag(TAG_PANEL).assertExists()
        // Then Huge, None and Dark box (headings don't take focus).
        press(Key.DirectionDown, times = 3)
        compose.onNodeWithText("Dark box").assertIsFocused()
        press(Key.DirectionCenter)
        assertEquals(CaptionLook(box = true), chosen.last())
    }

    @Test
    fun `the Audio panel moves the sound in 50 ms steps above its tracks`() {
        val controller = playing()
        show(controller)
        toSubtitles()
        press(Key.DirectionRight)
        press(Key.DirectionCenter)
        compose.onNodeWithTag(TAG_PANEL).assertExists()
        // The track playing has focus; the delay is above it.
        press(Key.DirectionUp)
        compose.onNodeWithTag(TAG_AUDIO_DELAY).assertIsFocused()
        press(Key.DirectionRight)
        assertEquals(listOf<Any?>(50L), controller.calls("setAudioDelay").single().args)
        press(Key.DirectionLeft)
        assertEquals(listOf<Any?>(-50L), controller.calls("setAudioDelay").last().args)
    }

    @Test
    fun `Back closes the panel, then hides the controls, then leaves`() {
        val controller = playing()
        show(controller)
        toSubtitles()
        press(Key.DirectionCenter)
        compose.onNodeWithTag(TAG_PANEL).assertExists()
        back()
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_PANEL).assertDoesNotExist()
        focusedOn(TAG_SUBTITLES)
        back()
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_SCRUBBER).assertDoesNotExist()
        assertEquals(0, exits)
        back()
        assertEquals(1, exits)
    }

    @Test
    fun `the subtitles panel ticks the current track and chooses another`() {
        val controller = playing()
        show(controller)
        toSubtitles()
        press(Key.DirectionCenter)
        compose.onNodeWithText("English (SRT)").assertExists()
        press(Key.DirectionDown)
        press(Key.DirectionCenter)
        assertEquals(listOf(3L), controller.calls("selectSubtitle").map { it.args.single() })
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_PANEL).assertDoesNotExist()
        focusedOn(TAG_SUBTITLES)
    }

    @Test
    fun `Info asks for the playback's details while it is open`() {
        val controller = playing()
        show(controller)
        toSubtitles()
        press(Key.DirectionRight, times = 2)
        focusedOn(TAG_INFO)
        press(Key.DirectionCenter)
        assertEquals(true, controller.calls("setDetailsOpen").last().args.single())
        back()
        assertEquals(false, controller.calls("setDetailsOpen").last().args.single())
    }

    @Test
    fun `Skip takes focus while its marker runs and Centre skips`() {
        val controller = playing {
            it.copy(
                positionMs = 3_000,
                skipLabel = "Skip Intro",
                markers = listOf(Marker(1, MarkerKind.Intro, 2_000, 60_000, MarkerSource.Fingerprint)),
            )
        }
        show(controller)
        focusedOn(TAG_SKIP)
        // Left and Right still seek; Centre is Skip's.
        press(Key.DirectionCenter)
        assertEquals(listOf("skip"), controller.calls.map { it.name })
        controller.update { it.copy(skipLabel = null, positionMs = 61_000) }
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_SKIP).assertDoesNotExist()
        // Skip gone, Centre is the video's again: it shows the controls.
        press(Key.DirectionCenter)
        assertEquals(listOf("skip"), controller.calls.map { it.name })
        focusedOn(TAG_SCRUBBER)
    }

    @Test
    fun `Up Next focuses Play now, which plays next, and Back cancels it`() {
        val controller = playing()
        show(controller)
        press(Key.DirectionUp)
        controller.update { it.copy(upNextCard = UpNextUi(20_002, "Second", "Up next · S1 E2", null, 7)) }
        compose.waitForIdle()
        focusedOn(TAG_PLAY_NOW)
        compose.onNodeWithText("Up next · S1 · E2").assertExists()
        compose.onNodeWithTag(TAG_SCRUBBER).assertDoesNotExist()
        press(Key.DirectionCenter)
        assertEquals("playNext", controller.calls.last().name)
        back()
        assertEquals("cancelUpNext", controller.calls.last().name)
        assertEquals(0, exits)
    }

    @Test
    fun `media keys work whatever shows`() {
        val controller = playing()
        show(controller)
        press(Key.MediaFastForward)
        compose.onNodeWithText("+30 s").assertExists()
        compose.mainClock.advanceTimeBy(TvSeeker.SETTLE_MS + 100)
        compose.waitForIdle()
        assertEquals(listOf(630_000L), controller.calls("seekTo").map { it.args.single() })
        // Play/Pause straight after fast forward plays on from there; later, it pauses.
        press(Key.MediaPlayPause)
        assertEquals("seekTo", controller.calls.last().name)
        compose.mainClock.advanceTimeBy(TvSeeker.CONFIRM_MS)
        compose.waitForIdle()
        press(Key.MediaPlayPause)
        assertEquals("pause", controller.calls.last().name)
        // Next on an episode plays the next one; Previous goes to the chapter's start.
        press(Key.MediaNext)
        assertEquals("playNext", controller.calls.last().name)
        press(Key.MediaPrevious)
        assertEquals(listOf(630_000L, 300_000L), controller.calls("seekTo").map { it.args.single() })
    }

    @Test
    fun `CAPTIONS turns subtitles on and off, saying so`() {
        val controller = playing()
        show(controller)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_CAPTIONS, 0, SystemClock.uptimeMillis())
        assertEquals(listOf(3L), controller.calls("selectSubtitle").map { it.args.single() })
        compose.onNodeWithText("Subtitles: English (SRT)").assertExists()
        controller.update { it.copy(subtitles = listOf(TrackChoice(null, "Off", false), TrackChoice(3, "English (SRT)", true))) }
        compose.waitForIdle()
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_CAPTIONS, 0, SystemClock.uptimeMillis())
        assertEquals(listOf(3L, null), controller.calls("selectSubtitle").map { it.args.single() })
        compose.onNodeWithText("Subtitles off").assertExists()
    }

    @Test
    fun `the audio key opens the Audio panel to choose in, changing nothing by itself`() {
        val controller = playing()
        show(controller)
        send(AndroidKeyEvent.ACTION_DOWN, AndroidKeyEvent.KEYCODE_MEDIA_AUDIO_TRACK, 0, SystemClock.uptimeMillis())
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_PANEL).assertExists()
        compose.onNodeWithText("French (EAC3 5.1)").assertExists()
        assertTrue(controller.calls("selectAudio").isEmpty())
        // Choosing there picks the track; the panel closes onto the Audio button.
        press(Key.DirectionDown)
        press(Key.DirectionCenter)
        assertEquals(listOf(2L), controller.calls("selectAudio").map { it.args.single() })
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_PANEL).assertDoesNotExist()
        compose.onNodeWithTag(TAG_SCRUBBER).assertExists()
    }

    @Test
    fun `the screen goes black with the ring while the TV matches its refresh rate, saying nothing`() {
        val controller = playing { it.copy(phase = PlayerPhase.Paused) }
        val engine = FakePlayerEngine()
        engine.update { it.copy(switchingDisplayMode = true, switchingToHz = 23.976f) }
        controller.engine = engine
        show(controller)
        compose.mainClock.advanceTimeBy(5_000)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_SWITCHING).assertExists()
        compose.onNodeWithTag(TAG_BUFFERING).assertExists()
        compose.onNodeWithText("Matching refresh rate…", substring = true).assertDoesNotExist()
        compose.onNodeWithText("23.976", substring = true).assertDoesNotExist()
        engine.update { it.copy(switchingDisplayMode = false, switchingToHz = null) }
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_SWITCHING).assertDoesNotExist()
    }

    @Test
    fun `the ring says Reconnecting while the server is out of reach`() {
        val controller = playing { it.copy(phase = PlayerPhase.Buffering, reconnecting = true) }
        show(controller)
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_BUFFERING).assertExists()
        compose.onNodeWithText("Reconnecting…").assertExists()
        controller.update { it.copy(phase = PlayerPhase.Playing, reconnecting = false) }
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_RECONNECTING).assertDoesNotExist()
    }

    @Test
    fun `the top bar badges how it plays, and a transcode the player chose says why`() {
        val controller = playing { it.copy(method = "Transcoding") }
        show(controller)
        press(Key.DirectionUp)
        compose.onNodeWithText("Transcoding").assertExists()

        controller.update { it.copy(note = "The original didn't play smoothly on this device.", canPlayOriginal = true) }
        compose.waitForIdle()
        compose.onNodeWithText("The original didn't play smoothly on this device. Play original is in Quality.").assertExists()

        controller.update { it.copy(note = null, suggestLowerQuality = true) }
        compose.waitForIdle()
        compose.onNodeWithText("Choose one in Quality.", substring = true).assertExists()
    }

    @Test
    fun `an error offers Try again, focused, and Back`() {
        val controller = playing { it.copy(phase = PlayerPhase.Error("The server stopped answering.", canRetry = true)) }
        show(controller)
        compose.onNodeWithText("Can't play this item").assertExists()
        compose.onNodeWithText("The server stopped answering.").assertExists()
        press(Key.DirectionCenter)
        assertEquals("retry", controller.calls.last().name)
        back()
        assertEquals(1, exits)
    }

    @Test
    fun `the quality panel puts Automatic under Original, says why it is lower, and chooses with the D-pad`() {
        val controller = playing {
            it.copy(
                quality = "Automatic · 8 Mbps 1080p",
                note = "Lowered for your connection",
                qualities = listOf(
                    QualityChoice("original", "Original", "4K · 62 Mbps", false),
                    QualityChoice("auto", "Automatic", "Now 8 Mbps 1080p", true),
                    QualityChoice("30m", "30 Mbps 4K", null, false),
                    QualityChoice("15m", "15 Mbps 4K", null, false),
                    QualityChoice("20m", "20 Mbps 1080p", null, false),
                    QualityChoice("8m", "8 Mbps 1080p", null, false),
                ),
            )
        }
        show(controller)
        toSubtitles()
        press(Key.DirectionRight, times = 2)
        press(Key.DirectionCenter)
        compose.onNodeWithTag(TAG_PANEL).assertExists()
        compose.onNodeWithText("Lowered for your connection").assertExists()
        // Focus starts on the choice, Automatic, saying what it plays.
        compose.onNode(isFocused()).assert(hasText("Automatic")).assert(hasText("Now 8 Mbps 1080p"))
        press(Key.DirectionUp)
        compose.onNode(isFocused()).assert(hasText("Original"))
        press(Key.DirectionDown, times = 3)
        compose.onNode(isFocused()).assert(hasText("15 Mbps 4K"))
        press(Key.DirectionCenter)
        assertEquals(listOf<Any?>("15m"), controller.calls("selectQuality").single().args)
    }

    @Test
    fun `the playback's notices show as a toast and are dismissed`() {
        val controller = playing { it.copy(notice = "This is the last episode") }
        show(controller)
        compose.onNodeWithText("This is the last episode").assertExists()
        assertTrue(controller.calls("dismissNotice").isNotEmpty())
        compose.mainClock.advanceTimeBy(TvPlayerUi.TOAST_MS + 100)
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_TOAST).assertDoesNotExist()
    }
}
