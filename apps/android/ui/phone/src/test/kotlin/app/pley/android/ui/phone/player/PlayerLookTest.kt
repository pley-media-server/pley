package app.pley.android.ui.phone.player

import app.pley.android.data.Settings
import app.pley.android.data.SubtitleBackground
import app.pley.android.designsystem.PleyColors
import app.pley.android.model.PlaybackMethod
import app.pley.android.player.ChapterMark
import app.pley.android.player.subtitles.CaptionLook
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The player's colours, sizes, times and picture arithmetic. */
class PlayerLookTest {
    @Test
    fun badgesAreGreenForDirectPlayBlueForDirectStreamsAndAccentForTranscodes() {
        // The session's words for them: PlayerWordsTest.
        assertEquals(PleyColors.Success, PlayerLook.badgeColors(PlaybackMethod.Direct).text)
        assertEquals(PleyColors.Info, PlayerLook.badgeColors(PlaybackMethod.Remux).text)
        assertEquals(PleyColors.Accent, PlayerLook.badgeColors(PlaybackMethod.Transcode).text)
        // The fill is the text's colour at 20 %.
        val badge = PlayerLook.badgeColors(PlaybackMethod.Direct)
        assertEquals(0.2f, badge.fill.alpha, 0.01f)
        assertEquals(badge.text.copy(alpha = 0.2f), badge.fill)
    }

    @Test
    fun subtitlesAreSizedAsAShareOfTheHeightAndFollowTheSystemUntilChosen() {
        val defaults = PlayerLook.captionLook(Settings())
        assertEquals(CaptionLook.DEFAULT_FRACTION, defaults.textFraction, 0.0001f)
        assertFalse(defaults.box)
        assertFalse("the system's captions win until Pley's own style is chosen", defaults.custom)

        val huge = PlayerLook.captionLook(Settings(subtitleSize = 0.076f))
        assertEquals(0.076f, huge.textFraction, 0.0001f)
        assertTrue(huge.custom)

        val boxed = PlayerLook.captionLook(Settings(subtitleBackground = SubtitleBackground.DarkBox))
        assertTrue(boxed.box)
        assertTrue(boxed.custom)

        // Sizes snap to the four offered.
        assertEquals(0, PlayerLook.sizeIndex(0.03f))
        assertEquals(1, PlayerLook.sizeIndex(0.049f))
        assertEquals(2, PlayerLook.sizeIndex(0.061f))
        assertEquals(3, PlayerLook.sizeIndex(0.2f))
        assertEquals(listOf(0.038f, 0.048f, 0.06f, 0.076f), PlayerLook.subtitleSizes)
    }

    @Test
    fun timesReadAsClocksAndTheTimeLeftHasATrueMinus() {
        assertEquals("0:00", PlayerLook.clock(0))
        assertEquals("4:05", PlayerLook.clock(245_900))
        assertEquals("1:02:03", PlayerLook.clock(3_723_000))
        assertEquals("−25:20", PlayerLook.remaining(32 * 60_000L + 40_000, 58 * 60_000L))
        assertEquals("−0:00", PlayerLook.remaining(10_000, 5_000))
    }

    @Test
    fun theChapterPlayingIsTheLastToHaveStarted() {
        val chapters = listOf(ChapterMark("Intro", 0, 4_000, null), ChapterMark(null, 4_000, 16_000, null), ChapterMark("Credits", 16_000, 20_000, null))
        assertEquals(0, PlayerLook.chapterAt(chapters, 0))
        assertEquals(1, PlayerLook.chapterAt(chapters, 4_000))
        assertEquals(2, PlayerLook.chapterAt(chapters, 19_000))
        assertEquals(-1, PlayerLook.chapterAt(emptyList(), 1_000))
        assertEquals("Chapter 2", PlayerLook.chapterTitle(chapters[1], 1) { "Chapter $it" })
        assertEquals("Intro", PlayerLook.chapterTitle(chapters[0], 0) { "Chapter $it" })
        // Named only for where it starts, as mkvmerge names chapters: numbered too.
        assertEquals("Chapter 3", PlayerLook.chapterTitle(ChapterMark("00:16:00.000", 16_000, 20_000, null), 2) { "Chapter $it" })
    }

    @Test
    fun fillCoversTheScreenAndZoomHidesBarsInsideASixteenByNinePicture() {
        val phone = 20f / 9f
        val hd = 16f / 9f
        assertEquals(1.25f, PictureGeometry.coverScale(hd, phone), 0.001f)
        // A 2.39:1 film stored in 16:9 needs 2.39 / 2.22 more to lose its bars.
        assertEquals(2.39f / phone, PictureGeometry.barsScale(hd, phone), 0.001f)
        // Pictures already wider than 2:1 have no bars to hide.
        assertEquals(1f, PictureGeometry.barsScale(2.39f, phone), 0.001f)
        assertEquals(1f, PictureGeometry.coverScale(0f, phone), 0.001f)
    }

    @Test
    fun aZoomedPictureIsCentredButKeepsItsBottomWhenSubtitlesAreBurnedIn() {
        // 2000 by 900 screen, 16:9 picture filled: 2000 by 1125.
        val centred = PictureGeometry.place(2000, 900, 16f / 9f, PictureGeometry.coverScale(16f / 9f, 2000f / 900f), keepBottom = false)
        assertEquals(2000, centred.width)
        assertEquals(1125, centred.height)
        assertEquals((900 - 1125) / 2, centred.top)
        val burned = PictureGeometry.place(2000, 900, 16f / 9f, PictureGeometry.coverScale(16f / 9f, 2000f / 900f), keepBottom = true)
        assertEquals(900, burned.bottom)
        assertEquals(900 - 1125, burned.top)
        // Fitted: 1600 by 900 in the middle.
        val fit = PictureGeometry.place(2000, 900, 16f / 9f, 1f, keepBottom = true)
        assertEquals(200, fit.left)
        assertEquals(1600, fit.width)
        assertEquals(0, fit.top)
        // What shows is the screen's part of it.
        val visible = PictureGeometry.visible(2000, 900, 16f / 9f, 1.5f, keepBottom = false)
        assertEquals(0, visible.top)
        assertEquals(900, visible.bottom)
    }

    @Test
    fun theButtonStepsFitFillZoomAndBackAndPinchesSnap() {
        val screen = 20f / 9f
        val chosen = mutableListOf<FitMode>()
        val picture = PictureState().apply {
            videoAspect = 16f / 9f
            onChosen = { chosen += it }
        }
        assertEquals(FitMode.Fill, picture.step(screen))
        assertEquals(FitMode.Zoom, picture.step(screen))
        assertEquals(FitMode.Fit, picture.step(screen))
        assertEquals(listOf(FitMode.Fill, FitMode.Zoom, FitMode.Fit), chosen)

        // A scope picture has no bars to zoom past: Fill goes back to Fit.
        picture.videoAspect = 2.39f
        assertEquals(FitMode.Fill, picture.step(screen))
        assertEquals(FitMode.Fit, picture.step(screen))

        // Pinching close to covering the screen counts as Fill.
        picture.videoAspect = 16f / 9f
        picture.pinchStart(screen)
        picture.pinchBy(1.24f)
        assertEquals(FitMode.Fill, picture.pinchEnd(screen))
        picture.pinchStart(screen)
        picture.pinchBy(1.5f)
        assertEquals(FitMode.Free, picture.pinchEnd(screen))
        assertEquals(PictureGeometry.MAX_SCALE, run { picture.pinchStart(screen); picture.pinchBy(10f); picture.scale(screen) }, 0.001f)
        picture.pinchBy(0.01f)
        assertEquals(FitMode.Fit, picture.pinchEnd(screen))

        // Picture in picture shows it all.
        picture.choose(FitMode.Zoom)
        picture.inPip = true
        assertEquals(1f, picture.scale(screen), 0.001f)
    }
}
