@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)

package app.pley.android.ui.phone.home

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.layout.LazyLayoutCacheWindow
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.TestTags
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.model.HubKeys
import app.pley.android.model.ItemSummary
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.HomeMessage
import app.pley.android.screens.home.HomeProfile
import app.pley.android.screens.home.HomeState
import app.pley.android.screens.home.HomeViewModel
import app.pley.android.screens.home.pageOf
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.Avatar
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.HubRow
import app.pley.android.ui.phone.kit.HubRowModel
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.rememberHubRowState
import app.pley.android.ui.phone.shell.Brand
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import kotlinx.coroutines.delay

/**
 * Home on phones and tablets (design spec §4.1, mockup phone-home): no
 * hero; the pley mark and the profile's avatar (which opens Me) in the
 * top bar, then the server's rows: Continue Watching as 16:9 stills with
 * their progress and time left, "Recently Added in …" as posters (squares
 * for music), each title leading to its library listed that way.
 *
 * Drawn from what was saved on the first frame it can be; a skeleton or
 * an error only when nothing was. Pull to refresh asks the server again.
 * A press going down on a card asks for its page's details ahead; a long
 * press opens the item's actions.
 */
@Composable
internal fun PhoneHomeScreen(vm: HomeViewModel = viewModel(factory = HomeViewModel.Factory)) {
    val model by vm.present(PhoneHomeModel::of).collectAsStateWithLifecycle()
    val profile by vm.profile.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    val list = rememberLazyListState(cacheWindow = remember { LazyLayoutCacheWindow(ahead = RowsAhead, behind = 0.dp) })

    LaunchedEffect(vm) {
        vm.messages.collect { message ->
            when (message) {
                HomeMessage.NothingToPlay -> shell.toasts.show(resources.getString(R.string.home_nothing_to_play))
                is HomeMessage.Failed -> shell.report(message.error)
            }
        }
    }
    val state = model?.state
    // The offline banner: up while what shows is what was saved, down once the server answers.
    ReportLoad(
        when (state) {
            is HomeState.Ready -> state.error?.let { Loadable.Failed(it, Unit) } ?: Loadable.Ready(Unit, state.fresh)
            is HomeState.Failed -> Loadable.Failed(state.error, Unit)
            else -> null
        },
    )

    Column(Modifier.fillMaxSize()) {
        PleyTopAppBar(
            title = { Brand() },
            scrolled = { list.canScrollBackward },
            actions = { ProfileButton(profile, onClick = { navigator.open(PleyKey.Me) }) },
        )
        PullToRefreshBox(
            isRefreshing = (state as? HomeState.Ready)?.refreshing == true,
            onRefresh = vm::refresh,
            modifier = Modifier.weight(1f).fillMaxWidth(),
        ) {
            val shown = model
            when (state) {
                // The first shaping takes a moment: an empty page, never a flash of something else.
                null -> Unit
                HomeState.Loading -> HomeSkeleton()
                is HomeState.Failed -> Scrollable {
                    ErrorState(stringResource(R.string.home_failed), state.error, onRetry = vm::refresh)
                }
                HomeState.NoLibraries -> Scrollable {
                    EmptyState(
                        stringResource(R.string.home_no_libraries),
                        stringResource(R.string.home_no_libraries_body),
                        icon = PleyIcons.FolderPlus,
                    )
                }
                is HomeState.Empty -> Scrollable {
                    EmptyState(
                        stringResource(if (state.scanning) R.string.home_scanning else R.string.home_empty),
                        stringResource(if (state.scanning) R.string.home_scanning_body else R.string.home_empty_body),
                        icon = PleyIcons.Clapperboard,
                    )
                }
                is HomeState.Ready -> if (shown != null) HomeRows(shown, list, vm, navigator)
            }
            if (shown != null && state != HomeState.Loading) MarkHomeDrawn(shown)
        }
    }
}

@Composable
private fun HomeRows(model: PhoneHomeModel, list: LazyListState, vm: HomeViewModel, navigator: Navigator) {
    val skeleton = rememberSkeleton(animated = true)
    LazyColumn(
        state = list,
        contentPadding = PaddingValues(top = 4.dp, bottom = PleyPhone.SectionGap),
        verticalArrangement = Arrangement.spacedBy(SectionGap),
        modifier = Modifier.fillMaxSize().testTag(TestTags.HOME_ROWS),
    ) {
        items(model.rows, key = { it.model.key }, contentType = { it.model.kind }) { row ->
            val rowState = rememberHubRowState()
            // Home told again what it shows leaves its rows alone: the press's prefetch is kept while the
            // row's items are, and the handlers read the row (compared by value), never a new key.
            val prefetch = remember(rowState, row.items) { Modifier.prefetchOnPress(rowState, row.items, vm::prefetch) }
            HubRow(
                row = row.model,
                skeleton = skeleton,
                state = rowState,
                onOpen = { tile -> row.itemOf(tile)?.let { navigator.open(pageOf(it)) } },
                onLongPress = { tile -> navigator.open(PleyKey.ItemActions(tile.id, continueWatching = HubKeys.isContinueWatching(row.model.key))) },
                onTitleClick = if (row.seeAll == null) null else { { row.seeAll?.let(navigator::open) } },
                modifier = prefetch,
            )
        }
    }
}

/**
 * Home with nothing saved yet (design spec §8): skeleton rows in the
 * shapes to come, after a moment, so a Home read from what was saved never
 * flashes one.
 */
@Composable
private fun HomeSkeleton() {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SKELETON_DELAY_MS)
        shown = true
    }
    val loading = stringResource(R.string.home_loading)
    Box(Modifier.fillMaxSize().semantics { contentDescription = loading }) {
        if (!shown) return@Box
        val skeleton = rememberSkeleton(animated = true)
        Column(Modifier.padding(top = 4.dp), verticalArrangement = Arrangement.spacedBy(SectionGap)) {
            SKELETON_ROWS.forEach { row -> HubRow(row, onOpen = { }, skeleton = skeleton) }
        }
    }
}

/** A page with a sentence on it, which still pulls to refresh. */
@Composable
private fun Scrollable(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) { content() }
}

/** The profile's avatar, 32 dp in a 48 dp target: opens Me. */
@Composable
private fun ProfileButton(profile: HomeProfile?, onClick: () -> Unit) {
    if (profile == null) return
    val label = stringResource(R.string.home_profile, profile.name)
    IconButton(onClick = onClick, modifier = Modifier.semantics { contentDescription = label }) {
        Avatar(profile.name, color = profile.color, imageUrl = profile.pictureUrl, size = 32.dp, modifier = Modifier.size(32.dp))
    }
}

/**
 * Asks for an item's details as a finger goes down on its card, before the
 * tap is even sure: by the time the page opens they are usually in. Only
 * looks at the press; the card still gets it.
 */
private fun Modifier.prefetchOnPress(state: LazyListState, items: List<ItemSummary>, prefetch: (ItemSummary) -> Unit): Modifier =
    pointerInput(state, items) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            val info = state.layoutInfo
            val x = down.position.x - info.viewportStartOffset
            val hit = info.visibleItemsInfo.firstOrNull { x >= it.offset && x < it.offset + it.size }
            hit?.let { items.getOrNull(it.index) }?.let(prefetch)
        }
    }

/** The design's section gap between rows, less the room each row keeps under its cards for a pressed one. */
private val SectionGap = PleyPhone.SectionGap - 6.dp

/** About two rows ahead of the screen are composed, so their pictures are on their way. */
private val RowsAhead = 520.dp

private val SKELETON_ROWS = listOf(
    HubRowModel("skeleton.continue", "", TileKind.Landscape, null),
    HubRowModel("skeleton.posters", "", TileKind.Poster, null),
    HubRowModel("skeleton.more", "", TileKind.Poster, null),
)

private const val SKELETON_DELAY_MS = 300L
