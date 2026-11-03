package app.pley.android.ui.tv.kit

import android.view.KeyEvent as AndroidKeyEvent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.BasicText
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
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.compose.foundation.text.input.TextFieldLineLimits
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import kotlinx.coroutines.delay

/**
 * Text entry on TV (D-pad rule 7; design spec §2.11), for server
 * addresses and playlist names. Focus lands on the field without opening the
 * keyboard, so D-pad users pass through it; Select opens the system
 * keyboard (Gboard, or Amazon's), and Back or the keyboard's action closes
 * it again, leaving focus on the field. With [typeNow] (a screen that only
 * asks for words), the keyboard opens as the field shows.
 *
 * 44 dp, [surface2][PleyColors.Surface2], radius 4, tvBody. At rest a 1 dp
 * [control][PleyColors.Control] edge; focused a 2 dp [accentBright][PleyColors.AccentBright]
 * edge, without scale (fields are wide).
 */
@Composable
fun TvTextField(
    state: TextFieldState,
    placeholder: String,
    modifier: Modifier = Modifier,
    keyboardType: KeyboardType = KeyboardType.Text,
    imeAction: ImeAction = ImeAction.Search,
    onAction: () -> Unit = {},
    focusModifier: Modifier = Modifier,
    typeNow: Boolean = false,
    inputTransformation: InputTransformation? = null,
) {
    var editing by remember { mutableStateOf(typeNow) }
    val outer = remember { MutableInteractionSource() }
    val outerFocused by outer.collectIsFocusedAsState()
    var innerFocused by remember { mutableStateOf(false) }
    val field = remember { FocusRequester() }
    val shell = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current

    fun stop() {
        editing = false
        keyboard?.hide()
        shell.requestFocus()
    }

    BackHandler(enabled = editing) { stop() }
    OverlayShowing(editing)
    val view = LocalView.current
    LaunchedEffect(editing) {
        if (editing) {
            field.requestFocus()
            keyboard?.show()
            // The keyboard's own Back closes it without the app hearing a key,
            // and the field keeps focus: once it has shown and gone, stop.
            var shown = false
            while (true) {
                delay(KEYBOARD_POLL_MS)
                val visible = ViewCompat.getRootWindowInsets(view)?.isVisible(WindowInsetsCompat.Type.ime()) == true
                if (visible) shown = true else if (shown) break
            }
            stop()
        }
    }
    val ring = outerFocused || innerFocused
    Box(
        modifier
            .height(FieldHeight)
            .fillMaxWidth()
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
            .focusable(interactionSource = outer)
            .padding(horizontal = 16.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        if (state.text.isEmpty()) {
            BasicText(placeholder, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 1)
        }
        BasicTextField(
            state = state,
            modifier = Modifier
                .fillMaxWidth()
                .focusRequester(field)
                .focusProperties { canFocus = editing }
                .onFocusChanged {
                    // Not when it is first laid out, unfocused: typeNow is about to focus it.
                    val lost = innerFocused && !it.isFocused
                    innerFocused = it.isFocused
                    // The keyboard closed (Back, or its own close key): back to the field.
                    if (lost && editing) editing = false
                },
            textStyle = PleyType.Tv.Body.copy(color = PleyColors.Fg),
            cursorBrush = SolidColor(PleyColors.AccentBright),
            inputTransformation = inputTransformation,
            lineLimits = TextFieldLineLimits.SingleLine,
            keyboardOptions = KeyboardOptions(keyboardType = keyboardType, imeAction = imeAction),
            onKeyboardAction = {
                onAction()
                stop()
            },
        )
    }
}

/**
 * D-pad rule 10: the remote's digit keys type into the PIN pad and the
 * address field. [onDigit] gets 0 to 9 from the number row and the number
 * pad; [onDelete] gets Delete and Clear; [onDot], when given (the address),
 * gets the period and the number pad's dot; [onText], when given, the
 * other printable characters a keyboard types (a colon, a server's name),
 * which a remote app's or a Bluetooth keyboard sends. Other keys pass on.
 */
fun Modifier.digitKeys(
    onDigit: (Int) -> Unit,
    onDelete: () -> Unit = {},
    onDot: (() -> Unit)? = null,
    onText: ((Char) -> Unit)? = null,
): Modifier = onKeyEvent { event ->
    if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
    val typed = event.nativeKeyEvent.unicodeChar
    if (onText != null && typed > ' '.code && !Character.isDigit(typed) && typed != '.'.code) {
        onText(typed.toChar())
        return@onKeyEvent true
    }
    when (val code = event.nativeKeyEvent.keyCode) {
        AndroidKeyEvent.KEYCODE_PERIOD, AndroidKeyEvent.KEYCODE_NUMPAD_DOT -> {
            onDot?.invoke()
            onDot != null
        }
        in AndroidKeyEvent.KEYCODE_0..AndroidKeyEvent.KEYCODE_9 -> {
            onDigit(code - AndroidKeyEvent.KEYCODE_0)
            true
        }
        in AndroidKeyEvent.KEYCODE_NUMPAD_0..AndroidKeyEvent.KEYCODE_NUMPAD_9 -> {
            onDigit(code - AndroidKeyEvent.KEYCODE_NUMPAD_0)
            true
        }
        AndroidKeyEvent.KEYCODE_DEL, AndroidKeyEvent.KEYCODE_FORWARD_DEL, AndroidKeyEvent.KEYCODE_CLEAR -> {
            onDelete()
            true
        }
        else -> false
    }
}

/** How often a field being typed in looks whether the keyboard is still up. */
private const val KEYBOARD_POLL_MS = 250L

/** A text field's height. */
internal val FieldHeight = 44.dp
