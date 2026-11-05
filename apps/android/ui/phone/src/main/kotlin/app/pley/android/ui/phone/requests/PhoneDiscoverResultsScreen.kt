package app.pley.android.ui.phone.requests

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.summary
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.model.DiscoverTitle
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.requests.DiscoverResultsUiState
import app.pley.android.screens.requests.DiscoverResultsViewModel
import app.pley.android.screens.requests.DiscoverSearch
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.opens
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ItemGrid
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.fullWidth
import app.pley.android.ui.phone.kit.rememberItemGridState
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter

/**
 * The See all of Search's Discover row: every
 * title TMDB finds for the words, with its status badge, in a grid under
 * "Results for “dune”", read on as it scrolls near its end. Back goes back
 * to Search, where the words are typed. When requests can't be used, or
 * TMDB can't be searched, the page says why.
 */
@Composable
internal fun PhoneDiscoverResultsScreen(key: PleyKey.DiscoverResults) {
    val vm: DiscoverResultsViewModel = viewModel { DiscoverResultsViewModel(key.query, RequestsDeps.of(screensGraph())) }
    val state by vm.state.collectAsStateWithLifecycle()
    DiscoverResultsContent(state, rememberRequestTexts(), onMore = vm::more, onRetry = vm::retry)
}

@Composable
private fun DiscoverResultsContent(state: DiscoverResultsUiState, texts: RequestTexts, onMore: () -> Unit, onRetry: () -> Unit) {
    val navigator = LocalPhoneNavigator.current
    val grid = rememberItemGridState()
    Column(Modifier.fillMaxSize().testTag(DISCOVER_RESULTS_PAGE)) {
        PleyTopAppBar(
            title = stringResource(R.string.requests_discover),
            onBack = navigator::back,
            scrolled = { grid.canScrollBackward },
        )
        val failed = stringResource(R.string.requests_search_failed)
        val problem = state.problem ?: (state.search as? DiscoverSearch.Failed)?.problem
        when {
            problem != null -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                RequestProblemState(problem, state.gate.summary, texts, failed, onRetry = onRetry)
            }
            state.search is DiscoverSearch.Empty -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                EmptyState(
                    title = stringResource(R.string.requests_search_none, state.query),
                    body = stringResource(R.string.requests_search_none_body),
                    icon = PleyIcons.Search,
                )
            }
            else -> ResultsGrid(state, texts, grid, open = { navigator.open(opens(it)) }, onMore = onMore)
        }
    }
}

/** TMDB's titles under "Results for “dune”", a spinner beside it while the next page is read; skeleton cards until the first comes. */
@Composable
private fun ResultsGrid(state: DiscoverResultsUiState, texts: RequestTexts, grid: LazyGridState, open: (DiscoverTitle) -> Unit, onMore: () -> Unit) {
    val skeleton = rememberSkeleton(animated = true)
    val pages = (state.search as? DiscoverSearch.Ready)?.pages
    val titles = pages?.titles
    val tiles = remember(titles, state.origin, texts) { titles?.map { titleTile(it, state.origin, texts) } }
    LaunchedEffect(grid) {
        snapshotFlow {
            val info = grid.layoutInfo
            info.totalItemsCount > 0 && (info.visibleItemsInfo.lastOrNull()?.index ?: 0) >= info.totalItemsCount - GRID_AHEAD
        }
            .distinctUntilChanged()
            .filter { it }
            .collect { onMore() }
    }
    ItemGrid(
        items = tiles,
        kind = TileKind.Poster,
        onOpen = { tile -> titles?.firstOrNull { it.tileId == tile.id }?.let(open) },
        skeleton = skeleton,
        state = grid,
        modifier = Modifier.fillMaxSize().testTag(DISCOVER_RESULTS),
        header = {
            fullWidth("for") {
                Row(Modifier.heightIn(min = 32.dp).padding(bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(app.pley.android.ui.phone.R.string.search_results_for, state.query),
                        style = MaterialTheme.typography.titleLarge,
                        color = PleyColors.Fg,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false).semantics { heading() },
                    )
                    if (pages?.loading == true) {
                        Spacer(Modifier.width(8.dp))
                        DelayedSpinner(stringResource(app.pley.android.ui.phone.R.string.search_loading), size = 16.dp)
                    }
                }
            }
        },
    )
}

/** Cards from the grid's end at which the next page is asked for. */
private const val GRID_AHEAD = 9

internal const val DISCOVER_RESULTS_PAGE = "pley.discover.results.page"
internal const val DISCOVER_RESULTS = "pley.discover.results"
