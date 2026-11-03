package app.pley.android.ui.tv.search

import app.pley.android.ui.tv.shell.failedTitle
import app.pley.android.ui.tv.shell.failedBody
import androidx.compose.foundation.focusGroup
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.ItemSummary
import app.pley.android.model.LiveChannel
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.TileFactory
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.screensGraph
import app.pley.android.screens.search.SearchDeps
import app.pley.android.screens.search.SearchGroupKind
import app.pley.android.screens.search.SearchResultsState
import app.pley.android.screens.search.SearchTiles
import app.pley.android.screens.search.SearchUiState
import app.pley.android.screens.search.SearchViewModel
import app.pley.android.ui.tv.R

import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.railFade
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.livetv.ChannelTileCard
import app.pley.android.ui.tv.library.TvChip
import app.pley.android.ui.tv.library.TvTiles
import app.pley.android.ui.tv.shell.LocalTvNavigator
import kotlinx.coroutines.delay

/**
 * Search on TV (design spec §4.5, mockup tv-search): the field at the
 * top, 520 wide, with initial focus; Select opens the system keyboard,
 * which has voice input, and Back closes it. Before typing, the recent
 * searches as chips. While typing (250 ms after the last key), the
 * results as rows, grouped (Movies, Shows, Seasons, Episodes, Artists,
 * Albums, Tracks, Live TV, People), the first header at y = 96; a
 * channel found plays at once. Down from the
 * field goes into the rows, Up from the first row back to it; Left goes
 * to the rail.
 */
@Composable
internal fun TvSearchScreen(key: PleyKey.Search) {
    val resources = LocalResources.current
    val vm: SearchViewModel<ArtworkTileModel> = viewModel {
        SearchViewModel(key.query, SearchDeps.of(screensGraph()), tvSearchTiles(TvTiles(resources)))
    }
    val state by vm.state.collectAsStateWithLifecycle()
    val field = rememberTextFieldState(key.query)
    LaunchedEffect(field, vm) {
        snapshotFlow { field.text.toString() }.collect(vm::onQueryChange)
    }
    val navigator = LocalTvNavigator.current
    TvSearchContent(
        state = state,
        field = field,
        onSearch = vm::submit,
        onRecent = { q ->
            field.setTextAndPlaceCursorAtEnd(q)
            vm.searchAgain(q)
        },
        onClearRecent = vm::clearRecent,
        onOpen = { group, id -> vm.open(group, id)?.let(navigator::open) },
        onActions = { id -> navigator.open(PleyKey.ItemActions(id)) },
        onRetry = vm::retry,
        onWatch = vm::watch,
    )
}

/**
 * The TV's search tiles: posters at their row size (not the library
 * grid's), stills, squares, and people as circles with their photo.
 */
internal fun tvSearchTiles(library: TvTiles): SearchTiles<ArtworkTileModel> {
    val items = object : TileFactory<ArtworkTileModel> by library {
        override fun item(item: ItemSummary, look: TileLook, origin: String): ArtworkTileModel = when (look) {
            TileLook.Poster -> ArtworkTileModel.of(item, TileKind.Poster, origin)
            else -> library.item(item, look, origin)
        }
    }
    return SearchTiles.of(items) { person, origin ->
        ArtworkTileModel(id = person.id, title = person.name, image = ArtworkImage.of(person.photoUrl, origin), description = person.name)
    }
}

@Composable
internal fun TvSearchContent(
    state: SearchUiState<ArtworkTileModel>,
    field: TextFieldState,
    onSearch: () -> Unit,
    onRecent: (String) -> Unit,
    onClearRecent: () -> Unit,
    onOpen: (SearchGroupKind, Long) -> Unit,
    onActions: (Long) -> Unit,
    onRetry: () -> Unit = {},
    onWatch: (LiveChannel) -> Unit = {},
) {
    val rail = LocalTvRailFocus.current
    val memory = rememberFocusMemory()
    val fieldFocus = remember { FocusRequester() }
    // What Down from the field goes to: the chips or the rows (the field is wide; nearest would be a guess).
    val below = remember { FocusRequester() }
    val hasBelow = (state.results is SearchResultsState.Ready) || (state.results is SearchResultsState.Failed) ||
        (state.results == SearchResultsState.Idle && state.recent.isNotEmpty())
    Box(Modifier.fillMaxSize().testTag(TV_SEARCH)) {
        when (val results = state.results) {
            SearchResultsState.Idle -> Recent(
                recent = state.recent,
                focus = below,
                onRecent = { q ->
                    // The chips go once the results come: focus waits on the field.
                    runCatching { fieldFocus.requestFocus() }
                    onRecent(q)
                },
                onClear = {
                    runCatching { fieldFocus.requestFocus() }
                    onClearRecent()
                },
            )
            is SearchResultsState.Loading -> Skeletons()
            is SearchResultsState.Empty -> Notice(
                stringResource(R.string.tv_search_none_title, results.query),
                stringResource(R.string.tv_search_none_body),
            )
            is SearchResultsState.Failed -> Notice(
                failedTitle(results.error, stringResource(R.string.tv_search_failed)),
                failedBody(results.error),
            ) {
                // Down from the field; focus stays on the field until then.
                TvPillButton(stringResource(R.string.tv_search_retry), onClick = onRetry, icon = PleyIcons.RotateCcw, modifier = Modifier.focusRequester(below))
            }
            is SearchResultsState.Ready -> {
                val names = groupNames()
                TvRowColumn(
                    modifier = Modifier.fillMaxSize().focusRequester(below).testTag(TV_SEARCH_RESULTS),
                    contentPadding = PaddingValues(top = ResultsTop, bottom = ResultsBottomRoom),
                    headerY = ResultsTop,
                ) {
                    results.groups.forEach { group ->
                        // Live TV's row comes after the titles and before People, as on the web.
                        if (group.kind == SearchGroupKind.People && results.channels.isNotEmpty()) {
                            channelRow(results, memory, onWatch)
                        }
                        val kind = group.kind.tvTileKind
                        item(key = group.kind.name, contentType = kind) {
                            TvRow(
                                rowKey = group.kind.name,
                                title = names.getValue(group.kind),
                                tiles = group.tiles,
                                kind = kind,
                                onOpen = { onOpen(group.kind, it.id) },
                                onActions = if (group.kind == SearchGroupKind.People) null else { tile -> onActions(tile.id) },
                                memory = memory,
                            )
                        }
                    }
                    if (results.groups.none { it.kind == SearchGroupKind.People } && results.channels.isNotEmpty()) {
                        channelRow(results, memory, onWatch)
                    }
                }
            }
        }
        // The field, over the rows that scroll up under it.
        Row(
            Modifier.padding(start = PleyTv.Gutter, top = TvSafeArea.Vertical),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.railOnLeftStopOnRight(rail, stopRight = true).focusGroup()) {
                TvSearchField(
                    state = field,
                    placeholder = stringResource(R.string.tv_search_field),
                    onSearch = onSearch,
                    modifier = Modifier.width(FieldWidth).testTag(TV_SEARCH_FIELD),
                    focusModifier = Modifier
                        .focusRequester(fieldFocus)
                        .focusProperties { down = if (hasBelow) below else FocusRequester.Default }
                        .initialFocus(memory),
                )
            }
            val loading = (state.results as? SearchResultsState.Ready)?.loading == true || state.results is SearchResultsState.Loading
            if (loading) {
                Spacer(Modifier.width(20.dp))
                BasicText(stringResource(R.string.tv_search_loading), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted))
            }
        }
    }
}

private fun androidx.compose.foundation.lazy.LazyListScope.channelRow(
    results: SearchResultsState.Ready<ArtworkTileModel>,
    memory: FocusMemory,
    onWatch: (LiveChannel) -> Unit,
) {
    item(key = CHANNELS_KEY, contentType = CHANNELS_KEY) {
        ChannelRow(results.channels, results.origin, memory, onWatch)
    }
}

/**
 * The Live TV channels found (the web's ChannelResults), as Live TV's
 * channel tiles: the logo with the number and how far what's on is,
 * captioned with the name and what's on. Select plays the channel.
 */
@Composable
private fun ChannelRow(channels: List<LiveChannel>, origin: String, memory: FocusMemory, onWatch: (LiveChannel) -> Unit) {
    val rail = LocalTvRailFocus.current
    val now by produceState(System.currentTimeMillis() / 1000) {
        while (true) {
            delay(NOW_TICK_MS)
            value = System.currentTimeMillis() / 1000
        }
    }
    Column(Modifier.fillMaxWidth().focusGroup()) {
        BasicText(
            stringResource(R.string.tv_search_group_live),
            style = PleyType.Tv.Section.copy(color = PleyColors.Fg),
            maxLines = 1,
            modifier = Modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal).semantics { heading() },
        )
        Spacer(Modifier.height(PleyTv.HeaderToTiles - ChannelBleed))
        LazyRow(
            modifier = Modifier
                .fillMaxWidth()
                .testTag(TV_SEARCH_CHANNELS)
                .railOnLeftStopOnRight(rail)
                .focusRestorer(),
            contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal + TileKind.Episode.tvWidth, top = ChannelBleed, bottom = ChannelBleed),
            horizontalArrangement = Arrangement.spacedBy(TileKind.Episode.tvGap),
        ) {
            items(channels, key = { it.id }, contentType = { "channel" }) { channel ->
                ChannelTileCard(
                    channel = channel,
                    name = channel.name,
                    origin = origin,
                    now = now,
                    focusModifier = Modifier.rememberedFocus(memory, "$CHANNELS_KEY/${channel.id}"),
                    onWatch = onWatch,
                    onDetails = null,
                    modifier = Modifier.testTag(TV_SEARCH_CHANNEL + channel.id),
                )
            }
        }
    }
}

/** Before typing: "Recent searches" and a chip for each, with Clear; or what search finds. */
@Composable
private fun Recent(recent: List<String>, focus: FocusRequester, onRecent: (String) -> Unit, onClear: () -> Unit) {
    if (recent.isEmpty()) {
        Notice(stringResource(R.string.tv_search_empty_title), stringResource(R.string.tv_search_empty_body))
        return
    }
    val rail = LocalTvRailFocus.current
    Column(Modifier.fillMaxWidth().padding(top = ResultsTop).testTag(TV_SEARCH_RECENT)) {
        BasicText(
            stringResource(R.string.tv_search_recent),
            style = PleyType.Tv.Section.copy(color = PleyColors.Fg),
            modifier = Modifier.padding(start = PleyTv.Gutter).semantics { heading() },
        )
        Spacer(Modifier.height(16.dp))
        LazyRow(
            modifier = Modifier.fillMaxWidth().railFade().focusRequester(focus).railOnLeftStopOnRight(rail).focusGroup(),
            contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = 6.dp, bottom = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            items(recent, key = { it }, contentType = { "recent" }) { q ->
                TvChip(label = q, onClick = { onRecent(q) }, icon = PleyIcons.History, modifier = Modifier.widthIn(max = 320.dp))
            }
            item(key = "pley.clear", contentType = "clear") {
                TvTextButton(stringResource(R.string.tv_search_recent_clear), onClick = onClear)
            }
        }
    }
}

/** `No results for "north"`, "Search failed": at the gutter, y = 120, in tvTitle over tvBody. */
@Composable
private fun Notice(title: String, body: String, action: (@Composable () -> Unit)? = null) {
    Column(
        Modifier.padding(start = PleyTv.Gutter, top = NoticeTop, end = TvSafeArea.Horizontal).widthIn(max = 560.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), modifier = Modifier.semantics { heading() })
        BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
        if (action != null) {
            Spacer(Modifier.height(12.dp))
            action()
        }
    }
}

/** Two rows of still skeleton posters while the first answer comes (no shimmer on TV). */
@Composable
private fun Skeletons() {
    val skeleton = rememberSkeleton(animated = false)
    Column(Modifier.padding(start = PleyTv.Gutter, top = ResultsTop), verticalArrangement = Arrangement.spacedBy(PleyTv.RowGap)) {
        repeat(2) {
            Column {
                Box(Modifier.size(140.dp, 22.dp).skeleton(skeleton, PleyShape.Badge))
                Spacer(Modifier.height(PleyTv.HeaderToTiles))
                Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.TileGap)) {
                    repeat(7) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Tv) }
                }
            }
        }
    }
}

@Composable
private fun groupNames(): Map<SearchGroupKind, String> {
    val people = stringResource(R.string.tv_search_group_people)
    val movies = stringResource(R.string.tv_search_group_movies)
    val shows = stringResource(R.string.tv_search_group_shows)
    val seasons = stringResource(R.string.tv_search_group_seasons)
    val episodes = stringResource(R.string.tv_search_group_episodes)
    val artists = stringResource(R.string.tv_search_group_artists)
    val albums = stringResource(R.string.tv_search_group_albums)
    val tracks = stringResource(R.string.tv_search_group_tracks)
    return remember(people) {
        mapOf(
            SearchGroupKind.People to people,
            SearchGroupKind.Movies to movies,
            SearchGroupKind.Shows to shows,
            SearchGroupKind.Seasons to seasons,
            SearchGroupKind.Episodes to episodes,
            SearchGroupKind.Artists to artists,
            SearchGroupKind.Albums to albums,
            SearchGroupKind.Tracks to tracks,
        )
    }
}

/** How a group's tiles are drawn on TV: people as circles, posters at their row size. */
internal val SearchGroupKind.tvTileKind: TileKind
    get() = if (this == SearchGroupKind.People) {
        TileKind.Person
    } else {
        when (look) {
            TileLook.Landscape -> TileKind.Landscape
            TileLook.Square -> TileKind.Square
            else -> TileKind.Poster
        }
    }

/** The first row's header (design spec §4.5). */
private val ResultsTop = 96.dp

/**
 * Room under the last row, so it too can come up to the header line
 * (y = 96): the screen, less the line and the shortest row (stills, with
 * their header and caption).
 */
private val ResultsBottomRoom = PleyTv.CanvasHeight - ResultsTop -
    (PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles + TileKind.Landscape.tvHeight + PleyTv.TilesToCaption + PleyTv.CaptionHeight)

/** Where "No results" sits. */
private val NoticeTop = 120.dp

private val FieldWidth = 520.dp

internal const val TV_SEARCH = "pley.tv.search"
internal const val TV_SEARCH_FIELD = "pley.tv.search.field"
internal const val TV_SEARCH_RESULTS = "pley.tv.search.results"
internal const val TV_SEARCH_RECENT = "pley.tv.search.recent"
internal const val TV_SEARCH_CHANNELS = "pley.tv.search.channels"
internal const val TV_SEARCH_CHANNEL = "pley.tv.search.channel."

/** The Live TV row's key among the results' rows. */
private const val CHANNELS_KEY = "Channels"

/** Room above and below the channel tiles for a focused tile's growth and glow. */
private val ChannelBleed = 16.dp

/** How often what's on a channel found is looked at again. */
private const val NOW_TICK_MS = 30_000L
