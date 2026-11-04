package app.pley.android.ui.tv.library

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.reducedMotion
import app.pley.android.screens.library.AlphaIndex
import app.pley.android.ui.tv.R

/**
 * The A to Z jump (design spec §4.2): a 40 dp circle beside the grid
 * showing the letter of the focused title's section, in tvTitle on
 * [surface3][PleyColors.Surface3]. Right from the grid's last column
 * focuses it (the accent fill, scaled to 1.08). Then Up and Down step
 * through #, A to Z, the letters before and after showing above and below
 * it, dimmed where no title starts with them; Select moves focus to the
 * first title of the letter (or of the next letter that has titles), and
 * Left goes back to the tile focus came from. Up from "#" leaves upwards.
 */
@Composable
internal fun JumpControl(
    index: AlphaIndex,
    focus: TvGridFocus,
    requester: FocusRequester,
    onJump: (position: Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    var focused by remember { mutableStateOf(false) }
    var letter by remember { mutableIntStateOf(0) }
    // The section's letter, not the focused cell: a move within a letter recomposes nothing here.
    val section by remember(index, focus) { derivedStateOf { index.letterAt(focus.focused.coerceAtLeast(0)) } }
    val shown = if (focused) letter else section
    val reduced = reducedMotion()
    val scale by animateFloatAsState(
        if (focused) PleyFocus.ROUND_SCALE else 1f,
        if (focused) PleyMotion.focus(reduced) else PleyMotion.unfocus(reduced),
        label = "jump",
    )
    val description = stringResource(R.string.tv_library_jump, AlphaIndex.LETTERS[shown])
    Column(modifier.width(JumpSize), horizontalAlignment = Alignment.CenterHorizontally) {
        Neighbour(if (focused) shown - 1 else -1, index)
        Box(
            Modifier
                .size(JumpSize)
                .testTag(JUMP_TAG)
                .focusRequester(requester)
                .onFocusChanged {
                    if (it.isFocused && !focused) letter = section
                    focused = it.isFocused
                }
                .onPreviewKeyEvent { event ->
                    if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    when (event.key) {
                        Key.DirectionUp -> if (letter > 0) {
                            letter--
                            true
                        } else {
                            false
                        }
                        Key.DirectionDown -> {
                            if (letter < AlphaIndex.LETTERS.lastIndex) letter++
                            true
                        }
                        else -> false
                    }
                }
                .semantics(mergeDescendants = true) { contentDescription = description }
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, role = Role.Button) {
                    onJump(index.jumpTarget(letter))
                }
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                }
                .background(if (focused) PleyColors.Accent else PleyColors.Surface3, PleyShape.Control),
            contentAlignment = Alignment.Center,
        ) {
            BasicText(
                AlphaIndex.LETTERS[shown],
                style = PleyType.Tv.Title.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Fg, textAlign = TextAlign.Center),
                modifier = Modifier.clearAndSetSemantics { },
            )
        }
        Neighbour(if (focused) shown + 1 else -1, index)
    }
}

/** The letter before or after, shown while the control has focus; [letter] -1 keeps its room empty. */
@Composable
private fun Neighbour(letter: Int, index: AlphaIndex) {
    Box(Modifier.height(NeighbourRoom).width(JumpSize).clearAndSetSemantics { }, contentAlignment = Alignment.Center) {
        if (letter in AlphaIndex.LETTERS.indices) {
            BasicText(
                AlphaIndex.LETTERS[letter],
                style = PleyType.Tv.Caption.copy(
                    color = if (index.has(letter)) PleyColors.Muted else PleyColors.Disabled.copy(alpha = 0.6f),
                    textAlign = TextAlign.Center,
                ),
            )
        }
    }
}

/** Above and below the circle, for the neighbouring letters. */
internal val NeighbourRoom = 20.dp

/** The jump control's test tag. */
internal const val JUMP_TAG = "pley.library.jump"
