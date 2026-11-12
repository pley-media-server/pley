package app.pley.android.ui.tv.kit

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.constrainWidth
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Border
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.IconButton
import androidx.tv.material3.IconButtonDefaults
import androidx.tv.material3.Text
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon

/**
 * The TV's buttons (design spec §2.5): tv-material's, in pley's look.
 * Focus is always the accent fill with a small scale; disabled buttons are
 * at 40 % and can't take focus, so focus skips them and is never trapped.
 * Buttons in a row sit [PleyTv.ButtonGap] apart.
 */

/**
 * A pill: Play, Resume, Get a new code, Play now. 36 tall, at least 116
 * wide. At rest a faint fill with a [control][PleyColors.Control] edge.
 * How far a title was played shows beside it, not in the button.
 * [compact] is the home banner's: 32 tall and at least 104 wide, its label
 * the same size.
 */
@Composable
fun TvPillButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: PleyIcon? = null,
    enabled: Boolean = true,
    onLongClick: (() -> Unit)? = null,
    compact: Boolean = false,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    HandOffFocus(enabled, focused)
    val minWidth = if (compact) CompactPillMinWidth else PillMinWidth
    val padding = if (compact) CompactPillPadding else PillPadding
    Button(
        onClick = onClick,
        onLongClick = onLongClick,
        enabled = enabled,
        interactionSource = interactions,
        modifier = modifier
            .menuKey(onLongClick)
            // tv-material's Row would keep the pill at 40: its height is set, not a least one.
            .height(if (compact) CompactHeight else PleyTv.ButtonHeight)
            .widthIn(min = minWidth)
            .alpha(if (enabled) 1f else DISABLED_ALPHA)
            .focusProperties { canFocus = enabled || focused },
        shape = ButtonDefaults.shape(PleyShape.Control),
        colors = ButtonDefaults.colors(
            containerColor = PleyColors.Fg.copy(alpha = 0.12f),
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
            disabledContainerColor = PleyColors.Fg.copy(alpha = 0.12f),
            disabledContentColor = PleyColors.Fg,
        ),
        scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
        glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
        border = ButtonDefaults.border(
            border = Border(BorderStroke(1.dp, PleyColors.Control), shape = PleyShape.Control),
            focusedBorder = Border.None,
            pressedBorder = Border.None,
            disabledBorder = Border(BorderStroke(1.dp, PleyColors.Control), shape = PleyShape.Control),
            focusedDisabledBorder = Border.None,
        ),
        // tv-material's Row starts at the top: as tall as the pill, it centres the label.
        contentPadding = PaddingValues(horizontal = padding, vertical = 0.dp),
    ) {
        // That Row starts its content at the left; a short label sits centred in the pill.
        Row(
            Modifier.widthIn(min = minWidth - padding * 2),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (icon != null) {
                Icon(icon, contentDescription = null, size = if (compact) CompactIconSize else PleyTv.IconSize, tint = androidx.tv.material3.LocalContentColor.current)
                Spacer(Modifier.width(8.dp))
            }
            Text(label, style = PleyType.Tv.Label)
        }
    }
}

/**
 * An icon button: From start, Watched, Audio and subtitles, More. A 36 dp
 * square, its corners a [control][PleyShape.Control]'s, with a 20 dp icon
 * and the pill's [control][PleyColors.Control] edge, so it reads on dark art
 * too; [onArt] puts it on [bg][PleyColors.Bg] at 80 % over a backdrop, else
 * on [surface3][PleyColors.Surface3]. Its [label] is
 * always what screen readers hear; on screen it fades in 6 dp below while
 * the button has focus. The button keeps that room ([PleyTv.ButtonLabelRoom])
 * under itself, so a row of buttons never jumps. Without [showLabel] (a
 * star that says it all) the label is only heard, and the button is the
 * square alone. [compact] is the home banner's: a 32 dp square with a
 * 16 dp icon, its label the same size.
 */
@Composable
fun TvRoundButton(
    label: String,
    icon: PleyIcon,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    onArt: Boolean = true,
    enabled: Boolean = true,
    onLongClick: (() -> Unit)? = null,
    showLabel: Boolean = true,
    compact: Boolean = false,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    HandOffFocus(enabled, focused)
    val labelAlpha by animateFloatAsState(if (focused) 1f else 0f, PleyMotion.reveal(), label = "label")
    val side = if (compact) CompactHeight else PleyTv.ButtonHeight
    Box(modifier.menuKey(onLongClick).width(side).alpha(if (enabled) 1f else DISABLED_ALPHA), contentAlignment = Alignment.TopCenter) {
        IconButton(
            onClick = onClick,
            onLongClick = onLongClick,
            enabled = enabled,
            interactionSource = interactions,
            modifier = Modifier
                .size(side)
                .focusProperties { canFocus = enabled || focused }
                .semantics { contentDescription = label },
            shape = IconButtonDefaults.shape(PleyShape.Control),
            colors = IconButtonDefaults.colors(
                containerColor = if (onArt) PleyColors.OnArt else PleyColors.Surface3,
                contentColor = PleyColors.Fg,
                focusedContainerColor = PleyColors.Accent,
                focusedContentColor = PleyColors.OnAccent,
                pressedContainerColor = PleyColors.AccentBright,
                pressedContentColor = PleyColors.OnAccent,
                disabledContainerColor = if (onArt) PleyColors.OnArt else PleyColors.Surface3,
                disabledContentColor = PleyColors.Fg,
            ),
            scale = IconButtonDefaults.scale(focusedScale = PleyFocus.ROUND_SCALE, pressedScale = 1f),
            glow = IconButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
            border = IconButtonDefaults.border(
                border = Border(BorderStroke(1.dp, PleyColors.Control), shape = PleyShape.Control),
                focusedBorder = Border.None,
                pressedBorder = Border.None,
                disabledBorder = Border(BorderStroke(1.dp, PleyColors.Control), shape = PleyShape.Control),
                focusedDisabledBorder = Border.None,
            ),
        ) {
            Icon(icon, contentDescription = null, size = if (compact) CompactIconSize else PleyTv.IconSize, tint = androidx.tv.material3.LocalContentColor.current)
        }
        // The visible label, under the button; the button above already says it to screen readers.
        if (showLabel) {
            BasicText(
                label,
                style = PleyType.Tv.Caption.copy(color = PleyColors.Fg, textAlign = TextAlign.Center),
                maxLines = 1,
                softWrap = false,
                modifier = Modifier
                    .padding(top = side + 6.dp)
                    .graphicsLayer { alpha = labelAlpha }
                    .wrapContentWidthUnbounded(),
            )
        }
    }
}

/**
 * A text button: Change server, Cancel, Clear filters. 36 tall, its label
 * on a faint fill, quieter than a pill's (no edge), so beside a focused
 * button it still reads as one. A [link] (an album's artist, in a column
 * of text) has no fill until focused. An [icon] goes before the label (the
 * globe of the screens before signing in).
 */
@Composable
fun TvTextButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    color: Color = PleyColors.Fg,
    link: Boolean = false,
    icon: PleyIcon? = null,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    HandOffFocus(enabled, focused)
    val fill = if (link) Color.Transparent else TextButtonFill
    Button(
        onClick = onClick,
        enabled = enabled,
        interactionSource = interactions,
        modifier = modifier
            // Set, as a pill's is: tv-material's Row would keep it at 40.
            .height(PleyTv.ButtonHeight)
            .alpha(if (enabled) 1f else DISABLED_ALPHA)
            .focusProperties { canFocus = enabled || focused },
        shape = ButtonDefaults.shape(PleyShape.Control),
        colors = ButtonDefaults.colors(
            containerColor = fill,
            contentColor = color,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
            disabledContainerColor = fill,
            disabledContentColor = color,
        ),
        scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
        glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
        contentPadding = PaddingValues(horizontal = 16.dp),
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = PleyTv.IconSize, tint = androidx.tv.material3.LocalContentColor.current)
            Spacer(Modifier.width(8.dp))
        }
        Text(label, style = PleyType.Tv.Label)
    }
}

/** Disabled buttons are drawn at 40 %. */
private const val DISABLED_ALPHA = 0.4f

/** A text button's fill at rest: fg 8 %, under a pill's 12 %. */
private val TextButtonFill = PleyColors.Fg.copy(alpha = 0.08f)

/** A pill's least width, and the room either side of its label. */
private val PillMinWidth = 116.dp
private val PillPadding = 18.dp

/** The home banner's buttons, smaller than a page's: their height, a pill's least width and padding, their icons. */
private val CompactHeight = 32.dp
private val CompactPillMinWidth = 104.dp
private val CompactPillPadding = 14.dp
private val CompactIconSize = 16.dp

/**
 * Disabled buttons can't take focus (§2.5), and focus always shows (§5.2
 * rule 1): a button disabled while it has focus keeps it only until it has
 * handed it to a neighbour, across first, then down or up.
 */
@Composable
private fun HandOffFocus(enabled: Boolean, focused: Boolean) {
    val focus = LocalFocusManager.current
    LaunchedEffect(enabled, focused) {
        if (!enabled && focused) {
            listOf(FocusDirection.Right, FocusDirection.Left, FocusDirection.Down, FocusDirection.Up).firstOrNull { focus.moveFocus(it) }
        }
    }
}

/** Lets a label be wider than the button it sits under, centred on it. */
private fun Modifier.wrapContentWidthUnbounded(): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints.copy(minWidth = 0, maxWidth = androidx.compose.ui.unit.Constraints.Infinity))
    val width = constraints.constrainWidth(placeable.width)
    layout(width, placeable.height) { placeable.place((width - placeable.width) / 2, 0) }
}
