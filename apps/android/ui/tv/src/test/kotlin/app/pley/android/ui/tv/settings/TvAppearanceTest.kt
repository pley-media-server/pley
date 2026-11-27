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
 * Settings → Appearance with the D-pad: focus starts on the theme shown,
 * Select on another shows it at once, and the accents follow the themes,
 * Default first.
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

    private fun show() {
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) { TvAppearanceScreen() }
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
    fun theAccentsComeAfterTheThemesDefaultFirst() {
        show()
        compose.press(Key.DirectionDown, times = 3)
        compose.press(Key.DirectionLeft, times = 6)
        accent(null).assertIsFocused().assertIsSelected()
        compose.press(Key.DirectionRight)
        accent(PleyAccent.Sky).assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(PleyAccent.Sky, Appearance.accent)
        // The theme stays as it was.
        assertEquals(PleyTheme.Midnight, Appearance.theme)
    }
}
