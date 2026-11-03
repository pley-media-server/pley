package app.pley.android.ui.tv.library

import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * A library's side panels (design spec §2.7): the choice in force has
 * focus when the panel opens, however far down its list it is.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvLibraryPanelTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aChoiceFarDownTheListTakesFocus() {
        val genres = List(40) { "Genre ${it + 1}" }
        val chosen = "Genre 36"
        compose.setContent {
            TvTheme(reducedMotion = true) {
                val requester = remember { FocusRequester() }
                TvSidePanel(visible = true, onDismiss = {}) {
                    PanelList("Genre", requester) {
                        items(genres) { genre ->
                            PanelRow(
                                genre,
                                onClick = {},
                                checked = genre == chosen,
                                modifier = if (genre == chosen) Modifier.focusRequester(requester) else Modifier,
                            )
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
        // Far past the first screen of rows, which is all a lazy list composes at first.
        compose.onNodeWithText(chosen).assertIsFocused()
    }
}
