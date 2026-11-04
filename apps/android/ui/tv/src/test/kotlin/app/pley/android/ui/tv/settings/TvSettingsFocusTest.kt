package app.pley.android.ui.tv.settings

import androidx.activity.ComponentActivity
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.Tier
import app.pley.android.common.perf.FrameRate
import app.pley.android.data.FrameRateMatching
import app.pley.android.data.Settings
import app.pley.android.data.TrackDefaultsUnsupported
import app.pley.android.network.ApiException
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.hold
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.After
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The TV's settings with the D-pad (design spec §4.8): focus starts on the
 * first category and the rows follow it; a held Down stops on the last
 * category; Right goes into the rows; a choice opens a side panel on the
 * current value and gives focus back to its row; switches toggle on
 * Select; Sign out asks first, with focus on Cancel.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi", application = TvAccountTestApplication::class)
class TvSettingsFocusTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val graph = TvAccountTestGraph()
    private val stack = mutableListOf<NavKey>(PleyKey.Settings)

    @Before
    fun graph() {
        TvAccountTestApplication.graph = graph
    }

    @After
    fun frameRateOff() {
        FrameRate.show(compose.activity, false)
    }

    /** On the test's clock. The test sends no repeats, as the system would: none is waited for. */
    private val gate = KeyRepeatGate(firstRepeatMs = 60_000) { compose.mainClock.currentTime }

    /** The page, with something focusable under it when [below], as a page's next list is. */
    private fun show(below: Boolean = false) {
        val navigator = TvNavigator(stack, RailState()) {}
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator, LocalKeyRepeatGate provides gate) {
                    Column(Modifier.fillMaxSize().keyRepeatGate(gate)) {
                        Box(Modifier.weight(1f)) { TvSettingsScreen() }
                        if (below) Box(Modifier.size(40.dp).testTag(BELOW).focusable())
                    }
                }
            }
        }
        compose.waitUntil(5_000) { compose.onAllNodesWithText("Anna").fetchSemanticsNodes().isNotEmpty() }
    }

    private fun category(c: Category) = compose.onNodeWithTag("$TV_SETTINGS_CATEGORY.${c.name}")

    @Test
    fun focusStartsOnProfileAndTheRowsFollowTheFocusedCategory() {
        show()
        category(Category.Profile).assertIsFocused()
        compose.onNodeWithText("Audio language").assertExists()
        compose.press(Key.DirectionDown)
        category(Category.Playback).assertIsFocused()
        compose.onNodeWithText("Match the refresh rate").assertExists()
        compose.onNodeWithText("Audio language").assertDoesNotExist()
    }

    @Test
    fun aHeldDownStopsOnTheLastCategoryAndOnlyAPressGoesOnBelow() {
        show(below = true)
        compose.press(Key.DirectionDown, times = 6)
        category(Category.About).assertIsFocused()
        // The press moves down to Sign out; held from 400 ms, Down goes no further.
        compose.hold(Key.DirectionDown)
        category(Category.SignOut).assertIsFocused()
        compose.onNodeWithTag(BELOW).assertIsNotFocused()
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(BELOW).assertIsFocused()
    }

    @Test
    fun aChoiceOpensItsPanelOnTheCurrentValueAndFocusComesBack() {
        show()
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Quality"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("Match the refresh rate"))
        compose.press(Key.DirectionCenter)
        compose.onNodeWithTag(TV_CHOICE_PANEL).assertExists()
        // Auto is a TV's default, and is where focus starts.
        compose.focused().assert(hasText("Auto"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("Always"))
        compose.press(Key.DirectionCenter)
        compose.waitUntil(5_000) { graph.settingsFlow.value.frameRateMatching == FrameRateMatching.Always }
        compose.waitForIdle()
        compose.focused().assert(hasText("Match the refresh rate"))
    }

    @Test
    fun qualityStartsOnAutomaticUnderOriginalAndKeepsAnOriginalChosen() {
        show()
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Quality"))
        compose.focused().assert(hasText("Automatic"))
        compose.press(Key.DirectionCenter)
        compose.onNodeWithTag(TV_CHOICE_PANEL).assertExists()
        // Automatic is the default where nothing was chosen, under Original, then 4K first.
        compose.focused().assert(hasText("Automatic"))
        compose.focused().assert(hasText("Dynamically adjust to conditions."))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("30 Mbps 4K"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("15 Mbps 4K"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("20 Mbps 1080p"))
        compose.press(Key.DirectionUp, times = 4)
        compose.focused().assert(hasText("Original"))
        compose.focused().assert(hasText("Full quality. May buffer on a slow connection."))
        compose.press(Key.DirectionCenter)
        compose.waitUntil(5_000) { graph.settingsFlow.value.qualityOnHomeNetwork == Settings.ORIGINAL }
        compose.waitForIdle()
        compose.focused().assert(hasText("Quality"))
        compose.focused().assert(hasText("Original"))
    }

    @Test
    fun switchesToggleOnSelect() {
        show()
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionRight)
        compose.press(Key.DirectionDown, times = 2)
        compose.focused().assert(hasText("Play the next episode by itself"))
        compose.press(Key.DirectionCenter)
        compose.waitUntil(5_000) { !graph.settingsFlow.value.autoplayNext }
        assertFalse(graph.settingsFlow.value.autoplayNext)
    }

    @Test
    fun aRefusedTrackDefaultShowsItCanOnlyChangeOnTheWeb() {
        graph.refuse = TrackDefaultsUnsupported(ApiException(404, "not_found", "Not found"))
        show()
        compose.press(Key.DirectionRight)
        // Switch profile, then the audio language and the subtitles.
        compose.press(Key.DirectionDown, times = 2)
        compose.focused().assert(hasText("Subtitles"))
        compose.press(Key.DirectionCenter)
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionCenter)
        compose.waitUntil(5_000) { compose.onAllNodesWithText("change on the web", substring = true).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle()
        // Read-only now: Select shows the web's address instead.
        compose.focused().assert(hasText("Subtitles"))
        compose.press(Key.DirectionCenter)
        compose.onNodeWithTag(TV_WEB_DIALOG).assertExists()
        compose.onNodeWithText("192.168.1.20:7539/account").assertExists()
    }

    @Test
    fun diagnosticsShowsTheFrameRateOnSelect() {
        show()
        compose.press(Key.DirectionDown, times = 5)
        category(Category.Diagnostics).assertIsFocused()
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Open diagnostics"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("Show frame rate"))
        assertFalse(FrameRate.on.value)
        compose.press(Key.DirectionCenter)
        compose.waitUntil(5_000) { FrameRate.on.value }
        compose.press(Key.DirectionCenter)
        compose.waitUntil(5_000) { !FrameRate.on.value }
    }

    @Test
    fun aboutCreditsTmdbAfterTheLicences() {
        show()
        compose.press(Key.DirectionDown, times = 6)
        category(Category.About).assertIsFocused()
        compose.press(Key.DirectionRight)
        compose.press(Key.DirectionDown, times = licences().size + 1)
        compose.focused().assert(hasText("TMDB")).assert(hasText("This product uses the TMDB API but is not endorsed or certified by TMDB."))
        compose.onNodeWithText("Credits").assertExists()
    }

    @Test
    fun signOutAsksFirstWithFocusOnCancel() {
        show()
        compose.press(Key.DirectionDown, times = 7)
        category(Category.SignOut).assertIsFocused()
        compose.press(Key.DirectionCenter)
        // Short and plain: the question, one line, Cancel and Sign out.
        compose.onNodeWithText("Sign out?").assertExists()
        compose.onNodeWithText("You'll be signed out of 192.168.1.20:7539.").assertExists()
        compose.focused().assert(hasText("Cancel"))
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Sign out"))
        compose.press(Key.DirectionCenter)
        compose.waitUntil(5_000) { stack.firstOrNull() is PleyKey.Link }
        assertNull(graph.sessionFlow.value)
        assertEquals(listOf<NavKey>(PleyKey.Link(TvAccountTestGraph.SERVER)), stack.toList())
    }

    private companion object {
        const val BELOW = "below"
    }
}
