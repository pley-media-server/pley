@file:OptIn(ExperimentalFoundationApi::class)

package app.pley.android.ui.tv.item

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.BringIntoViewSpec
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.Credit
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.network.OfflineException
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.CrewLine
import app.pley.android.screens.item.EpisodeLine
import app.pley.android.screens.item.ItemEvent
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.ItemUiState
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.screens.item.SeasonsSection
import app.pley.android.screens.item.extraSubtitle
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.LocalTvRowColumn
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.railFade
import app.pley.android.ui.tv.kit.readOn
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberRested
import app.pley.android.ui.tv.kit.rememberTvRowColumnFocus
import app.pley.android.ui.tv.kit.rememberTvRowColumnState
import app.pley.android.ui.tv.kit.tvRowColumnFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.ReportOffline
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.delay

/**
 * An item's page on TV (design spec §4.3, §4.4; mockups tv-movie and
 * tv-show): a movie, show, season or episode over its full-bleed
 * backdrop. Focus starts on Resume or Play. Down goes to the episodes
 * (shows, seasons, episodes) and the rows (Extras, Cast, More Like
 * This), then Details; once focus has left the buttons, an info block
 * stays at the top, for the focused episode in the episode row.
 * Artists, albums and tracks open their music pages instead, an extra
 * its movie's or show's page.
 */
@Composable
internal fun TvItemScreen(itemId: Long) {
    TvItemScreen(viewModel<ItemViewModel>(factory = ItemViewModel.factory(itemId)))
}

/** The page of [model]'s item. */
@Composable
internal fun TvItemScreen(model: ItemViewModel) {
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val toasts = remember { TvToastState() }
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                ItemEvent.NothingToPlay -> toasts.show(UiText.res(R.string.tv_item_nothing_to_play))
                is ItemEvent.Failed -> toasts.report(event.error)
            }
        }
    }
    Box(Modifier.fillMaxSize().testTag(TV_ITEM_PAGE)) {
        when (val s = state) {
            ItemUiState.Loading -> {
                // A page whose details aren't held in memory is often made within a frame or two all the
                // same (its snapshot, a quick answer): the outline waits, so it doesn't flash before the page.
                var late by remember { mutableStateOf(false) }
                LaunchedEffect(Unit) {
                    delay(SKELETON_AFTER_MS)
                    late = true
                }
                if (late) TvItemSkeleton()
            }
            ItemUiState.NotFound -> TvMessage(
                title = stringResource(R.string.tv_item_not_found),
                body = stringResource(R.string.tv_item_not_found_body),
                action = stringResource(R.string.tv_item_back),
                onAction = navigator::back,
            )
            is ItemUiState.Failed -> TvMessage(
                title = failedTitle(s.error, stringResource(R.string.tv_item_failed)),
                body = failedBody(s.error),
                action = stringResource(R.string.tv_item_retry),
                onAction = model::retry,
            )
            is ItemUiState.Elsewhere -> LaunchedEffect(s.key) { navigator.replace(s.key) }
            is ItemUiState.Ready -> {
                LaunchedEffect(Unit) {
                    withFrameNanos { }
                    model.onShown()
                }
                // Out of reach, the offline notice says so; another failure, a toast.
                ReportOffline(s.status)
                (s.status as? app.pley.android.data.Loadable.Failed)?.takeIf { it.error !is OfflineException }?.let { failed ->
                    LaunchedEffect(failed.error) { toasts.report(failed.error) }
                }
                TvItemPage(s.page, model, navigator, toasts)
            }
        }
        TvToastHost(toasts)
    }
}

/** Where focus is on the page, which says where the page scrolls and what the info block shows. */
internal enum class Area { Hero, Tabs, Episodes, Rows, Details }

/** The page's focus, shared by its sections and the scrolling. */
@Stable
internal class ItemPageFocus {
    var area by mutableStateOf(Area.Hero)

    /** Read on past what has focus, into the details ([readOn]): the info block steps aside for the text. */
    var reading by mutableStateOf(false)

    /** The focused hero element's top inside the hero, in px, for scrolling back to it from far down. */
    var heroElementY: Float = 0f
}

/** The side panel open over the page. */
internal sealed interface Panel {
    data object Tracks : Panel

    data object More : Panel

    /** The actions of a tile's [item] (a long press of Select); focus returns to [returnTo] after. */
    data class Actions(val item: ItemSummary, val returnTo: FocusRequester?) : Panel
}

@Composable
private fun TvItemPage(page: ItemPage, model: ItemViewModel, navigator: TvNavigator, toasts: TvToastState) {
    val focus = remember { ItemPageFocus() }
    val memory = rememberFocusMemory()
    // Composes the row below ahead, as Home does: Down onto Extras or Cast glides onto a row already made.
    val list = rememberTvRowColumnState()
    val tier = LocalTvTier.current
    var panel by remember { mutableStateOf<Panel?>(null) }
    var summaryOpen by remember { mutableStateOf(false) }
    var restore by remember { mutableStateOf<FocusRequester?>(null) }
    var focusedEpisode by remember { mutableStateOf<EpisodeLine?>(null) }
    val buttons = remember { HeroButtons() }
    val episodesRow = remember { FocusRequester() }
    val seasonsRow = remember { FocusRequester() }
    val sections = remember { SectionFocus() }
    val backdrop = remember(page.item, page.origin, tier) { ArtworkCandidates.backdrop(page.item, page.origin, tier) }
    val actions = remember(model, navigator) { TvPageActions(model, navigator) }
    // What requests say of a movie or show: asked once the page has drawn.
    val request by model.request.collectAsStateWithLifecycle()

    Box(Modifier.fillMaxSize()) {
        TvBackdrop(backdrop, page.context?.showTitle ?: page.title, scrim = BackdropScrim.Item)
        ItemColumn(list, focus, episodesTop = episodeTilesTop(page.seasons)) {
            item(key = "hero", contentType = "hero") {
                TvHero(
                    page = page,
                    focus = focus,
                    memory = memory,
                    buttons = buttons,
                    down = when {
                        // Season posters are a row of their own: Down comes to it, as to any row.
                        seasonPosterRow(page) -> seasonsRow
                        page.seasons?.episodes?.isNotEmpty() == true -> episodesRow
                        else -> null
                    },
                    actions = actions,
                    onTracks = { panel = Panel.Tracks },
                    onMore = { panel = Panel.More },
                    onPrimaryActions = { panel = Panel.Actions(page.item, buttons.play) },
                    request = request,
                )
            }
            page.seasons?.let { seasons ->
                item(key = "episodes", contentType = "episodes") {
                    TvEpisodesSection(
                        page = page,
                        seasons = seasons,
                        focus = focus,
                        memory = memory,
                        rowRequester = episodesRow,
                        seasonsRequester = seasonsRow,
                        onSelectSeason = model::selectSeason,
                        onFocusEpisode = { focusedEpisode = it },
                        onOpen = { actions.open(it.id) },
                        onActions = { panel = Panel.Actions(it, episodesRow) },
                        onSeasonActions = { panel = Panel.Actions(it, seasonsRow) },
                    )
                }
            }
            rowsOf(page, focus, memory, sections, actions) { item, returnTo -> panel = Panel.Actions(item, returnTo) }
            if (page.details.versions.isNotEmpty() || page.mediaInfo.isNotEmpty()) {
                item(key = DETAILS_KEY, contentType = DETAILS_KEY) {
                    TvDetails(page, onChooseVersion = model::chooseVersion, modifier = Modifier.onFocusChanged { if (it.hasFocus) focus.area = Area.Details })
                }
            }
        }
        InfoBlock(page, focus, focusedEpisode)
        TvPanels(
            panel = panel,
            page = page,
            actions = actions,
            onClose = { returnTo ->
                panel = null
                restore = returnTo
            },
            buttons = buttons,
            onSummary = {
                panel = null
                summaryOpen = true
            },
        )
        if (summaryOpen) {
            val text = listOfNotNull(page.tagline, page.summary).joinToString("\n\n")
            TvTextDialog(title = page.title, text = text, onDismiss = {
                summaryOpen = false
                restore = buttons.more
            })
        }
    }
    // Focus goes back to what opened the panel or dialog once it has left the
    // screen: asked at once, the closing panel still holds it and it falls to the rail.
    LaunchedEffect(restore) {
        val target = restore ?: return@LaunchedEffect
        withFrameNanos { }
        withFrameNanos { }
        runCatching { target.requestFocus() }
        restore = null
    }
}

/** The focus requesters of the hero's buttons, for panels to give focus back to, and whether its summary was cut. */
@Stable
internal class HeroButtons {
    val play = FocusRequester()
    val tracks = FocusRequester()
    val more = FocusRequester()

    /** Whether the summary runs past its lines, so More offers it all. */
    var summaryCut by mutableStateOf(false)
}

/** Each row's focus group, by key, so focus can go back to it after a panel. */
@Stable
internal class SectionFocus {
    private val requesters = HashMap<String, FocusRequester>()

    fun of(key: String): FocusRequester = requesters.getOrPut(key) { FocusRequester() }
}

/** What the page's parts do: the model's actions and the navigation. */
@Stable
internal class TvPageActions(val model: ItemViewModel, private val navigator: TvNavigator) {
    fun open(id: Long) = navigator.open(PleyKey.Item(id))

    fun openPerson(id: Long) = navigator.open(PleyKey.Person(id))

    fun open(key: PleyKey) = navigator.open(key)
}

/** The rows under the episodes: Extras, Cast or Guest Stars, who made it and the facts, More Like This. */
private fun LazyListScope.rowsOf(
    page: ItemPage,
    focus: ItemPageFocus,
    memory: app.pley.android.ui.tv.kit.FocusMemory,
    sections: SectionFocus,
    actions: TvPageActions,
    onTileActions: (ItemSummary, FocusRequester) -> Unit,
) {
    val origin = page.origin
    if (page.extras.isNotEmpty()) {
        item(key = "extras", contentType = TileKind.Landscape) {
            val resources = LocalResources.current
            val words = formatWords()
            val tiles = remember(page.extras, origin, resources, words) {
                page.extras.map { ArtworkTileModel.of(it, TileKind.Landscape, origin, words).copy(subtitle = extraSubtitle(it, words).resolve(resources)) }
            }
            val byId = remember(page.extras) { page.extras.associateBy { it.id } }
            val group = sections.of("extras")
            Box(Modifier.focusRequester(group).onFocusChanged { if (it.hasFocus) focus.area = Area.Rows }) {
                TvRow(
                    rowKey = "extras",
                    title = stringResource(R.string.tv_item_extras),
                    tiles = tiles,
                    kind = TileKind.Landscape,
                    onOpen = { tile -> byId[tile.id]?.let { actions.model.play(it) } },
                    memory = memory,
                )
            }
        }
    }
    val crew = page.details.crew
    if (page.cast.isNotEmpty() || crew.isNotEmpty()) {
        item(key = "cast", contentType = TileKind.Person) {
            val roles = mapOf(
                CreditRole.Director to stringResource(R.string.tv_item_director),
                CreditRole.Writer to stringResource(R.string.tv_item_writer),
                CreditRole.Creator to stringResource(R.string.tv_item_creator),
            )
            val tiles = remember(page.cast, crew, origin, roles) { castAndCrew(page.cast, crew, roles, origin) }
            val episode = page.kind == ItemKind.Episode
            val title = when {
                crew.isEmpty() -> if (episode) R.string.tv_item_guest_stars else R.string.tv_item_cast
                page.cast.isEmpty() -> R.string.tv_item_crew
                else -> if (episode) R.string.tv_item_guest_stars_and_crew else R.string.tv_item_cast_and_crew
            }
            Box(Modifier.focusRequester(sections.of("cast")).onFocusChanged { if (it.hasFocus) focus.area = Area.Rows }) {
                TvRow(
                    rowKey = "cast",
                    title = stringResource(title),
                    tiles = tiles,
                    kind = TileKind.Person,
                    onOpen = { actions.openPerson(it.id) },
                    memory = memory,
                )
            }
        }
    }
    // Who made it and the facts, under the cast, in sight while it has focus.
    if (page.details.hasFacts) {
        item(key = "facts", contentType = "facts") { TvFacts(page) }
    }
    if (page.related.isNotEmpty()) {
        item(key = "related", contentType = TileKind.Poster) {
            val words = formatWords()
            val tiles = remember(page.related, origin, words) { page.related.map { ArtworkTileModel.of(it, TileKind.Poster, origin, words) } }
            val byId = remember(page.related) { page.related.associateBy { it.id } }
            val group = sections.of("related")
            Box(Modifier.focusRequester(group).onFocusChanged { if (it.hasFocus) focus.area = Area.Rows }) {
                TvRow(
                    rowKey = "related",
                    title = stringResource(R.string.tv_item_more_like_this),
                    tiles = tiles,
                    kind = TileKind.Poster,
                    onOpen = { actions.open(it.id) },
                    onActions = { tile -> byId[tile.id]?.let { onTileActions(it, group) } },
                    memory = memory,
                )
            }
        }
    }
}

/**
 * The page's column (design spec §2.4, §4.4): one list from the hero down,
 * which scrolls so that what has focus sits where the spec puts it — the
 * hero at the top; the season tabs at y = 218 and the episodes under them
 * at 286 (a single season's at 272, under its name, as under season posters); other rows' tiles at 210, their header at 172; the details'
 * header there too — and, once focus has left the hero, draws nothing
 * above that line, so the rows pass under the info block. It moves itself
 * to a row's tile, and back to the hero, as a page of rows does
 * ([TvRowColumn]): over 150 ms, eased out, for a press, evenly under a
 * held Up or Down; Compose scrolls to the tabs and the details. Tiles that
 * scroll left pass under the rail ([railFade]). Down past the last thing
 * that takes focus reads on through the details ([readOn]); the info block
 * steps aside then and the text has the screen down from the safe area.
 */
@Composable
private fun ItemColumn(state: LazyListState, focus: ItemPageFocus, episodesTop: Dp, content: LazyListScope.() -> Unit) {
    val density = LocalDensity.current
    val column = rememberTvRowColumnFocus(state, with(density) { RowTilesTop.toPx() })
    column.tileTops[EPISODE_ROW] = with(density) { episodesTop.toPx() }
    column.tileTops[SEASON_ROW] = with(density) { TabsTop.toPx() }
    val spec = remember(density, state, focus, column, episodesTop) { column.spec(ItemPageSpec(density, state, focus, episodesTop)) }
    CompositionLocalProvider(LocalBringIntoViewSpec provides spec, LocalTvRowColumn provides column) {
        LazyColumn(
            state = state,
            contentPadding = PaddingValues(bottom = BottomRoom),
            verticalArrangement = Arrangement.spacedBy(PleyTv.RowGap),
            modifier = Modifier
                .fillMaxSize()
                .tvRowColumnFocus(column)
                .heldKeyStaysIn(HeldEnds.UpDown)
                .readOn(state, line = PleyTv.FocusedRowHeaderY, onReading = { focus.reading = it })
                .railFade()
                .drawWithContent {
                    val line = when {
                        focus.reading -> TvSafeArea.Vertical
                        else -> when (focus.area) {
                            Area.Hero -> null
                            Area.Tabs, Area.Episodes -> TabsTop - Clearance
                            Area.Rows, Area.Details -> PleyTv.FocusedRowHeaderY - Clearance
                        }
                    }
                    if (line == null) drawContent() else clipRect(top = line.toPx()) { this@drawWithContent.drawContent() }
                },
            content = content,
        )
    }
}

/** Where the column scrolls what has focus, by the area it is in. */
private class ItemPageSpec(
    density: Density,
    private val state: LazyListState,
    private val focus: ItemPageFocus,
    episodesTop: Dp,
) : BringIntoViewSpec {
    private val tabs = with(density) { TabsTop.toPx() }
    private val episodes = with(density) { episodesTop.toPx() }
    private val rows = with(density) { RowTilesTop.toPx() }
    private val header = with(density) { PleyTv.FocusedRowHeaderY.toPx() }
    private val bottom = with(density) { TvSafeArea.Vertical.toPx() }

    override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float = when (focus.area) {
        Area.Hero -> if (state.firstVisibleItemIndex == 0) -state.firstVisibleItemScrollOffset.toFloat() else offset - focus.heroElementY
        Area.Tabs -> offset - tabs
        Area.Episodes -> offset - episodes
        Area.Rows -> offset - rows
        Area.Details -> details(offset, size, containerSize)
    }

    /**
     * The details' header at the rows' header line, so the text above a
     * focused version row shows too; further while that row would end
     * below the safe area, never so far that it goes under the info block.
     */
    private fun details(offset: Float, size: Float, containerSize: Float): Float {
        val block = state.layoutInfo.visibleItemsInfo.firstOrNull { it.key == DETAILS_KEY } ?: return offset - rows
        val top = (block.offset + state.layoutInfo.beforeContentPadding).toFloat()
        return maxOf(top - header, offset + size - (containerSize - bottom)).coerceAtMost(offset - header)
    }
}

/**
 * The info block (design spec §4.1, §4.4; mockup tv-show) once focus has
 * left the hero: the title, on one line with the hero's room for it
 * ([HeroColumn]), and the metadata line, then the focused episode's
 * title, line and two lines of summary in the episode row, or the item's
 * own summary by the other rows. On the seasons it says no more than the
 * title and the metadata line: neither the show's summary nor an episode
 * of the season focus has left. It follows the focused episode once focus
 * has rested a moment ([EPISODE_REST_MS]), and shows at once an episode
 * focus comes to from elsewhere than its own season's row.
 */
@Composable
private fun InfoBlock(page: ItemPage, focus: ItemPageFocus, focusedEpisode: EpisodeLine?) {
    val area = focus.area
    // Gone over the hero, and while reading on through the details, which then have the whole screen.
    val hidden = area == Area.Hero || focus.reading
    val visible = animateFloatAsState(if (hidden) 0f else 1f, if (reducedMotion()) androidx.compose.animation.core.snap() else PleyMotion.reveal(), label = "info")
    // Read through derivedStateOf, the fade's frames recompose nothing: only its end does.
    val gone by remember { derivedStateOf { (focus.area == Area.Hero || focus.reading) && visible.value == 0f } }
    if (gone) return
    val rested = rememberRested(focusedEpisode, EPISODE_REST_MS)
    // In the episodes only, and never one of another season than the episode focused: coming into a
    // season's row, its episode shows at once rather than the one last shown.
    val episode = when {
        area != Area.Episodes -> null
        rested?.item?.parentId == focusedEpisode?.item?.parentId -> rested
        else -> focusedEpisode
    }
    Column(
        Modifier
            .padding(start = PleyTv.Gutter, top = 28.dp, end = TvSafeArea.Horizontal)
            .graphicsLayer { alpha = visible.value },
    ) {
        // The title has the hero's room for it; the rest keeps to the text column.
        BasicText(
            heroTitle(page),
            style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.widthIn(max = HeroColumn),
        )
        Spacer(Modifier.height(10.dp))
        TvMetaLine(page.meta)
        if (episode != null) {
            Spacer(Modifier.height(14.dp))
            BasicText(episode.item.title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
            Spacer(Modifier.height(4.dp))
            val aired = episode.aired?.let { stringResource(R.string.tv_item_aired, it) }
            BasicText(
                listOfNotNull(episode.longLabel.ifEmpty { null }, episode.runtime.ifEmpty { null }, aired).joinToString(" · "),
                style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                maxLines = 1,
                modifier = Modifier.width(TextColumn),
            )
            episode.summary?.let {
                Spacer(Modifier.height(6.dp))
                BasicText(it, style = PleyType.Tv.Body.copy(color = PleyColors.Fg.copy(alpha = 0.85f)), maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
            }
        } else if (area != Area.Tabs && area != Area.Episodes) {
            page.summary?.let {
                Spacer(Modifier.height(8.dp))
                BasicText(it, style = PleyType.Tv.Body.copy(color = PleyColors.Fg.copy(alpha = 0.85f)), maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
            }
        }
    }
}

/**
 * How long focus rests on an episode before the info block says it: long
 * enough that an episode focus passes over stays out, short enough that
 * the block keeps up with a press.
 */
private const val EPISODE_REST_MS = 150L

/** The hero's title: an episode's or season's show, else the item's own. */
internal fun heroTitle(page: ItemPage): String = when (page.kind) {
    ItemKind.Episode, ItemKind.Season -> page.context?.showTitle ?: page.title
    else -> page.title
}

/**
 * The loading page (design spec §8): still blocks where the hero's text
 * and buttons will be, once the page has been [SKELETON_AFTER_MS] coming.
 * No spinner.
 */
@Composable
private fun TvItemSkeleton() {
    Column(Modifier.fillMaxSize().padding(start = PleyTv.Gutter, top = 42.dp)) {
        Block(320.dp, 40.dp)
        Spacer(Modifier.height(14.dp))
        Block(220.dp, 18.dp)
        Spacer(Modifier.height(24.dp))
        Block(464.dp, 15.dp)
        Spacer(Modifier.height(7.dp))
        Block(440.dp, 15.dp)
        Spacer(Modifier.height(7.dp))
        Block(300.dp, 15.dp)
        Spacer(Modifier.height(38.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap)) {
            Box(Modifier.size(140.dp, PleyTv.ButtonHeight).background(PleyColors.Surface2, PleyShape.Control))
            repeat(3) { Box(Modifier.size(PleyTv.ButtonHeight).background(PleyColors.Surface2, PleyShape.Control)) }
        }
    }
}

@Composable
private fun Block(width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp) {
    Box(Modifier.size(width, height).background(PleyColors.Surface2, PleyShape.Badge))
}

/** How long a page loads before its outline shows: longer than a held page takes to be made. */
private const val SKELETON_AFTER_MS = 150L

/** The season tabs' top once focus is in the episodes (mockup tv-show). */
internal val TabsTop = 218.dp

/** The episode tiles' top under a single season's name. */
internal val EpisodeTilesTop = 272.dp

/** From the season tabs to the episode tiles: room for the focused tile's growth and ring, and a gap past them. */
internal val TabsToTiles = 28.dp

/** The episode tiles' top under the season tabs. */
internal val TabbedEpisodeTilesTop = TabsTop + TabHeight + TabsToTiles

/**
 * Where the episode tiles sit with focus in them: under the season tabs, or
 * under the season's name, which is where they sit under season posters
 * too, the posters gone above.
 */
private fun episodeTilesTop(seasons: SeasonsSection?): Dp = when {
    seasons == null || seasons.seasons.size <= 1 || seasons.posters -> EpisodeTilesTop
    else -> TabbedEpisodeTilesTop
}

/** Other rows' tiles' top: their header at y = 172. */
internal val RowTilesTop = PleyTv.FocusedRowHeaderY + PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles

/** Room kept above the line under the info block. */
private val Clearance = 12.dp

/** Room under the last section, so it too can come up to its line. */
private val BottomRoom = 300.dp

internal const val TV_ITEM_PAGE = "pley.tv.item"

/** The details' item key in the page's column. */
private const val DETAILS_KEY = "details"

/**
 * The tiles of Cast & Crew: the cast with their parts, then who made it
 * with their [roles] ("Director"), each person once, so that their names
 * lead to their pages as on the web.
 */
internal fun castAndCrew(cast: List<Credit>, crew: List<CrewLine>, roles: Map<CreditRole, String>, origin: String): List<ArtworkTileModel> {
    val seen = HashSet<Long>()
    return buildList {
        cast.filter { seen.add(it.id) }.forEach {
            add(ArtworkTileModel(id = it.id, title = it.name, subtitle = it.character.orEmpty(), image = ArtworkImage.of(it.photoUrl, origin)))
        }
        crew.forEach { line ->
            line.people.filter { seen.add(it.id) }.forEach {
                add(ArtworkTileModel(id = it.id, title = it.name, subtitle = roles[line.role].orEmpty(), image = ArtworkImage.of(it.photoUrl, origin)))
            }
        }
    }
}
