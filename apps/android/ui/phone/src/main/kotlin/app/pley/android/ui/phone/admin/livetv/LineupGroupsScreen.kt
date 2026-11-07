package app.pley.android.ui.phone.admin.livetv

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.admin.LineupGroup
import app.pley.android.screens.admin.livetv.GroupSection
import app.pley.android.screens.admin.livetv.GroupsShown
import app.pley.android.screens.admin.livetv.LineupGroupsViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.RowSwitch
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PleyChip
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.search.SearchField
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * Live TV's Groups, as the web's tab: the counts, a field to find a group
 * and which to list, then the groups by region. A region's row hides or
 * shows all its groups and opens to list them (found ones listed open);
 * each group's switch shows or hides it. Each is sent at once.
 */
@Composable
internal fun LineupGroupsScreen() {
    val model: LineupGroupsViewModel = viewModel(factory = LineupGroupsViewModel.factory)
    val load by model.groups.state.collectAsStateWithLifecycle()
    val sections by model.sections.collectAsStateWithLifecycle()
    val shown by model.shown.collectAsStateWithLifecycle()
    val query by model.query.collectAsStateWithLifecycle()
    val open by model.open.collectAsStateWithLifecycle()
    val busy by model.busy.collectAsStateWithLifecycle()
    val metrics = LocalPhoneMetrics.current
    val field = rememberTextFieldState(query)
    LaunchedEffect(field) { snapshotFlow { field.text.toString() }.collect(model::find) }
    AdminEvents(model, liveTvSaid())
    LiveListPage(
        title = stringResource(R.string.admin_livetv_tab_groups),
        load = load,
        onRetry = model.groups::reload,
        tag = LIVETV_GROUPS_PAGE,
        header = { all ->
            if (all.isNotEmpty()) {
                // Counted once a read, not at each letter typed: a provider's groups run to thousands.
                val (hiddenGroups, toWatch) = remember(all) { all.count { it.hidden == it.count } to all.sumOf { it.count - it.hidden } }
                val line = pluralStringResource(R.plurals.admin_livetv_groups_shown, all.size - hiddenGroups, all.size - hiddenGroups) +
                    (if (hiddenGroups > 0) pluralStringResource(R.plurals.admin_livetv_groups_also_hidden, hiddenGroups, hiddenGroups) else "") +
                    " · " + pluralStringResource(R.plurals.admin_livetv_groups_to_watch, toWatch, toWatch) +
                    " " + stringResource(R.string.admin_livetv_groups_hint)
                PageLine(line, Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd))
                SearchField(field, stringResource(R.string.admin_livetv_groups_find), onSubmit = {}, autoFocus = false, tag = LIVETV_GROUPS_FIND)
                Row(
                    Modifier.horizontalScroll(rememberScrollState()).padding(start = metrics.marginStart, end = metrics.marginEnd, bottom = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    for ((which, words) in listOf(
                        GroupsShown.All to R.string.admin_livetv_groups_shown_and_hidden,
                        GroupsShown.Shown to R.string.admin_livetv_groups_shown_only,
                        GroupsShown.Hidden to R.string.admin_livetv_groups_hidden_only,
                    )) {
                        PleyChip(stringResource(words), selected = shown == which, onClick = { model.show(which) })
                    }
                }
            }
        },
    ) { all ->
        val listed = sections
        when {
            all.isEmpty() -> item(key = "none") {
                EmptyState(stringResource(R.string.admin_livetv_groups_none_title), stringResource(R.string.admin_livetv_groups_none_body), icon = PleyIcons.Folder)
            }
            listed == null -> Unit
            listed.isEmpty() -> item(key = "nomatch") {
                Text(
                    stringResource(R.string.admin_livetv_groups_no_match),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    modifier = Modifier.padding(24.dp),
                )
            }
            else -> for (section in listed) {
                val region = section.region
                if (region == null) {
                    val g = section.groups.single()
                    item(key = "g:${g.name}", contentType = "group") { GroupRow(g, g.name, indent = false, enabled = busy == 0, onHidden = { model.setHidden(g, it) }) }
                } else {
                    val expanded = region in open || query.isNotBlank()
                    item(key = "r:$region", contentType = "region") {
                        RegionRow(section, region, expanded, enabled = busy == 0, onToggle = { model.toggle(region) }, onHidden = { model.setHidden(section, it) })
                    }
                    if (expanded) {
                        items(section.groups, key = { "g:${it.name}" }, contentType = { "group" }) { g ->
                            GroupRow(g, g.label, indent = true, enabled = busy == 0, onHidden = { model.setHidden(g, it) })
                        }
                    }
                }
            }
        }
    }
}

/** A region: its groups and channels, whether some are hidden, Show all or Hide all; it opens to list them. */
@Composable
private fun RegionRow(section: GroupSection, region: String, expanded: Boolean, enabled: Boolean, onToggle: () -> Unit, onHidden: (Boolean) -> Unit) {
    Column(Modifier.adminWidth.background(PleyColors.Surface)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(role = Role.Button, onClick = onToggle).padding(start = 12.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(if (expanded) PleyIcons.ChevronDown else PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
            Spacer(Modifier.width(8.dp))
            Box(Modifier.background(PleyColors.Surface3, PleyShape.Badge).padding(horizontal = 6.dp, vertical = 2.dp)) {
                Text(region, style = MaterialTheme.typography.labelMedium, color = PleyColors.Fg)
            }
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    pluralStringResource(R.plurals.admin_livetv_groups_region_groups, section.groups.size, section.groups.size) + " · " +
                        pluralStringResource(R.plurals.admin_livetv_groups_region_channels, section.channels, section.channels),
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (section.allHidden) PleyColors.Subtle else PleyColors.Fg,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (!section.allHidden && !section.allShown) {
                    Text(stringResource(R.string.admin_livetv_groups_some_hidden), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
                }
            }
            PleyTextButton(
                stringResource(if (section.allHidden) R.string.admin_livetv_groups_show_all else R.string.admin_livetv_groups_hide_all),
                onClick = { onHidden(!section.allHidden) },
                enabled = enabled,
            )
        }
        HorizontalDivider(thickness = 1.dp, color = PleyColors.Surface4)
    }
}

/** A group: its name (crossed out when hidden), its hidden, offline and total counts, and whether it shows. */
@Composable
private fun GroupRow(group: LineupGroup, label: String, indent: Boolean, enabled: Boolean, onHidden: (Boolean) -> Unit) {
    val hidden = group.hidden == group.count
    val name = label.ifEmpty { stringResource(R.string.admin_livetv_groups_no_group) }
    val counts = listOfNotNull(
        group.hidden.takeIf { it > 0 && !hidden }?.let { pluralStringResource(R.plurals.admin_livetv_groups_hidden_count, it, it) },
        group.offline.takeIf { it > 0 }?.let { pluralStringResource(R.plurals.admin_livetv_groups_offline_count, it, it) },
        pluralStringResource(R.plurals.admin_livetv_channels_count, group.count, group.count),
    ).joinToString(" · ")
    Column(Modifier.adminWidth.background(PleyColors.Surface)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .toggleable(value = !hidden, enabled = enabled, role = Role.Switch, onValueChange = { show -> onHidden(!show) })
                .padding(start = if (indent) 44.dp else 16.dp, end = 16.dp, top = 6.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    name,
                    style = MaterialTheme.typography.bodyLarge,
                    color = if (hidden) PleyColors.Subtle else PleyColors.Fg,
                    textDecoration = if (hidden) TextDecoration.LineThrough else null,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(counts, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.width(12.dp))
            RowSwitch(checked = !hidden, enabled = enabled)
        }
        HorizontalDivider(thickness = 1.dp, color = PleyColors.Surface4)
    }
}

internal const val LIVETV_GROUPS_PAGE = "pley.admin.livetv.groups"
internal const val LIVETV_GROUPS_FIND = "pley.admin.livetv.groups.find"
