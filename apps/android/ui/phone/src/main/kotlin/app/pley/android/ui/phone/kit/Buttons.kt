package app.pley.android.ui.phone.kit

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon

/**
 * The primary action (design spec §3.3): Play, Resume, Continue in
 * browser. 48 dp tall, a pill in [accent][PleyColors.Accent] with
 * labelLarge in [onAccent][PleyColors.OnAccent], and an optional 20 dp
 * [icon] before the text or [trailingIcon] after it (Continue in browser's
 * arrow out of a box). Full width on compact item pages: pass `Modifier.fillMaxWidth()`.
 */
@Composable
fun PrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: PleyIcon? = null,
    enabled: Boolean = true,
    trailingIcon: PleyIcon? = null,
) {
    Button(
        onClick = onClick,
        modifier = modifier.heightIn(min = 48.dp),
        enabled = enabled,
        shape = PleyShape.Control,
        colors = ButtonDefaults.buttonColors(containerColor = PleyColors.Accent, contentColor = PleyColors.OnAccent),
        contentPadding = PaddingValues(horizontal = 24.dp, vertical = 12.dp),
    ) {
        ButtonContent(text, icon, trailingIcon)
    }
}

/**
 * A secondary action (design spec §3.3): From start, the season picker,
 * Enter an address. 40 dp tall, a [surface3][PleyColors.Surface3] pill
 * with [fg][PleyColors.Fg] text.
 */
@Composable
fun TonalButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: PleyIcon? = null,
    enabled: Boolean = true,
    trailingIcon: PleyIcon? = null,
) {
    FilledTonalButton(
        onClick = onClick,
        modifier = modifier.heightIn(min = 40.dp),
        enabled = enabled,
        shape = PleyShape.Control,
        colors = ButtonDefaults.filledTonalButtonColors(containerColor = PleyColors.Surface3, contentColor = PleyColors.Fg),
        contentPadding = PaddingValues(horizontal = 20.dp, vertical = 8.dp),
    ) {
        ButtonContent(text, icon, trailingIcon)
    }
}

/** A text action (design spec §3.3): More, Use a code instead. [accentBright][PleyColors.AccentBright] on the page. */
@Composable
fun PleyTextButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    TextButton(
        onClick = onClick,
        modifier = modifier.heightIn(min = 48.dp),
        enabled = enabled,
        colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.AccentBright),
    ) {
        Text(text, style = MaterialTheme.typography.labelLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A 48 dp icon button for bars and the player (design spec §3.3), its 24 dp [icon] read out as [label]. */
@Composable
fun PleyIconButton(
    icon: PleyIcon,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tint: Color = PleyColors.Fg,
) {
    IconButton(
        onClick = onClick,
        modifier = modifier.size(48.dp),
        enabled = enabled,
        colors = IconButtonDefaults.iconButtonColors(contentColor = tint),
    ) {
        Icon(icon, contentDescription = label, size = 24.dp, tint = if (enabled) tint else PleyColors.Disabled)
    }
}

/**
 * One of the item page's actions (design spec §3.3): From start, Watched,
 * Playlist, Trailer. A column 72 dp wide at least (the page gives it its
 * slot), a 24 dp [icon] over a labelMedium [label] that shrinks rather than
 * break a word ([FittedLabel]), no container; pressed, a [surface3][PleyColors.Surface3]
 * circle shows behind the icon. The touch target is 72 by 56 at least. [selected]
 * draws the icon in [accentBright][PleyColors.AccentBright] (watched).
 */
@Composable
fun ActionButton(
    icon: PleyIcon,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    selected: Boolean = false,
    enabled: Boolean = true,
) {
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    Column(
        modifier
            .widthIn(min = 72.dp)
            .defaultMinSize(minHeight = 56.dp)
            .clickable(interactionSource = interactions, indication = null, enabled = enabled, role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Box(
            Modifier
                .size(40.dp)
                .drawBehind { if (pressed) drawCircle(PleyColors.Surface3) },
            contentAlignment = Alignment.Center,
        ) {
            val tint = when {
                !enabled -> PleyColors.Disabled
                selected -> PleyColors.AccentBright
                else -> PleyColors.Fg
            }
            Icon(icon, contentDescription = null, size = 24.dp, tint = tint)
        }
        Spacer(Modifier.height(2.dp))
        FittedLabel(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = if (enabled) PleyColors.Fg else PleyColors.Disabled,
        )
    }
}

@Composable
private fun RowScope.ButtonContent(text: String, icon: PleyIcon?, trailingIcon: PleyIcon? = null) {
    if (icon != null) {
        Icon(icon, contentDescription = null, size = 20.dp, tint = LocalContentColor.current)
        Spacer(Modifier.width(8.dp))
    }
    // Text first, so it shrinks before the icon after it does.
    Text(text, style = MaterialTheme.typography.labelLarge, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
    if (trailingIcon != null) {
        Spacer(Modifier.width(8.dp))
        Icon(trailingIcon, contentDescription = null, size = 20.dp, tint = LocalContentColor.current)
    }
}
