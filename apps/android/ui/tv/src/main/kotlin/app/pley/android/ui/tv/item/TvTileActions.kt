package app.pley.android.ui.tv.item

import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import androidx.lifecycle.viewmodel.compose.rememberViewModelStoreOwner
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemActionsUiState
import app.pley.android.screens.item.ItemActionsViewModel
import app.pley.android.screens.item.ItemEvent
import app.pley.android.screens.item.destination
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.focusRow
import app.pley.android.ui.tv.shell.LocalTvNavigator
import kotlinx.coroutines.flow.first

/** Opens a tile's actions (D-pad rule 6: Select held, or Menu). */
@Stable
fun interface TvTileActions {
    /** The actions of [itemId]; from a Continue Watching row ([continueWatching]), leaving it too. */
    fun open(itemId: Long, continueWatching: Boolean)
}

/** The shell's tile actions, a side panel over the page ([TvTileActionsPanel]); none outside the shell. */
val LocalTvTileActions = staticCompositionLocalOf<TvTileActions?> { null }

/**
 * What a tile's actions open in: the shell's side panel, or where there
 * is no shell (a screen on its own, in a test) the actions' page.
 */
@Composable
internal fun tileActions(): TvTileActions {
    LocalTvTileActions.current?.let { return it }
    val navigator = LocalTvNavigator.current
    return remember(navigator) { TvTileActions { id, continueWatching -> navigator.open(PleyKey.ItemActions(id, continueWatching)) } }
}

/** A tile's actions asked for, on [page]: each request is its own, the same item's again too. */
@Immutable
internal class TileActionsRequest(val itemId: Long, val continueWatching: Boolean, val page: Any?)

/**
 * The shell's [TvTileActions]: the request on its way or showing, and the
 * page's focus, kept while the panel has it and given back when it closes
 * (D-pad rule 2). [page] is the page's focus group; [top] the page
 * showing, which a request belongs to.
 */
@Stable
internal class TvTileActionsState(private val page: FocusRequester, private val top: () -> Any?) : TvTileActions {
    var request by mutableStateOf<TileActionsRequest?>(null)
        private set

    override fun open(itemId: Long, continueWatching: Boolean) {
        // The page has focus again (a panel that went with its page never gave it back): this
        // panel hasn't taken it yet.
        left = false
        request = TileActionsRequest(itemId, continueWatching, top())
    }

    /** Whether the panel took focus from the page ([leavePage]). */
    private var left = false

    /**
     * The panel takes focus with [take]: which element of the page had it is
     * kept first. Only once [take] got it has the page lost it; until then
     * the page still has it, and a Back meanwhile leaves it there (asking
     * the page for focus it holds would leave nothing focused).
     */
    internal suspend fun leavePage(take: suspend () -> Boolean) {
        runCatching { page.saveFocusedChild() }
        if (take()) left = true
    }

    /**
     * The panel closes: focus goes back to where it was on the page, or to
     * its first element. Closed before it took focus (Back as it slid in),
     * the page still has it, where it was.
     */
    internal fun backToPage() {
        if (!left) return
        left = false
        runCatching { if (!page.restoreFocusedChild()) page.requestFocus() }
    }
}

/**
 * A tile's actions in a side panel over the page they were asked on
 * (design spec §2.7, D-pad rule 6), as the item page shows its tiles':
 * from Home, a library's grid and Recommended, search, a collection, a
 * person's and an artist's page. The panel slides in once the item's actions are in: its
 * title, then Play (or Resume and From start), the watched mark, leaving
 * Continue Watching, My List, Add to playlist and Go to show or season,
 * with focus on the first. Back closes it; a choice closes it and runs,
 * focus back on the tile first, so a tile the choice takes away hands
 * focus on as it does in its row. The page underneath stays as it was.
 *
 * Actions that fail to load, and what a choice comes to (nothing to play,
 * a change the server refused), show as a toast, after the panel has gone
 * too. A request whose page has gone before its actions came is dropped.
 */
@Composable
internal fun TvTileActionsPanel(state: TvTileActionsState, page: Any?, navigator: Navigator) {
    val request = state.request ?: return
    // Without the app's graph (the shell's own tests) there are no ViewModels to make.
    if (LocalContext.current.applicationContext !is PleyGraphOwner) return
    key(request) {
        // A ViewModel for each request, cleared when the next comes: it outlives the panel to tell
        // what a choice came to.
        val owner = rememberViewModelStoreOwner()
        CompositionLocalProvider(LocalViewModelStoreOwner provides owner) {
            TileActions(request, page, state, navigator)
        }
    }
}

private enum class Phase { Waiting, Shown, Closed }

@Composable
private fun TileActions(request: TileActionsRequest, page: Any?, state: TvTileActionsState, navigator: Navigator) {
    val model: ItemActionsViewModel = viewModel(factory = ItemActionsViewModel.factory(request.itemId, request.continueWatching))
    val toasts = remember { TvToastState() }
    val resources = LocalResources.current
    var phase by remember { mutableStateOf(Phase.Waiting) }
    var shown by remember { mutableStateOf<ItemActionsUiState.Ready?>(null) }
    val pageNow by rememberUpdatedState(page)
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                ItemEvent.NothingToPlay -> toasts.show(resources.getString(R.string.tv_item_nothing_to_play))
                is ItemEvent.Failed -> toasts.report(event.error)
            }
        }
    }
    // The actions, once: the panel shows what they were when it opened.
    LaunchedEffect(model) {
        val actions = model.state.first { it !is ItemActionsUiState.Loading }
        phase = when {
            pageNow != request.page -> Phase.Closed
            actions is ItemActionsUiState.Failed -> {
                toasts.report(actions.error)
                Phase.Closed
            }
            actions is ItemActionsUiState.Ready && actions.actions.isNotEmpty() -> {
                shown = actions
                Phase.Shown
            }
            else -> Phase.Closed
        }
    }
    // Another page took its place (the server signed the TV out, say): the panel goes with it.
    if (phase == Phase.Shown && page != request.page) {
        LaunchedEffect(Unit) { phase = Phase.Closed }
    }
    val close: () -> Unit = {
        if (phase == Phase.Shown) {
            phase = Phase.Closed
            state.backToPage()
        }
    }
    TvSidePanel(visible = phase == Phase.Shown, onDismiss = close, modifier = Modifier.testTag(TV_TILE_ACTIONS)) {
        val ready = shown ?: return@TvSidePanel
        val first = remember { FocusRequester() }
        val list = rememberLazyListState()
        LaunchedEffect(Unit) {
            // Once the panel is placed, as the item page's panels do; where focus was on the page is kept first.
            withFrameNanos { }
            withFrameNanos { }
            // Back within those frames, or before the first row takes focus, closed it: focus stays
            // on the page, not in a panel going away.
            if (phase != Phase.Shown) return@LaunchedEffect
            state.leavePage { list.focusRow(first) { phase == Phase.Shown } }
        }
        LazyColumn(
            state = list,
            modifier = Modifier
                .fillMaxSize()
                // Focus stays in the panel until a choice or Back closes it.
                .focusProperties { onExit = { if (phase == Phase.Shown) cancelFocusChange() } }
                .focusGroup(),
        ) {
            item(key = "title", contentType = PANEL_TITLE) { PanelTitle(ready.title) }
            ready.actions.forEachIndexed { i, action ->
                item(key = action.name, contentType = PANEL_ROW) {
                    val (label, icon) = actionLook(action)
                    PanelRow(label, icon = icon, focusRequester = if (i == 0) first else null, onClick = {
                        close()
                        if (!model.perform(action)) destination(action, ready.item)?.let(navigator::open)
                    })
                }
            }
        }
    }
    TvToastHost(toasts)
}

internal const val TV_TILE_ACTIONS = "pley.tv.tile.actions"
