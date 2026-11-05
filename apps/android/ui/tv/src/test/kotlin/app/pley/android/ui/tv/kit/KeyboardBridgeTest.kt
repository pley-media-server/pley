package app.pley.android.ui.tv.kit

import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.ExtractedText
import android.view.inputmethod.ExtractedTextRequest
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import androidx.activity.ComponentActivity
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.input.ImeAction
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowInputMethodManager

/**
 * The kit's field and a keyboard that shows its own copy of the text, as
 * Amazon's does on Fire TV, through the connection the keyboard gets: it
 * is told of each change once it asks, and its Previous (the remote's
 * Back) closes it, keeping the text, without the field's Done.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi", shadows = [KeyboardBridgeTest.ToldKeyboard::class])
class KeyboardBridgeTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val state = TextFieldState("Road")
    private var done = 0
    private lateinit var view: View

    @Before
    fun forget() = told.clear()

    private fun keyboard(): InputConnection {
        compose.setContent {
            view = LocalView.current
            TvTheme {
                TvTextField(state, "A name", Modifier.testTag(FIELD), imeAction = ImeAction.Done, onAction = { done++ }, typeNow = true)
            }
        }
        return compose.runOnIdle { checkNotNull(view.onCreateInputConnection(EditorInfo())) }
    }

    private fun InputConnection.watch(token: Int) =
        getExtractedText(ExtractedTextRequest().also { it.token = token }, InputConnection.GET_EXTRACTED_TEXT_MONITOR)

    @Test
    fun aKeyboardThatAsksIsToldOfEachChange() {
        val keyboard = keyboard()
        assertEquals("Road", keyboard.watch(7).text.toString())
        compose.runOnIdle { keyboard.commitText(" trip", 1) }
        compose.runOnIdle { keyboard.deleteSurroundingText(4, 0) }
        compose.runOnIdle {
            assertEquals(listOf(Told(7, "Road trip", 9), Told(7, "Road ", 5)), told)
        }
    }

    @Test
    fun aKeyboardThatDoesntAskIsToldNothing() {
        val keyboard = keyboard()
        keyboard.getExtractedText(ExtractedTextRequest(), 0)
        compose.runOnIdle { keyboard.commitText(" trip", 1) }
        compose.runOnIdle {
            assertEquals("Road trip", state.text.toString())
            assertEquals(emptyList<Told>(), told)
        }
    }

    @Test
    fun previousClosesTheKeyboardKeepingTheText() {
        val keyboard = keyboard()
        keyboard.watch(7)
        compose.runOnIdle { keyboard.commitText(" trip", 1) }
        compose.runOnIdle { keyboard.performEditorAction(EditorInfo.IME_ACTION_PREVIOUS) }
        assertEquals(0, done)
        assertEquals("Road trip", state.text.toString())
        compose.onNodeWithTag(FIELD).assertIsFocused()
    }

    @Test
    fun doneIsTheFieldsAction() {
        val keyboard = keyboard()
        compose.runOnIdle { keyboard.performEditorAction(EditorInfo.IME_ACTION_DONE) }
        assertEquals(1, done)
        compose.onNodeWithTag(FIELD).assertIsFocused()
    }

    /** What the keyboard was told: the token it asked with, the text and where the cursor is. */
    data class Told(val token: Int, val text: String, val cursor: Int)

    /** Robolectric's InputMethodManager, noting what keyboards are told. */
    @Implements(InputMethodManager::class)
    class ToldKeyboard : ShadowInputMethodManager() {
        @Implementation
        fun updateExtractedText(@Suppress("UNUSED_PARAMETER") view: View?, token: Int, text: ExtractedText) {
            told += Told(token, text.text.toString(), text.selectionEnd)
        }
    }

    private companion object {
        const val FIELD = "field"
        val told = mutableListOf<Told>()
    }
}
