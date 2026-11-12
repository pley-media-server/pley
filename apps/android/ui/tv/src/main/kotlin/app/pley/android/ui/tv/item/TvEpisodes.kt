package app.pley.android.ui.tv.item

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.screens.item.EpisodeLine
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.SeasonsSection
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.ROW_TAG
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvScrolling
import app.pley.android.ui.tv.kit.rememberRested
import app.pley.android.ui.tv.kit.rememberTvRowState
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlin.math.roundToInt

/**
 * The seasons and episodes (design spec §2.4, §4.4; mockup tv-show): the
 * season tabs, when the show has more than one season, then the season's
 * episodes as 208 by 117 tiles captioned "5. Glasshouse" and "26 min left".
 * Focus comes into the row on the episode to watch (or the page's own
 * episode, which is marked in the row: the one the page is about), and
 * stays at the gutter while the row moves. A tab chosen
 * with focus resting on it shows its season. A show whose seasons have
 * posters of their own ([SeasonsSection.posters]) shows them as a row of
 * poster tiles instead, as the web does, with the season shown named over
 * its episodes. Resting on a season shows its episodes under the row,
 * and Select goes down into them, on the episode to watch: the page has
 * them already, so nothing opens and nothing loads.
 */
@Composable
internal fun TvEpisodesSection(
    page: ItemPage,
    seasons: SeasonsSection,
    focus: ItemPageFocus,
    memory: FocusMemory,
    rowRequester: FocusRequester,
    seasonsRequester: FocusRequester,
    onSelectSeason: (Long) -> Unit,
    onFocusEpisode: (EpisodeLine) -> Unit,
    onOpen: (ItemSummary) -> Unit,
    onActions: (ItemSummary) -> Unit,
    onSeasonActions: (ItemSummary) -> Unit,
) {
    Column(Modifier.fillMaxWidth()) {
        // A season chosen on its poster: focus goes into its episodes once they are the ones shown,
        // unless it has gone elsewhere by then (up to the buttons, along to another season).
        var enter by remember { mutableStateOf<Long?>(null) }
        val inTabs = Modifier.onFocusChanged { if (it.hasFocus) focus.area = Area.Tabs else enter = null }
        LaunchedEffect(enter, seasons.selected, seasons.episodes) {
            val season = enter ?: return@LaunchedEffect
            val episodes = seasons.episodes ?: return@LaunchedEffect
            if (seasons.selected != season || episodes.any { it.item.parentId != season }) return@LaunchedEffect
            enter = null
            if (episodes.isNotEmpty()) runCatching { rowRequester.requestFocus() }
        }
        if (seasonPosterRow(page)) {
            val choose = { id: Long ->
                enter = id
                // Select before focus had rested there: shown now.
                if (id != seasons.selected) onSelectSeason(id)
            }
            SeasonPosters(
                seasons = seasons,
                origin = page.origin,
                memory = memory,
                onSelect = onSelectSeason,
                onChoose = choose,
                onActions = onSeasonActions,
                onFocusSeason = { id -> if (enter != id) enter = null },
                modifier = inTabs.focusRequester(seasonsRequester),
            )
            Spacer(Modifier.height(TabsToTiles))
        } else if (seasons.seasons.size > 1 && !seasons.posters) {
            SeasonTabs(seasons, onSelectSeason, inTabs)
            Spacer(Modifier.height(TabsToTiles))
        }
        if (seasons.seasons.size <= 1 || seasons.posters) {
            BasicText(
                seasons.selectedTab?.title ?: page.context?.seasonTitle ?: stringResource(R.string.tv_item_episodes),
                style = PleyType.Tv.Section.copy(color = PleyColors.Fg),
                modifier = Modifier.padding(start = PleyTv.Gutter),
            )
            Spacer(Modifier.height(PleyTv.HeaderToTiles))
        }
        val episodes = seasons.episodes
        when {
            episodes == null -> Box(Modifier.height(TileKind.Episode.tvHeight + PleyTv.TilesToCaption + PleyTv.CaptionHeight))
            episodes.isEmpty() -> BasicText(
                stringResource(R.string.tv_item_no_episodes),
                style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                modifier = Modifier.padding(start = PleyTv.Gutter).height(TileKind.Episode.tvHeight),
            )
            else -> key(seasons.selected) {
                EpisodeRow(
                    episodes = episodes,
                    origin = page.origin,
                    target = seasons.focus,
                    current = page.id.takeIf { page.kind == ItemKind.Episode },
                    memory = memory,
                    rowRequester = rowRequester,
                    modifier = Modifier.onFocusChanged { if (it.hasFocus) focus.area = Area.Episodes },
                    onFocusEpisode = onFocusEpisode,
                    onOpen = onOpen,
                    onActions = onActions,
                )
            }
        }
    }
}

/**
 * Whether the page shows its seasons as a row of posters: a show's page
 * does. A season's own page goes
 * without the row: it would stand between the buttons and the episodes
 * the season was opened for.
 */
internal fun seasonPosterRow(page: ItemPage): Boolean = page.seasons?.posters == true && page.kind != ItemKind.Season

/**
 * The season tabs (design spec §2.6): 36 dp pills, labels in [muted][PleyColors.Muted];
 * the shown season on [surface3][PleyColors.Surface3] in [fg][PleyColors.Fg];
 * the focused tab in [accent][PleyColors.Accent], at 1.05. Focus coming
 * into the tabs (Up from the episodes, Down from the buttons) lands on the
 * shown season's tab, as on the Samsung app, never on the tab nearest the
 * focus, which would show its season. A show with more tabs than the screen
 * holds scrolls them, each tab at its own width, just far enough to show the
 * focused one ([TvScrolling.inView]).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SeasonTabs(seasons: SeasonsSection, onSelect: (Long) -> Unit, modifier: Modifier = Modifier) {
    var focused by remember { mutableStateOf<Long?>(null) }
    val rested = rememberRested(focused, SEASON_REST_MS)
    LaunchedEffect(rested) {
        if (rested != null && rested != seasons.selected) onSelect(rested)
    }
    val selected = remember { FocusRequester() }
    val density = LocalDensity.current
    val scroll = rememberScrollState()
    val start = PleyTv.Gutter - TabInset
    val inView = remember(density, start) { with(density) { TvScrolling.inView(start.toPx(), TvSafeArea.Horizontal.toPx()) } }
    // The tab of the season shown is in sight as the tabs open: a long show lists its newest
    // season first, and opens on the season to watch. Once: after that focus moves the tabs.
    var shownEnd by remember { mutableIntStateOf(UNPLACED) }
    var opened by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        if (opened) return@LaunchedEffect
        val end = snapshotFlow { shownEnd }.first { it != UNPLACED }
        opened = true
        val past = with(density) { start.roundToPx() + end + TvSafeArea.Horizontal.roundToPx() } - scroll.viewportSize
        if (past > 0) scroll.scrollTo(past)
    }
    CompositionLocalProvider(LocalBringIntoViewSpec provides inView) {
        Row(
            modifier
                .fillMaxWidth()
                .focusProperties { onEnter = { runCatching { selected.requestFocus() } } }
                .focusGroup()
                .horizontalScroll(scroll)
                .padding(start = start, end = TvSafeArea.Horizontal)
                .testTag(TV_SEASON_TABS),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            seasons.seasons.forEach { season ->
                val current = season.id == seasons.selected
                SeasonTab(
                    title = season.title,
                    current = current,
                    onFocus = { focused = season.id },
                    onClick = { onSelect(season.id) },
                    modifier = if (current) {
                        Modifier.focusRequester(selected).onPlaced { shownEnd = (it.positionInParent().x + it.size.width).roundToInt() }
                    } else {
                        Modifier
                    },
                )
            }
        }
    }
}

@Composable
private fun SeasonTab(title: String, current: Boolean, onFocus: () -> Unit, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val scale by animateFloatAsState(
        if (focused) PleyFocus.PILL_SCALE else 1f,
        if (reducedMotion()) androidx.compose.animation.core.snap() else PleyMotion.focus(),
        label = "tab",
    )
    val fill = when {
        focused -> PleyColors.Accent
        current -> PleyColors.Surface3
        else -> androidx.compose.ui.graphics.Color.Transparent
    }
    val text = when {
        focused -> PleyColors.OnAccent
        current -> PleyColors.Fg
        else -> PleyColors.Muted
    }
    Box(
        modifier
            .onFocusChanged { if (it.isFocused) onFocus() }
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .heightIn(min = TabHeight)
            .background(fill, PleyShape.Control)
            .semantics { selected = current }
            .clickable(interactionSource = interactions, indication = null, role = Role.Tab, onClick = onClick)
            .padding(horizontal = TabInset),
        contentAlignment = Alignment.CenterStart,
    ) {
        BasicText(title, style = PleyType.Tv.Label.copy(color = text), softWrap = false, maxLines = 1)
    }
}

/**
 * The seasons as poster tiles, as the web shows them: each season's poster
 * (else the show's), its name and episodes under it, its unwatched count
 * on it, in a row of the TV kit ([TvRow]). As with the tabs, the season
 * focus rests on is shown under the row; Select [chooses][onChoose] it,
 * which takes focus down into its episodes, and holding it gives the
 * season's [actions][onActions]. [onFocusSeason] hears each season focus
 * comes to.
 */
@Composable
private fun SeasonPosters(
    seasons: SeasonsSection,
    origin: String,
    memory: FocusMemory,
    onSelect: (Long) -> Unit,
    onChoose: (Long) -> Unit,
    onActions: (ItemSummary) -> Unit,
    onFocusSeason: (Long) -> Unit,
    modifier: Modifier,
) {
    val words = formatWords()
    val tiles = remember(seasons.seasons, origin, words) {
        seasons.seasons.mapNotNull { season -> season.item?.let { ArtworkTileModel.of(it.copy(title = season.title), TileKind.Poster, origin, words) } }
    }
    val byId = remember(seasons.seasons) { seasons.seasons.mapNotNull { it.item }.associateBy { it.id } }
    var focused by remember { mutableStateOf<Long?>(null) }
    val rested = rememberRested(focused, SEASON_REST_MS)
    LaunchedEffect(rested) {
        if (rested != null && rested != seasons.selected) onSelect(rested)
    }
    // The season shown isn't always the first poster (a long show lists its newest first, and
    // opens on the season to watch): the row opens at it, so its poster is there for Down from
    // the buttons to land on. Once: after that the row is where focus left it.
    val shownAt = tiles.indexOfFirst { it.id == seasons.selected }
    val state = rememberTvRowState(initialFirstVisibleItemIndex = shownAt.coerceAtLeast(0))
    var opened by rememberSaveable { mutableStateOf(false) }
    val density = LocalDensity.current
    LaunchedEffect(shownAt >= 0) {
        // The season shown isn't known until the episode to watch is.
        if (opened || shownAt < 0) return@LaunchedEffect
        opened = true
        val endRoom = with(density) { TvSafeArea.Horizontal.toPx() }
        // Seasons that all fit on screen open unscrolled, whichever is shown.
        val at = if (snapshotFlow { TvScrolling.fits(state, endRoom) }.filterNotNull().first()) 0 else shownAt
        if (state.firstVisibleItemIndex != at || (at == 0 && state.firstVisibleItemScrollOffset != 0)) state.scrollToItem(at)
    }
    TvRow(
        rowKey = SEASON_ROW,
        title = null,
        tiles = tiles,
        kind = TileKind.Poster,
        onOpen = { tile -> onChoose(tile.id) },
        onActions = { tile -> byId[tile.id]?.let(onActions) },
        onFocus = {
            focused = it.id
            onFocusSeason(it.id)
        },
        memory = memory,
        enterOn = seasons.selected,
        state = state,
        modifier = modifier.testTag(TV_SEASON_TABS),
    )
}

/**
 * The episodes, as a row of the TV kit ([TvRow]): the focused tile's left
 * edge at the gutter, focus coming back to the tile it left (or the first
 * time to [target]), Left from the first tile to the rail, Right stopping
 * at the end, and a held key sliding the row as any row's does. [current]
 * is the episode whose page this is, marked among the others.
 */
@Composable
private fun EpisodeRow(
    episodes: List<EpisodeLine>,
    origin: String,
    target: Long?,
    current: Long?,
    memory: FocusMemory,
    rowRequester: FocusRequester,
    modifier: Modifier,
    onFocusEpisode: (EpisodeLine) -> Unit,
    onOpen: (ItemSummary) -> Unit,
    onActions: (ItemSummary) -> Unit,
) {
    val words = formatWords()
    val tiles = remember(episodes, origin, words) { episodes.map { ArtworkTileModel.of(it.item, TileKind.Episode, origin, words) } }
    val byId = remember(episodes) { episodes.associateBy { it.item.id } }
    val start = remember(episodes, target) { episodes.indexOfFirst { it.item.id == target }.coerceAtLeast(0) }
    val state = rememberTvRowState(initialFirstVisibleItemIndex = start)
    val density = LocalDensity.current
    // A season that fits on screen opens unscrolled, whichever episode is next.
    LaunchedEffect(state, density) {
        val endRoom = with(density) { TvSafeArea.Horizontal.toPx() }
        val fits = snapshotFlow { TvScrolling.fits(state, endRoom) }.filterNotNull().first()
        if (fits && (state.firstVisibleItemIndex != 0 || state.firstVisibleItemScrollOffset != 0)) state.scrollToItem(0)
    }
    TvRow(
        rowKey = EPISODE_ROW,
        title = null,
        tiles = tiles,
        kind = TileKind.Episode,
        onOpen = { tile -> byId[tile.id]?.let { onOpen(it.item) } },
        onActions = { tile -> byId[tile.id]?.let { onActions(it.item) } },
        onFocus = { tile -> byId[tile.id]?.let(onFocusEpisode) },
        memory = memory,
        enterOn = episodes.getOrNull(start)?.item?.id,
        state = state,
        current = current,
        modifier = modifier.focusRequester(rowRequester),
    )
}

/** Tabs are as tall as the buttons (design spec §2.6). */
internal val TabHeight = PleyTv.ButtonHeight

/**
 * How long focus rests on a season before it shows: its episodes are asked
 * for ahead, so only long enough that a season focus passes over stays out.
 */
private const val SEASON_REST_MS = 150L

/** From a tab's start to its label, so the first tab's label is at the gutter. */
private val TabInset = 20.dp

/** Where the shown season's tab ends, before the tabs are laid out. */
private const val UNPLACED = -1

internal const val TV_SEASON_TABS = "pley.tv.item.seasons"

/** The season posters' row's key: its tiles are remembered as "season/<id>". */
internal const val SEASON_ROW = "season"

/** The episode row's key: its tiles are remembered as "episode/<id>". */
internal const val EPISODE_ROW = "episode"
internal const val TV_EPISODE_ROW = ROW_TAG + EPISODE_ROW
