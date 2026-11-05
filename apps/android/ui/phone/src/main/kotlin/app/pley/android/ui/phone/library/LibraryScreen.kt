package app.pley.android.ui.phone.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRowDefaults
import androidx.compose.material3.TabRowDefaults.tabIndicatorOffset
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.library.LibraryUiState
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

/**
 * A library's page on the phone (design spec §4.2, mockup phone-library):
 * the library's name and Search in the top bar, the Recommended, Library
 * and Collections tabs, then the tab's content. The tab, sort and
 * filters are kept with the page, so they come back after process death,
 * and on the device for each library and profile, so the library opens
 * again on the tab it was left on.
 */
@Composable
internal fun LibraryScreen(key: PleyKey.Library) {
    var saved by rememberSaveable { mutableStateOf<String?>(null) }
    val tiles = rememberPhoneTiles()
    val vm: LibraryViewModel<ArtworkTileModel> = viewModel {
        LibraryViewModel(key.libraryId, key.tab, saved, LibraryDeps.of(screensGraph()), tiles, key.view)
    }
    LaunchedEffect(vm) {
        vm.state.map { it.params.toQueryString() }.distinctUntilChanged().collect { saved = it }
    }
    val state by vm.state.collectAsStateWithLifecycle()
    LibraryPage(state, vm)
}

@Composable
private fun LibraryPage(state: LibraryUiState<ArtworkTileModel>, vm: LibraryViewModel<ArtworkTileModel>) {
    val navigator = LocalPhoneNavigator.current
    val skeleton = rememberSkeleton(animated = true)
    Column(Modifier.fillMaxSize()) {
        PleyTopAppBar(
            title = state.library?.name.orEmpty(),
            onBack = navigator::back,
            actions = {
                PleyIconButton(PleyIcons.Search, stringResource(R.string.library_search), onClick = { navigator.open(PleyKey.Search()) })
            },
        )
        when {
            state.missing -> EmptyState(
                title = stringResource(R.string.library_missing_title),
                body = stringResource(R.string.library_missing_body),
                icon = PleyIcons.FolderSearch,
            )
            state.libraryError != null -> ErrorState(stringResource(R.string.library_failed), state.libraryError!!, onRetry = vm::retry)
            // The tab the library was left on is read first: no other flashes before it.
            state.openedOn == null -> Unit
            else -> {
                // Saved tiles shown while the server is out of reach raise the offline banner.
                ReportLoad(
                    when (state.params.tab) {
                        LibraryTab.Recommended -> state.recommended
                        LibraryTab.Library -> state.grid.cells
                        LibraryTab.Collections -> state.collections
                    },
                )
                LibraryTabs(state.tabs, state.params.tab, onSelect = vm::select)
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    when (state.params.tab) {
                        LibraryTab.Recommended -> RecommendedPane(state, vm, skeleton)
                        LibraryTab.Library -> LibraryPane(state, vm, skeleton)
                        LibraryTab.Collections -> CollectionsPane(state.collections, state.kind, onRetry = vm::retry, skeleton = skeleton)
                    }
                }
            }
        }
    }
}

/**
 * The tabs (design spec §4.2): Material's primary tabs from the page's
 * margin, the current one in [fg][PleyColors.Fg] over a 3 dp accent
 * indicator, the rest [muted][PleyColors.Muted], over a hairline.
 */
@Composable
private fun LibraryTabs(tabs: List<LibraryTab>, selected: LibraryTab, onSelect: (LibraryTab) -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val index = tabs.indexOf(selected).coerceAtLeast(0)
    PrimaryScrollableTabRow(
        selectedTabIndex = index,
        modifier = Modifier.fillMaxWidth(),
        containerColor = PleyColors.Bg,
        contentColor = PleyColors.Fg,
        edgePadding = (metrics.marginStart - 16.dp).coerceAtLeast(0.dp),
        minTabWidth = 0.dp,
        indicator = {
            TabRowDefaults.PrimaryIndicator(
                modifier = Modifier.tabIndicatorOffset(index, matchContentSize = true),
                width = androidx.compose.ui.unit.Dp.Unspecified,
                height = 3.dp,
                color = PleyColors.Accent,
                shape = PleyShape.Control,
            )
        },
        divider = { HorizontalDivider(Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd), thickness = 1.dp, color = PleyColors.Surface4) },
    ) {
        tabs.forEach { tab ->
            Tab(
                selected = tab == selected,
                onClick = { onSelect(tab) },
                selectedContentColor = PleyColors.Fg,
                unselectedContentColor = PleyColors.Muted,
                text = { Text(stringResource(tabLabel(tab)), style = MaterialTheme.typography.titleMedium, maxLines = 1) },
            )
        }
    }
}

internal fun tabLabel(tab: LibraryTab): Int = when (tab) {
    LibraryTab.Recommended -> R.string.library_tab_recommended
    LibraryTab.Library -> R.string.library_tab_library
    LibraryTab.Collections -> R.string.library_tab_collections
}
