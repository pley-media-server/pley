package app.pley.android.ui.tv.requests

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.Text
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.MediaRequest
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.DiscoverHero
import app.pley.android.screens.requests.DiscoverRow
import app.pley.android.screens.requests.DiscoverUiState
import app.pley.android.screens.requests.DiscoverViewModel
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.screens.requests.opens
import app.pley.android.screens.requests.stagePill
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvMetaLine
import app.pley.android.ui.tv.item.TvToastHost
import app.pley.android.ui.tv.item.TvToastState
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.LocalTvRowColumn
import app.pley.android.ui.tv.kit.SeeAll
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.rememberBackdropPrefetcher
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberRested
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Discover on TV (packages/tv's discover.tsx), the rail's way to ask for
 * movies and shows. At rest, the hero for the first trending title (the
 * next row's first when trending has none for this profile): its
 * backdrop, facts, a line of overview, its main action and Details. Down
 * goes into TMDB's rows of posters, each read as it comes near the
 * screen, with their badges, then the profile's requests; the backdrop
 * and an info block follow the tile focus rests on, as on Home. At the
 * top, the way to the requests with their number, for a profile that has
 * requests to see. Back from the rows goes to the hero, then the rail.
 * TMDB's search is in Search.
 */
@Composable
internal fun TvDiscoverScreen() {
    TvDiscoverScreen(viewModel { DiscoverViewModel(RequestsDeps.of(screensGraph())) })
}

@Composable
internal fun TvDiscoverScreen(vm: DiscoverViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val texts = rememberRequestTexts()
    val toasts = remember { TvToastState() }
    TvRequestEvents(vm.events, texts, toasts) { state.gate.summary }
    // Back on the page, what failed is asked again.
    var shownBefore by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        if (shownBefore && (state.problem != null || state.rows.any { it.pages.problem != null })) vm.retry()
        shownBefore = true
    }
    Box(Modifier.fillMaxSize().testTag(TV_DISCOVER)) {
        val problem = state.problem
        val hero = state.hero
        when {
            problem != null -> TvRequestMessage(
                problem = problem,
                summary = state.gate.summary,
                texts = texts,
                failed = stringResource(ScreensR.string.requests_discover_failed),
                server = state.origin,
                onRetry = vm::retry,
            )
            hero != null -> DiscoverRows(state, hero, texts, vm)
            state.nothing -> TvRequestsNotice(
                title = stringResource(R.string.tv_requests_nothing_title),
                body = stringResource(R.string.tv_requests_nothing_body),
            )
            else -> DiscoverSkeleton()
        }
        TvToastHost(toasts)
    }
}

/** What the backdrop and the info block follow in the rows: a title, or one of the profile's requests. */
@Immutable
private sealed interface Focused {
    data class Title(val title: DiscoverTitle) : Focused

    data class Request(val request: MediaRequest) : Focused
}

@Composable
private fun DiscoverRows(state: DiscoverUiState, hero: DiscoverHero, texts: RequestTexts, vm: DiscoverViewModel) {
    val navigator = LocalTvNavigator.current
    val tier = LocalTvTier.current
    val scope = rememberCoroutineScope()
    val memory = rememberFocusMemory()
    val column = rememberLazyListState()
    val heroGroup = remember { FocusRequester() }
    val heroMain = remember { FocusRequester() }
    var inRows by rememberSaveable { mutableStateOf(false) }
    var hasFocus by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf<Focused?>(null) }
    val rested = rememberRested(focused)
    val backdrops = rememberBackdropPrefetcher()
    val heroAlpha by animateFloatAsState(if (inRows) 0f else 1f, PleyMotion.reveal(), label = "hero")
    val origin = state.origin
    val about = if (inRows) rested else null
    val backdrop = remember(about, hero.title, origin, tier) {
        val picture = when (about) {
            is Focused.Title -> about.title.backdropUrl
            is Focused.Request -> about.request.backdropUrl
            null -> hero.title.backdropUrl
        }
        ArtworkImage.of(picture, origin)
    }
    val name = when (about) {
        is Focused.Title -> about.title.title
        is Focused.Request -> about.request.title
        null -> hero.title.title
    }

    // Back from the rows: the hero again, on the button it had; the next Back goes to the rail.
    BackHandler(enabled = inRows && hasFocus) {
        inRows = false
        scope.launch { column.scrollToItem(0) }
        runCatching { heroGroup.requestFocus() }
    }

    Box(Modifier.fillMaxSize().onFocusChanged { hasFocus = it.hasFocus }) {
        TvBackdrop(backdrop, name, scrim = if (inRows) BackdropScrim.Rows else BackdropScrim.Hero)
        InfoBlock(about, state.manage, texts, Modifier.graphicsLayer { alpha = 1f - heroAlpha })
        TvRowColumn(
            state = column,
            heroHeight = HeroHeight,
            modifier = Modifier.fillMaxSize().testTag(TV_DISCOVER_ROWS),
        ) {
            item(HERO_KEY, contentType = HERO_KEY) {
                Hero(
                    hero = hero,
                    state = state,
                    texts = texts,
                    memory = memory,
                    group = heroGroup,
                    main = heroMain,
                    onFocused = { inRows = false },
                    onAction = vm::heroAction,
                    onDetails = { navigator.open(PleyKey.DiscoverTitle(hero.title.kind, hero.title.tmdbId)) },
                    modifier = Modifier.graphicsLayer { alpha = heroAlpha },
                )
            }
            state.rows.forEach { row ->
                // A row TMDB gave nothing for goes; one still being read keeps its place.
                if (row.pages.titles.isEmpty() && row.pages.settled) return@forEach
                item(row.list.wire, contentType = ROW_TYPE) {
                    LaunchedEffect(row.list) { vm.showRow(row.list) }
                    TitleRow(
                        row = row,
                        origin = origin,
                        texts = texts,
                        memory = memory,
                        onFocus = { title, beside ->
                            inRows = true
                            focused = Focused.Title(title)
                            backdrops.prefetch(ArtworkImage.of(title.backdropUrl, origin), beside.map { ArtworkImage.of(it.backdropUrl, origin) })
                        },
                        onNearEnd = { vm.more(row.list) },
                    )
                }
            }
            if (state.yours.isNotEmpty()) {
                item(YOURS_KEY, contentType = YOURS_KEY) {
                    YoursRow(
                        yours = state.yours,
                        origin = origin,
                        manage = state.manage,
                        mayAsk = state.gate.mayAsk,
                        texts = texts,
                        memory = memory,
                        onFocus = { request ->
                            inRows = true
                            focused = Focused.Request(request)
                        },
                    )
                }
            }
            item(CREDIT_KEY, contentType = CREDIT_KEY) { TvTmdbCredit() }
        }
        RequestsChip(state, hidden = inRows, hero = heroGroup, onFocus = { inRows = false })
    }
}

/**
 * The hero (discover.tsx's DiscoverHero): the title's two lines ending at
 * y = 172 whatever their number, clear of the way to the requests above;
 * its facts; a line of overview; the main action (its first line of
 * words over it) and Details, which opens the title's page.
 */
@Composable
private fun Hero(
    hero: DiscoverHero,
    state: DiscoverUiState,
    texts: RequestTexts,
    memory: FocusMemory,
    group: FocusRequester,
    main: FocusRequester,
    onFocused: () -> Unit,
    onAction: () -> Unit,
    onDetails: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val column = LocalTvRowColumn.current
    if (column != null) {
        DisposableEffect(column) {
            column.rows[HERO_KEY] = group
            onDispose { if (column.rows[HERO_KEY] === group) column.rows.remove(HERO_KEY) }
        }
    }
    val title = hero.title
    var signIn by remember { mutableStateOf(false) }
    val handlers = remember(onAction) {
        TvTitleHandlers(
            request = onAction,
            more = onAction,
            tellMeToo = onAction,
            stopFollowing = onAction,
            open = { onAction() },
            signInAgain = { signIn = true },
        )
    }
    Column(
        modifier
            .height(HeroHeight)
            .wrapContentHeight(Alignment.Top, unbounded = true)
            .focusRequester(group)
            .onFocusChanged {
                if (it.hasFocus) {
                    onFocused()
                    column?.focusedRow = HERO_KEY
                }
            }
            .focusRestorer(main)
            .focusGroup()
            .padding(start = PleyTv.Gutter, top = HeroTop)
            .testTag(TV_DISCOVER_HERO),
    ) {
        Box(Modifier.width(TextColumn).height(TitleBox), contentAlignment = Alignment.BottomStart) {
            BasicText(
                title.title,
                style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.semantics { heading() },
            )
        }
        Spacer(Modifier.height(8.dp))
        TvMetaLine(remember(title, texts) { titleMeta(title, texts) })
        if (title.overview.isNotBlank()) {
            Spacer(Modifier.height(10.dp))
            BasicText(
                title.overview,
                style = PleyType.Tv.BodyLarge.copy(color = SummaryColor),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.width(TextColumn),
            )
        }
        Spacer(Modifier.height(20.dp))
        TvTitleActions(
            action = hero.action,
            kind = title.kind,
            allowance = state.allowance,
            manage = state.manage,
            texts = texts,
            on = handlers,
            compact = true,
            mainModifier = Modifier.focusRequester(main).initialFocus(memory),
        ) { claimed ->
            TvRoundButton(
                label = stringResource(ScreensR.string.requests_details),
                icon = PleyIcons.Info,
                onClick = onDetails,
                modifier = if (claimed) Modifier else Modifier.initialFocus(memory),
            )
        }
    }
    if (signIn) TvSignInAgainDialog(state.origin, onDismiss = { signIn = false }, returnTo = main)
}

/** One of TMDB's rows: posters with their badges, then See all; near its end, the next page. */
@Composable
private fun TitleRow(
    row: DiscoverRow,
    origin: String,
    texts: RequestTexts,
    memory: FocusMemory,
    onFocus: (DiscoverTitle, List<DiscoverTitle>) -> Unit,
    onNearEnd: () -> Unit,
) {
    val navigator = LocalTvNavigator.current
    val title = texts.row(row.list)
    val titles = row.pages.titles
    if (titles.isEmpty()) {
        RowSkeleton(title)
        return
    }
    val tiles = remember(titles, origin, texts) { titles.map { titleTile(it, origin, texts) } }
    val byId = remember(titles) { titles.withIndex().associateBy { it.value.tileId } }
    val seeAllLabel = stringResource(ScreensR.string.requests_see_all)
    val seeAll = remember(row.list, seeAllLabel) { SeeAll(seeAllLabel) { navigator.open(PleyKey.DiscoverList(row.list)) } }
    TvRow(
        rowKey = row.list.wire,
        title = title,
        tiles = tiles,
        kind = TileKind.Poster,
        memory = memory,
        seeAll = seeAll,
        onOpen = { tile -> byId[tile.id]?.value?.let { navigator.open(PleyKey.DiscoverTitle(it.kind, it.tmdbId)) } },
        onFocus = { tile ->
            byId[tile.id]?.let { at ->
                onFocus(at.value, titles.subList(maxOf(0, at.index - 1), minOf(titles.size, at.index + 2)))
                if (at.index >= titles.size - NEAR_END) onNearEnd()
            }
        },
    )
}

/** The profile's requests, newest first, as wide tiles with their stage; See all opens the requests. */
@Composable
private fun YoursRow(
    yours: List<MediaRequest>,
    origin: String,
    manage: Boolean,
    mayAsk: Boolean,
    texts: RequestTexts,
    memory: FocusMemory,
    onFocus: (MediaRequest) -> Unit,
) {
    val navigator = LocalTvNavigator.current
    val tiles = remember(yours, origin, texts, manage) { yours.map { requestTile(it, origin, texts, manage) } }
    val byId = remember(yours) { yours.associateBy { it.id } }
    val seeAllLabel = stringResource(ScreensR.string.requests_see_all)
    val seeAll = remember(seeAllLabel) { SeeAll(seeAllLabel) { navigator.open(PleyKey.Requests) } }
    TvRow(
        rowKey = YOURS_KEY,
        title = stringResource(ScreensR.string.requests_yours),
        tiles = tiles,
        kind = TileKind.Landscape,
        memory = memory,
        seeAll = seeAll,
        onOpen = { tile ->
            byId[tile.id]?.let { request ->
                navigator.open(opens(request, mayAsk) ?: PleyKey.DiscoverTitle(request.kind, request.tmdbId))
            }
        },
        onFocus = { tile -> byId[tile.id]?.let(onFocus) },
    )
}

/**
 * The block about where focus rests in the rows (design spec §4.1): a
 * title's name, facts and two lines of overview; a request's name, its
 * stage with the seasons asked for, who asked and when, and its stage's
 * line.
 */
@Composable
private fun InfoBlock(about: Focused?, manage: Boolean, texts: RequestTexts, modifier: Modifier = Modifier) {
    if (about == null) return
    Column(modifier.padding(start = PleyTv.Gutter, top = 28.dp).width(TextColumn)) {
        val name = when (about) {
            is Focused.Title -> about.title.title
            is Focused.Request -> about.request.title
        }
        Box(Modifier.height(52.dp).wrapContentHeight(Alignment.Bottom, unbounded = true), contentAlignment = Alignment.BottomStart) {
            BasicText(name, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Spacer(Modifier.height(10.dp))
        when (about) {
            is Focused.Title -> {
                TvMetaLine(remember(about, texts) { titleMeta(about.title, texts) })
                if (about.title.overview.isNotBlank()) {
                    Spacer(Modifier.height(8.dp))
                    BasicText(about.title.overview, style = PleyType.Tv.Body.copy(color = SummaryColor), maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
            }
            is Focused.Request -> {
                val request = about.request
                val pill = stagePill(request, manage)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TvStagePill(pill, texts, small = true)
                    request.seasons?.takeIf { it.isNotEmpty() }?.let {
                        BasicText(texts.seasons(it), style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 1)
                    }
                }
                Spacer(Modifier.height(8.dp))
                BasicText(texts.asked(request, manage), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1)
                texts.pillLine(pill)?.let {
                    Spacer(Modifier.height(2.dp))
                    BasicText(it, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1)
                }
            }
        }
    }
}

/**
 * The way to the requests at the top, where other pages have their title
 * (discover.tsx's DiscoverBar): Your requests (Requests for managers)
 * with their number, red while some couldn't be sent; for a profile with
 * requests to see. It fades while focus is in the rows; Down goes to the
 * hero's button.
 */
@Composable
private fun RequestsChip(state: DiscoverUiState, hidden: Boolean, hero: FocusRequester, onFocus: () -> Unit) {
    val summary = state.gate.summary ?: return
    if (!summary.requestsEntry || !(summary.own > 0 || summary.manage)) return
    val navigator = LocalTvNavigator.current
    val texts = rememberRequestTexts()
    val entry = remember(state.gate) { RequestsEntry.of(state.gate) }
    val alpha by animateFloatAsState(if (hidden) 0f else 1f, PleyMotion.reveal(), label = "chip")
    TvCountChip(
        label = stringResource(if (summary.manage) ScreensR.string.requests_manage else ScreensR.string.requests_yours),
        count = entry.badge,
        alarm = entry.alarm,
        said = if (entry.badge > 0) texts.count(entry) else null,
        onClick = { navigator.open(PleyKey.Requests) },
        modifier = Modifier
            .padding(start = PleyTv.Gutter, top = TvSafeArea.Vertical)
            .graphicsLayer { this.alpha = alpha }
            .onFocusChanged { if (it.hasFocus) onFocus() }
            // Down: the hero's button it had, else its first.
            .focusProperties {
                canFocus = !hidden
                down = hero
            }
            .testTag(TV_DISCOVER_REQUESTS),
    )
}

/**
 * A chip (design spec §2.6) with a number after its label: the list
 * icon, [label], and [count] in a small pill, red while [alarm]. Screen
 * readers hear [said] after the label.
 */
@Composable
internal fun TvCountChip(label: String, count: Int, alarm: Boolean, said: String?, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    Button(
        onClick = onClick,
        interactionSource = interactions,
        modifier = modifier.height(40.dp).semantics(mergeDescendants = true) { contentDescription = listOfNotNull(label, said).joinToString(", ") },
        shape = ButtonDefaults.shape(PleyShape.Control),
        colors = ButtonDefaults.colors(
            containerColor = PleyColors.Surface3,
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
        glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
        contentPadding = PaddingValues(horizontal = 16.dp),
    ) {
        Icon(PleyIcons.ListVideo, contentDescription = null, size = 20.dp, tint = LocalContentColor.current)
        Spacer(Modifier.width(8.dp))
        Text(label, style = PleyType.Tv.Label, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.clearAndSetSemantics { })
        if (count > 0) {
            Spacer(Modifier.width(8.dp))
            CountBadge(count, alarm, focused)
        }
    }
}

/** A number on its own small pill, "99+" past 99: red while [alarm], else the accent (inverted on a focused chip or entry). */
@Composable
internal fun CountBadge(count: Int, alarm: Boolean, focused: Boolean, modifier: Modifier = Modifier) {
    val fill = when {
        alarm -> PleyColors.Danger
        focused -> PleyColors.OnAccent
        else -> PleyColors.Accent
    }
    val ink = when {
        alarm -> PleyColors.Bg
        focused -> PleyColors.AccentBright
        else -> PleyColors.OnAccent
    }
    Box(
        modifier
            .heightIn(min = 20.dp)
            .widthIn(min = 20.dp)
            .background(fill, PleyShape.Circle)
            .padding(horizontal = 6.dp)
            .clearAndSetSemantics { },
        contentAlignment = Alignment.Center,
    ) {
        BasicText(if (count > MAX_COUNT) "$MAX_COUNT+" else count.toString(), style = PleyType.Tv.Badge.copy(color = ink))
    }
}

/** A row's place while its first page comes: its title and still wells; nothing in it takes focus. */
@Composable
private fun RowSkeleton(title: String) {
    val skeleton = rememberSkeleton(animated = false)
    Column {
        BasicText(title, style = PleyType.Tv.Section.copy(color = PleyColors.Fg), maxLines = 1, modifier = Modifier.padding(start = PleyTv.Gutter))
        Spacer(Modifier.height(PleyTv.HeaderToTiles))
        Row(Modifier.padding(start = PleyTv.Gutter), horizontalArrangement = Arrangement.spacedBy(PleyTv.TileGap)) {
            repeat(SKELETON_TILES) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Tv) }
        }
    }
}

/** Discover while its first list comes, after a moment: still blocks where the hero and the first row go. No spinner. */
@Composable
private fun DiscoverSkeleton() {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SKELETON_DELAY_MS)
        shown = true
    }
    val loading = stringResource(R.string.tv_requests_loading)
    Box(Modifier.fillMaxSize().semantics { contentDescription = loading }) {
        if (!shown) return@Box
        val skeleton = rememberSkeleton(animated = false)
        @Composable
        fun block(x: Dp, y: Dp, w: Dp, h: Dp) = Box(Modifier.offset(x, y).size(w, h).skeleton(skeleton, PleyShape.Badge))
        block(PleyTv.Gutter, 131.dp, 300.dp, 38.dp)
        block(PleyTv.Gutter, 180.dp, 200.dp, 18.dp)
        block(PleyTv.Gutter, 212.dp, 440.dp, 15.dp)
        block(PleyTv.Gutter, 272.dp, 128.dp, PleyTv.ButtonHeight)
        block(PleyTv.Gutter, 356.dp, 180.dp, 18.dp)
        val tile = TileKind.Poster
        repeat(SKELETON_TILES) { i ->
            Box(Modifier.offset(PleyTv.Gutter + (tile.tvWidth + tile.tvGap) * i, 392.dp).size(tile.tvWidth, tile.tvHeight).skeleton(skeleton, TileStyle.Tv.shape(tile)))
        }
    }
}

/** The hero, down to the first row's header at y = 356 less the gap between rows. */
private val HeroHeight = 356.dp - PleyTv.RowGap

/** The hero's title box: two lines end at y = 172 whatever their number. */
private val HeroTop = 84.dp
private val TitleBox = 88.dp

/** The text column over backdrops: x 96 to 560. */
private val TextColumn = 464.dp
private val SummaryColor = PleyColors.Fg.copy(alpha = 0.85f)

/** Tiles from a row's end at which its next page is asked for. */
private const val NEAR_END = 6
private const val SKELETON_TILES = 7
private const val SKELETON_DELAY_MS = 300L

/** Past it, a count says "99+". */
private const val MAX_COUNT = 99

private const val HERO_KEY = "pley.discover.hero"
private const val YOURS_KEY = "pley.discover.yours"
private const val CREDIT_KEY = "pley.discover.credit"
private const val ROW_TYPE = "pley.discover.row"

internal const val TV_DISCOVER = "pley.tv.discover"
internal const val TV_DISCOVER_ROWS = "pley.tv.discover.rows"
internal const val TV_DISCOVER_HERO = "pley.tv.discover.hero"
internal const val TV_DISCOVER_REQUESTS = "pley.tv.discover.requests"
