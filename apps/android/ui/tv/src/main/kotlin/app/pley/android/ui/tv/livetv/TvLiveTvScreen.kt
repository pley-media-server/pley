package app.pley.android.ui.tv.livetv

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import app.pley.android.screens.livetv.GroupSections
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import kotlinx.coroutines.launch
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.Surface
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.designsystem.tile.tvTileSize
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveGroup
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveProgram
import app.pley.android.screens.livetv.ChannelPages
import app.pley.android.ui.tv.search.TvSearchField
import app.pley.android.screens.livetv.Landing
import app.pley.android.screens.livetv.LivePage
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.screens.livetv.LiveTvEvent
import app.pley.android.screens.livetv.LiveTvViewModel
import app.pley.android.screens.livetv.LiveView
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.TvTileFrame
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.library.TvMessage
import app.pley.android.ui.tv.library.TvTab
import app.pley.android.ui.tv.shell.ReportOffline
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * Live TV on TV (the web's LiveTvPage, for a remote):
 * "Live TV" with the Guide and Channels tabs; down the left the
 * categories (Favourites, Recently watched, All channels, then the
 * provider's groups, those of a region under it, opened with OK); beside
 * them the chosen category's guide ([TvGuide], the page's first focus) or
 * tiles, a page of channels loaded at a time, opening on the channel last
 * watched. OK on a channel watches it; holding OK (or Menu) opens a side
 * panel with what's on, Watch and the favourite.
 *
 * Back goes up a step at a time (Samsung's page, the web's search): the
 * guide back to now first, a search back to the category before, the
 * guide or the tiles to the categories, then the rail.
 */
@Composable
internal fun TvLiveTvScreen(vm: LiveTvViewModel = viewModel(factory = LiveTvViewModel.Factory)) {
    val page by vm.state.collectAsStateWithLifecycle()
    val pages by vm.channels.collectAsStateWithLifecycle()
    val now by vm.now.collectAsStateWithLifecycle()
    var details by remember { mutableStateOf<Pair<LiveChannel, LiveProgram?>?>(null) }
    var notice by remember { mutableStateOf<UiText?>(null) }
    val categories = remember { FocusRequester() }
    val shownCategory = remember { FocusRequester() }
    val categoryList = rememberLazyListState()
    var shownIndex by remember { mutableIntStateOf(0) }
    val scope = rememberCoroutineScope()

    /** Left from the channels: to the category shown, brought into view first; to the column when it can't be. */
    fun toShownCategory() {
        scope.launch {
            if (categoryList.layoutInfo.visibleItemsInfo.none { it.index == shownIndex }) {
                categoryList.scrollToItem((shownIndex - 2).coerceAtLeast(0))
                withFrameNanos { }
            }
            if (runCatching { shownCategory.requestFocus() }.isFailure) runCatching { categories.requestFocus() }
        }
    }
    val content = remember { FocusRequester() }
    val search = rememberTextFieldState(page.query)
    /** Focus is in the categories (the search with them): channels shown meanwhile leave it there… */
    var inCategories by remember { mutableStateOf(false) }

    /** …unless it was sent to them (a category chosen, a search made, the page opened) and they don't have it yet. */
    var toChannels by remember { mutableStateOf(true) }

    /** Focus is in the guide or the tiles. */
    var inContent by remember { mutableStateOf(false) }
    // Composed before the guide's own Back (to now), which comes first; the side panel's after.
    BackHandler(enabled = page.allowed && inContent) { toShownCategory() }
    BackHandler(enabled = page.allowed && page.searching) { search.clearText() }
    LaunchedEffect(search) { snapshotFlow { search.text.toString() }.collect(vm::setQuery) }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is LiveTvEvent.Failed -> notice = UiText.res(R.string.tv_livetv_favorite_failed, errorSentence(event.error))
            }
        }
    }
    ReportOffline(
        when {
            pages.error != null -> Loadable.Failed(pages.error!!, Unit)
            pages.total != null -> Loadable.Ready(Unit, fresh = true)
            else -> null
        },
    )
    val rail = LocalTvRailFocus.current
    Box(Modifier.fillMaxSize().testTag(TV_LIVE_TV)) {
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier.fillMaxWidth().padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BasicText(
                    stringResource(R.string.tv_livetv_title),
                    style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                if (page.allowed) {
                    Row(Modifier.railOnLeftStopOnRight(rail).focusGroup(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TvTab(stringResource(R.string.tv_livetv_guide), page.view == LiveView.Guide, { vm.setView(LiveView.Guide) })
                        TvTab(stringResource(R.string.tv_livetv_channels), page.view == LiveView.Channels, { vm.setView(LiveView.Channels) })
                    }
                }
            }
            if (!page.allowed) {
                TvMessage(stringResource(R.string.tv_livetv_not_allowed_title), stringResource(R.string.tv_livetv_not_allowed_body), top = 40.dp)
                return@Column
            }
            Row(Modifier.fillMaxSize().padding(top = 16.dp)) {
                Categories(
                    page = page,
                    search = search,
                    selectedRequester = shownCategory,
                    onSelect = {
                        // Choosing a category ends a search, as the web's.
                        search.clearText()
                        toChannels = true
                        vm.setCategory(it)
                        runCatching { content.requestFocus() }
                    },
                    onSearched = {
                        toChannels = true
                        runCatching { content.requestFocus() }
                    },
                    state = categoryList,
                    onShownIndex = { shownIndex = it },
                    modifier = Modifier
                        .padding(start = PleyTv.Gutter - CATEGORY_INSET)
                        .width(CATEGORIES_WIDTH)
                        .fillMaxHeight()
                        .focusRequester(categories)
                        .onFocusChanged { inCategories = it.hasFocus },
                )
                Spacer(Modifier.width(16.dp))
                val total = pages.total
                Box(
                    Modifier.weight(1f).fillMaxHeight().onFocusChanged {
                        inContent = it.hasFocus
                        if (it.hasFocus) toChannels = false
                    },
                ) {
                    when {
                        total == null && pages.error != null -> Box(Modifier.focusRequester(content).focusGroup()) {
                            TvMessage(
                                title = failedTitle(pages.error!!, stringResource(R.string.tv_livetv_failed)),
                                body = failedBody(pages.error!!),
                                action = stringResource(R.string.tv_livetv_retry),
                                onAction = vm::retry,
                                top = 24.dp,
                            )
                        }
                        total == null -> Unit
                        total == 0 -> Empty(page)
                        page.view == LiveView.Guide -> key(page.listed) {
                            // Shown as a search is typed, it leaves focus in the search.
                            val takeFocus = remember { toChannels || !inCategories }
                            TvGuide(
                                takeFocus = takeFocus,
                                vm = vm,
                                pages = pages,
                                category = page.listed,
                                origin = page.origin,
                                now = now,
                                onWatch = vm::watch,
                                onDetails = { channel, program -> details = channel to program },
                                // To the category shown; to the column when it is scrolled away.
                                onLeave = ::toShownCategory,
                                modifier = Modifier.fillMaxSize(),
                                requester = content,
                            )
                        }
                        else -> key(page.listed) {
                            val takeFocus = remember { toChannels || !inCategories }
                            ChannelTiles(
                                vm,
                                pages,
                                page,
                                now,
                                Modifier.focusRequester(content),
                                onDetails = { details = it to it.onAt(now) },
                                takeFocus = takeFocus,
                                onLeave = ::toShownCategory,
                            )
                        }
                    }
                }
            }
        }
        val shown = details
        TvSidePanel(visible = shown != null, onDismiss = { details = null }) {
            if (shown != null) {
                ProgramPanel(
                    channel = shown.first,
                    program = shown.second,
                    origin = page.origin,
                    now = now,
                    onWatch = {
                        details = null
                        vm.watch(shown.first)
                    },
                    onFavorite = {
                        vm.toggleFavorite(shown.first)
                        details = shown.first.copy(favorite = !shown.first.favorite) to shown.second
                    },
                )
            }
        }
        notice?.let { text ->
            LaunchedEffect(text) {
                kotlinx.coroutines.delay(NOTICE_MS)
                notice = null
            }
            BasicText(
                text.asString(),
                style = PleyType.Tv.Body.copy(color = PleyColors.Fg),
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 32.dp)
                    .clip(PleyShape.Control)
                    .background(PleyColors.Surface3)
                    .padding(horizontal = 20.dp, vertical = 12.dp),
            )
        }
    }
}

/** A category or search without channels: why, in its own words. */
@Composable
private fun Empty(page: LivePage) {
    if (page.searching) {
        TvMessage(stringResource(R.string.tv_livetv_no_match_title), stringResource(R.string.tv_livetv_no_match_search, page.query.trim()), top = 24.dp, takeFocus = false)
        return
    }
    when (page.category) {
        LiveCategory.Favorites -> TvMessage(stringResource(R.string.tv_livetv_none_favorites_title), stringResource(R.string.tv_livetv_none_favorites_body), top = 24.dp, takeFocus = false)
        LiveCategory.Recent -> TvMessage(stringResource(R.string.tv_livetv_none_recent_title), stringResource(R.string.tv_livetv_none_recent_body), top = 24.dp, takeFocus = false)
        is LiveCategory.Group -> TvMessage(stringResource(R.string.tv_livetv_no_match), "", top = 24.dp, takeFocus = false)
        LiveCategory.All -> TvMessage(stringResource(R.string.tv_livetv_none_title), stringResource(R.string.tv_livetv_none_body), top = 24.dp, takeFocus = false)
    }
}

/**
 * The categories down the left (the web's CategoryRail), under the search
 * (every channel, and the groups it names): the marked ones and All
 * channels with their counts, then the regions, opened and closed with OK,
 * then the other groups. OK on a category shows it and moves focus to its
 * channels; Left goes to the rail. Focus comes back to the category shown.
 */
@Composable
private fun Categories(
    page: LivePage,
    search: TextFieldState,
    selectedRequester: FocusRequester,
    onSelect: (LiveCategory) -> Unit,
    onSearched: () -> Unit,
    modifier: Modifier,
    state: LazyListState,
    onShownIndex: (Int) -> Unit,
) {
    val rail = LocalTvRailFocus.current
    CategoryColumn(
        state = state,
        onShownIndex = onShownIndex,
        groups = page.groups,
        selected = page.category,
        selectedRequester = selectedRequester,
        onSelect = onSelect,
        modifier = modifier.railOnLeftStopOnRight(rail, stopRight = false).heldKeyStaysIn(HeldEnds.UpDown).testTag(TV_LIVE_CATEGORIES),
        found = page.found,
        search = {
            TvSearchField(
                state = search,
                placeholder = stringResource(R.string.tv_livetv_search),
                onSearch = onSearched,
                modifier = Modifier.fillMaxWidth().padding(start = CATEGORY_INSET - 10.dp, bottom = 8.dp),
                focusModifier = Modifier.testTag(TV_LIVE_SEARCH),
            )
        },
    )
}

/**
 * The categories as a column: beside the guide, and in the live player's
 * channel list when another group is chosen. Focus comes back to
 * [selected]'s row, which takes [selectedRequester]. Under [search], when
 * there is one, the groups a search names ([found]).
 */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun CategoryColumn(
    groups: LiveGroups?,
    selected: LiveCategory,
    selectedRequester: FocusRequester,
    onSelect: (LiveCategory) -> Unit,
    modifier: Modifier = Modifier,
    found: List<LiveGroup> = emptyList(),
    search: (@Composable () -> Unit)? = null,
    state: LazyListState = rememberLazyListState(),
    /** Where [selected]'s row is in the column (its region's, the region closed), to bring it into view. */
    onShownIndex: (Int) -> Unit = {},
) {
    val all = groups?.groups.orEmpty()
    val sections = remember(all) { LiveTv.groupSections(all) }
    val selectedRegion = (selected as? LiveCategory.Group)?.let { s -> sections.regions.firstOrNull { r -> r.groups.any { it.name == s.name } }?.region }
    var open by remember(selectedRegion) { mutableStateOf(selectedRegion) }
    val shownIndex = shownIndexOf(selected, sections, open, hasSearch = search != null, found = found.size)
    SideEffect { onShownIndex(shownIndex) }
    CompositionLocalProvider(LocalSelectedCategory provides selectedRequester) {
    LazyColumn(
        state = state,
        modifier = modifier.focusRestorer(selectedRequester),
        contentPadding = PaddingValues(bottom = TvSafeArea.Vertical),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (search != null) {
            item(key = "search", contentType = "search") { search() }
        }
        if (found.isNotEmpty()) {
            item(key = "found", contentType = HEADING) { SectionHeading(stringResource(R.string.tv_livetv_found_groups)) }
            items(found, key = { "found:${it.name}" }, contentType = { CATEGORY }) { g ->
                CategoryRow(null, LiveTv.groupName(g, g.name), g.count, selected = false, indent = GROUP_INSET) { onSelect(LiveCategory.Group(g.name)) }
            }
        }
        item(key = "favorites", contentType = CATEGORY) {
            CategoryRow(PleyIcons.Star, stringResource(R.string.tv_livetv_category_favorites), groups?.favorites, selected == LiveCategory.Favorites) {
                onSelect(LiveCategory.Favorites)
            }
        }
        item(key = "recent", contentType = CATEGORY) {
            CategoryRow(PleyIcons.History, stringResource(R.string.tv_livetv_category_recent), groups?.recent, selected == LiveCategory.Recent) {
                onSelect(LiveCategory.Recent)
            }
        }
        item(key = "all", contentType = CATEGORY) {
            CategoryRow(PleyIcons.LayoutGrid, stringResource(R.string.tv_livetv_category_all), groups?.total, selected == LiveCategory.All) {
                onSelect(LiveCategory.All)
            }
        }
        val regions = sections.regions
        if (regions.isNotEmpty()) {
            item(key = "regions", contentType = HEADING) { SectionHeading(stringResource(R.string.tv_livetv_regions)) }
            for (r in regions) {
                // One open at a time, as the web's.
                val expanded = open == r.region
                item(key = "region:${r.region}", contentType = CATEGORY) {
                    CategoryRow(if (expanded) PleyIcons.ChevronDown else PleyIcons.ChevronRight, r.region, r.count, selected = false) {
                        open = if (expanded) null else r.region
                    }
                }
                if (expanded) {
                    items(r.groups, key = { "group:${it.name}" }, contentType = { CATEGORY }) { g ->
                        val category = LiveCategory.Group(g.name)
                        CategoryRow(null, g.label, g.count, selected == category, indent = GROUP_INSET + 16.dp) { onSelect(category) }
                    }
                }
            }
        }
        val others = sections.others
        if (others.size > 1 || (others.size == 1 && regions.isNotEmpty())) {
            item(key = "others", contentType = HEADING) {
                SectionHeading(stringResource(if (regions.isEmpty()) R.string.tv_livetv_groups else R.string.tv_livetv_more_groups))
            }
            items(others, key = { "group:${it.name}" }, contentType = { CATEGORY }) { g ->
                val category = LiveCategory.Group(g.name)
                CategoryRow(null, g.name.ifEmpty { stringResource(R.string.tv_livetv_no_group_name) }, g.count, selected == category, indent = GROUP_INSET) { onSelect(category) }
            }
        }
    }
}
}

/**
 * [selected]'s row in [CategoryColumn], counted as its items are laid out:
 * the search, the groups found, the three marked categories, then the
 * regions (with [open]'s groups) and the others, each under a heading.
 */
private fun shownIndexOf(selected: LiveCategory, sections: GroupSections, open: String?, hasSearch: Boolean, found: Int): Int {
    var i = if (hasSearch) 1 else 0
    if (found > 0) i += 1 + found
    when (selected) {
        LiveCategory.Favorites -> return i
        LiveCategory.Recent -> return i + 1
        LiveCategory.All -> return i + 2
        is LiveCategory.Group -> Unit
    }
    i += 3
    val name = (selected as LiveCategory.Group).name
    if (sections.regions.isNotEmpty()) {
        i += 1
        for (r in sections.regions) {
            val at = r.groups.indexOfFirst { it.name == name }
            val expanded = open == r.region
            if (at >= 0) return if (expanded) i + 1 + at else i
            i += 1 + if (expanded) r.groups.size else 0
        }
    }
    val at = sections.others.indexOfFirst { it.name == name }
    return if (at >= 0) i + 1 + at else 0
}

/** The categories' kinds of row, for the column to reuse their layouts in kind. */
private const val CATEGORY = "category"
private const val HEADING = "heading"

/**
 * A guide row or a tile whose page is still loading: an item type of its
 * own, so the list keeps the skeletons' layouts for skeletons and the
 * channels' for channels.
 */
internal const val LOADING = "loading"

/** A section's heading in the categories: "REGIONS", "MORE GROUPS". */
@Composable
private fun SectionHeading(text: String) {
    BasicText(
        text.uppercase(),
        style = PleyType.Tv.Badge.copy(color = PleyColors.Subtle),
        modifier = Modifier.padding(start = CATEGORY_INSET, top = 14.dp, bottom = 6.dp),
    )
}

/** The category shown's row takes this, so focus comes back to it. */
private val LocalSelectedCategory = staticCompositionLocalOf<FocusRequester?> { null }

/** One category: its icon, name and count; the one shown marked; focused, the accent fill. */
@Composable
private fun CategoryRow(icon: PleyIcon?, name: String, count: Int?, selected: Boolean, indent: Dp = CATEGORY_INSET, onClick: () -> Unit) {
    val requester = LocalSelectedCategory.current
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    Surface(
        onClick = onClick,
        interactionSource = interactions,
        modifier = Modifier
            .fillMaxWidth()
            .height(40.dp)
            .then(if (selected && requester != null) Modifier.focusRequester(requester) else Modifier)
            .semantics { this.selected = selected },
        shape = ClickableSurfaceDefaults.shape(PleyShape.Control),
        colors = ClickableSurfaceDefaults.colors(
            containerColor = if (selected) PleyColors.Surface3 else Color.Transparent,
            contentColor = if (selected) PleyColors.Fg else PleyColors.Muted,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ClickableSurfaceDefaults.scale(focusedScale = 1f, pressedScale = 1f),
        glow = ClickableSurfaceDefaults.glow(Glow.None, Glow.None, Glow.None),
    ) {
        val content = LocalContentColor.current
        Row(Modifier.fillMaxSize().padding(start = indent, end = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (icon != null) {
                Icon(icon, contentDescription = null, size = 16.dp, tint = if (selected && !focused) PleyColors.Accent else content)
                Spacer(Modifier.width(10.dp))
            }
            BasicText(name, style = PleyType.Tv.Label.copy(color = content), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            if (count != null) BasicText(formatCount(count), style = PleyType.Tv.Caption.copy(color = content.copy(alpha = 0.7f)))
        }
    }
}

/** The channels as tiles, three across beside the categories, a page loaded at a time, opening on the channel last watched. */
@Composable
private fun ChannelTiles(
    vm: LiveTvViewModel,
    pages: ChannelPages,
    page: LivePage,
    now: Long,
    modifier: Modifier,
    onDetails: (LiveChannel) -> Unit,
    takeFocus: Boolean = true,
    /** Left from the first column: to the category shown, as from the guide. */
    onLeave: () -> Unit = {},
) {
    val grid = rememberLazyGridState()
    val memory = rememberFocusMemory()
    val gate = LocalKeyRepeatGate.current
    val landing by vm.landing.collectAsStateWithLifecycle()
    val skeleton = rememberSkeleton(animated = false)
    val placed = remember { mutableStateOf<Landing?>(null) }
    var initial by remember { mutableStateOf(0) }
    LaunchedEffect(landing) {
        val l = landing ?: return@LaunchedEffect
        if (l.category != page.category || placed.value == l) return@LaunchedEffect
        placed.value = l
        initial = l.index ?: 0
        grid.scrollToItem(initial)
    }
    LaunchedEffect(grid) {
        snapshotFlow { grid.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
            .distinctUntilChanged()
            .collect { (first, last) -> vm.showingRows(first, last) }
    }
    LazyVerticalGrid(
        columns = GridCells.Fixed(COLUMNS),
        state = grid,
        modifier = modifier
            .fillMaxSize()
            .heldKeyStaysIn(setOf(FocusDirection.Right, FocusDirection.Down))
            .focusProperties {
                onExit = {
                    when (requestedFocusDirection) {
                        // To the category shown on a press of its own: a held Left stops at the first column.
                        FocusDirection.Left -> {
                            cancelFocusChange()
                            if (!gate.holding) onLeave()
                        }
                        // A held Up stops at the top row rather than run on to the tabs.
                        FocusDirection.Up -> if (gate.holding) cancelFocusChange()
                        else -> Unit
                    }
                }
            }
            .focusGroup()
            .testTag(TV_LIVE_TILES),
        contentPadding = PaddingValues(end = TvSafeArea.Horizontal, top = 8.dp, bottom = 48.dp),
        horizontalArrangement = Arrangement.spacedBy(PleyTv.WideTileGap),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        items(count = pages.total ?: 0, key = { index -> pages[index]?.id ?: -index - 1L }, contentType = { index -> if (pages[index] == null) LOADING else "channel" }) { index ->
            val channel = pages[index]
            if (channel == null) {
                Box(Modifier.size(TileKind.Episode.tvWidth, TileKind.Episode.tvHeight + 40.dp).skeleton(skeleton, PleyShape.TileTv))
            } else {
                ChannelTileCard(
                    channel,
                    LiveTv.channelName(channel, page.listed),
                    page.origin,
                    now,
                    Modifier.rememberedFocus(memory, "channel/${channel.id}", initial = takeFocus && index == initial),
                    onWatch = vm::watch,
                    onDetails = onDetails,
                )
            }
        }
    }
}

/**
 * A channel's tile: its logo with its number and how far what's on is,
 * captioned with its name and what's on (Live TV's cards, and search's).
 * The long press, Menu and Info open [onDetails], where there are any.
 */
@Composable
internal fun ChannelTileCard(
    channel: LiveChannel,
    name: String,
    origin: String,
    now: Long,
    focusModifier: Modifier,
    onWatch: (LiveChannel) -> Unit,
    onDetails: ((LiveChannel) -> Unit)?,
    modifier: Modifier = Modifier,
) {
    val on = channel.onAt(now)
    TvTileFrame(
        kind = TileKind.Episode,
        onClick = { onWatch(channel) },
        onLongClick = onDetails?.let { { it(channel) } },
        modifier = modifier,
        focusModifier = if (onDetails == null) {
            focusModifier
        } else {
            // Info too, as in the guide (Samsung's cards); Fire TV remotes have none: Menu, or OK held.
            focusModifier.onKeyEvent { event ->
                if (event.key != Key.Info) return@onKeyEvent false
                if (event.type == KeyEventType.KeyUp) onDetails(channel)
                true
            }
        },
        caption = { focused ->
            TileCaption(
                name,
                on?.title ?: stringResource(if (channel.hasGuide) R.string.tv_livetv_nothing_now else R.string.tv_livetv_no_guide),
                TileStyle.Tv,
                emphasized = focused,
            )
        },
    ) {
        Box(Modifier.tvTileSize(TileKind.Episode)) {
            ChannelTile(channel, origin, Modifier.fillMaxSize(), name = true, style = PleyType.Tv.Title)
            channel.number?.let {
                BasicText(
                    it,
                    style = PleyType.Tv.Badge.copy(color = PleyColors.Fg),
                    modifier = Modifier.padding(6.dp).clip(PleyShape.Badge).background(PleyColors.OnArt).padding(horizontal = 5.dp, vertical = 2.dp),
                )
            }
            if (channel.favorite) {
                Icon(PleyIcons.Star, contentDescription = null, size = 16.dp, tint = PleyColors.Accent, modifier = Modifier.align(Alignment.TopEnd).padding(6.dp))
            }
            if (on != null) ProgramProgress(on, now, Modifier.align(Alignment.BottomCenter).fillMaxWidth(), track = PleyColors.ProgressTrackOnArt)
        }
    }
}

/**
 * A programme's details in the side panel (the web's ProgramModal): the
 * channel, the title, episode, day and times with its badges, how far it
 * is, what the guide says, then Watch and the favourite. Without a
 * programme, the channel alone.
 *
 * As Samsung's panel, the buttons keep their room at the foot and the
 * text takes what is left above them: the title two lines at most, what
 * the guide says the lines that fit, one to six. Watch is a pill and the
 * favourite a star beside it, filled for a favourite, as in the live
 * player; its words are only heard. As a pill, Remove from favourites
 * didn't fit beside Watch and went under it.
 */
@Composable
private fun ProgramPanel(
    channel: LiveChannel,
    program: LiveProgram?,
    origin: String,
    now: Long,
    onWatch: () -> Unit,
    onFavorite: () -> Unit,
) {
    val clock = rememberClock()
    val airing = program?.airingAt(now) ?: true
    Column(Modifier.fillMaxSize().testTag(TV_LIVE_PROGRAM)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            ChannelTile(channel, origin, Modifier.size(width = 64.dp, height = 38.dp))
            Spacer(Modifier.width(12.dp))
            BasicText(listOfNotNull(channel.number, channel.name).joinToString("  "), style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 1)
        }
        // Measured after the buttons: they keep their height however long the programme's text. Cut at
        // its foot: a picture, a long title and badges can leave the description less than a line,
        // which would otherwise draw over the buttons.
        Column(Modifier.weight(1f).fillMaxWidth().clipToBounds()) {
            if (program != null) ProgramText(program, origin, now, clock)
        }
        Spacer(Modifier.height(12.dp))
        Row(Modifier.focusGroup(), horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap)) {
            TvPillButton(
                stringResource(if (airing) R.string.tv_livetv_watch_now else R.string.tv_livetv_watch_channel),
                onClick = onWatch,
                icon = PleyIcons.Play,
                modifier = Modifier.initialFocus(),
            )
            TvRoundButton(
                stringResource(if (channel.favorite) R.string.tv_livetv_favorite_remove else R.string.tv_livetv_favorite_add),
                if (channel.favorite) PleyIcons.Star else PleyIcons.StarOutline,
                onClick = onFavorite,
                onArt = false,
                showLabel = false,
            )
        }
    }
}

/**
 * The programme in [ProgramPanel], in the room above its buttons: what the
 * guide says gets the lines left after the rest (and Ended under it), one
 * at least, six at most, cut with "…" at the last that fits.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ColumnScope.ProgramText(program: LiveProgram, origin: String, now: Long, clock: (Long) -> String) {
    ArtworkImage.of(program.image, origin)?.let { image ->
        Spacer(Modifier.height(14.dp))
        Artwork(image, program.title, Modifier.fillMaxWidth().height(160.dp).clip(PleyShape.TileTv))
    }
    Spacer(Modifier.height(14.dp))
    BasicText(program.title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
    val episode = listOfNotNull(episodeOf(program), program.subtitle).joinToString(" · ")
    if (episode.isNotEmpty()) BasicText(episode, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 1, modifier = Modifier.padding(top = 2.dp))
    Spacer(Modifier.height(8.dp))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp), itemVerticalAlignment = Alignment.CenterVertically) {
        BasicText("${dayOf(program.start, now)} · ${spanOf(program, clock)}", style = PleyType.Tv.Caption.copy(color = PleyColors.Muted))
        if (program.airingAt(now)) ProgramBadge(stringResource(R.string.tv_livetv_on_now), live = true)
        if (program.isLive) ProgramBadge(stringResource(R.string.tv_livetv_badge_live), live = true)
        if (program.isNew) ProgramBadge(stringResource(R.string.tv_livetv_badge_new), live = false)
        program.rating?.let {
            BasicText(
                it,
                style = PleyType.Tv.Badge.copy(color = PleyColors.Muted),
                modifier = Modifier.border(1.dp, PleyColors.Outline, PleyShape.Badge).padding(horizontal = 5.dp, vertical = 2.dp),
            )
        }
    }
    if (program.airingAt(now)) {
        Row(Modifier.padding(top = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            ProgramProgress(program, now, Modifier.weight(1f))
            Spacer(Modifier.width(10.dp))
            BasicText(timeLeft(program, now), style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle))
        }
    }
    Spacer(Modifier.height(10.dp))
    // Weighted, it is measured last, in the height left: a text cut by height ends its last line with "…".
    BasicText(
        program.description?.takeIf { it.isNotBlank() } ?: stringResource(R.string.tv_livetv_no_description),
        style = PleyType.Tv.Body.copy(color = if (program.description.isNullOrBlank()) PleyColors.Subtle else PleyColors.Fg),
        maxLines = 6,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.weight(1f, fill = false).testTag(TV_LIVE_PROGRAM_TEXT),
    )
    if (program.stop <= now) {
        BasicText(stringResource(R.string.tv_livetv_ended), style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle), modifier = Modifier.padding(top = 8.dp))
    }
}

/** Three 184 dp tiles, 18 apart, beside the categories. */
private const val COLUMNS = 3

/** The categories' column, and how far its rows' fill reaches left of the gutter, so their text lines up with it. */
private val CATEGORIES_WIDTH = 210.dp
private val CATEGORY_INSET = 12.dp

/** A group's name starts where the categories' names do, after their icons. */
private val GROUP_INSET = CATEGORY_INSET + 26.dp

private const val NOTICE_MS = 4_000L

internal const val TV_LIVE_TV = "pley.tv.live"
internal const val TV_LIVE_CATEGORIES = "pley.tv.live.categories"
internal const val TV_LIVE_SEARCH = "pley.tv.live.search"
internal const val TV_LIVE_TILES = "pley.tv.live.tiles"
internal const val TV_LIVE_PROGRAM = "pley.tv.live.program"
internal const val TV_LIVE_PROGRAM_TEXT = "pley.tv.live.program.text"
