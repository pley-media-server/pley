package app.pley.android.ui.tv.settings

import android.content.Context
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.Tier
import app.pley.android.designsystem.theme.Appearance
import app.pley.android.designsystem.theme.PleyAccent
import app.pley.android.designsystem.theme.PleyTheme
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsSelected
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Settings → Appearance with the D-pad: focus starts on the theme shown
 * (on its accent from the Accent row), Select on another shows it at once,
 * the themes are five to a row, and Up and Down between the themes and the
 * accents land on what is in use.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvAppearanceTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context get() = ApplicationProvider.getApplicationContext()

    @Before
    fun fresh() {
        context.getSharedPreferences("appearance", Context.MODE_PRIVATE).edit().clear().commit()
        Appearance.attach(context)
    }

    @After
    fun midnight() {
        context.getSharedPreferences("appearance", Context.MODE_PRIVATE).edit().clear().commit()
        Appearance.attach(context)
    }

    private fun show(accent: Boolean = false) {
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) { TvAppearanceScreen(accent) }
        }
        compose.waitForIdle()
    }

    private fun theme(theme: PleyTheme) = compose.onNodeWithTag("$TV_APPEARANCE_THEME.${theme.id}")

    private fun accent(accent: PleyAccent?) = compose.onNodeWithTag("$TV_APPEARANCE_ACCENT.${accent?.id ?: "own"}")

    @Test
    fun focusStartsOnTheThemeShownAndSelectShowsAnother() {
        show()
        theme(PleyTheme.Midnight).assertIsFocused().assertIsSelected()
        compose.onNodeWithText("How Pley looks for you on this TV. Each profile keeps its own.").assertExists()
        compose.press(Key.DirectionRight)
        theme(PleyTheme.Midday).assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(PleyTheme.Midday, Appearance.theme)
        theme(PleyTheme.Midday).assertIsSelected()
    }

    @Test
    fun fiveToARowThenTheAccentInUse() {
        show()
        compose.press(Key.DirectionDown)
        theme(PleyTheme.Nightshade).assertIsFocused()
        compose.press(Key.DirectionDown)
        accent(null).assertIsFocused().assertIsSelected()
        compose.press(Key.DirectionRight)
        accent(PleyAccent.Sky).assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(PleyAccent.Sky, Appearance.accent)
        // The theme stays as it was, and Up goes back to it.
        assertEquals(PleyTheme.Midnight, Appearance.theme)
        compose.press(Key.DirectionUp)
        theme(PleyTheme.Midnight).assertIsFocused()
    }

    @Test
    fun fromTheAccentRowFocusStartsOnTheAccentInUse() {
        Appearance.pickAccent(context, PleyAccent.Gold)
        show(accent = true)
        accent(PleyAccent.Gold).assertIsFocused().assertIsSelected()
        compose.press(Key.DirectionUp)
        theme(PleyTheme.Midnight).assertIsFocused()
        compose.press(Key.DirectionRight, times = 4)
        theme(PleyTheme.Moonlight).assertIsFocused()
        // Down from Moonlight, with no theme under it, goes to the accent in use too.
        compose.press(Key.DirectionDown)
        accent(PleyAccent.Gold).assertIsFocused()
    }
}
