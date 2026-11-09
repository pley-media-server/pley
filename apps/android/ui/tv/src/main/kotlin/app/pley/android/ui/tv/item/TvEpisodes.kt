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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
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
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
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

/**
 * The seasons and episodes (design spec §2.4, §4.4; mockup tv-show): the
 * season tabs, when the show has more than one season, then the season's
 * episodes as 208 by 117 tiles captioned "5. Glasshouse" and "26 min left".
 * Focus comes into the row on the episode to watch (or the page's own
 * episode), and stays at the gutter while the row moves. A tab chosen
 * with focus resting on it shows its season.
 */
@Composable
internal fun TvEpisodesSection(
    page: ItemPage,
    seasons: SeasonsSection,
    focus: ItemPageFocus,
    memory: FocusMemory,
    rowRequester: FocusRequester,
    onSelectSeason: (Long) -> Unit,
    onFocusEpisode: (EpisodeLine) -> Unit,
    onOpen: (ItemSummary) -> Unit,
    onActions: (ItemSummary) -> Unit,
) {
    Column(Modifier.fillMaxWidth()) {
        if (seasons.seasons.size > 1) {
            SeasonTabs(seasons, page.origin, onSelectSeason, Modifier.onFocusChanged { if (it.hasFocus) focus.area = Area.Tabs })
            Spacer(Modifier.height(TabsToTiles))
        } else {
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
 * The season tabs (design spec §2.6): 40 dp pills, labels in [muted][PleyColors.Muted];
 * the shown season on [surface3][PleyColors.Surface3] in [fg][PleyColors.Fg];
 * the focused tab in [accent][PleyColors.Accent], at 1.05. Focus coming
 * into the tabs (Up from the episodes, Down from the buttons) lands on the
 * shown season's tab, as on the Samsung app, never on the tab nearest the
 * focus, which would show its season. A show with more tabs than the screen
 * holds scrolls them, each tab at its own width, just far enough to show the
 * focused one ([TvScrolling.inView]). Once a season has a poster of its
 * own ([SeasonsSection.posters]) every tab shows its season's poster, else
 * the show's, beside its name and episodes, 72 tall.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SeasonTabs(seasons: SeasonsSection, origin: String, onSelect: (Long) -> Unit, modifier: Modifier = Modifier) {
    var focused by remember { mutableStateOf<Long?>(null) }
    val rested = rememberRested(focused)
    LaunchedEffect(rested) {
        if (rested != null && rested != seasons.selected) onSelect(rested)
    }
    val selected = remember { FocusRequester() }
    val density = LocalDensity.current
    val scroll = rememberScrollState()
    val start = PleyTv.Gutter - if (seasons.posters) PosterTabInset else TabInset
    val inView = remember(density, start) { with(density) { TvScrolling.inView(start.toPx(), TvSafeArea.Horizontal.toPx()) } }
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
                val episodes = season.episodes?.asString()
                val poster = remember(seasons.posters, season.posterUrl, origin, episodes) {
                    if (seasons.posters) PosterTab(ArtworkImage.of(season.posterUrl, origin), episodes) else null
                }
                SeasonTab(
                    title = season.title,
                    current = current,
                    onFocus = { focused = season.id },
                    onClick = { onSelect(season.id) },
                    modifier = if (current) Modifier.focusRequester(selected) else Modifier,
                    poster = poster,
                )
            }
        }
    }
}

/** A poster tab's picture (`null`: neither the season nor the show has one) and "8 episodes". */
private class PosterTab(val image: ArtworkImage?, val episodes: String?)

@Composable
private fun SeasonTab(
    title: String,
    current: Boolean,
    onFocus: () -> Unit,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    poster: PosterTab? = null,
) {
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
            .heightIn(min = if (poster == null) TabHeight else PosterTabHeight)
            .background(fill, PleyShape.Control)
            .semantics { selected = current }
            .clickable(interactionSource = interactions, indication = null, role = Role.Tab, onClick = onClick)
            .padding(start = if (poster == null) TabInset else PosterTabInset, end = TabInset),
        contentAlignment = Alignment.CenterStart,
    ) {
        if (poster == null) {
            BasicText(title, style = PleyType.Tv.Label.copy(color = text), softWrap = false, maxLines = 1)
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Artwork(poster.image, title, Modifier.size(PosterWidth, PosterHeight).clip(PleyShape.Badge))
                Column {
                    BasicText(title, style = PleyType.Tv.Label.copy(color = text), softWrap = false, maxLines = 1)
                    poster.episodes?.let {
                        val count = if (focused) PleyColors.OnAccent.copy(alpha = 0.8f) else PleyColors.Muted
                        BasicText(it, style = PleyType.Tv.Caption.copy(color = count), softWrap = false, maxLines = 1)
                    }
                }
            }
        }
    }
}

/**
 * The episodes, as a row of the TV kit ([TvRow]): the focused tile's left
 * edge at the gutter, focus coming back to the tile it left (or the first
 * time to [target]), Left from the first tile to the rail, Right stopping
 * at the end, and a held key sliding the row as any row's does.
 */
@Composable
private fun EpisodeRow(
    episodes: List<EpisodeLine>,
    origin: String,
    target: Long?,
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
        modifier = modifier.focusRequester(rowRequester),
    )
}

/** Tabs are 40 tall (design spec §2.6). */
internal val TabHeight = 40.dp

/** Poster tabs: a 40 by 60 poster, 6 from the tab's top, bottom and start. */
internal val PosterTabHeight = 72.dp
private val PosterWidth = 40.dp
private val PosterHeight = 60.dp

/** From a tab's start to its label, so the first tab's label is at the gutter. */
private val TabInset = 20.dp

/** From a poster tab's start to its poster, so the first tab's poster is at the gutter. */
private val PosterTabInset = 6.dp

internal const val TV_SEASON_TABS = "pley.tv.item.seasons"

/** The episode row's key: its tiles are remembered as "episode/<id>". */
internal const val EPISODE_ROW = "episode"
internal const val TV_EPISODE_ROW = ROW_TAG + EPISODE_ROW
