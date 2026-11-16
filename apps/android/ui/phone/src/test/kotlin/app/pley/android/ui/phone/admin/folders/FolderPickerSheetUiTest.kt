package app.pley.android.ui.phone.admin.folders

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import app.pley.android.screens.admin.folders.FolderBrowser
import app.pley.android.testing.data.FakeServerAdmin
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The folder picker keeps Select this folder on screen, whatever the room
 * it has: the list of folders takes what is left above it.
 */
@RunWith(RobolectricTestRunner::class)
class FolderPickerSheetUiTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    @Config(qualifiers = "w640dp-h320dp-land-xhdpi", fontScale = 1.5f)
    fun selectStaysOnScreenOnASmallScreenInLandscapeWithLargeText() {
        val server = FakeServerAdmin().apply {
            val folders = (1..40).joinToString(",") { """{"name":"Folder $it","path":"/srv/folder$it"}""" }
            answer("POST /api/fs", """{"path":"/srv","parent":"/","entries":[$folders]}""")
        }
        val browser = FolderBrowser(CoroutineScope(Dispatchers.Unconfined), server::api)
        val chosen = mutableListOf<String>()
        browser.open("/srv")
        compose.setContent { PhoneTheme { FolderPickerSheet(browser, onSelect = { chosen += it }) } }
        compose.waitForIdle()
        compose.onNodeWithTag(FOLDER_SELECT).assertIsDisplayed().assertIsEnabled().performClick()
        assertEquals(listOf("/srv"), chosen)
    }
}
