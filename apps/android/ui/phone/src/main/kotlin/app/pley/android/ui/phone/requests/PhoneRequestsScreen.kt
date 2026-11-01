package app.pley.android.ui.phone.requests

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.summary
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestUser
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.requests.CardAction
import app.pley.android.screens.requests.RequestListState
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestView
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.RequestsUiState
import app.pley.android.screens.requests.RequestsViewModel
import app.pley.android.screens.requests.opens
import app.pley.android.screens.requests.readyToWatch
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PleyChip
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.search.SearchField
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter

/**
 * Your requests, and for managers everyone's
 * (Requests), opening on Needs attention: the views with how many each
 * holds, movies or shows, and for managers a person, their own and a
 * title; then the cards, read on as the list scrolls and pulled to read
 * again. Approve… asks for the profile, folder and seasons, Decline for
 * an optional reason, Withdraw, Remove and Delete whether to.
 */
@Composable
internal fun PhoneRequestsScreen() {
    val vm: RequestsViewModel = viewModel { RequestsViewModel(RequestsDeps.of(screensGraph())) }
    val state by vm.state.collectAsStateWithLifecycle()
    val approving by vm.approving.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val texts = rememberRequestTexts()
    val list = rememberLazyListState()
    RequestEvents(vm.events, texts) { state.gate.summary }
    val act = rememberRequestActions(texts) { request, action, reason -> vm.act(request, action, reason) }
    Column(Modifier.fillMaxSize().testTag(REQUESTS_PAGE)) {
        PleyTopAppBar(
            title = stringResource(if (state.manage) R.string.requests_manage else R.string.requests_yours),
            onBack = navigator::back,
            scrolled = { list.canScrollBackward },
        )
        val problem = state.problem
        if (problem != null) {
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                RequestProblemState(problem, state.gate.summary, texts, stringResource(R.string.requests_list_failed), onRetry = vm::refresh)
            }
            return@Column
        }
        if (state.signInForApprovals) SignInForApprovals()
        Filters(state, vm)
        PullToRefreshBox(
            isRefreshing = state.refreshing,
            onRefresh = vm::refresh,
            modifier = Modifier.weight(1f).fillMaxWidth(),
        ) {
            RequestList(state, list, texts, vm, act)
        }
    }
    approving?.let { dialog -> ApproveDialogView(dialog, texts, onApprove = vm::approve, onDismiss = vm::closeApprove) }
}

/** An administrator's app signed in before approvals: "Sign in again to approve and decline here." */
@Composable
private fun SignInForApprovals() {
    val shell = LocalPhoneShell.current
    val metrics = LocalPhoneMetrics.current
    Row(
        Modifier
            .padding(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp)
            .fillMaxWidth()
            .background(PleyColors.Surface2, PleyShape.Card)
            .padding(start = 16.dp, end = 4.dp, top = 4.dp, bottom = 4.dp)
            .testTag(REQUESTS_SIGN_IN),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.LogOut, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
        Spacer(Modifier.width(12.dp))
        Text(
            stringResource(R.string.requests_sign_in_approvals),
            style = MaterialTheme.typography.bodyMedium,
            color = PleyColors.Fg,
            modifier = Modifier.weight(1f),
        )
        PleyTextButton(stringResource(R.string.requests_sign_in), onClick = shell::signInAgain)
    }
}

/**
 * The filters over the list: the title's field (managers); the views, each
 * with how many it holds (those holding none left out, unless chosen);
 * movies or shows; a person and their own (managers).
 */
@Composable
private fun Filters(state: RequestsUiState, vm: RequestsViewModel) {
    val metrics = LocalPhoneMetrics.current
    val keyboard = LocalSoftwareKeyboardController.current
    val filter = state.filter
    val ready = state.list as? RequestListState.Ready
    // What the list said last: the chips stay put while the next one is read.
    var counts by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }
    var people by remember { mutableStateOf<List<RequestUser>>(emptyList()) }
    LaunchedEffect(ready?.counts, ready?.people) {
        if (ready != null) {
            counts = ready.counts
            people = ready.people
        }
    }
    var choosingPerson by rememberSaveable { mutableStateOf(false) }
    if (state.manage) {
        // What was typed before, for a field made again (the page coming back).
        val typed by vm.titleQuery.collectAsStateWithLifecycle()
        val field = rememberTextFieldState(typed)
        LaunchedEffect(field, vm) {
            snapshotFlow { field.text.toString() }.collect(vm::onTitleChange)
        }
        SearchField(
            field = field,
            label = stringResource(R.string.requests_filter_title),
            onSubmit = { keyboard?.hide() },
            autoFocus = false,
            tag = REQUESTS_FIELD,
        )
    }
    val chips = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 4.dp, bottom = 4.dp)
    val views = RequestView.entries.filter { view ->
        when (view) {
            RequestView.Attention -> state.manage
            RequestView.All -> true
            else -> view == filter.view || (view.status?.let { counts[it.wire] } ?: 0) > 0
        }
    }
    LazyRow(
        modifier = Modifier.fillMaxWidth().testTag(REQUESTS_VIEWS),
        contentPadding = chips,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(views, key = { it.name }) { view ->
            val name = stringResource(viewLabel(view))
            val count = when (view) {
                RequestView.Attention -> state.attention
                RequestView.All -> 0
                else -> view.status?.let { counts[it.wire] } ?: 0
            }
            PleyChip(
                label = if (count > 0) stringResource(R.string.requests_filter_count, name, count) else name,
                selected = view == filter.view,
                onClick = { vm.view(view) },
                modifier = Modifier.testTag(REQUESTS_VIEW + view.name),
            )
        }
    }
    LazyRow(
        modifier = Modifier.fillMaxWidth().testTag(REQUESTS_FILTERS),
        contentPadding = chips,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item(key = "movies") {
            PleyChip(
                label = stringResource(R.string.requests_filter_movies),
                selected = filter.kind == TitleKind.Movie,
                onClick = { vm.kind(if (filter.kind == TitleKind.Movie) null else TitleKind.Movie) },
            )
        }
        item(key = "shows") {
            PleyChip(
                label = stringResource(R.string.requests_filter_shows),
                selected = filter.kind == TitleKind.Show,
                onClick = { vm.kind(if (filter.kind == TitleKind.Show) null else TitleKind.Show) },
            )
        }
        if (state.manage) {
            if (people.isNotEmpty() || filter.userId != null) {
                item(key = "person") {
                    PleyChip(
                        label = people.firstOrNull { it.id == filter.userId }?.name ?: stringResource(R.string.requests_filter_everyone),
                        selected = filter.userId != null,
                        onClick = { choosingPerson = true },
                        dropdown = true,
                    )
                }
            }
            item(key = "mine") {
                PleyChip(
                    label = stringResource(R.string.requests_filter_mine),
                    selected = filter.mine,
                    onClick = { vm.mine(!filter.mine) },
                )
            }
        }
    }
    if (choosingPerson) {
        PleySheet(onDismiss = { choosingPerson = false }) {
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = PERSON_SHEET_MAX)) {
                item(key = "everyone") {
                    PersonRow(stringResource(R.string.requests_filter_everyone), selected = filter.userId == null) {
                        vm.person(null)
                        choosingPerson = false
                    }
                }
                items(people, key = { it.id }) { person ->
                    PersonRow(person.name, selected = person.id == filter.userId) {
                        vm.person(person.id)
                        choosingPerson = false
                    }
                }
            }
        }
    }
}

private fun viewLabel(view: RequestView): Int = when (view) {
    RequestView.Attention -> R.string.requests_attention
    RequestView.All -> R.string.requests_filter_all
    RequestView.Pending -> R.string.requests_filter_pending
    RequestView.Approved -> R.string.requests_filter_approved
    RequestView.Available -> R.string.requests_filter_available
    RequestView.Declined -> R.string.requests_filter_declined
    RequestView.Failed -> R.string.requests_filter_failed
}

/** One person to filter by, the chosen one with a check (as a library's choices). */
@Composable
private fun PersonRow(name: String, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(48.dp)
            .selectable(selected = selected, role = Role.RadioButton, onClick = onClick)
            .padding(horizontal = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(name, style = MaterialTheme.typography.bodyLarge, color = if (selected) PleyColors.Fg else PleyColors.Muted, modifier = Modifier.weight(1f))
        if (selected) Icon(PleyIcons.Check, contentDescription = null, size = 22.dp, tint = PleyColors.Accent)
    }
}

/** The list as it stands: skeleton cards, the cards, why there are none, or why it couldn't be read. */
@Composable
private fun RequestList(
    state: RequestsUiState,
    list: LazyListState,
    texts: RequestTexts,
    vm: RequestsViewModel,
    act: (MediaRequest, CardAction) -> Unit,
) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val skeleton = rememberSkeleton(animated = true)
    // Centred on tablets, as history's rows are.
    val sides = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd).widthIn(max = REQUEST_CARD_MAX)
    when (val shown = state.list) {
        RequestListState.Loading -> Column(
            Modifier.fillMaxSize().padding(top = 8.dp).testTag(REQUESTS_SKELETON),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            repeat(SKELETON_CARDS) { RequestCardSkeleton(skeleton, sides) }
        }
        is RequestListState.Failed -> Scrollable {
            RequestProblemState(shown.problem, state.gate.summary, texts, stringResource(R.string.requests_list_failed), onRetry = vm::refresh)
        }
        is RequestListState.Empty -> Scrollable { NoRequests(state, shown.filtered) }
        is RequestListState.Ready -> {
            LaunchedEffect(list) {
                snapshotFlow {
                    val info = list.layoutInfo
                    info.totalItemsCount > 0 && (info.visibleItemsInfo.lastOrNull()?.index ?: 0) >= info.totalItemsCount - LIST_AHEAD
                }
                    .distinctUntilChanged()
                    .filter { it }
                    .collect { vm.more() }
            }
            LazyColumn(
                state = list,
                modifier = Modifier.fillMaxSize().testTag(REQUESTS_LIST),
                contentPadding = PaddingValues(top = 8.dp, bottom = 32.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                // Read again, and the server didn't answer: what shows is from before (out of reach, the banner says so).
                shown.problem?.takeIf { it !is RequestProblem.Offline }?.let { problem ->
                    item(key = "problem", contentType = "problem") { StaleLine(texts.problem(problem, state.gate.summary), problem.retry, vm::refresh, sides) }
                }
                items(shown.requests, key = { it.id }, contentType = { "request" }) { request ->
                    val key = opens(request, state.mayAsk)
                    val itemId = request.itemId?.takeIf { readyToWatch(request) }
                    RequestCard(
                        request = request,
                        origin = state.origin,
                        manage = state.manage,
                        busy = request.id in state.busy,
                        texts = texts,
                        onOpen = key?.let { { navigator.open(it) } },
                        onAction = { action -> act(request, action) },
                        modifier = sides.animateItem(),
                        onWatch = itemId?.let { id -> { navigator.open(PleyKey.Item(id)) } },
                    )
                }
                if (shown.loadingMore) {
                    item(key = "more", contentType = "skeleton") { RequestCardSkeleton(skeleton, sides) }
                }
            }
        }
    }
}

/** None to show: none match the filters, none need a manager, or none asked for yet (with the way to Discover). */
@Composable
private fun NoRequests(state: RequestsUiState, filtered: Boolean) {
    val navigator = LocalPhoneNavigator.current
    when {
        filtered -> EmptyState(
            title = stringResource(R.string.requests_filtered_none),
            body = stringResource(R.string.requests_filtered_none_body),
            icon = PleyIcons.Search,
        )
        state.filter.view == RequestView.Attention -> EmptyState(
            title = stringResource(R.string.requests_attention_none),
            body = stringResource(R.string.requests_attention_none_body),
            icon = PleyIcons.CircleCheck,
        )
        else -> EmptyState(
            title = stringResource(R.string.requests_none_title),
            body = stringResource(if (state.mayAsk) R.string.requests_none_body else R.string.requests_none_body_off),
            icon = PleyIcons.ListVideo,
            action = if (state.mayAsk) {
                { PrimaryButton(stringResource(R.string.requests_discover), onClick = { navigator.open(PleyKey.Discover()) }, icon = PleyIcons.Compass) }
            } else {
                null
            },
        )
    }
}

/** The list couldn't be read again: why, with Retry where it may help. */
@Composable
private fun StaleLine(text: String, retry: Boolean, onRetry: () -> Unit, modifier: Modifier) {
    Row(modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(PleyIcons.CircleAlert, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
        Spacer(Modifier.width(8.dp))
        Text(text, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, modifier = Modifier.weight(1f))
        if (retry) PleyTextButton(stringResource(R.string.requests_retry), onClick = onRetry)
    }
}

/** A card while the list is read: the poster's block and three lines, shimmering. */
@Composable
internal fun RequestCardSkeleton(skeleton: SkeletonState, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth().background(PleyColors.Surface, PleyShape.Card).padding(12.dp)) {
        Box(Modifier.width(64.dp).height(96.dp).skeleton(skeleton, PleyShape.TilePhone))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(Modifier.fillMaxWidth(0.7f).height(18.dp).skeleton(skeleton, PleyShape.Badge))
            Box(Modifier.fillMaxWidth(0.4f).height(12.dp).skeleton(skeleton, PleyShape.Badge))
            Box(Modifier.fillMaxWidth(0.5f).height(24.dp).skeleton(skeleton, PleyShape.Control))
        }
    }
}

@Composable
private fun Scrollable(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) { content() }
}

/** Cards from the list's end at which its next page is asked for. */
private const val LIST_AHEAD = 5

private const val SKELETON_CARDS = 4

/** The person sheet's tallest, its list scrolling past it. */
private val PERSON_SHEET_MAX = 520.dp

internal const val REQUESTS_PAGE = "pley.requests"
internal const val REQUESTS_LIST = "pley.requests.list"
internal const val REQUESTS_SKELETON = "pley.requests.skeleton"
internal const val REQUESTS_FIELD = "pley.requests.field"
internal const val REQUESTS_VIEWS = "pley.requests.views"
internal const val REQUESTS_VIEW = "pley.requests.view."
internal const val REQUESTS_FILTERS = "pley.requests.filters"
internal const val REQUESTS_SIGN_IN = "pley.requests.signIn"
