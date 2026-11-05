package app.pley.android.ui.tv.kit

import android.view.inputmethod.EditorInfo
import android.view.inputmethod.ExtractedText
import android.view.inputmethod.ExtractedTextRequest
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputConnectionWrapper
import android.view.inputmethod.InputMethodManager
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.platform.InterceptPlatformTextInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.PlatformTextInputMethodRequest
import androidx.compose.ui.text.TextRange

/**
 * What a TV keyboard that shows its own copy of the text, as Amazon's
 * does on Fire TV, needs from [content]'s field on [state]:
 *
 * - Its box follows the field. Such a keyboard reads the text once, asking
 *   to be told of changes, and then shows only what it is told. A field on
 *   a [TextFieldState] answers the read but never tells, so the box kept
 *   the text it opened with while every key reached the field. This tells
 *   it after each change, as Android's EditText does; a keyboard that
 *   doesn't ask (Gboard) hears nothing.
 * - Its Previous, which the remote's Back presses, calls [onPrevious]
 *   (close the keyboard, keeping the text) instead of reaching the field,
 *   which would take it for the field's own action (Done, Search) and act.
 */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun KeyboardBridge(state: TextFieldState, onPrevious: () -> Unit, content: @Composable () -> Unit) {
    val view = LocalView.current
    val previous by rememberUpdatedState(onPrevious)
    val bridge = remember { Bridge { previous() } }
    InterceptPlatformTextInput(
        interceptor = { request, next ->
            try {
                next.startInputMethod(
                    object : PlatformTextInputMethodRequest {
                        override fun createInputConnection(outAttributes: EditorInfo): InputConnection =
                            bridge.connect(request.createInputConnection(outAttributes))
                    },
                )
            } finally {
                bridge.token = null
            }
        },
        content = content,
    )
    LaunchedEffect(state, view) {
        val keyboards = view.context.getSystemService(InputMethodManager::class.java)
        snapshotFlow { state.text.toString() to state.selection }.collect { (text, selection) ->
            val token = bridge.token ?: return@collect
            keyboards?.updateExtractedText(view, token, extracted(text, selection))
        }
    }
}

private class Bridge(private val onPrevious: () -> Unit) {
    /** The token of the keyboard's request to be told of changes, once it has asked. */
    var token: Int? = null

    fun connect(connection: InputConnection): InputConnection {
        // A new connection is a new keyboard session, which asks again.
        token = null
        return object : InputConnectionWrapper(connection, false) {
            override fun getExtractedText(request: ExtractedTextRequest?, flags: Int): ExtractedText? {
                if (flags and InputConnection.GET_EXTRACTED_TEXT_MONITOR != 0) token = request?.token ?: 0
                return super.getExtractedText(request, flags)
            }

            override fun performEditorAction(editorAction: Int): Boolean {
                if (editorAction != EditorInfo.IME_ACTION_PREVIOUS) return super.performEditorAction(editorAction)
                onPrevious()
                return true
            }
        }
    }
}

private fun extracted(text: String, selection: TextRange) = ExtractedText().apply {
    this.text = text
    startOffset = 0
    partialStartOffset = -1
    partialEndOffset = -1
    selectionStart = selection.start
    selectionEnd = selection.end
    flags = if ('\n' in text) 0 else ExtractedText.FLAG_SINGLE_LINE
}
