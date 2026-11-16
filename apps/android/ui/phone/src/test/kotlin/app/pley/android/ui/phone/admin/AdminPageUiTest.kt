package app.pley.android.ui.phone.admin

import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * A page of managing the server, once the phone can no longer manage it:
 * it says so, in place of what it read, with a way back to Me.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class AdminPageUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val opened = mutableListOf<PleyKey>()

    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit

        override fun restart(key: PleyKey) = Unit
    }

    private fun page(lost: Boolean) {
        compose.setContent {
            PhoneTheme {
                CompositionLocalProvider(LocalPhoneNavigator provides navigator) {
                    AdminPage(title = "Users", load = AdminLoad.Ready(listOf("Ann")), onRetry = {}, tag = "page", lost = lost) { names ->
                        item { Text(names.joinToString()) }
                    }
                }
            }
        }
    }

    @Test
    fun aPageShowsWhatItRead() {
        page(lost = false)
        compose.onNodeWithText("Ann").assertExists()
        compose.onNodeWithTag(ADMIN_LOST).assertDoesNotExist()
    }

    @Test
    fun oncePhoneCanNoLongerManageTheServerThePageSaysSoAndGoesBackToMe() {
        page(lost = true)
        compose.onNodeWithText("Ann").assertDoesNotExist()
        compose.onNodeWithText("You can no longer manage this server").assertExists()
        compose.onNodeWithText("Back to Me").performClick()
        assertEquals(listOf<PleyKey>(PleyKey.Me), opened)
    }
}
