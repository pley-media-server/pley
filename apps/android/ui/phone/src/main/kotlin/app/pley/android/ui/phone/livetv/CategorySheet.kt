package app.pley.android.ui.phone.livetv

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveGroups
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleySheet

/**
 * The categories, in a sheet (the web's CategoryRail, a picker on
 * phones): Favourites, Recently watched and All channels with their
 * counts, then the provider's groups, those of a region gathered under it
 * (a tap opens it), found by name when there are many.
 */
@Composable
internal fun CategorySheet(
    groups: LiveGroups?,
    selected: LiveCategory,
    onSelect: (LiveCategory) -> Unit,
    onDismiss: () -> Unit,
) {
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.livetv_categories), modifier = Modifier.testTag(LIVE_TV_CATEGORIES)) {
        CategoryList(groups, selected, onSelect, Modifier.fillMaxWidth().heightIn(max = 560.dp))
    }
}

/**
 * The categories as a list: the sheet's on the Live TV page, and the live
 * player's channel list when another group is chosen. [surface] is what
 * it is drawn on.
 */
@Composable
internal fun CategoryList(
    groups: LiveGroups?,
    selected: LiveCategory,
    onSelect: (LiveCategory) -> Unit,
    modifier: Modifier = Modifier,
    surface: Color = PleyColors.Surface2,
) {
    val all = groups?.groups.orEmpty()
    val sections = remember(all) { LiveTv.groupSections(all) }
    val selectedRegion = (selected as? LiveCategory.Group)?.let { s -> sections.regions.firstOrNull { r -> r.groups.any { it.name == s.name } }?.region }
    var open by remember(selectedRegion) { mutableStateOf(selectedRegion) }
    CompositionLocalProvider(LocalEntrySurface provides surface) {
        LazyColumn(modifier, contentPadding = PaddingValues(bottom = 16.dp)) {
            item(key = "favorites") {
                Entry(PleyIcons.Star, stringResource(R.string.livetv_category_favorites), groups?.favorites, selected == LiveCategory.Favorites) {
                    onSelect(LiveCategory.Favorites)
                }
            }
            item(key = "recent") {
                Entry(PleyIcons.History, stringResource(R.string.livetv_category_recent), groups?.recent, selected == LiveCategory.Recent) {
                    onSelect(LiveCategory.Recent)
                }
            }
            item(key = "all") {
                Entry(PleyIcons.LayoutGrid, stringResource(R.string.livetv_category_all), groups?.total, selected == LiveCategory.All) {
                    onSelect(LiveCategory.All)
                }
            }
            val regions = sections.regions
            if (regions.isNotEmpty()) {
                item(key = "regions") { Heading(stringResource(R.string.livetv_regions), regions.size) }
                for (r in regions) {
                    // One open at a time, as the web's.
                    val expanded = open == r.region
                    item(key = "region:${r.region}") {
                        Entry(
                            if (expanded) PleyIcons.ChevronDown else PleyIcons.ChevronRight,
                            r.region,
                            r.count,
                            selected = false,
                            holds = r.region == selectedRegion,
                        ) { open = if (expanded) null else r.region }
                    }
                    if (expanded) {
                        items(r.groups, key = { "group:${it.name}" }, contentType = { "group" }) { g ->
                            val category = LiveCategory.Group(g.name)
                            Entry(null, g.label, g.count, selected == category, indent = GROUP_INSET + 20.dp) { onSelect(category) }
                        }
                    }
                }
            }
            val others = sections.others
            if (others.size > 1 || (others.size == 1 && regions.isNotEmpty())) {
                item(key = "others") {
                    Heading(stringResource(if (regions.isEmpty()) R.string.livetv_groups else R.string.livetv_more_groups), others.size)
                }
                items(others, key = { "group:${it.name}" }, contentType = { "group" }) { g ->
                    val category = LiveCategory.Group(g.name)
                    Entry(null, g.name.ifEmpty { stringResource(R.string.livetv_no_group_name) }, g.count, selected == category, indent = GROUP_INSET) { onSelect(category) }
                }
            }
        }
    }
}

/** A section's heading, with how many it holds. */
@Composable
private fun Heading(text: String, count: Int) {
    Row(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 16.dp, bottom = 4.dp)) {
        Text(text.uppercase(), style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle, modifier = Modifier.weight(1f))
        Text("%,d".format(count), style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle)
    }
}

/** What the entries are drawn on. */
private val LocalEntrySurface = staticCompositionLocalOf { PleyColors.Surface2 }

/** One category: its icon (none for a group), its name and count; the chosen one, and a region that [holds] it, in the accent. */
@Composable
private fun Entry(icon: PleyIcon?, name: String, count: Int?, selected: Boolean, indent: Dp = 24.dp, holds: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .background(if (selected) PleyColors.PrimaryContainer else LocalEntrySurface.current)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(start = indent, end = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 18.dp, tint = if (selected) PleyColors.AccentBright else PleyColors.Muted)
            Spacer(Modifier.width(14.dp))
        }
        Text(
            name,
            style = MaterialTheme.typography.bodyLarge,
            color = if (selected || holds) PleyColors.AccentBright else PleyColors.Fg,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (count != null) Text("%,d".format(count), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
    }
}

/** A group's name starts where the categories' names do, after their icons. */
private val GROUP_INSET = 56.dp

internal const val LIVE_TV_CATEGORIES = "pley.live.categories"
