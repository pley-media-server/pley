package app.pley.android.ui.phone.requests

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.DiscoverTitle
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.requests.Ask
import app.pley.android.screens.requests.DiscoverHero
import app.pley.android.screens.requests.DiscoverRow
import app.pley.android.screens.requests.DiscoverUiState
import app.pley.android.screens.requests.DiscoverViewModel
import app.pley.android.screens.requests.MainAction
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.badgeOf
import app.pley.android.screens.requests.opens
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.kit.HubRow
import app.pley.android.ui.phone.kit.HubRowModel
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.kit.rememberHubRowState
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlin.math.roundToInt
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter

/**
 * Discover on the phone: a hero for the first trending
 * title, with its main action; rows of posters with their status badges
 * (Trending this week, Popular movies, Popular shows, Coming to cinemas,
 * Popular anime), each read as it comes into view and page by page as it
 * scrolls; the person's requests; and TMDB's credit. TMDB is searched
 * from Search. When requests can't be used, the page says why, and only
 * that.
 */
@Composable
internal fun PhoneDiscoverScreen() {
    val vm: DiscoverViewModel = viewModel { DiscoverViewModel(RequestsDeps.of(screensGraph())) }
    val state by vm.state.collectAsStateWithLifecycle()
    val texts = rememberRequestTexts()
    RequestEvents(vm.events, texts) { state.gate.summary }
    DiscoverContent(state, texts, vm)
}

@Composable
private fun DiscoverContent(state: DiscoverUiState, texts: RequestTexts, vm: DiscoverViewModel) {
    val navigator = LocalPhoneNavigator.current
    val list = rememberLazyListState()
    val open: (DiscoverTitle) -> Unit = { navigator.open(PleyKey.DiscoverTitle(it.kind, it.tmdbId)) }
    Column(Modifier.fillMaxSize().testTag(DISCOVER_PAGE)) {
        PleyTopAppBar(
            title = stringResource(R.string.requests_discover),
            onBack = navigator::back,
            scrolled = { list.canScrollBackward },
        )
        val problem = state.problem
        if (problem != null) {
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                RequestProblemState(problem, state.gate.summary, texts, stringResource(R.string.requests_discover_failed), onRetry = vm::retry)
            }
            return@Column
        }
        DiscoverHome(state, texts, list, vm, open)
    }
}

/** The hero, the rows, the person's requests and the credit. */
@Composable
private fun DiscoverHome(state: DiscoverUiState, texts: RequestTexts, list: LazyListState, vm: DiscoverViewModel, open: (DiscoverTitle) -> Unit) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val skeleton = rememberSkeleton(animated = true)
    LazyColumn(
        state = list,
        modifier = Modifier.fillMaxSize().testTag(DISCOVER_ROWS),
        contentPadding = PaddingValues(top = 4.dp, bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(PleyPhone.SectionGap),
    ) {
        item(key = "hero", contentType = "hero") {
            val hero = state.hero
            if (hero != null) {
                Hero(hero, state, texts, onDetails = { open(hero.title) }, onAction = vm::heroAction)
            } else {
                HeroSkeleton(skeleton)
            }
        }
        state.rows.filter { it.shows }.forEach { row ->
            item(key = row.list.wire, contentType = "row") {
                LaunchedEffect(row.list) { vm.showRow(row.list) }
                TitleRow(row, state.origin, texts, skeleton, onOpen = open, onMore = { vm.more(row.list) })
            }
        }
        if (state.yours.isNotEmpty()) {
            item(key = "yours", contentType = "yours") {
                val tiles = remember(state.yours, state.origin, texts, state.manage) {
                    state.yours.map { requestTile(it, state.origin, texts, state.manage) }
                }
                val mayAsk = state.gate.mayAsk
                HubRow(
                    row = HubRowModel("yours", stringResource(R.string.requests_yours), TileKind.Poster, tiles),
                    onOpen = { tile -> state.yours.firstOrNull { it.id == tile.id }?.let { opens(it, mayAsk) }?.let(navigator::open) },
                    skeleton = skeleton,
                    onTitleClick = { navigator.open(PleyKey.Requests) },
                )
            }
        }
        item(key = "credit", contentType = "credit") {
            TmdbCredit(Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd))
        }
    }
}

/** A row shows once it has titles, while it's read, and until it comes into view; never empty once read. */
private val DiscoverRow.shows: Boolean
    get() = pages.titles.isNotEmpty() || (pages.problem == null && (pages.page == 0 || pages.loading))

/** One of Discover's rows: skeleton cards until its first page comes, the next page asked for near its end. */
@Composable
private fun TitleRow(row: DiscoverRow, origin: String, texts: RequestTexts, skeleton: SkeletonState, onOpen: (DiscoverTitle) -> Unit, onMore: () -> Unit) {
    val titles = row.pages.titles
    val tiles = remember(titles, origin, texts) { if (titles.isEmpty()) null else titles.map { titleTile(it, origin, texts) } }
    val state = rememberHubRowState()
    LaunchedEffect(state) {
        snapshotFlow {
            val info = state.layoutInfo
            info.totalItemsCount > 0 && (info.visibleItemsInfo.lastOrNull()?.index ?: 0) >= info.totalItemsCount - ROW_AHEAD
        }
            .distinctUntilChanged()
            .filter { it }
            .collect { onMore() }
    }
    HubRow(
        row = HubRowModel(row.list.wire, texts.row(row.list), TileKind.Poster, tiles),
        onOpen = { tile -> titles.firstOrNull { it.tileId == tile.id }?.let(onOpen) },
        skeleton = skeleton,
        state = state,
    )
}

/**
 * The first trending title: its backdrop fading into the page with its
 * kind and year, title and a line of overview over the fade; its main
 * action and Details under it. The picture opens its page too.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Hero(hero: DiscoverHero, state: DiscoverUiState, texts: RequestTexts, onDetails: () -> Unit, onAction: () -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val tier = LocalPhoneDeviceTier.current
    val title = hero.title
    val image = remember(title.backdropUrl, title.posterUrl, state.origin, tier) {
        (ArtworkImage.of(title.backdropUrl, state.origin) ?: ArtworkImage.of(title.posterUrl, state.origin))
            ?.copy(maxWidth = ArtworkImage.backdropCap(tier))
    }
    val badge = badgeOf(title)?.let(texts::badge)
    val tallest = heroMaxHeight()
    Column(Modifier.fillMaxWidth().padding(start = metrics.marginStart, end = metrics.marginEnd).testTag(DISCOVER_HERO)) {
        Box(
            Modifier
                .fillMaxWidth()
                .heroSize(tallest)
                .clip(PleyShape.Card)
                .clickable(role = Role.Button, onClickLabel = stringResource(R.string.requests_details), onClick = onDetails),
        ) {
            Artwork(
                image = image,
                title = title.title,
                modifier = Modifier
                    .fillMaxSize()
                    .drawWithCache {
                        val fade = Brush.verticalGradient(
                            0f to Color.Transparent,
                            1f to PleyColors.Bg.copy(alpha = 0.92f),
                            startY = size.height * 0.35f,
                            endY = size.height,
                        )
                        onDrawWithContent {
                            drawContent()
                            drawRect(fade)
                        }
                    },
            )
            Column(Modifier.align(Alignment.BottomStart).padding(16.dp)) {
                Text(
                    listOfNotNull(titleLine(title.year, title.kind, texts), badge).joinToString(" · "),
                    style = MaterialTheme.typography.labelMedium,
                    color = PleyColors.Muted,
                )
                Text(
                    title.title,
                    style = MaterialTheme.typography.headlineSmall,
                    color = PleyColors.Fg,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.semantics { heading() },
                )
                if (title.overview.isNotBlank()) {
                    Text(
                        title.overview,
                        style = MaterialTheme.typography.bodyMedium,
                        color = PleyColors.Fg.copy(alpha = 0.85f),
                        maxLines = HERO_OVERVIEW_LINES,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            heroLabel(hero.action)?.let { label ->
                val limited = (hero.action as? MainAction.Request)?.ask == Ask.Limit
                PrimaryButton(label, onClick = onAction, icon = heroIcon(hero.action), enabled = !limited && !state.asking)
            }
            TonalButton(stringResource(R.string.requests_details), onClick = onDetails, icon = PleyIcons.Info)
        }
        when (val action = hero.action) {
            is MainAction.Mine -> action.request?.let { RequestStage(it, state.manage, texts, Modifier.padding(top = 10.dp)) }
            is MainAction.Following -> action.request?.let { RequestStage(it, state.manage, texts, Modifier.padding(top = 10.dp)) }
            is MainAction.Declined -> Text(texts.declined(action), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = Modifier.padding(top = 10.dp))
            is MainAction.Request -> if (action.ask == Ask.Limit) AllowanceLines(state.allowance, title.kind, texts, Modifier.padding(top = 10.dp))
            else -> Unit
        }
    }
}

/** The hero's main button, for the actions it does from here; the rest are on the title's page. */
@Composable
private fun heroLabel(action: MainAction): String? = when (action) {
    is MainAction.Request -> if (action.ask == Ask.NoAccess) null else stringResource(R.string.requests_request)
    is MainAction.TellMeToo -> if (action.ask == Ask.Yes) stringResource(R.string.requests_tell_me_too) else null
    is MainAction.Open -> action.itemId?.let { stringResource(R.string.requests_open) }
    is MainAction.Partial -> when {
        action.itemId != null -> stringResource(R.string.requests_open)
        action.more == Ask.Yes -> stringResource(R.string.requests_more_seasons)
        else -> null
    }
    else -> null
}

private fun heroIcon(action: MainAction) = when (action) {
    is MainAction.Open -> PleyIcons.Play
    is MainAction.Partial -> if (action.itemId != null) PleyIcons.Play else PleyIcons.Plus
    is MainAction.TellMeToo -> PleyIcons.BookmarkPlus
    else -> PleyIcons.Plus
}

@Composable
private fun HeroSkeleton(skeleton: SkeletonState) {
    val metrics = LocalPhoneMetrics.current
    val tallest = heroMaxHeight()
    Column(Modifier.fillMaxWidth().padding(start = metrics.marginStart, end = metrics.marginEnd)) {
        Box(Modifier.fillMaxWidth().heroSize(tallest).skeleton(skeleton, PleyShape.Card))
        Spacer(Modifier.height(12.dp))
        Box(Modifier.fillMaxWidth(0.4f).height(48.dp).skeleton(skeleton, PleyShape.Control))
    }
}

/** The hero's tallest: half the window, so its buttons show on a tablet held sideways. */
@Composable
private fun heroMaxHeight(): Dp {
    val window = with(LocalDensity.current) { LocalWindowInfo.current.containerSize.height.toDp() }
    return if (window > 0.dp) window * HERO_MAX_SHARE else Dp.Infinity
}

/** A backdrop's 16:9 across the page, lower where that would pass [tallest] (the picture cropped). */
private fun Modifier.heroSize(tallest: Dp): Modifier = layout { measurable, constraints ->
    val width = constraints.maxWidth
    val natural = (width / HERO_RATIO).roundToInt()
    val height = if (tallest == Dp.Infinity) natural else minOf(natural, tallest.roundToPx())
    val placeable = measurable.measure(Constraints.fixed(width, height))
    layout(width, height) { placeable.place(0, 0) }
}

/** The hero's picture: a backdrop's 16:9, so the first row shows under it. */
private const val HERO_RATIO = 16f / 9f

private const val HERO_MAX_SHARE = 0.5f

private const val HERO_OVERVIEW_LINES = 2

/** Cards from a row's end at which its next page is asked for. */
private const val ROW_AHEAD = 4

internal const val DISCOVER_PAGE = "pley.discover"
internal const val DISCOVER_ROWS = "pley.discover.rows"
internal const val DISCOVER_HERO = "pley.discover.hero"
