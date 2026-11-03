package app.pley.android.ui.phone.player

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.click
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind
import app.pley.android.model.MarkerSource
import app.pley.android.player.ChapterMark
import app.pley.android.player.FoundSubtitle
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.QualityChoice
import app.pley.android.player.SubtitleSearchUi
import app.pley.android.player.TrackChoice
import app.pley.android.player.UpNextUi
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.testing.player.session.FakePlaybackController
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The phone player over a FakePlaybackController: timing, Skip, Up Next, menus, errors and recompositions. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w915dp-h412dp-land")
class PhonePlayerScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private class Host : PlayerHost {
        var backs = 0
        var level = 0.5f

        override fun back() {
            backs++
        }

        override val pipSupported = true

        override fun enterPip() = Unit

        override fun volume() = level

        override fun setVolume(level: Float) {
            this.level = level
        }

        override fun brightness() = level

        override fun setBrightness(level: Float) {
            this.level = level
        }
    }

    private val host = Host()

    private fun playing(change: (PlayerUiState) -> PlayerUiState = { it }) = change(
        PlayerUiState(
            phase = PlayerPhase.Playing,
            itemId = 20_001,
            title = "Fixture Show",
            subtitle = "S1 E1 — Pilot",
            positionMs = 30_000,
            durationMs = 600_000,
            bufferedMs = 60_000,
            method = "Direct play",
            audio = listOf(TrackChoice(1, "English (AAC Stereo)", true), TrackChoice(2, "French (EAC3 5.1)", false)),
            subtitles = listOf(
                TrackChoice(null, "Off", false),
                TrackChoice(3, "English (SRT)", true),
                TrackChoice(4, "French (PGS)", false, burnIn = true),
            ),
            quality = "Original",
            qualities = listOf(
                QualityChoice("original", "Original", "4K · 62 Mbps", true),
                QualityChoice("4m", "4 Mbps 720p", null, false),
            ),
            chapters = listOf(ChapterMark("Intro", 0, 60_000, null), ChapterMark("Part 1", 60_000, 600_000, null)),
        ),
    )

    private fun show(controller: FakePlaybackController) {
        compose.setContent {
            PhoneTheme {
                PhonePlayerScreen(controller, host, PictureState(), CaptionLook(), SubtitleChoice(), {}, inPip = false)
            }
        }
        compose.waitForIdle()
    }

    @After
    fun forget() {
        PlayerProbe.composed = null
    }

    @Test
    fun controlsHideAfterThreeSecondsWhilePlayingAndStayWhilePaused() {
        val fake = FakePlaybackController(playing())
        compose.mainClock.autoAdvance = false
        show(fake)
        compose.mainClock.advanceTimeBy(500)
        compose.onNodeWithText("Fixture Show").assertIsDisplayed()
        compose.mainClock.advanceTimeBy(2_000)
        compose.onNodeWithText("Fixture Show").assertIsDisplayed()
        compose.mainClock.advanceTimeBy(1_500)
        assertTrue(compose.onAllNodesWithTextCount("Fixture Show") == 0)

        // A tap in the middle brings them back; paused, they stay.
        fake.update { it.copy(phase = PlayerPhase.Paused) }
        compose.mainClock.autoAdvance = true
        compose.onNodeWithTag(TAG_PLAYER).performTouchInput { click(center) }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        compose.onNodeWithText("Fixture Show").assertIsDisplayed()
        compose.mainClock.advanceTimeBy(10_000)
        compose.onNodeWithText("Fixture Show").assertIsDisplayed()
    }

    private fun androidx.compose.ui.test.junit4.ComposeContentTestRule.onAllNodesWithTextCount(text: String): Int =
        onAllNodes(androidx.compose.ui.test.hasText(text)).fetchSemanticsNodes().size

    @Test
    fun skipShowsWhileAMarkerRunsAndSkips() {
        val fake = FakePlaybackController(
            playing {
                it.copy(
                    skipLabel = "Skip Intro",
                    markers = listOf(Marker(1, MarkerKind.Intro, 2_000, 80_000, MarkerSource.Fingerprint)),
                )
            },
        )
        show(fake)
        compose.onNodeWithText("Skip Intro").assertIsDisplayed().performClick()
        assertEquals(1, fake.calls("skip").size)

        fake.update { it.copy(skipLabel = null) }
        compose.waitForIdle()
        assertEquals(0, compose.onAllNodesWithTextCount("Skip Intro"))
    }

    @Test
    fun upNextCountsDownAndCancelsOrPlaysNow() {
        val fake = FakePlaybackController(
            playing { it.copy(upNextCard = UpNextUi(20_002, "Second", "Up next · S1 E2", null, 7), upNext = "Second", upNextSecondsLeft = 7) },
        )
        show(fake)
        compose.onNodeWithText("Up next · S1 E2").assertIsDisplayed()
        compose.onNodeWithText("Play now (7)").assertIsDisplayed()

        fake.update { it.copy(upNextCard = it.upNextCard?.copy(secondsLeft = 6), upNextSecondsLeft = 6) }
        compose.onNodeWithText("Play now (6)").assertIsDisplayed()

        compose.onNodeWithText("Cancel").performClick()
        assertEquals(1, fake.calls("cancelUpNext").size)
        compose.onNodeWithText("Play now (6)").performClick()
        assertEquals(1, fake.calls("playNext").size)
    }

    @Test
    fun qualityListsTheOriginalWithItsHintAndTheLowerQualities() {
        val fake = FakePlaybackController(playing())
        show(fake)
        compose.onNodeWithText("Quality").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("4K · 62 Mbps").assertIsDisplayed()
        compose.onNodeWithText("4 Mbps 720p").performClick()
        assertEquals(listOf<Any?>("4m"), fake.calls("selectQuality").single().args)
        compose.waitForIdle()
        assertEquals(0, compose.onAllNodesWithTextCount("4K · 62 Mbps"))
    }

    @Test
    fun qualityPutsAutomaticUnderOriginalAndSaysWhatItPlays() {
        val fake = FakePlaybackController(
            playing {
                it.copy(
                    quality = "Automatic · 8 Mbps 1080p",
                    note = "Lowered for your connection",
                    qualities = listOf(
                        QualityChoice("original", "Original", "4K · 62 Mbps", false),
                        QualityChoice("auto", "Automatic", "Now 8 Mbps 1080p", true),
                        QualityChoice("30m", "30 Mbps 4K", null, false),
                        QualityChoice("15m", "15 Mbps 4K", null, false),
                        QualityChoice("20m", "20 Mbps 1080p", null, false),
                    ),
                )
            },
        )
        show(fake)
        compose.onNodeWithContentDescription("Playback settings").performClick()
        compose.waitForIdle()
        // The settings' row says what Automatic plays.
        compose.onNodeWithText("Automatic · 8 Mbps 1080p").assertIsDisplayed().performClick()
        compose.waitForIdle()
        val tops = listOf("Original", "Automatic", "30 Mbps 4K", "15 Mbps 4K").map { compose.onNodeWithText(it).getBoundsInRoot().top }
        assertEquals(tops.sorted(), tops)
        compose.onNodeWithText("Now 8 Mbps 1080p").assertIsDisplayed()
        compose.onNodeWithText("15 Mbps 4K").performClick()
        assertEquals(listOf<Any?>("15m"), fake.calls("selectQuality").single().args)
    }

    @Test
    fun findSubtitlesListsWhatTheServerFoundAndAddsThePickedOne() {
        val fake = FakePlaybackController(playing { it.copy(canFindSubtitles = true) })
        show(fake)
        compose.onNodeWithText("Subtitles").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Find subtitles…").performScrollTo().performClick()
        compose.waitForIdle()
        assertEquals(1, fake.calls("searchSubtitles").size)
        fake.update {
            it.copy(subtitleSearch = SubtitleSearchUi.Found(listOf(FoundSubtitle("opensubtitles:7", "Pilot.1080p.WEB", "English · Made for this file")), null))
        }
        compose.waitForIdle()
        compose.onNodeWithText("English · Made for this file").assertIsDisplayed()
        compose.onNodeWithText("Pilot.1080p.WEB").performClick()
        assertEquals(listOf<Any?>("opensubtitles:7"), fake.calls("addFoundSubtitle").single().args)
        // Not set up on the server: it says so.
        fake.update { it.copy(subtitleSearch = SubtitleSearchUi.Unavailable("Subtitle search isn't set up on this server")) }
        compose.waitForIdle()
        compose.onNodeWithText("Subtitle search isn't set up on this server").assertIsDisplayed()
    }

    @Test
    fun speedAndAudioDelayAreInTheSettings() {
        val fake = FakePlaybackController(playing())
        show(fake)
        compose.onNodeWithContentDescription("Playback settings").performClick()
        compose.waitForIdle()
        compose.onNode(hasText("Speed") and hasText("1×")).performScrollTo().performClick()
        compose.waitForIdle()
        compose.onNodeWithText("1.5×").performClick()
        assertEquals(listOf<Any?>(1.5f), fake.calls("setSpeed").single().args)
        compose.onNodeWithContentDescription("Playback settings").performClick()
        compose.waitForIdle()
        compose.onNode(hasText("Audio") and hasText("English (AAC Stereo)")).performClick()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Play the sound later").performClick()
        assertEquals(listOf<Any?>(50L), fake.calls("setAudioDelay").single().args)
    }

    @Test
    fun subtitlesListPictureTracksByNameAlone() {
        val fake = FakePlaybackController(playing())
        show(fake)
        compose.onNodeWithText("Subtitles").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("French (PGS)").performClick()
        assertEquals(listOf<Any?>(4L), fake.calls("selectSubtitle").single().args)
    }

    @Test
    fun settingsLeadToEachMenuAndBack() {
        val fake = FakePlaybackController(playing())
        show(fake)
        compose.onNodeWithContentDescription("Playback settings").performClick()
        compose.waitForIdle()
        compose.onNode(hasText("Quality") and hasText("Original")).assertIsDisplayed()
        compose.onNode(hasText("Audio") and hasText("English (AAC Stereo)")).performClick()
        compose.waitForIdle()
        compose.onNodeWithText("French (EAC3 5.1)").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back to settings").performClick()
        compose.waitForIdle()
        // Below Speed and the sleep timer: scrolled to on a short sheet.
        compose.onNodeWithText("Playback details").performScrollTo().performClick()
        compose.waitForIdle()
        assertEquals(listOf<Any?>(true), fake.calls("setDetailsOpen").first().args)
        compose.onNodeWithContentDescription("Close").performClick()
        compose.waitForIdle()
        assertEquals(listOf<Any?>(false), fake.calls("setDetailsOpen").last().args)
    }

    @Test
    fun aDoubleTapOnTheRightSeeksTenSecondsAndMoreTapsAddUp() {
        val fake = FakePlaybackController(playing())
        show(fake)
        val player = compose.onNodeWithTag(TAG_PLAYER)
        player.performTouchInput { click(androidx.compose.ui.geometry.Offset(width * 0.9f, height * 0.5f)) }
        compose.mainClock.advanceTimeBy(80)
        player.performTouchInput { click(androidx.compose.ui.geometry.Offset(width * 0.9f, height * 0.5f)) }
        compose.waitForIdle()
        assertEquals(listOf<Any?>(40_000L), fake.calls("seekTo").single().args)
        compose.onNodeWithText("+10 s").assertIsDisplayed()
        player.performTouchInput { click(androidx.compose.ui.geometry.Offset(width * 0.9f, height * 0.5f)) }
        compose.waitForIdle()
        compose.onNodeWithText("+20 s").assertIsDisplayed()
    }

    @Test
    fun errorsSayWhyAndOfferTryAgainAndBack() {
        val fake = FakePlaybackController(playing { it.copy(phase = PlayerPhase.Error("The server couldn't be reached.", canRetry = true)) })
        show(fake)
        compose.onNodeWithText("Can't play this item").assertIsDisplayed()
        compose.onNodeWithText("The server couldn't be reached.").assertIsDisplayed()
        compose.onNodeWithText("Try again").performClick()
        assertEquals(1, fake.calls("retry").size)
        compose.onNodeWithText("Back").performClick()
        assertEquals(1, host.backs)
    }

    @Test
    fun theSpinnerSaysReconnectingWhileTheServerIsOutOfReach() {
        val fake = FakePlaybackController(playing { it.copy(phase = PlayerPhase.Buffering, reconnecting = true) })
        show(fake)
        // The spinner shows after 400 ms of waiting.
        compose.mainClock.advanceTimeBy(500)
        compose.waitForIdle()
        compose.onNodeWithText("Reconnecting…").assertIsDisplayed()
        fake.update { it.copy(phase = PlayerPhase.Playing, reconnecting = false) }
        compose.waitForIdle()
        assertTrue(compose.onAllNodesWithTextCount("Reconnecting…") == 0)
    }

    @Test
    fun theBackArrowAndTheTitleLeave() {
        val fake = FakePlaybackController(playing())
        show(fake)
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Fixture Show").performClick()
        assertEquals(2, host.backs)
    }

    @Test
    fun nothingRecomposesWhilePlayingWithTheControlsHidden() {
        val composed = mutableListOf<String>()
        PlayerProbe.composed = { synchronized(composed) { composed += it } }
        val fake = FakePlaybackController(playing())
        compose.mainClock.autoAdvance = false
        show(fake)
        // The controls hide.
        compose.mainClock.advanceTimeBy(4_000)
        val before = synchronized(composed) { composed.toList() }
        assertTrue(before.contains("screen"))

        // Twenty ticks of the playhead and the buffer, as the session sends them.
        repeat(20) { i ->
            fake.update { it.copy(positionMs = 30_000L + i * 500, bufferedMs = 60_000L + i * 500) }
            compose.mainClock.advanceTimeBy(500)
        }
        val after = synchronized(composed) { composed.toList() }
        assertEquals("compositions while playing hidden: ${after.drop(before.size)}", before.size, after.size)
    }

    @Test
    fun draggingTheSeekBarMovesTheKnobWithoutRecomposingTheControls() {
        val composed = mutableListOf<String>()
        PlayerProbe.composed = { synchronized(composed) { composed += it } }
        // Skip shows: it steps aside for the drag, once.
        val fake = FakePlaybackController(playing { it.copy(skipLabel = "Skip Intro") })
        show(fake)
        val bar = compose.onNodeWithContentDescription("Position")
        bar.performTouchInput { down(androidx.compose.ui.geometry.Offset(width * 0.2f, height / 2f)) }
        compose.waitForIdle()
        val before = synchronized(composed) { composed.size }

        // Twenty moves of the finger along the bar: only the knob, the preview and the times follow it.
        repeat(20) {
            bar.performTouchInput { moveBy(androidx.compose.ui.geometry.Offset(width * 0.02f, 0f)) }
            compose.waitForIdle()
        }
        val during = synchronized(composed) { composed.drop(before) }
        bar.performTouchInput { up() }
        compose.waitForIdle()

        val controls = setOf("seekBar", "centre", "skip", "top", "bottom", "screen")
        assertEquals("compositions while dragging: $during", emptyList<String>(), during.filter { it in controls })
        assertEquals(1, fake.calls("seekTo").size)
    }

    @Test
    fun theSleepNoticeKeepsTheWidthOfItsThirtySecondsAsTheCountGoesDown() {
        val fake = FakePlaybackController(playing { it.copy(sleep = SleepState(SleepChoice.Minutes(30), secondsLeft = 30, minutesLeft = 1)) })
        show(fake)
        compose.onNodeWithText("Pausing in 30 s").assertExists()
        val wide = compose.onNodeWithTag(TAG_SLEEP_NOTICE).getBoundsInRoot()

        fake.update { it.copy(sleep = it.sleep.copy(secondsLeft = 9)) }
        compose.waitForIdle()
        compose.onNodeWithText("Pausing in 9 s").assertExists()
        assertEquals(wide, compose.onNodeWithTag(TAG_SLEEP_NOTICE).getBoundsInRoot())
    }

    @Test
    fun aVolumeDragMovesItsBarWithoutRecomposingIt() {
        val composed = mutableListOf<String>()
        PlayerProbe.composed = { synchronized(composed) { composed += it } }
        val fake = FakePlaybackController(playing())
        show(fake)
        val player = compose.onNodeWithTag(TAG_PLAYER)
        // Up on the right half, past the touch slop: the volume shows.
        player.performTouchInput {
            down(androidx.compose.ui.geometry.Offset(width * 0.85f, height * 0.6f))
            moveBy(androidx.compose.ui.geometry.Offset(0f, -40f))
        }
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Volume").assertIsDisplayed()
        val before = synchronized(composed) { composed.size }

        repeat(10) {
            player.performTouchInput { moveBy(androidx.compose.ui.geometry.Offset(0f, -8f)) }
            compose.waitForIdle()
        }
        val during = synchronized(composed) { composed.drop(before) }
        player.performTouchInput { up() }

        assertEquals("compositions while dragging: $during", emptyList<String>(), during.filter { it == "level" })
        assertTrue(host.level > 0.5f)
    }
}
