package app.pley.android.ui.phone.library

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.selection.selectable
import androidx.compose.ui.Alignment
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.pley.android.common.resolve
import app.pley.android.data.GridDensity
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.LibraryKind
import app.pley.android.model.SortOrder
import app.pley.android.screens.library.LibraryUiState
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.labelText
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.SegmentedControl

/**
 * The toolbar's sheets (design spec §3.4): Sort (the sorts, the order,
 * which titles to show, the card size), Genre, Year, Resolution and
 * Dynamic range (the library's, from its filters). The choice in force
 * has a check.
 */
@Composable
internal fun LibrarySheetHost(
    sheet: LibrarySheet,
    state: LibraryUiState<ArtworkTileModel>,
    vm: LibraryViewModel<ArtworkTileModel>,
    onDismiss: () -> Unit,
) {
    val params = state.params
    when (sheet) {
        LibrarySheet.Sort -> PleySheet(onDismiss = onDismiss, title = stringResource(R.string.library_sort_by)) {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                state.sorts.forEach { sort ->
                    ChoiceRow(stringResource(sortLabel(sort)), selected = sort == params.sort, onClick = { vm.sortBy(sort) })
                }
                SheetSection(stringResource(R.string.library_order)) {
                    SegmentedControl(
                        options = listOf(stringResource(R.string.library_ascending), stringResource(R.string.library_descending)),
                        selected = if (params.effectiveOrder == SortOrder.Ascending) 0 else 1,
                        onSelect = { vm.order(if (it == 0) SortOrder.Ascending else SortOrder.Descending) },
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                if (state.kind != LibraryKind.Music) {
                    SheetSection(stringResource(R.string.library_show)) {
                        SegmentedControl(
                            options = listOf(
                                stringResource(R.string.library_show_all),
                                stringResource(R.string.library_unwatched),
                                stringResource(R.string.library_watched),
                            ),
                            selected = when (params.watched) {
                                null -> 0
                                false -> 1
                                true -> 2
                            },
                            onSelect = { vm.watched(listOf(null, false, true)[it]) },
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
                SheetSection(stringResource(R.string.library_card_size)) {
                    SegmentedControl(
                        options = GridDensity.entries.map { stringResource(densityLabel(it)) },
                        selected = state.density.ordinal,
                        onSelect = { vm.density(GridDensity.entries[it]) },
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
        LibrarySheet.Genre -> PleySheet(onDismiss = onDismiss, title = stringResource(R.string.library_genre)) {
            Choices(
                all = stringResource(R.string.library_all_genres),
                values = state.filters.genres,
                selected = params.genre,
                label = { it },
                onSelect = {
                    vm.genre(it)
                    onDismiss()
                },
            )
        }
        LibrarySheet.Year -> PleySheet(onDismiss = onDismiss, title = stringResource(R.string.library_year)) {
            Choices(
                all = stringResource(R.string.library_all_years),
                values = state.filters.years,
                selected = params.year,
                label = { it.toString() },
                onSelect = {
                    vm.year(it)
                    onDismiss()
                },
            )
        }
        LibrarySheet.Resolution -> PleySheet(onDismiss = onDismiss, title = stringResource(R.string.library_resolution)) {
            Choices(
                all = stringResource(R.string.library_all_resolutions),
                values = state.resolutions,
                selected = params.resolution,
                label = { it.label },
                onSelect = {
                    vm.resolution(it)
                    onDismiss()
                },
            )
        }
        LibrarySheet.DynamicRange -> PleySheet(onDismiss = onDismiss, title = stringResource(R.string.library_dynamic_range)) {
            val resources = LocalResources.current
            Choices(
                all = stringResource(R.string.library_all_dynamic_ranges),
                values = state.dynamicRanges,
                selected = params.dynamicRange,
                label = { it.labelText.resolve(resources) },
                onSelect = {
                    vm.dynamicRange(it)
                    onDismiss()
                },
            )
        }
    }
}

@Composable
private fun <T : Any> Choices(all: String, values: List<T>, selected: T?, label: (T) -> String, onSelect: (T?) -> Unit) {
    LazyColumn(Modifier.fillMaxWidth().heightIn(max = 520.dp)) {
        item(key = "all", contentType = "choice") { ChoiceRow(all, selected = selected == null, onClick = { onSelect(null) }) }
        items(values, key = { it.toString() }, contentType = { "choice" }) { value ->
            ChoiceRow(label(value), selected = value == selected, onClick = { onSelect(value) })
        }
    }
}

/** One choice: its label, and a check when it is the one in force. Screen readers hear a radio button. */
@Composable
private fun ChoiceRow(label: String, selected: Boolean, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(48.dp)
            .selectable(selected = selected, role = Role.RadioButton, onClick = onClick)
            .padding(horizontal = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge,
            color = if (selected) PleyColors.Fg else PleyColors.Muted,
            modifier = Modifier.weight(1f),
        )
        if (selected) Icon(PleyIcons.Check, contentDescription = null, size = 22.dp, tint = PleyColors.Accent)
    }
}

@Composable
private fun SheetSection(title: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp)) {
        Text(title, style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted, modifier = Modifier.padding(bottom = 8.dp))
        content()
    }
}
