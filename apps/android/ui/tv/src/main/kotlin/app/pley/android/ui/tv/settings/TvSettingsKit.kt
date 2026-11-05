package app.pley.android.ui.tv.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.focusRow

/**
 * The TV's settings pieces (design spec §4.8): rows that fill with
 * [accent][PleyColors.Accent] when focused and never scale (rows in lists
 * don't), with a label in tvBody and the current value in tvCaption muted;
 * switch rows with a switch on the right; and the side panel a choice is
 * made in.
 */

/**
 * A focusable settings row. [checked] makes it a switch row (Select
 * toggles it); otherwise [chevron] says it opens something. [danger] reads
 * in [dangerText][PleyColors.DangerText] (Sign out). [onLongClick] is the
 * long press of Select (rule 6).
 */
@Composable
internal fun TvSettingRow(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    value: String? = null,
    icon: PleyIcon? = null,
    checked: Boolean? = null,
    chevron: Boolean = false,
    danger: Boolean = false,
    enabled: Boolean = true,
    focusRequester: FocusRequester? = null,
    onFocused: (() -> Unit)? = null,
    trailing: (@Composable (focused: Boolean) -> Unit)? = null,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    if (focused && onFocused != null) LaunchedEffect(Unit) { onFocused() }
    val fg = when {
        focused -> PleyColors.OnAccent
        danger -> PleyColors.DangerText
        enabled -> PleyColors.Fg
        else -> PleyColors.Disabled
    }
    val sub = when {
        focused -> PleyColors.OnAccent.copy(alpha = 0.8f)
        enabled -> PleyColors.Muted
        else -> PleyColors.Disabled
    }
    val switchSaid = checked?.let { stringResource(if (it) R.string.tv_settings_switch_on else R.string.tv_settings_switch_off) }
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = PleyTv.ListRowHeight)
            .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
            .background(if (focused) PleyColors.Accent else Color.Transparent, PleyShape.Pill)
            .semantics {
                if (switchSaid != null) stateDescription = switchSaid
                if (value != null) stateDescription = value
            }
            .clickable(interactionSource = interactions, indication = null, enabled = enabled, role = if (checked != null) Role.Switch else Role.Button, onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = PleyTv.IconSize, tint = if (focused) PleyColors.OnAccent else if (danger) PleyColors.DangerText else PleyColors.Muted)
            Spacer(Modifier.width(12.dp))
        }
        Column(Modifier.weight(1f)) {
            BasicText(label, style = PleyType.Tv.Body.copy(color = fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (!value.isNullOrEmpty()) {
                BasicText(value, style = PleyType.Tv.Caption.copy(color = sub), maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
        when {
            trailing != null -> {
                Spacer(Modifier.width(12.dp))
                trailing(focused)
            }
            checked != null -> {
                Spacer(Modifier.width(12.dp))
                TvSwitch(checked, focused)
            }
            chevron -> {
                Spacer(Modifier.width(12.dp))
                Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = if (focused) PleyColors.OnAccent else PleyColors.Subtle)
            }
        }
    }
}

/**
 * A switch drawn in the row (the row takes focus, not the switch): a
 * 40 by 22 track, [accent][PleyColors.Accent] when on, with its thumb at
 * the end it is on. On a focused (accent) row it turns to onAccent.
 */
@Composable
internal fun TvSwitch(checked: Boolean, focused: Boolean, modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(40.dp, 22.dp)
            .drawBehind {
                val track = when {
                    focused && checked -> PleyColors.OnAccent
                    focused -> PleyColors.OnAccent.copy(alpha = 0.3f)
                    checked -> PleyColors.Accent
                    else -> PleyColors.Surface4
                }
                val thumb = when {
                    focused && checked -> PleyColors.Accent
                    focused -> PleyColors.OnAccent
                    checked -> PleyColors.OnAccent
                    else -> PleyColors.Control
                }
                val r = size.height / 2f
                drawRoundRect(track, cornerRadius = CornerRadius(r, r))
                val pad = 3.dp.toPx()
                val tr = r - pad
                val cx = if (checked) size.width - r else r
                drawCircle(thumb, tr, Offset(cx, r))
            },
    )
}

/** A heading over a pane's rows: "Advanced". */
@Composable
internal fun TvPaneHeading(text: String, modifier: Modifier = Modifier) {
    BasicText(text, style = PleyType.Tv.Label.copy(color = PleyColors.Subtle), modifier = modifier.padding(start = 20.dp, top = 16.dp, bottom = 6.dp))
}

/** One choice of a [TvChoicePanel]. */
internal data class TvChoice<T>(val value: T, val label: String, val hint: String? = null)

/**
 * A choice made in a side panel (design spec §2.7): its [title], one row
 * per choice with the current one checked and focused first; choosing
 * closes it, Back too, and focus goes back to [returnTo] (the row that
 * opened it).
 */
@Composable
internal fun <T> TvChoicePanel(
    open: Boolean,
    title: String,
    choices: List<TvChoice<T>>,
    selected: T,
    onPick: (T) -> Unit,
    onDismiss: () -> Unit,
    returnTo: FocusRequester?,
) {
    val first = remember { FocusRequester() }
    // Focus goes back once the panel has let go of it (it keeps focus while it is open).
    var giveBack by remember { mutableStateOf(false) }
    val close: () -> Unit = {
        onDismiss()
        giveBack = true
    }
    LaunchedEffect(open, giveBack) {
        if (!open && giveBack) {
            withFrameNanos { }
            returnTo?.let { r -> runCatching { r.requestFocus() } }
            giveBack = false
        }
    }
    TvSidePanel(visible = open, onDismiss = close, modifier = Modifier.testTag(TV_CHOICE_PANEL)) {
        val list = rememberLazyListState()
        LaunchedEffect(open, choices) {
            if (open) {
                withFrameNanos { }
                withFrameNanos { }
                list.focusRow(first)
            }
        }
        BasicText(
            title,
            style = PleyType.Tv.Title.copy(color = PleyColors.Fg),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = 16.dp, top = 8.dp, bottom = 12.dp),
        )
        val start = choices.indexOfFirst { it.value == selected }.coerceAtLeast(0)
        LazyColumn(
            state = list,
            modifier = Modifier
                .fillMaxSize()
                .focusProperties { onExit = { if (open) cancelFocusChange() } }
                .focusGroup(),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            itemsIndexed(choices, key = { i, _ -> i }) { i, choice ->
                ChoiceLine(
                    choice.label,
                    choice.hint,
                    checked = choice.value == selected,
                    focusRequester = if (i == start) first else null,
                    onClick = {
                        onPick(choice.value)
                        close()
                    },
                )
            }
        }
    }
}

@Composable
private fun ChoiceLine(label: String, hint: String?, checked: Boolean, focusRequester: FocusRequester?, onClick: () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val content = if (focused) PleyColors.OnAccent else PleyColors.Fg
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = PleyTv.ListRowHeight)
            .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
            .background(if (focused) PleyColors.Accent else Color.Transparent, PleyShape.Pill)
            .semantics { selected = checked }
            .clickable(interactionSource = interactions, indication = null, role = Role.RadioButton, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            BasicText(label, style = PleyType.Tv.Body.copy(color = content), maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (hint != null) {
                BasicText(hint, style = PleyType.Tv.Caption.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted), maxLines = 3)
            }
        }
        if (checked) {
            Spacer(Modifier.width(12.dp))
            Icon(PleyIcons.Check, contentDescription = null, size = PleyTv.IconSize, tint = if (focused) PleyColors.OnAccent else PleyColors.Accent)
        }
    }
}

/** Center (and Enter) presses, for rows that act on Select without being clickable. */
internal fun Modifier.onSelect(action: () -> Unit): Modifier = onPreviewKeyEvent {
    if (it.type == KeyEventType.KeyUp && (it.key == Key.DirectionCenter || it.key == Key.Enter || it.key == Key.NumPadEnter)) {
        action()
        true
    } else {
        false
    }
}

/** "http://192.168.1.20:7539" → "192.168.1.20:7539". */
internal fun hostPort(server: String): String = server.substringAfter("://").trimEnd('/')

/** A path from the server made absolute on [origin]. */
internal fun absolute(path: String, origin: String): String =
    if (path.startsWith("http://") || path.startsWith("https://")) path else origin.trimEnd('/') + "/" + path.trimStart('/')

/** Fills a rounded rect of [size] at [topLeft]: drawn, for skeleton blocks. */
internal fun Modifier.block(color: Color = PleyColors.Surface2): Modifier = drawBehind {
    drawRoundRect(color, Offset.Zero, Size(size.width, size.height), CornerRadius(4.dp.toPx(), 4.dp.toPx()))
}

internal const val TV_CHOICE_PANEL = "pley.tv.settings.choices"
