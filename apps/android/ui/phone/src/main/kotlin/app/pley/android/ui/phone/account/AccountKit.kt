package app.pley.android.ui.phone.account

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.ui.phone.kit.PleySheet

/**
 * The phone's settings pieces (design spec §4.8, §1.5 level 1): groups of
 * rows on [surface][PleyColors.Surface] with 12 dp corners, a heading over
 * each, rows that open a choice, switch rows, and the sheet a choice is
 * made in. Me, Settings and Diagnostics are built from them.
 */

/** A group of rows under [title], with an optional [footer] line under it. */
@Composable
internal fun SettingsGroup(
    title: String?,
    modifier: Modifier = Modifier,
    footer: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier.fillMaxWidth()) {
        if (title != null) {
            Text(
                title,
                style = MaterialTheme.typography.labelLarge,
                color = PleyColors.Muted,
                modifier = Modifier.padding(start = 4.dp, bottom = 8.dp).semantics { heading() },
            )
        }
        Column(
            Modifier
                .fillMaxWidth()
                .clip(PleyShape.Card)
                .background(PleyColors.Surface),
            content = content,
        )
        if (footer != null) {
            Text(
                footer,
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Subtle,
                modifier = Modifier.padding(start = 4.dp, end = 4.dp, top = 8.dp),
            )
        }
    }
}

/** The hairline between two rows of a group, inset past the icons. */
@Composable
internal fun RowDivider(inset: Boolean = true) {
    HorizontalDivider(Modifier.padding(start = if (inset) 56.dp else 16.dp), thickness = 1.dp, color = PleyColors.Surface4)
}

/**
 * A row that opens something: a 24 dp [icon], the [title] and its current
 * [value] under it, and a chevron. [danger] rows read in
 * [dangerText][PleyColors.DangerText] ("Sign out"); disabled rows are
 * shown, at 38 %, and do nothing.
 */
@Composable
internal fun SettingsRow(
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    value: String? = null,
    icon: PleyIcon? = null,
    enabled: Boolean = true,
    danger: Boolean = false,
    chevron: Boolean = true,
    trailing: (@Composable () -> Unit)? = null,
) {
    val fg = when {
        danger -> PleyColors.DangerText
        enabled -> PleyColors.Fg
        else -> PleyColors.Disabled
    }
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 24.dp, tint = if (danger) PleyColors.DangerText else if (enabled) PleyColors.Muted else PleyColors.Disabled)
            Spacer(Modifier.width(16.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, color = fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (!value.isNullOrEmpty()) {
                Text(
                    value,
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (enabled) PleyColors.Muted else PleyColors.Disabled,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (trailing != null) {
            Spacer(Modifier.width(12.dp))
            trailing()
        } else if (chevron && enabled && !danger) {
            Spacer(Modifier.width(12.dp))
            Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
        }
    }
}

/**
 * A row with a switch; the whole row toggles it. One that isn't [enabled]
 * shows dimmed and does nothing (managing the server: what the server
 * wouldn't take).
 */
@Composable
internal fun SwitchRow(
    title: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    icon: PleyIcon? = null,
    enabled: Boolean = true,
) {
    val ink = if (enabled) PleyColors.Fg else PleyColors.Disabled
    val muted = if (enabled) PleyColors.Muted else PleyColors.Disabled
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onChange)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 24.dp, tint = muted)
            Spacer(Modifier.width(16.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, color = ink)
            if (subtitle != null) Text(subtitle, style = MaterialTheme.typography.bodyMedium, color = muted)
        }
        Spacer(Modifier.width(12.dp))
        RowSwitch(checked, enabled)
    }
}

/** The switch of a row the whole of which toggles it, so the switch itself takes no touch. */
@Composable
internal fun RowSwitch(checked: Boolean, enabled: Boolean = true) {
    Switch(
        checked = checked,
        onCheckedChange = null,
        enabled = enabled,
        colors = SwitchDefaults.colors(
            checkedThumbColor = PleyColors.OnAccent,
            checkedTrackColor = PleyColors.Accent,
            uncheckedThumbColor = PleyColors.Control,
            uncheckedTrackColor = PleyColors.Surface3,
            uncheckedBorderColor = PleyColors.Control,
            disabledCheckedTrackColor = PleyColors.Accent.copy(alpha = 0.38f),
            disabledCheckedThumbColor = PleyColors.OnAccent.copy(alpha = 0.6f),
            disabledUncheckedTrackColor = PleyColors.Surface3,
            disabledUncheckedThumbColor = PleyColors.Disabled,
            disabledUncheckedBorderColor = PleyColors.Disabled,
        ),
    )
}

/** One choice of a [ChoiceSheet]: its value, its label and an optional line under it. */
internal data class Choice<T>(val value: T, val label: String, val hint: String? = null)

/**
 * A choice made in a bottom sheet (design spec §3.4): [title], then one
 * 56 dp row per choice, the current one checked. Picking one closes the
 * sheet. Long lists (languages) scroll, the current one in view.
 */
@Composable
internal fun <T> ChoiceSheet(
    title: String,
    choices: List<Choice<T>>,
    selected: T,
    onPick: (T) -> Unit,
    onDismiss: () -> Unit,
) {
    PleySheet(onDismiss = onDismiss, title = title) {
        val start = choices.indexOfFirst { it.value == selected }.coerceAtLeast(0)
        val state = androidx.compose.foundation.lazy.rememberLazyListState(initialFirstVisibleItemIndex = (start - 3).coerceAtLeast(0))
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 520.dp), state = state) {
            itemsIndexed(choices, key = { i, _ -> i }) { _, choice ->
                val on = choice.value == selected
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = 56.dp)
                        .selectable(selected = on, role = Role.RadioButton) {
                            onPick(choice.value)
                            onDismiss()
                        }
                        .padding(horizontal = 24.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(choice.label, style = MaterialTheme.typography.bodyLarge, color = if (on) PleyColors.AccentBright else PleyColors.Fg)
                        if (choice.hint != null) {
                            Text(choice.hint, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
                        }
                    }
                    Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
                        if (on) Icon(PleyIcons.Check, contentDescription = null, size = 22.dp, tint = PleyColors.Accent)
                    }
                }
            }
        }
    }
}

/** "http://192.168.1.20:7539" → "192.168.1.20:7539": a server as people read it. */
internal fun hostPort(server: String): String = server.substringAfter("://").trimEnd('/')
