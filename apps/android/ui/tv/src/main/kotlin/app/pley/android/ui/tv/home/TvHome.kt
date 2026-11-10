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
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.compose.currentStateAsState
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.TestTags
import app.pley.android.common.Tier
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.text.asString
import app.pley.android.model.HubKeys
import app.pley.android.model.ItemSummary
import app.pley.android.screens.Navigator
import app.pley.android.screens.home.HomeInfo
import app.pley.android.screens.home.HomeMessage
import app.pley.android.screens.home.HomeState
import app.pley.android.screens.home.HomeViewModel
import app.pley.android.screens.home.homeInfo
import app.pley.android.screens.home.pageOf
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.tileActions
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.SeeAll
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.rememberBackdropPrefetcher
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberRested
import app.pley.android.ui.tv.kit.tvRowColumnHero
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.ReportOffline
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import app.pley.android.ui.tv.theme.LocalTvTier
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.launch

/**
 * Home on TV (design spec §2.2, §4.1; TV addendum §3.4).
 *
 * At rest: the hero for the featured titles (the web's banner: the newest
 * unwatched movies and shows, [app.pley.android.screens.home.featuredOf]),
 * each with its backdrop, why it is there, Play and My List, with initial
 * focus on Play; without any, the first Continue Watching item. Then the
 * server's rows. The hero turns to the next title every [TURN_MS] while
 * nothing is pressed (each key starts the time over); with less motion
 * asked for, it never turns by itself. Right from its last button stays
 * there. Dashes at the top right show which title is on and how
 * long it has left; Up from the buttons reaches them, and Left and Right
 * there turn the titles. Down moves into the rows: the hero's buttons go,
 * the rows come up under a compact info block about the focused tile, and
 * the backdrop and the info block follow the focused tile at once (a
 * held key, where it lands). Nothing plays on its own.
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
    ReportHomeStatus(vm)

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
            is HomeState.Ready -> if (shown.heroes.isNotEmpty()) TvHomeRows(shown, state.origin, vm, navigator)
        }
        if (shown != null && shown.state != HomeState.Loading) MarkHomeDrawn(shown)
        TvHomeToast(message, onShown = { message = null })
    }
}

/**
 * The status of what Home shows, to the offline notice: up while it is
 * what was saved, down once the server answers. Read here, its changes
 * compose nothing else.
 */
@Composable
private fun ReportHomeStatus(vm: HomeViewModel) {
    val status by vm.status.collectAsStateWithLifecycle()
    ReportOffline(status)
}

/** The tile focus is on, in its row. */
@Immutable
private class FocusedTile(val item: ItemSummary, val row: TvHomeRow)

/**
 * Once focus rests on a tile ([rememberRested]), its details are asked
 * for, for the info block's summary and badges, and the next tiles' ahead:
 * what costs a request waits for focus to rest.
 */
@Composable
private fun RestedFocus(focused: State<FocusedTile?>, vm: HomeViewModel) {
    val rested = rememberRested(focused.value)
    LaunchedEffect(rested) { rested?.let { vm.focus(it.item, it.row.after(it.item)) } }
}

/**
 * The backdrop and the info block. At rest they are the [hero]'s. In the
 * rows they follow the focused tile at once: after a press, and where a
 * held key lands ([KeyRepeatGate.FOLLOW_MS]), never through each tile the
 * key passes. They say what the tile's row lists of it, then the model's
 * word on it, which has its summary and badges, once focus has rested
 * there ([RestedFocus]). Only this and [RestedFocus] read [focused] and
 * the model's word, so a focus move composes these and nothing else on
 * the page.
 */
@Composable
private fun HomeAbout(
    hero: TvHero,
    focused: State<FocusedTile?>,
    vm: HomeViewModel,
    inRows: Boolean,
    heroAlpha: State<Float>,
    origin: String,
    tier: Tier,
) {
    val tile = rememberRested(focused.value, KeyRepeatGate.FOLLOW_MS)
    val listed = remember(tile) { tile?.let { homeInfo(it.item, null) } }
    val detailed by vm.info.collectAsStateWithLifecycle()
    val about: HomeInfo = when {
        !inRows -> hero.info
        listed == null -> detailed ?: hero.info
        else -> detailed?.takeIf { it.item.id == listed.item.id } ?: listed
    }
    val backdrop = remember(about.item.id, origin, tier) {
        if (about.item.id == hero.info.item.id) hero.backdrop else TvHomeModel.backdropOf(about.item, origin, tier)
    }
    TvBackdrop(backdrop, about.title, scrim = if (inRows) BackdropScrim.Rows else BackdropScrim.Hero)
    // Only in the rows is it there for screen readers too; at rest the hero says the same.
    TvInfoBlock(about, Modifier.graphicsLayer { alpha = 1f - heroAlpha.value }.then(if (inRows) Modifier else Modifier.clearAndSetSemantics { }))
}

@Composable
private fun TvHomeRows(model: TvHomeModel, origin: String, vm: HomeViewModel, navigator: Navigator) {
    val tier = LocalTvTier.current
    val rail = LocalTvRailFocus.current
    val actions = tileActions()
    val scope = rememberCoroutineScope()
    val memory = rememberFocusMemory()
    val resume = remember { FocusRequester() }
    // Composes about a row ahead, so the next row's pictures are on their way before focus gets there.
    val column = rememberLazyListState(cacheWindow = remember { LazyLayoutCacheWindow(ahead = RowAhead, behind = 0.dp) })
    var inRows by rememberSaveable { mutableStateOf(false) }
    var hasFocus by remember { mutableStateOf(false) }
    val focused = remember { mutableStateOf<FocusedTile?>(null) }
    RestedFocus(focused, vm)
    val backdrops = rememberBackdropPrefetcher()
    // Read as it is drawn: its frames compose nothing.
    val heroAlpha = animateFloatAsState(if (inRows) 0f else 1f, PleyMotion.reveal(), label = "hero")
    // What the rows' callbacks read when they run: rows that didn't change keep theirs, and aren't composed again.
    val current by rememberUpdatedState(model)

    // The hero's turns. Keys go to the clock as events, not state: a key press composes nothing.
    val heroes = model.heroes
    var turn by rememberSaveable { mutableIntStateOf(0) }
    // The titles can change under the banner (the top rated ones come a moment after the newest):
    // the one that is on stays on, wherever it now is among them.
    var on by rememberSaveable { mutableStateOf<Long?>(null) }
    val at = remember(heroes, turn, on) { heroes.indexOfFirst { it.info.item.id == on }.takeIf { it >= 0 } ?: (turn % heroes.size) }
    val hero = heroes[at]
    SideEffect {
        turn = at
        on = hero.info.item.id
    }
    val titles by rememberUpdatedState(heroes)
    val keys = remember { MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST) }
    val calm = reducedMotion()
    // Under another page, or with the app away, the hero waits: its clock runs only in front.
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val inFront = lifecycle.isAtLeast(Lifecycle.State.RESUMED)
    // How far the title on is through its time: its dash fills with it, read as the dash draws.
    // Small timed steps rather than an animation: each draws the dashes again and nothing else.
    val time = remember { mutableFloatStateOf(1f) }
    // Whether focus is on the dashes. Only the clock reads it, when it turns: it composes nothing.
    val onDashes = remember { mutableStateOf(false) }
    // The title on and its time change together: a dash coming on is never drawn with the last one's time.
    val turnTo: (Int) -> Unit = { index ->
        val to = index.mod(titles.size)
        turn = to
        on = titles[to].info.item.id
        if (!calm) time.floatValue = 0f
    }
    LaunchedEffect(turn, inRows, heroes.size, calm, inFront) {
        // The dash stays as it is under the page that covers it, and starts over when that one goes.
        if (!inFront) return@LaunchedEffect
        if (inRows || heroes.size < 2 || calm) {
            time.floatValue = 1f
            return@LaunchedEffect
        }
        val timed = turn
        // TURN_MS with no key pressed: each press starts the time over, and the dash with it.
        keys.onStart { emit(Unit) }.collectLatest {
            var gone = 0L
            time.floatValue = 0f
            while (gone < TURN_MS) {
                delay(FILL_STEP_MS)
                gone += FILL_STEP_MS
                time.floatValue = gone.toFloat() / TURN_MS
            }
            // A turn made by hand at this very moment has its own time coming: it isn't turned past.
            if (turn != timed) return@collectLatest
            turnTo(timed + 1)
            // Its buttons may not be the last one's (From start): focus starts again on Play. The dashes stay, and keep theirs.
            if (hasFocus && !onDashes.value) runCatching { resume.requestFocus() }
        }
    }
    // The dashes turn the titles under their own focus.
    val step: (Int) -> Unit = { by -> turnTo(turn + by) }
    // With one title left to feature the dashes go, and focus that was on them goes to Play.
    // Read as they go and not observed: focus on the dashes composes nothing.
    val dashesGone = heroes.size < 2 && Snapshot.withoutReadObservation { onDashes.value }
    LaunchedEffect(dashesGone) { if (dashesGone) runCatching { resume.requestFocus() } }

    // A tile its actions took away (Remove from Continue Watching): focus comes back to its neighbour.
    memory.settle(model::hasTile)

    // Back from the rows: the page goes back to its hero (focus on Resume), then to the rail.
    BackHandler(enabled = inRows && hasFocus) {
        inRows = false
        runCatching { resume.requestFocus() }
        scope.launch { column.scrollToItem(0) }
        rail?.focusCurrent()
    }

    Box(
        Modifier
            .fillMaxSize()
            .onFocusChanged { hasFocus = it.hasFocus }
            .onPreviewKeyEvent {
                keys.tryEmit(Unit)
                false
            },
    ) {
        HomeAbout(hero, focused, vm, inRows, heroAlpha, origin, tier)
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
                    onList = { vm.setInList(hero.info.item, !hero.info.inList) },
                    modifier = Modifier.graphicsLayer { alpha = heroAlpha.value }.tvRowColumnHero(),
                    label = hero.label?.asString(),
                    turns = heroes.size,
                    turn = at,
                    progress = { time.floatValue },
                    onTurn = step,
                    onDashesFocus = { onDashes.value = it },
                )
            }
            model.rows.forEach { row ->
                item(row.key, contentType = row.kind) {
                    val seeAllLabel = stringResource(R.string.tv_home_see_all)
                    val seeAll = remember(row.seeAll, seeAllLabel) { row.seeAll?.let { key -> SeeAll(seeAllLabel) { navigator.open(key) } } }
                    TvRow(
                        rowKey = row.key,
                        title = row.title,
                        tiles = row.tiles,
                        kind = row.kind,
                        memory = memory,
                        seeAll = seeAll,
                        onOpen = { tile -> row.itemOf(tile)?.let { navigator.open(pageOf(it)) } },
                        onActions = { tile ->
                            memory.mayLeave("${row.key}/${tile.id}", current.standIn(row.key, tile.id))
                            actions.open(tile.id, continueWatching = HubKeys.isContinueWatching(row.key))
                        },
                        onFocus = { tile ->
                            inRows = true
                            row.itemOf(tile)?.let {
                                focused.value = FocusedTile(it, row)
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

/**
 * The hero, down to the first row's header at y = 332 less the gap between
 * rows: Continue Watching, the first row, shows whole under it, its
 * pictures and the two lines under them, which end 7 dp above the
 * screen's bottom edge.
 */
internal val HeroHeight = 332.dp - PleyTv.RowGap

/** How long each featured title stays in the hero while nothing is pressed: the web's banner's. */
private const val TURN_MS = 9_000L

/** The dash of the title on fills in steps this far apart: about a pixel each at 1080p, five small draws a second. */
private const val FILL_STEP_MS = 200L

/** About one row of posters: the next row is composed before focus reaches it. */
private val RowAhead = 300.dp

internal const val HERO_KEY = "pley.home.hero"
