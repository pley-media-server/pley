package app.pley.android.ui.tv.requests

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.summary
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.DiscoverResultsUiState
import app.pley.android.screens.requests.DiscoverResultsViewModel
import app.pley.android.screens.requests.DiscoverSearch
import app.pley.android.screens.requests.DiscoverSource
import app.pley.android.screens.requests.Pages
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvSeeAllTile
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.library.TvGridFocus
import app.pley.android.ui.tv.library.TvPosterGrid
import app.pley.android.ui.tv.library.tvGridColumns
import app.pley.android.ui.tv.shell.LocalTvNavigator

/**
 * A grid of TMDB's titles on TV (discover.tsx's TitlesPage): the See all
 * of a Discover row, the whole list under its name, or of Search's
 * Discover row, every title TMDB finds under "Results for “dune”", each
 * with its badge. It reads four rows' worth first, five pages at most
 * (for a profile with an age limit, page after page can come empty), then
 * three rows more as focus comes within two rows of its end, with still
 * wells while they come. At the end, Show more once five pages brought
 * too few, which reads five more, or Retry after a page failed. Back goes
 * back to the page it came from.
 */
@Composable
internal fun TvDiscoverGridScreen(source: DiscoverSource) {
    val screenful = tvGridColumns(GridKind) * FIRST_ROWS
    TvDiscoverGridScreen(
        viewModel { DiscoverResultsViewModel(source, RequestsDeps.of(screensGraph()), screenful, DiscoverResultsViewModel.TV_FILL) },
    )
}

@Composable
internal fun TvDiscoverGridScreen(vm: DiscoverResultsViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val texts = rememberRequestTexts()
    // Back on the page, what failed is asked again.
    var shownBefore by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        if (shownBefore && state.search is DiscoverSearch.Failed) vm.retry()
        shownBefore = true
    }
    val title = state.list?.let(texts::row) ?: stringResource(R.string.tv_requests_results_for, state.query)
    val failed = stringResource(if (state.list != null) R.string.tv_requests_list_failed else ScreensR.string.requests_search_failed)
    Box(Modifier.fillMaxSize().testTag(TV_DISCOVER_GRID)) {
        val search = state.search
        val problem = state.problem ?: (search as? DiscoverSearch.Failed)?.problem
        when {
            problem != null -> TvRequestMessage(problem, state.gate.summary, texts, failed, state.origin, onRetry = vm::retry)
            search is DiscoverSearch.Empty -> TvRequestsNotice(
                title = if (state.list != null) stringResource(R.string.tv_requests_nothing_title) else stringResource(ScreensR.string.requests_search_none, state.query),
                body = stringResource(if (state.list != null) R.string.tv_requests_list_nothing_body else ScreensR.string.requests_search_none_body),
                icon = if (state.list != null) PleyIcons.Compass else PleyIcons.Search,
            )
            search is DiscoverSearch.Ready -> TitleGrid(state, search.pages, texts, failed, vm)
            else -> GridSkeleton()
        }
        BasicText(
            title,
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .padding(start = PleyTv.Gutter, top = TvSafeArea.Vertical, end = TvSafeArea.Horizontal)
                .semantics { heading() },
        )
    }
}

/**
 * The titles as posters; while more come, still wells to the end of the
 * row (the first of them can hold focus), and at the end Show more or
 * Retry. Beside a lone Show more or Retry, why there are no titles yet.
 */
@Composable
private fun TitleGrid(state: DiscoverResultsUiState, pages: Pages, texts: RequestTexts, failed: String, vm: DiscoverResultsViewModel) {
    val navigator = LocalTvNavigator.current
    val memory = rememberFocusMemory()
    val skeleton = rememberSkeleton(animated = false)
    val focus = remember { TvGridFocus() }
    val columns = tvGridColumns(GridKind)
    val titles = pages.titles
    val tiles = remember(titles, state.origin, texts) { titles.map { titleTile(it, state.origin, texts) } }
    val coming = !pages.settled
    val end = !coming && (pages.stopped || pages.problem != null)
    val count = if (coming) ((titles.size + 1 + columns - 1) / columns) * columns else titles.size
    // Show more or Retry goes while the pages come: the well in its place takes focus, and keeps it as its title comes.
    var refocus by remember { mutableIntStateOf(-1) }
    LaunchedEffect(refocus) {
        val at = refocus
        if (at < 0) return@LaunchedEffect
        for (frame in 0 until REFOCUS_FRAMES) {
            withFrameNanos { }
            if (focus.focus(at)) break
        }
        refocus = -1
    }
    // Two rows from the end: the next pages, until three rows more are in.
    val shown by rememberUpdatedState(titles.size)
    LaunchedEffect(focus, columns) {
        snapshotFlow { focus.focused }.collect { at ->
            if (at >= 0 && at >= shown - 2 * columns) vm.want(shown + 3 * columns)
        }
    }
    Box(Modifier.fillMaxSize().padding(top = GridTop).clipToBounds()) {
        TvPosterGrid(
            total = count,
            tileAt = { tiles.getOrNull(it) },
            // By place: a well that has focus as its title comes keeps it.
            keyAt = { it },
            kind = GridKind,
            onOpen = { i, _ -> titles.getOrNull(i)?.let { navigator.open(PleyKey.DiscoverTitle(it.kind, it.tmdbId)) } },
            memory = memory,
            memoryPrefix = GRID_MEMORY,
            skeleton = skeleton,
            focus = focus,
            columns = columns,
            footer = if (!end) {
                null
            } else {
                {
                    item(key = END_KEY, contentType = END_KEY) {
                        val retry = pages.problem != null
                        TvSeeAllTile(
                            kind = GridKind,
                            label = stringResource(if (retry) ScreensR.string.requests_retry else R.string.tv_requests_show_more),
                            icon = if (retry) PleyIcons.RefreshCw else PleyIcons.Plus,
                            onClick = {
                                refocus = titles.size
                                vm.showMore()
                            },
                            focusModifier = Modifier.rememberedFocus(memory, "$GRID_MEMORY/$END_KEY", initial = titles.isEmpty()),
                            modifier = Modifier.testTag(TV_DISCOVER_GRID_MORE),
                        )
                    }
                }
            },
        )
        if (titles.isEmpty() && end) Aside(state, pages, texts, failed)
    }
}

/** Beside a grid's lone Show more or Retry: why it has no titles yet. */
@Composable
private fun Aside(state: DiscoverResultsUiState, pages: Pages, texts: RequestTexts, failed: String) {
    val problem = pages.problem
    val title = when {
        problem != null -> problemTitle(problem, state.gate.summary, texts, failed)
        state.list != null -> stringResource(R.string.tv_requests_none_yet)
        else -> stringResource(R.string.tv_requests_search_none_yet, state.query)
    }
    val body = if (problem != null) problemBody(problem, state.gate.summary, texts) else stringResource(R.string.tv_requests_none_yet_body)
    Column(
        Modifier
            .padding(start = PleyTv.Gutter + GridKind.tvWidth + AsideGap, top = AsideTop, end = TvSafeArea.Horizontal)
            .widthIn(max = 520.dp)
            .semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
        if (body != null) {
            Spacer(Modifier.height(8.dp))
            BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
        }
    }
}

/** A grid's place while its first page comes: two rows of still wells. No spinner. */
@Composable
private fun GridSkeleton() {
    val skeleton = rememberSkeleton(animated = false)
    Column(Modifier.padding(start = PleyTv.Gutter, top = GridTop + 14.dp)) {
        repeat(2) { row ->
            if (row > 0) Spacer(Modifier.height(GridKind.tvHeight / 3))
            Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.TileGap)) {
                repeat(tvGridColumns(GridKind)) { SkeletonTile(skeleton, GridKind, TileStyle.Tv) }
            }
        }
    }
}

/** The grid's posters: the library grid's, seven columns. */
private val GridKind = TileKind.LibraryGrid

/** Rows read before the first answer shows whole. */
private const val FIRST_ROWS = 4

/** The grid under the page's title: it scrolls up to here. */
private val GridTop = 72.dp

/** Beside a lone Show more, level with the middle of its well. */
private val AsideTop = 68.dp
private val AsideGap = 32.dp

/** Frames the well in Show more's place is waited for. */
private const val REFOCUS_FRAMES = 10

private const val GRID_MEMORY = "grid"
private const val END_KEY = "pley.grid.more"

internal const val TV_DISCOVER_GRID = "pley.tv.discover.grid"
internal const val TV_DISCOVER_GRID_MORE = "pley.tv.discover.grid.more"
