package app.pley.android.ui.tv.playlists

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.Tier
import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.screens.PleyKey
import app.pley.android.screens.playlists.PlaylistCard
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The Playlists page's cards, as screen readers hear them: each its title and what it holds. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvPlaylistsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private fun card(id: Long, title: String, meta: String, smart: Boolean = false) =
        PlaylistCard(id = id, title = title, metaTexts = listOf(UiText.raw(meta)), video = true, smart = smart, artwork = emptyList(), origin = "http://pley")

    @Test
    fun aCardSaysItsTitleAndWhatItHolds() {
        val navigator = TvNavigator(mutableListOf<NavKey>(PleyKey.Playlists), RailState()) {}
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) {
                    TvPlaylistsContent(Loadable.Ready(listOf(card(1, "Road trip", "12 videos"), card(2, "Comedies", "30 videos", smart = true)), fresh = true), onRetry = {})
                }
            }
        }
        compose.waitForIdle()
        compose.focused().assert(hasContentDescription("Road trip, 12 videos"))
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasContentDescription("Comedies, 30 videos · ", substring = true))
    }
}
