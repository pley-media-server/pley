package app.pley.android.ui.phone.admin

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTextInput
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * What is typed into a dialog of managing the server: a name survives the
 * activity being recreated, a password or a PIN never goes into its saved
 * state; and Save waits while what was saved is on its way.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class TextEntryDialogUiTest {
    @get:Rule
    val compose = createComposeRule()

    private fun typedThenRecreated(entry: Entry) {
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            PhoneTheme { TextEntryDialog(title = "Value", initial = "", onSave = {}, onDismiss = {}, entry = entry) }
        }
        compose.onNodeWithTag(TEXT_ENTRY_FIELD).performTextInput("hunter2a")
        restoration.emulateSavedInstanceStateRestore()
    }

    @Test
    fun aNameTypedSurvivesTheAppBeingRecreated() {
        typedThenRecreated(Entry.Text)
        compose.onNodeWithTag(TEXT_ENTRY_FIELD).assertTextContains("hunter2a")
    }

    @Test
    fun aPasswordTypedIsntKeptInSavedState() {
        typedThenRecreated(Entry.Secret)
        compose.onNodeWithTag(TEXT_ENTRY_FIELD).assert(emptyText)
    }

    @Test
    fun aPinTypedIsntKeptInSavedState() {
        typedThenRecreated(Entry.Pin)
        compose.onNodeWithTag(TEXT_ENTRY_FIELD).assert(emptyText)
    }

    @Test
    fun saveWaitsWhileWhatWasSavedIsOnItsWay() {
        compose.setContent {
            PhoneTheme { TextEntryDialog(title = "Password", initial = "", onSave = {}, onDismiss = {}, entry = Entry.Secret, busy = true) }
        }
        compose.onNodeWithTag(TEXT_ENTRY_SAVE).assertIsNotEnabled()
    }

    /** A field with nothing typed: its editable text is empty, hidden or not. */
    private val emptyText = SemanticsMatcher("has no text typed") { node ->
        node.config.getOrElseNullable(SemanticsProperties.EditableText) { null }?.text.isNullOrEmpty()
    }
}
