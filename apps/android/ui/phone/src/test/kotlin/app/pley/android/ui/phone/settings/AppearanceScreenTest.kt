package app.pley.android.ui.phone.settings

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasTestTag
import androidx.test.core.app.ApplicationProvider
import app.pley.android.designsystem.theme.Appearance
import app.pley.android.designsystem.theme.PleyAccent
import app.pley.android.designsystem.theme.PleyPalette
import app.pley.android.designsystem.theme.PleyTheme
import app.pley.android.designsystem.theme.LocalPleyPalette
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * Settings → Appearance on a phone: the theme shown is the one picked, a
 * tap on another shows it at once, everywhere below the theme, and an
 * accent keeps the theme.
 */
@RunWith(RobolectricTestRunner::class)
class AppearanceScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context get() = ApplicationProvider.getApplicationContext()

    private val navigator = object : Navigator {
        override fun open(key: PleyKey) = Unit
        override fun back() = Unit
        override fun restart(key: PleyKey) = Unit
    }

    /** What the page's own colors are, read below the theme. */
    private var drawn: PleyPalette? = null

    @Before
    fun fresh() {
        context.getSharedPreferences("appearance", Context.MODE_PRIVATE).edit().clear().commit()
        Appearance.attach(context)
    }

    @After
    fun midnight() = fresh()

    private fun show() {
        compose.setContent {
            PhoneTheme {
                drawn = LocalPleyPalette.current
                CompositionLocalProvider(LocalPhoneNavigator provides navigator) { AppearanceScreen() }
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun aTapShowsATheme() {
        show()
        compose.onNodeWithText("How Pley looks for you on this device. Each profile keeps its own.").assertExists()
        compose.onNodeWithTag("$APPEARANCE_THEME.midnight").assertIsSelected()
        compose.onNodeWithTag(APPEARANCE_PAGE).performScrollToNode(hasTestTag("$APPEARANCE_THEME.sunshine"))
        compose.onNodeWithTag("$APPEARANCE_THEME.sunshine").performClick()
        compose.waitForIdle()
        assertEquals(PleyTheme.Sunshine, Appearance.theme)
        assertSame(PleyPalette.of(PleyTheme.Sunshine, null), drawn)
        compose.onNodeWithTag("$APPEARANCE_THEME.sunshine").assertIsSelected()
        compose.onNodeWithTag(APPEARANCE_PAGE).performScrollToNode(hasTestTag("$APPEARANCE_THEME.midnight"))
        compose.onNodeWithTag("$APPEARANCE_THEME.midnight").assertIsNotSelected()
    }

    @Test
    fun anAccentKeepsTheTheme() {
        show()
        compose.onNodeWithTag(APPEARANCE_PAGE).performScrollToNode(hasTestTag("$APPEARANCE_ACCENT.gold"))
        compose.onNodeWithTag("$APPEARANCE_ACCENT.gold").performClick()
        compose.waitForIdle()
        assertSame(PleyPalette.of(PleyTheme.Midnight, PleyAccent.Gold), drawn)
        compose.onNodeWithTag("$APPEARANCE_ACCENT.gold").assertIsSelected()
        compose.onNodeWithTag("$APPEARANCE_ACCENT.own").assertIsNotSelected()
    }
}
