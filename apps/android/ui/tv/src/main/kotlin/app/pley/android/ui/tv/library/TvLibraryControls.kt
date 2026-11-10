package app.pley.android.ui.tv.library

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.Surface
import androidx.tv.material3.Text
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons

/**
 * A filter chip (design spec §2.6): a 36 dp pill in [surface3][PleyColors.Surface3]
 * with an [fg][PleyColors.Fg] label. Selected, an [accent][PleyColors.Accent]
 * check leads it (a state glyph, not a fill). Focused, it is the accent
 * fill with [onAccent][PleyColors.OnAccent] content, scaled to 1.05.
 */
@Composable
internal fun TvChip(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    selected: Boolean = false,
    icon: PleyIcon? = null,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    Button(
        onClick = onClick,
        interactionSource = interactions,
        // Set, as a pill's is: tv-material's Row would keep it at 40.
        modifier = modifier.height(PleyTv.ButtonHeight).semantics { this.selected = selected },
        shape = ButtonDefaults.shape(PleyShape.Control),
        colors = ButtonDefaults.colors(
            containerColor = PleyColors.Surface3,
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
        glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
        contentPadding = PaddingValues(horizontal = 16.dp),
    ) {
        val glyph = if (selected) PleyIcons.Check else icon
        if (glyph != null) {
            val tint = if (selected && !focused) PleyColors.Accent else LocalContentColor.current
            Icon(glyph, contentDescription = null, size = 20.dp, tint = tint)
            Spacer(Modifier.width(8.dp))
        }
        Text(label, style = PleyType.Tv.Label, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * A tab (design spec §2.6): a 32 dp pill, smaller than a page's buttons
 * as it sits on the page title's line, its label [muted][PleyColors.Muted];
 * the current tab on [surface3][PleyColors.Surface3] in [fg][PleyColors.Fg];
 * focused, the accent fill, scaled to 1.05. Select shows it: focus passing
 * over the tabs changes nothing.
 */
@Composable
internal fun TvTab(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Button(
        onClick = onClick,
        // Set, as a pill's is: tv-material's Row would keep it at 40.
        modifier = modifier.height(TabHeight).semantics {
            role = Role.Tab
            this.selected = selected
        },
        shape = ButtonDefaults.shape(PleyShape.Control),
        colors = ButtonDefaults.colors(
            containerColor = if (selected) PleyColors.Surface3 else Color.Transparent,
            contentColor = if (selected) PleyColors.Fg else PleyColors.Muted,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
        glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
        contentPadding = PaddingValues(horizontal = TabPadding),
    ) {
        Text(label, style = PleyType.Tv.Label, maxLines = 1)
    }
}

/** A tab's height, and the room either side of its label. */
private val TabHeight = 32.dp
private val TabPadding = 12.dp

/**
 * A row of a side panel (design spec §2.7): 44 dp, its label in tvBody,
 * an [accent][PleyColors.Accent] check on the choice in force, or a
 * chevron for a row that opens another list. Focused, an accent pill
 * without scale: the panel would clip it.
 */
@Composable
internal fun PanelRow(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    checked: Boolean = false,
    value: String? = null,
    opens: Boolean = false,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    Surface(
        onClick = onClick,
        interactionSource = interactions,
        modifier = modifier.fillMaxWidth().height(PleyTv.ListRowHeight).semantics { selected = checked },
        shape = ClickableSurfaceDefaults.shape(PleyShape.Control),
        colors = ClickableSurfaceDefaults.colors(
            containerColor = Color.Transparent,
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ClickableSurfaceDefaults.scale(focusedScale = 1f, pressedScale = 1f),
        glow = ClickableSurfaceDefaults.glow(Glow.None, Glow.None, Glow.None),
    ) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp).align(Alignment.CenterStart),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(label, style = PleyType.Tv.Body, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            if (value != null) {
                Text(value, style = PleyType.Tv.Caption, color = if (focused) PleyColors.OnAccent else PleyColors.Muted, maxLines = 1)
            }
            when {
                checked -> Icon(PleyIcons.Check, contentDescription = null, size = 20.dp, tint = if (focused) PleyColors.OnAccent else PleyColors.Accent)
                opens -> Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = LocalContentColor.current)
            }
        }
    }
}

/** A heading inside a side panel: tvCaption in [muted][PleyColors.Muted]. */
@Composable
internal fun PanelHeading(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = PleyType.Tv.Caption,
        color = PleyColors.Muted,
        modifier = modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp),
    )
}
