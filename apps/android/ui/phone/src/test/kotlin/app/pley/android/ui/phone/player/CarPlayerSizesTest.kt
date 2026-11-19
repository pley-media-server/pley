package app.pley.android.ui.phone.player

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.QualityChoice
import app.pley.android.player.TrackChoice
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.testing.player.session.FakePlaybackController
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The video player on each of the screens Android Auto runs on ([CarScreenChecks]). */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class CarPlayerSizesTest {
    @get:Rule
    val compose = createComposeRule()

    private val host = object : PlayerHost {
        override fun back() = Unit

        override val pipSupported = false

        override fun enterPip() = Unit

        override fun volume() = 0.5f

        override fun setVolume(level: Float) = Unit

        override fun brightness() = 0.5f

        override fun setBrightness(level: Float) = Unit
    }

    private fun check(screen: String) {
        val fake = FakePlaybackController(
            PlayerUiState(
                phase = PlayerPhase.Paused,
                itemId = 20_001,
                title = "Fixture Show",
                subtitle = "S1 E1 — Pilot",
                positionMs = 30_000,
                durationMs = 600_000,
                method = "Direct play",
                audio = listOf(TrackChoice(1, "English (AAC Stereo)", true), TrackChoice(2, "French (EAC3 5.1)", false)),
                subtitles = listOf(TrackChoice(null, "Off", false), TrackChoice(3, "English (SRT)", true)),
                quality = "Original",
                qualities = listOf(QualityChoice("original", "Original", null, true)),
            ),
        )
        compose.setContent {
            PhoneTheme {
                CarSized(car = true) {
                    PhonePlayerScreen(fake, host, PictureState(), CaptionLook(), SubtitleChoice(), {}, inPip = false)
                }
            }
        }
        CarScreenChecks.save(compose, "player-$screen")
        val controls = listOf("Back", "Playback settings", "Back 10 seconds", "Play", "Forward 10 seconds", "Position", "Subtitles", "Audio", fit(), "Next episode")
        CarScreenChecks.controls(compose, screen, controls)
        CarScreenChecks.apart(compose, screen, controls)
    }

    /** Fit's button, said by what it does next (fill, zoom or fit), which the screen's shape decides. */
    private fun fit(): String {
        compose.waitForIdle()
        return FIT.firstOrNull { compose.onAllNodes(hasContentDescription(it)).fetchSemanticsNodes().isNotEmpty() } ?: FIT.first()
    }

    private companion object {
        val FIT = listOf("Fill the screen", "Zoom past the black bars", "Show the whole picture")
    }

    @Test
    @Config(qualifiers = CarScreenChecks.SMALL)
    fun smallLandscape() = check("small")

    @Test
    @Config(qualifiers = CarScreenChecks.WIDE)
    fun wideLandscape() = check("wide")

    @Test
    @Config(qualifiers = CarScreenChecks.PORTRAIT)
    fun portrait() = check("portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.DENSE)
    fun dense() = check("dense")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL)
    fun tall() = check("tall")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_SPLIT)
    fun tallSplit() = check("tall-split")

    @Test
    @Config(qualifiers = CarScreenChecks.ULTRAWIDE)
    fun ultrawide() = check("ultrawide")

    @Test
    @Config(qualifiers = CarScreenChecks.FULL_HD)
    fun fullHd() = check("full-hd")

    @Test
    @Config(qualifiers = CarScreenChecks.TALL_PORTRAIT)
    fun tallPortrait() = check("tall-portrait")

    @Test
    @Config(qualifiers = CarScreenChecks.NARROW)
    fun narrow() = check("narrow")
}
