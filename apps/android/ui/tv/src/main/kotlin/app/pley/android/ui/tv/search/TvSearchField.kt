package app.pley.android.ui.tv.search

import android.view.KeyEvent as AndroidKeyEvent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.ui.tv.kit.FieldHeight
import app.pley.android.ui.tv.kit.KeyboardBridge
import app.pley.android.ui.tv.kit.OverlayShowing

/**
 * The search field on TV (design spec §2.11, §4.5; D-pad rule 7): the
 * kit's text field with the search glyph. Focus lands on it without
 * opening the keyboard, so D-pad users pass through it; Select opens the
 * system keyboard (Gboard, or Amazon's, both with voice input), and Back
 * or the keyboard's Search closes it again, leaving focus on the field.
 *
 * 44 dp, [surface2][PleyColors.Surface2], radius 8, tvBody. At rest a 1 dp
 * [control][PleyColors.Control] edge; focused a 2 dp [accentBright][PleyColors.AccentBright]
 * edge, without scale.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun TvSearchField(
    state: TextFieldState,
    placeholder: String,
    onSearch: () -> Unit,
    modifier: Modifier = Modifier,
    focusModifier: Modifier = Modifier,
) {
    var editing by remember { mutableStateOf(false) }
    val outer = remember { MutableInteractionSource() }
    val outerFocused by outer.collectIsFocusedAsState()
    var innerFocused by remember { mutableStateOf(false) }
    val field = remember { FocusRequester() }
    val shell = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current

    fun stop() {
        editing = false
        keyboard?.hide()
        runCatching { shell.requestFocus() }
    }

    BackHandler(enabled = editing) { stop() }
    OverlayShowing(editing)
    LaunchedEffect(editing) {
        if (editing) {
            runCatching { field.requestFocus() }
            keyboard?.show()
        }
    }
    // The keyboard's own Back closes it without reaching the app: once it has gone, editing ends too.
    val imeShown = WindowInsets.isImeVisible
    var imeSeen by remember { mutableStateOf(false) }
    LaunchedEffect(imeShown, editing) {
        if (!editing) {
            imeSeen = false
        } else if (imeShown) {
            imeSeen = true
        } else if (imeSeen) {
            stop()
        }
    }
    val focusManager = LocalFocusManager.current
    val ring = outerFocused || innerFocused
    Row(
        modifier
            .height(FieldHeight)
            .background(PleyColors.Surface2, PleyShape.Field)
            .border(if (ring) 2.dp else 1.dp, if (ring) PleyColors.AccentBright else PleyColors.Control, PleyShape.Field)
            .then(focusModifier)
            .focusRequester(shell)
            .focusProperties { canFocus = !editing }
            .onKeyEvent { event ->
                val code = event.nativeKeyEvent.keyCode
                val select = code == AndroidKeyEvent.KEYCODE_DPAD_CENTER || code == AndroidKeyEvent.KEYCODE_ENTER ||
                    code == AndroidKeyEvent.KEYCODE_NUMPAD_ENTER
                if (select && event.type == KeyEventType.KeyUp && !editing) {
                    editing = true
                    true
                } else {
                    select
                }
            }
            .semantics { contentDescription = placeholder }
            .focusable(interactionSource = outer)
            .padding(start = 16.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.Search, contentDescription = null, size = PleyTv.IconSize, tint = if (ring) PleyColors.Fg else PleyColors.Muted)
        Spacer(Modifier.width(12.dp))
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (state.text.isEmpty()) {
                BasicText(placeholder, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 1)
            }
            KeyboardBridge(state, onPrevious = { stop() }) {
                BasicTextField(
                    state = state,
                    modifier = Modifier
                        .fillMaxWidth()
                        .focusRequester(field)
                        .focusProperties { canFocus = editing }
                        // Up or Down while typing (a remote, the keyboard closed) leaves the field that way.
                        .onPreviewKeyEvent { event ->
                            val direction = when (event.key) {
                                Key.DirectionDown -> FocusDirection.Down
                                Key.DirectionUp -> FocusDirection.Up
                                else -> return@onPreviewKeyEvent false
                            }
                            if (event.type == KeyEventType.KeyDown && editing) {
                                stop()
                                focusManager.moveFocus(direction)
                            }
                            true
                        }
                        .onFocusChanged {
                            innerFocused = it.isFocused
                            // The keyboard closed (Back, or its own close key): back to the field.
                            if (!it.isFocused && editing) editing = false
                        },
                    textStyle = PleyType.Tv.Body.copy(color = PleyColors.Fg),
                    cursorBrush = SolidColor(PleyColors.AccentBright),
                    lineLimits = TextFieldLineLimits.SingleLine,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    onKeyboardAction = {
                        onSearch()
                        stop()
                    },
                )
            }
        }
    }
}
