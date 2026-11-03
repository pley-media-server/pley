package app.pley.android.ui.tv.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.common.sentenceOf
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.HubKeys
import app.pley.android.model.LibraryKind
import app.pley.android.model.SortOrder
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.library.LibraryUiState
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.library.TileRow
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.tileActions
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.SeeAll
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.focusRow
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * A page or tab with nothing to show (design spec §8): the title in
 * tvSection, the sentence under it in tvBody [muted][PleyColors.Muted], and
 * the one thing to do ("Retry", "Clear filters") as a pill with focus.
 * Without one, focus goes to [focusInstead] (the page's selected tab), else
 * to the rail, so the remote is never dead (§5.2 rule 1); not when
 * [takeFocus] is false, on a tab chosen after the page opened, where focus
 * is already on the tabs.
 */
@Composable
internal fun TvMessage(
    title: String,
    body: String,
    action: String? = null,
    onAction: () -> Unit = {},
    top: Dp = 140.dp,
    takeFocus: Boolean = true,
    focusInstead: FocusRequester? = null,
) {
    if (action == null && takeFocus) {
        val rail = LocalTvRailFocus.current
        LaunchedEffect(Unit) {
            withFrameNanos { }
            if (focusInstead?.let { runCatching { it.requestFocus() }.isSuccess } != true) rail?.focusCurrent()
        }
    }
    Column(
        Modifier.fillMaxWidth().padding(start = PleyTv.Gutter, top = top, end = PleyTv.Gutter),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        BasicText(title, style = PleyType.Tv.Section.copy(color = PleyColors.Fg), modifier = Modifier.semantics { heading() })
        BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), modifier = Modifier.widthIn(max = 520.dp))
        if (action != null) {
            Spacer(Modifier.height(16.dp))
            TvPillButton(action, onClick = onAction, modifier = Modifier.initialFocus())
        }
    }
}

/** An error's sentence, capitalised and ended. */
internal fun sentence(error: Throwable): String =
    sentenceOf(error)

/**
 * The Recommended tab (the web's RecommendedTab): the library's hubs as
 * rows from y = 80, Continue Watching as stills and the rest as posters,
 * then the collections, whose "See all" opens the Collections tab. The
 * focused row's header comes up to y = 80 as focus moves down.
 */
@Composable
internal fun TvRecommendedTab(
    state: LibraryUiState<ArtworkTileModel>,
    vm: LibraryViewModel<ArtworkTileModel>,
    memory: FocusMemory,
    initial: Boolean,
    tabFocus: FocusRequester,
    modifier: Modifier = Modifier,
) {
    val navigator = LocalTvNavigator.current
    val actions = tileActions()
    val recommended = state.recommended
    val rows = recommended.valueOrNull
    when {
        rows == null && recommended is Loadable.Failed -> TvMessage(
            title = failedTitle(recommended.error, stringResource(R.string.tv_library_recommended_failed)),
            body = failedBody(recommended.error),
            action = stringResource(R.string.tv_library_retry),
            onAction = vm::retry,
            top = RowsTop,
        )
        rows == null -> SkeletonRows()
        rows.isEmpty() -> TvMessage(
            title = stringResource(R.string.tv_library_recommended_empty_title),
            body = stringResource(R.string.tv_library_recommended_empty_body),
            top = RowsTop,
            takeFocus = initial,
            focusInstead = tabFocus,
        )
        else -> {
            val collections = stringResource(R.string.tv_library_tab_collections)
            val seeAll = stringResource(R.string.tv_library_see_all)
            // A tile its actions took away (Remove from Continue Watching): focus comes back to its neighbour.
            memory.settle { key -> hasTile(rows, key) }
            TvRowColumn(
                modifier = modifier.fillMaxSize(),
                headerY = RowsTop,
                contentPadding = PaddingValues(top = RowsTop, bottom = RowsBottom),
            ) {
                rows.forEachIndexed { at, row ->
                    item(key = row.key, contentType = row.look) {
                        TvRow(
                            rowKey = row.key,
                            title = row.title ?: collections,
                            tiles = row.tiles,
                            kind = row.tileKind,
                            onOpen = { tile -> row.opens.getOrNull(row.tiles.indexOf(tile))?.let(navigator::open) },
                            onActions = if (row.key == TileRow.COLLECTIONS) {
                                null
                            } else {
                                { tile ->
                                    memory.mayLeave("${row.key}/${tile.id}", standIn(rows, row.key, tile.id))
                                    actions.open(tile.id, continueWatching = HubKeys.isContinueWatching(row.key))
                                }
                            },
                            memory = memory,
                            initialFocus = initial && at == 0,
                            seeAll = if (row.key == TileRow.COLLECTIONS) {
                                SeeAll(seeAll) { vm.select(LibraryTab.Collections) }
                            } else {
                                row.seeAll?.let { view -> SeeAll(seeAll) { vm.seeAll(view) } }
                            },
                        )
                    }
                }
            }
        }
    }
}

/** Where focus goes if tile [tileId] leaves row [rowKey]: the next tile, else the one before, else the next row's first. */
private fun standIn(rows: List<TileRow<ArtworkTileModel>>, rowKey: String, tileId: Long): String? {
    val at = rows.indexOfFirst { it.key == rowKey }
    val row = rows.getOrNull(at) ?: return null
    val i = row.tiles.indexOfFirst { it.id == tileId }
    val beside = row.tiles.getOrNull(i + 1) ?: row.tiles.getOrNull(i - 1)?.takeIf { i > 0 }
    if (beside != null) return "$rowKey/${beside.id}"
    val other = rows.getOrNull(at + 1) ?: rows.getOrNull(at - 1) ?: return null
    return other.tiles.firstOrNull()?.let { "${other.key}/${it.id}" }
}

/** Whether focus key [key] ("<row key>/<tile id>") is one of [rows]' tiles: looked up when asked, not a set made per answer. */
private fun hasTile(rows: List<TileRow<ArtworkTileModel>>, key: String): Boolean {
    val slash = key.lastIndexOf('/')
    if (slash < 0) return false
    val id = key.substring(slash + 1).toLongOrNull() ?: return false
    val rowKey = key.substring(0, slash)
    return rows.any { row -> row.key == rowKey && row.tiles.any { it.id == id } }
}

private val TileRow<ArtworkTileModel>.tileKind: TileKind
    get() = when (look) {
        TileLook.Landscape -> TileKind.Landscape
        TileLook.Square -> TileKind.Square
        else -> TileKind.Poster
    }

/** Two rows' still skeletons, while the tab's rows load (design spec §8). */
@Composable
private fun SkeletonRows() {
    val skeleton = rememberSkeleton(animated = false)
    Column(Modifier.padding(start = PleyTv.Gutter, top = RowsTop), verticalArrangement = Arrangement.spacedBy(PleyTv.RowGap)) {
        repeat(2) {
            Column {
                Box(Modifier.size(180.dp, 18.dp).skeleton(skeleton, PleyShape.Badge))
                Spacer(Modifier.height(PleyTv.HeaderToTiles + 8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.TileGap)) {
                    repeat(7) { Box(Modifier.size(TileKind.Poster.tvWidth, TileKind.Poster.tvHeight).skeleton(skeleton, PleyShape.TileTv)) }
                }
            }
        }
    }
}

/** The side panels the chips open. */
internal enum class TvPanel { Sort, Filter, Genre, Year }

/**
 * The side panel (design spec §2.7) of the chip pressed: the sorts and
 * the order; the filters (which titles to show, genre, year, Clear
 * filters, Save as…); or the genres or years. The choice in force has the
 * check and the focus. A choice applies at once; picking a sort, order,
 * genre or year closes the panel, and Back closes it too.
 */
@Composable
internal fun TvLibraryPanel(
    panel: TvPanel?,
    state: LibraryUiState<ArtworkTileModel>,
    vm: LibraryViewModel<ArtworkTileModel>,
    onPanel: (TvPanel?) -> Unit,
) {
    val navigator = LocalTvNavigator.current
    var last by remember { mutableStateOf(panel) }
    if (panel != null) last = panel
    val params = state.params
    val close = { onPanel(null) }
    TvSidePanel(visible = panel != null, onDismiss = close) {
        // The row of the choice in force, which takes focus however far down the list it is.
        val chosen = remember(last) { FocusRequester() }
        fun Modifier.chosenIf(on: Boolean): Modifier = if (on) focusRequester(chosen) else this
        when (last) {
            TvPanel.Sort -> PanelList(stringResource(R.string.tv_library_sort_by), chosen) {
                items(state.sorts, key = { it.name }, contentType = { PANEL_ROW }) { sort ->
                    val on = sort == params.sort
                    PanelRow(
                        stringResource(tvSortLabel(sort)),
                        checked = on,
                        onClick = {
                            vm.sortBy(sort)
                            close()
                        },
                        modifier = Modifier.chosenIf(on),
                    )
                }
                item(key = "order", contentType = PANEL_HEADING) { PanelHeading(stringResource(R.string.tv_library_order)) }
                items(SortOrder.entries, key = { it.name }, contentType = { PANEL_ROW }) { order ->
                    PanelRow(
                        stringResource(if (order == SortOrder.Ascending) R.string.tv_library_ascending else R.string.tv_library_descending),
                        checked = order == params.effectiveOrder,
                        onClick = {
                            vm.order(order)
                            close()
                        },
                    )
                }
            }
            TvPanel.Filter -> PanelList(stringResource(R.string.tv_library_filter), chosen) {
                item(key = "show", contentType = PANEL_HEADING) { PanelHeading(stringResource(R.string.tv_library_show)) }
                val shows = listOf(null to R.string.tv_library_show_all, false to R.string.tv_library_unwatched, true to R.string.tv_library_watched)
                items(shows, key = { "show/${it.first}" }, contentType = { PANEL_ROW }) { (watched, label) ->
                    val on = params.watched == watched
                    PanelRow(stringResource(label), checked = on, onClick = { vm.watched(watched) }, modifier = Modifier.chosenIf(on))
                }
                if (state.kind != LibraryKind.Music) {
                    item(key = "genre", contentType = PANEL_ROW) {
                        PanelRow(
                            stringResource(R.string.tv_library_genre),
                            value = params.genre ?: stringResource(R.string.tv_library_all_genres),
                            opens = true,
                            onClick = { onPanel(TvPanel.Genre) },
                        )
                    }
                    item(key = "year", contentType = PANEL_ROW) {
                        PanelRow(
                            stringResource(R.string.tv_library_year),
                            value = params.year?.toString() ?: stringResource(R.string.tv_library_all_years),
                            opens = true,
                            onClick = { onPanel(TvPanel.Year) },
                        )
                    }
                }
                if (params.filtered) {
                    item(key = "clear", contentType = PANEL_ROW) {
                        PanelRow(stringResource(R.string.tv_library_clear_filters), onClick = {
                            vm.clearFilters()
                            close()
                        })
                    }
                    item(key = "save", contentType = PANEL_ROW) {
                        PanelRow(stringResource(R.string.tv_library_save_as), onClick = {
                            close()
                            navigator.open(vm.saveAs())
                        })
                    }
                }
            }
            TvPanel.Genre -> PanelList(stringResource(R.string.tv_library_genre), chosen) {
                // A genre chosen that the list no longer has: focus starts on All genres.
                val listed = params.genre != null && params.genre in state.filters.genres
                item(key = "all", contentType = PANEL_ROW) {
                    PanelRow(
                        stringResource(R.string.tv_library_all_genres),
                        checked = params.genre == null,
                        onClick = {
                            vm.genre(null)
                            close()
                        },
                        modifier = Modifier.chosenIf(!listed),
                    )
                }
                items(state.filters.genres, key = { "g/$it" }, contentType = { PANEL_ROW }) { genre ->
                    val on = genre == params.genre
                    PanelRow(genre, checked = on, onClick = {
                        vm.genre(genre)
                        close()
                    }, modifier = Modifier.chosenIf(on))
                }
            }
            TvPanel.Year -> PanelList(stringResource(R.string.tv_library_year), chosen) {
                val listed = params.year != null && params.year in state.filters.years
                item(key = "all", contentType = PANEL_ROW) {
                    PanelRow(
                        stringResource(R.string.tv_library_all_years),
                        checked = params.year == null,
                        onClick = {
                            vm.year(null)
                            close()
                        },
                        modifier = Modifier.chosenIf(!listed),
                    )
                }
                items(state.filters.years, key = { "y/$it" }, contentType = { PANEL_ROW }) { year ->
                    val on = year == params.year
                    PanelRow(year.toString(), checked = on, onClick = {
                        vm.year(year)
                        close()
                    }, modifier = Modifier.chosenIf(on))
                }
            }
            null -> Unit
        }
    }
}

/**
 * A panel's title and its rows; [chosen] (the choice in force) takes
 * focus once it is laid out, the list scrolling down to it when it is
 * past the first screen ([focusRow]): a lazy list composes only what
 * shows, so a requester further down isn't there to ask yet.
 */
@Composable
internal fun PanelList(title: String, chosen: FocusRequester, content: LazyListScope.() -> Unit) {
    val list = rememberLazyListState()
    LaunchedEffect(list, chosen) { list.focusRow(chosen) }
    Column(Modifier.fillMaxSize()) {
        BasicText(
            title,
            style = PleyType.Tv.Title.copy(color = PleyColors.Fg),
            modifier = Modifier.padding(start = 16.dp, top = 8.dp, bottom = 12.dp).semantics { heading() },
        )
        LazyColumn(Modifier.fillMaxWidth().weight(1f), state = list, verticalArrangement = Arrangement.spacedBy(2.dp), content = content)
    }
}

/** The content types of a panel's list: its rows and its headings. */
private const val PANEL_ROW = "row"
private const val PANEL_HEADING = "heading"

/** Where the Recommended tab's rows begin, and their focused row's header line. */
private val RowsTop = 80.dp

/** Room under the last row, so it too comes up to [RowsTop]. */
private val RowsBottom = 180.dp
