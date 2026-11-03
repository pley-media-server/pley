@file:OptIn(ExperimentalFoundationApi::class)

package app.pley.android.ui.tv.home

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.layout.LazyLayoutCacheWindow
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.TestTags
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.image.rememberImagePrefetcher
import app.pley.android.model.HubKeys
import app.pley.android.model.ItemSummary
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.HomeInfo
import app.pley.android.screens.home.HomeMessage
import app.pley.android.screens.home.HomeState
import app.pley.android.screens.home.HomeViewModel
import app.pley.android.screens.home.pageOf
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.SeeAll
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.rememberBackdropPrefetcher
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberRested
import app.pley.android.ui.tv.kit.rememberTvRowState
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.ReportOffline
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.launch

/**
 * Home on TV (design spec §2.2, §4.1; TV addendum §3.4).
 *
 * At rest: the hero for the first Continue Watching item (its backdrop,
 * progress and Resume, with initial focus), then the server's rows. Down
 * moves into the rows: the hero's buttons go, the rows come up under a
 * compact info block about the focused tile, and the backdrop and the info
 * block follow the tile once focus has rested about 400 ms. Nothing
 * rotates or plays on its own.
 *
 * Back from the rows gives focus to the rail's Home and puts the page back
 * at its hero, where focus lands when the rail closes. Coming back from an
 * item, focus lands on the tile it was opened from; when that tile's
 * actions took it out of its row, on its neighbour.
 */
@Composable
internal fun TvHomeScreen(vm: HomeViewModel = viewModel(factory = HomeViewModel.Factory)) {
    val tier = LocalTvTier.current
    val model by vm.present(TvHomeModel.shaping(tier)).collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    var message by remember { mutableStateOf<HomeMessage?>(null) }
    LaunchedEffect(vm) { vm.messages.collect { message = it } }
    // The offline notice: up while what shows is what was saved, down once the server answers.
    ReportOffline(
        when (val reported = model?.state) {
            is HomeState.Ready -> reported.error?.let { Loadable.Failed(it, Unit) } ?: Loadable.Ready(Unit, reported.fresh)
            is HomeState.Failed -> Loadable.Failed(reported.error, Unit)
            else -> null
        },
    )

    Box(Modifier.fillMaxSize()) {
        val shown = model
        when (val state = shown?.state) {
            // The first shaping takes a moment: an empty page, never a flash of something else.
            null -> Unit
            HomeState.Loading -> TvHomeSkeleton()
            is HomeState.Failed -> TvHomeNotice(
                title = failedTitle(state.error, stringResource(R.string.tv_home_failed)),
                body = failedBody(state.error),
                retry = vm::refresh,
            )
            HomeState.NoLibraries -> TvHomeNotice(
                title = stringResource(R.string.tv_home_no_libraries),
                body = stringResource(R.string.tv_home_no_libraries_body),
            )
            is HomeState.Empty -> TvHomeNotice(
                title = stringResource(if (state.scanning) R.string.tv_home_scanning else R.string.tv_home_empty),
                body = stringResource(if (state.scanning) R.string.tv_home_scanning_body else R.string.tv_home_empty_body),
            )
            is HomeState.Ready -> if (shown.hero != null) TvHomeRows(shown, shown.hero, state.origin, vm, navigator)
        }
        if (shown != null && shown.state != HomeState.Loading) MarkHomeDrawn(shown)
        TvHomeToast(message, onShown = { message = null })
    }
}

/** The tile focus is on, and the ones after it in its row. */
@Immutable
private data class FocusedTile(val item: ItemSummary, val ahead: List<ItemSummary>)

@Composable
private fun TvHomeRows(model: TvHomeModel, hero: TvHero, origin: String, vm: HomeViewModel, navigator: Navigator) {
    val tier = LocalTvTier.current
    val density = LocalDensity.current
    val rail = LocalTvRailFocus.current
    val scope = rememberCoroutineScope()
    val memory = rememberFocusMemory()
    val resume = remember { FocusRequester() }
    // Composes about a row ahead, so the next row's pictures are on their way before focus gets there.
    val column = rememberLazyListState(cacheWindow = remember { LazyLayoutCacheWindow(ahead = RowAhead, behind = 0.dp) })
    var inRows by rememberSaveable { mutableStateOf(false) }
    var hasFocus by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf<FocusedTile?>(null) }
    val rested = rememberRested(focused)
    val backdrops = rememberBackdropPrefetcher()
    LaunchedEffect(rested) { rested?.let { vm.focus(it.item, it.ahead) } }
    val info by vm.info.collectAsStateWithLifecycle()
    val heroAlpha by animateFloatAsState(if (inRows) 0f else 1f, PleyMotion.reveal(), label = "hero")

    // In the rows, the backdrop and the info block show the tile focus rests on.
    val about: HomeInfo = if (inRows) info ?: hero.info else hero.info
    // A tile its actions took away (Remove from Continue Watching): focus comes back to its neighbour.
    val tileKeys = remember(model.rows) { model.rows.flatMapTo(HashSet()) { row -> row.tiles.map { "${row.key}/${it.id}" } } }
    memory.settle { it in tileKeys }
    val backdrop = remember(about.item.id, origin, tier) {
        if (about.item.id == hero.info.item.id) hero.backdrop else TvHomeModel.backdropOf(about.item, origin, tier)
    }

    // Back from the rows: the page goes back to its hero (focus on Resume), then to the rail.
    BackHandler(enabled = inRows && hasFocus) {
        inRows = false
        runCatching { resume.requestFocus() }
        scope.launch { column.scrollToItem(0) }
        rail?.focusCurrent()
    }

    Box(Modifier.fillMaxSize().onFocusChanged { hasFocus = it.hasFocus }) {
        TvBackdrop(backdrop, about.title, scrim = if (inRows) BackdropScrim.Rows else BackdropScrim.Hero)
        // Only in the rows is it there for screen readers too; at rest the hero says the same.
        TvInfoBlock(about, Modifier.graphicsLayer { alpha = 1f - heroAlpha }.then(if (inRows) Modifier else Modifier.clearAndSetSemantics { }))
        TvRowColumn(
            state = column,
            heroHeight = HeroHeight,
            modifier = Modifier.fillMaxSize().testTag(TestTags.HOME_ROWS),
        ) {
            item(HERO_KEY, contentType = HERO_KEY) {
                TvHomeHero(
                    hero = hero.info,
                    resume = resume,
                    memory = memory,
                    onFocused = { inRows = false },
                    onPlay = { fromStart -> vm.play(hero.info.item, fromStart, navigator) },
                    onInfo = { navigator.open(pageOf(hero.info.item)) },
                    onWatched = { vm.setWatched(hero.info.item, !hero.info.watched) },
                    modifier = Modifier.graphicsLayer { alpha = heroAlpha },
                )
            }
            model.rows.forEach { row ->
                item(row.key, contentType = row.kind) {
                    val rowState = rememberTvRowState()
                    val images = remember(row.tiles) { row.tiles.map { it.image } }
                    rememberImagePrefetcher(
                        state = rowState,
                        images = images,
                        widthPx = with(density) { row.kind.tvWidth.roundToPx() },
                        heightPx = with(density) { row.kind.tvHeight.roundToPx() },
                        tier = tier,
                    )
                    val seeAllLabel = stringResource(R.string.tv_home_see_all)
                    val seeAll = remember(row.seeAll, seeAllLabel) { row.seeAll?.let { key -> SeeAll(seeAllLabel) { navigator.open(key) } } }
                    TvRow(
                        rowKey = row.key,
                        title = row.title,
                        tiles = row.tiles,
                        kind = row.kind,
                        state = rowState,
                        memory = memory,
                        seeAll = seeAll,
                        onOpen = { tile -> row.itemOf(tile)?.let { navigator.open(pageOf(it)) } },
                        onActions = { tile ->
                            memory.mayLeave("${row.key}/${tile.id}", model.standIn(row.key, tile.id))
                            navigator.open(PleyKey.ItemActions(tile.id, continueWatching = HubKeys.isContinueWatching(row.key)))
                        },
                        onFocus = { tile ->
                            inRows = true
                            row.itemOf(tile)?.let {
                                focused = FocusedTile(it, row.after(it))
                                // Its backdrop and its neighbours' load now: focus may rest on any of them next.
                                backdrops.prefetch(
                                    TvHomeModel.backdropOf(it, origin, tier),
                                    row.beside(it).map { item -> TvHomeModel.backdropOf(item, origin, tier) },
                                )
                            }
                        },
                    )
                }
            }
        }
    }
}

/** The hero, down to the first row's header at y = 384 less the gap between rows (design spec §2.2): the first row's tiles show whole under it. */
internal val HeroHeight = 384.dp - PleyTv.RowGap

/** About one row of posters: the next row is composed before focus reaches it. */
private val RowAhead = 300.dp

internal const val HERO_KEY = "pley.home.hero"
