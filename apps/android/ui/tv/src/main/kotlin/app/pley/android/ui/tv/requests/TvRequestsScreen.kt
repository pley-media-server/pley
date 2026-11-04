package app.pley.android.ui.tv.requests

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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.clickable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.summary
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.model.MediaRequest
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.RequestListState
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestView
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.RequestsUiState
import app.pley.android.screens.requests.RequestsViewModel
import app.pley.android.screens.requests.stagePill
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvToastHost
import app.pley.android.ui.tv.item.TvToastState
import app.pley.android.ui.tv.item.focusRing
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTextField
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.library.TvChip
import app.pley.android.ui.tv.library.TvTab
import app.pley.android.ui.tv.settings.TvChoice
import app.pley.android.ui.tv.settings.TvChoicePanel
import app.pley.android.ui.tv.shell.LocalTvNavigator
import androidx.compose.ui.text.input.ImeAction

/**
 * Your requests on TV, or for managers everyone's (Requests), as the
 * Samsung and LG apps have them (packages/tv's requests-page.tsx): tabs
 * with their numbers (Needs attention first for managers, who open on
 * it, then All, Pending, Approved, Available, Declined, Failed); for
 * managers, movies or shows, a person (Asked by) and a title's words;
 * then a card per request with its poster, what was asked for, who asked
 * and when, its note and its stage. OK on a card opens what may be done
 * with it ([TvRequestPanel]). The list reads on as focus comes near its
 * end; news since the profile last looked keeps its dot while the page
 * shows.
 */
@Composable
internal fun TvRequestsScreen() {
    TvRequestsScreen(viewModel { RequestsViewModel(RequestsDeps.of(screensGraph())) })
}

@Composable
internal fun TvRequestsScreen(vm: RequestsViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val texts = rememberRequestTexts()
    val toasts = remember { TvToastState() }
    TvRequestEvents(vm.events, texts, toasts) { vm.state.value.gate.summary }
    Box(Modifier.fillMaxSize().testTag(TV_REQUESTS)) {
        val problem = state.problem
        if (problem != null) {
            TvRequestMessage(problem, state.gate.summary, texts, stringResource(ScreensR.string.requests_list_failed), state.origin, onRetry = vm::refresh)
        } else {
            RequestsPage(state, texts, vm)
        }
        TvToastHost(toasts)
    }
}

@Composable
private fun RequestsPage(state: RequestsUiState, texts: RequestTexts, vm: RequestsViewModel) {
    val navigator = LocalTvNavigator.current
    val cards = remember { CardFocus() }
    // Where focus was on the page (a card, a tab, a filter), for coming back to it from a title (D-pad rule 2).
    val memory = rememberFocusMemory()
    val tabFocus = remember { FocusRequester() }
    val kind = remember { FocusRequester() }
    val person = remember { FocusRequester() }
    val signInPill = remember { FocusRequester() }
    var open by remember { mutableStateOf<Long?>(null) }
    var people by remember { mutableStateOf(false) }
    var signIn by remember { mutableStateOf(false) }
    // The list is made again as each tab, filter or typed title is read: what shows then takes the
    // page's first focus only while nothing on the page has it, so focus stays on the tab, the chip
    // or the field being typed in.
    var pageHasFocus by remember { mutableStateOf(false) }
    val list = state.list
    val ready = list as? RequestListState.Ready
    // News since the profile last looked keeps its dot while the page shows, though seeing it clears it on the server.
    var news by remember { mutableStateOf(emptySet<Long>()) }
    LaunchedEffect(ready?.requests) {
        val fresh = ready?.requests.orEmpty().filter { it.unseen && it.id !in news }
        if (fresh.isNotEmpty()) news = news + fresh.map { it.id }
    }
    val listTop = if (state.manage || state.signInForApprovals) ListTopLow else ListTop
    Box(Modifier.fillMaxSize().onFocusChanged { pageHasFocus = it.hasFocus }) {
        when (list) {
            RequestListState.Loading -> CardSkeletons(listTop)
            is RequestListState.Failed -> ListNote(
                top = listTop,
                title = problemTitle(list.problem, state.gate.summary, texts, stringResource(ScreensR.string.requests_list_failed)),
                body = problemBody(list.problem, state.gate.summary, texts),
                action = Triple(stringResource(ScreensR.string.requests_retry), PleyIcons.RefreshCw, vm::refresh),
                memory = memory,
                initial = !pageHasFocus,
            )
            is RequestListState.Empty -> {
                val find = state.mayAsk && !state.manage && state.filter.view == RequestView.All && !list.filtered
                ListNote(
                    top = listTop,
                    title = stringResource(emptyTitle(state, list.filtered)),
                    body = stringResource(emptyBody(state, list.filtered)),
                    action = if (find) Triple(stringResource(R.string.tv_requests_find), PleyIcons.Compass) { navigator.open(PleyKey.Discover) } else null,
                    memory = memory,
                    initial = !pageHasFocus,
                )
            }
            is RequestListState.Ready -> CardList(
                list = list,
                state = state,
                origin = state.origin,
                texts = texts,
                news = news,
                cards = cards,
                memory = memory,
                initial = !pageHasFocus,
                top = listTop,
                onOpen = {
                    open = it.id
                    // Until focus is back on a card, this one counts as focused: should it go meanwhile, the card in its place takes focus.
                    cards.panelFor = it.id
                },
                onNearEnd = vm::more,
                onLost = { runCatching { tabFocus.requestFocus() } },
                // Up from the cards: the filters' first, else the tab shown.
                up = if (state.manage) kind else tabFocus,
            )
        }
        Column(Modifier.padding(start = PleyTv.Gutter, top = TvSafeArea.Vertical, end = TvSafeArea.Horizontal)) {
            BasicText(
                stringResource(if (state.manage) ScreensR.string.requests_manage else ScreensR.string.requests_yours),
                style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
                modifier = Modifier.semantics { heading() },
            )
            Spacer(Modifier.height(10.dp))
            Tabs(
                state = state,
                texts = texts,
                focus = tabFocus,
                memory = memory,
                initial = !pageHasFocus && list is RequestListState.Empty && !(state.mayAsk && !state.manage && state.filter.view == RequestView.All && !list.filtered),
                onView = vm::view,
            )
            if (state.manage) {
                Spacer(Modifier.height(10.dp))
                Filters(state, vm, kind, person, memory, onPeople = { people = true })
            } else if (state.signInForApprovals) {
                Spacer(Modifier.height(10.dp))
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    BasicText(stringResource(ScreensR.string.requests_sign_in_approvals), style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 1)
                    TvPillButton(
                        stringResource(ScreensR.string.requests_sign_in),
                        onClick = { signIn = true },
                        icon = PleyIcons.LogOut,
                        modifier = Modifier.focusRequester(signInPill).rememberedFocus(memory, SIGN_IN_KEY).testTag(TV_REQUESTS_SIGN_IN),
                    )
                }
            }
        }
        val request = (list as? RequestListState.Ready)?.requests?.firstOrNull { it.id == open }
        // The request went while its panel was open (withdrawn, approved off Needs attention): the panel goes too.
        if (open != null && request == null && ready != null) LaunchedEffect(open) { open = null }
        TvRequestPanel(
            request = request,
            manage = state.manage,
            busy = request?.id in state.busy,
            texts = texts,
            onAction = { r, action, reason ->
                open = null
                vm.act(r, action, reason)
            },
            onOpen = { key ->
                open = null
                navigator.open(key)
            },
            onClose = { open = null },
            returnTo = cards::returnTo,
        )
        if (state.manage) {
            val choices = remember(ready?.people, state.filter.userId) {
                listOf(TvChoice<Long?>(null, "")) + ready?.people.orEmpty().map { TvChoice<Long?>(it.id, it.name) }
            }
            val everyone = stringResource(ScreensR.string.requests_filter_everyone)
            TvChoicePanel(
                open = people,
                title = stringResource(R.string.tv_requests_asked_by),
                choices = choices.map { if (it.value == null) it.copy(label = everyone) else it },
                selected = state.filter.userId,
                onPick = vm::person,
                onDismiss = { people = false },
                returnTo = person,
            )
        }
        if (signIn) TvSignInAgainDialog(state.origin, onDismiss = { signIn = false }, returnTo = signInPill)
    }
}

/** The tabs, each with its number after its name (none while 0). Select shows it. */
@Composable
private fun Tabs(state: RequestsUiState, texts: RequestTexts, focus: FocusRequester, memory: FocusMemory, initial: Boolean, onView: (RequestView) -> Unit) {
    val rail = LocalTvRailFocus.current
    val views = RequestView.entries.filter { it != RequestView.Attention || state.manage }
    val ready = state.list as? RequestListState.Ready
    Row(
        Modifier.railOnLeftStopOnRight(rail).focusGroup(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        views.forEach { view ->
            val label = stringResource(viewLabel(view))
            val count = countOf(view, state, ready)
            val shown = view == state.filter.view
            TvTab(
                label = if (count != null && count > 0) stringResource(ScreensR.string.requests_filter_count, label, count) else label,
                selected = shown,
                onClick = { onView(view) },
                modifier = Modifier
                    .then(if (shown) Modifier.focusRequester(focus) else Modifier)
                    .rememberedFocus(memory, "tab/${view.name}", initial = shown && initial)
                    .testTag(TV_REQUESTS_TAB + view.name),
            )
        }
    }
}

/**
 * A tab's number: Needs attention, what waits for managers (with filters
 * on, what its list holds, known while it shows); All, every status's;
 * a status, its own. `null` while not known.
 */
private fun countOf(view: RequestView, state: RequestsUiState, ready: RequestListState.Ready?): Int? {
    val counts = ready?.counts
    return when {
        view == RequestView.Attention -> if (state.filter.narrowed) ready?.total?.takeIf { state.filter.view == RequestView.Attention } else state.attention
        counts == null -> null
        view == RequestView.All -> counts.values.sum()
        else -> view.status?.let { counts[it.wire] }
    }
}

private fun viewLabel(view: RequestView): Int = when (view) {
    RequestView.Attention -> ScreensR.string.requests_attention
    RequestView.All -> ScreensR.string.requests_filter_all
    RequestView.Pending -> ScreensR.string.requests_filter_pending
    RequestView.Approved -> ScreensR.string.requests_filter_approved
    RequestView.Available -> ScreensR.string.requests_filter_available
    RequestView.Declined -> ScreensR.string.requests_filter_declined
    RequestView.Failed -> ScreensR.string.requests_filter_failed
}

/** Managers' filters: movies or shows (each press the next), a person (Asked by), and a title's words, applied once typing pauses. */
@Composable
private fun Filters(state: RequestsUiState, vm: RequestsViewModel, kind: FocusRequester, person: FocusRequester, memory: FocusMemory, onPeople: () -> Unit) {
    val rail = LocalTvRailFocus.current
    val filter = state.filter
    // What was typed before, for a field made again (the page coming back).
    val typed by vm.titleQuery.collectAsStateWithLifecycle()
    val field = rememberTextFieldState(typed)
    LaunchedEffect(field, vm) { snapshotFlow { field.text.toString() }.collect(vm::onTitleChange) }
    val name = (state.list as? RequestListState.Ready)?.people?.firstOrNull { it.id == filter.userId }?.name
    Row(
        Modifier.railOnLeftStopOnRight(rail).focusGroup(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TvChip(
            label = stringResource(
                when (filter.kind) {
                    TitleKind.Movie -> ScreensR.string.requests_filter_movies
                    TitleKind.Show -> ScreensR.string.requests_filter_shows
                    else -> R.string.tv_requests_kind_all
                },
            ),
            onClick = { vm.kind(nextKind(filter.kind)) },
            selected = filter.kind != null,
            icon = PleyIcons.Film,
            modifier = Modifier.focusRequester(kind).rememberedFocus(memory, "kind").testTag(TV_REQUESTS_KIND),
        )
        TvChip(
            label = name ?: stringResource(ScreensR.string.requests_filter_everyone),
            onClick = onPeople,
            selected = filter.userId != null,
            icon = PleyIcons.Users,
            modifier = Modifier.widthIn(max = 180.dp).focusRequester(person).rememberedFocus(memory, "person"),
        )
        TvTextField(
            state = field,
            placeholder = stringResource(ScreensR.string.requests_filter_title),
            imeAction = ImeAction.Search,
            focusModifier = Modifier.rememberedFocus(memory, "title"),
            modifier = Modifier.width(240.dp).testTag(TV_REQUESTS_TITLE),
        )
    }
}

/** Movies and shows, then movies, then shows. */
private fun nextKind(kind: TitleKind?): TitleKind? = when (kind) {
    null -> TitleKind.Movie
    TitleKind.Movie -> TitleKind.Show
    else -> null
}

private fun emptyTitle(state: RequestsUiState, filtered: Boolean): Int = when {
    filtered -> ScreensR.string.requests_filtered_none
    state.filter.view == RequestView.Attention -> ScreensR.string.requests_attention_none
    else -> ScreensR.string.requests_none_title
}

private fun emptyBody(state: RequestsUiState, filtered: Boolean): Int = when {
    filtered -> ScreensR.string.requests_filtered_none_body
    state.filter.view == RequestView.Attention -> ScreensR.string.requests_attention_none_body
    state.mayAsk -> ScreensR.string.requests_none_body
    else -> ScreensR.string.requests_none_body_off
}

/**
 * Which card has focus, for giving it to the right one when cards go: a
 * card that leaves with focus hands it to the card now in its place, else
 * the one before, else ([CardList]'s onLost) the tab. A card whose panel
 * was opened ([panelFor]) counts as focused until a card has focus again:
 * the panel has focus, and gives it back to the card ([returnTo]) only
 * once it has closed, by when the server may have taken the card out
 * (Withdraw).
 */
@Stable
private class CardFocus {
    var hasFocus = false
    var focusedId: Long? = null
    var panelFor: Long? = null
    private var ids: List<Long> = emptyList()
    private val requesters = HashMap<Long, FocusRequester>()

    fun requester(id: Long): FocusRequester = requesters.getOrPut(id) { FocusRequester() }

    /** Where focus goes back to as [id]'s panel closes: its card while it is there; else nowhere, its heir has taken focus. */
    fun returnTo(id: Long): FocusRequester? = if (id in ids) requester(id) else null

    /** The list is now [next]: when the focused card left it, the card that takes focus from it (`-1`: none, the tab does). */
    fun heirOf(next: List<Long>): Long? {
        val before = ids
        ids = next
        requesters.keys.retainAll(next.toSet())
        val focused = (if (hasFocus) focusedId else panelFor)?.takeIf { id -> next.none { it == id } } ?: return null
        val at = before.indexOf(focused).takeIf { it >= 0 } ?: return null
        return next.getOrNull(at) ?: next.getOrNull(at - 1) ?: NONE
    }

    companion object {
        const val NONE = -1L
    }
}

/** The cards, 88 tall on a 98 pitch, read on five from the end; the card focus rests on scrolls to the second line. */
@Composable
private fun CardList(
    list: RequestListState.Ready,
    state: RequestsUiState,
    origin: String,
    texts: RequestTexts,
    news: Set<Long>,
    cards: CardFocus,
    memory: FocusMemory,
    initial: Boolean,
    top: Dp,
    onOpen: (MediaRequest) -> Unit,
    onNearEnd: () -> Unit,
    onLost: () -> Unit,
    up: FocusRequester,
) {
    val rail = LocalTvRailFocus.current
    val listState = rememberLazyListState()
    val requests = list.requests
    val ids = remember(requests) { requests.map { it.id } }
    val heir = remember(ids) { Snapshot.withoutReadObservation { cards.heirOf(ids) } }
    // The card remembered went while a title was open: the one in its place, else the one before, is focused instead.
    val keys = remember(ids) { ids.mapTo(HashSet()) { cardKey(it) } }
    memory.settle { it in keys }
    if (heir != null) {
        LaunchedEffect(heir) {
            withFrameNanos { }
            if (heir == CardFocus.NONE) onLost() else runCatching { cards.requester(heir).requestFocus() }
        }
    }
    LazyColumn(
        state = listState,
        modifier = Modifier
            .fillMaxSize()
            .padding(top = top)
            .testTag(TV_REQUESTS_LIST)
            .onFocusChanged { cards.hasFocus = it.hasFocus }
            .railOnLeftStopOnRight(rail)
            .heldKeyStaysIn(HeldEnds.UpDown)
            .focusGroup(),
        contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal + 16.dp, top = 8.dp, bottom = TvSafeArea.Vertical + CardPitch),
        verticalArrangement = Arrangement.spacedBy(CardGap),
    ) {
        itemsIndexed(requests, key = { _, r -> r.id }, contentType = { _, _ -> CARD_TYPE }) { i, request ->
            RequestCard(
                request = request,
                origin = origin,
                manage = state.manage,
                news = request.id in news,
                texts = texts,
                onClick = { onOpen(request) },
                modifier = Modifier
                    .focusRequester(cards.requester(request.id))
                    .rememberedFocus(memory, cardKey(request.id), initial = i == 0 && initial)
                    // Up from the first card: the filters' first, else the tab shown, not whichever is nearest.
                    .then(if (i == 0) Modifier.focusProperties { this.up = up } else Modifier)
                    .onFocusChanged {
                        if (it.isFocused) {
                            cards.focusedId = request.id
                            cards.panelFor = null
                            // Should it go while a title is open, the card after it (else before it) stands in.
                            memory.mayLeave(cardKey(request.id), (requests.getOrNull(i + 1) ?: requests.getOrNull(i - 1))?.id?.let(::cardKey))
                            if (i >= requests.size - NEAR_END) onNearEnd()
                        }
                    },
            )
        }
    }
}

/**
 * A request as a card (requests-page.tsx's RequestCard): its 48 dp
 * poster with a dot for news, "Title · year", what was asked for and who
 * asked when, the stage's line and the managers' note, and the stage at
 * the end. Focused, it lifts to surface3 with the ring, never scaled past
 * 1.02.
 */
@Composable
private fun RequestCard(
    request: MediaRequest,
    origin: String,
    manage: Boolean,
    news: Boolean,
    texts: RequestTexts,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val reduced = reducedMotion()
    val pill = stagePill(request, manage)
    val what = request.seasons?.takeIf { request.kind == TitleKind.Show && it.isNotEmpty() }?.let(texts::seasons) ?: texts.kind(request.kind)
    val asked = texts.asked(request, manage)
    val note = listOfNotNull(texts.pillLine(pill), request.note?.takeIf { manage && it.isNotBlank() }).joinToString(" ")
    val stage = texts.pill(pill)
    val said = listOfNotNull(request.title, request.year?.toString(), what, stage, note.ifEmpty { null }, asked).joinToString(", ")
    Row(
        modifier
            .fillMaxWidth()
            .height(CardHeight)
            .graphicsLayer {
                val s = if (focused && !reduced) CARD_SCALE else 1f
                scaleX = s
                scaleY = s
            }
            .focusRing(8.dp) { focused }
            .background(if (focused) PleyColors.Surface3 else PleyColors.Surface, PleyShape.Control)
            .clickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = said }
            .padding(start = 12.dp, end = 20.dp)
            .testTag(TV_REQUESTS_CARD + request.id),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val poster = remember(request.posterUrl, origin) { ArtworkImage.of(request.posterUrl, origin) }
        Box {
            Artwork(poster, request.title, Modifier.size(PosterWidth, PosterHeight).clip(PleyShape.Badge).background(PleyColors.Surface3))
            // On the poster's top right corner, ringed in the card's colour, so the words beside it never move.
            if (news) {
                Box(
                    Modifier
                        .align(Alignment.TopEnd)
                        .offset(6.dp, (-4).dp)
                        .size(14.dp)
                        .background(if (focused) PleyColors.Surface3 else PleyColors.Surface, PleyShape.Circle)
                        .padding(2.dp)
                        .background(PleyColors.Accent, PleyShape.Circle)
                        .clearAndSetSemantics { },
                )
            }
        }
        Column(Modifier.weight(1f).padding(horizontal = 16.dp).clearAndSetSemantics { }) {
            BasicText(
                buildAnnotatedString {
                    append(request.title)
                    request.year?.let { withStyle(SpanStyle(color = PleyColors.Muted)) { append(" · $it") } }
                },
                style = PleyType.Tv.Title.copy(color = PleyColors.Fg),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(3.dp))
            BasicText("$what · $asked", style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (note.isNotEmpty()) {
                Spacer(Modifier.height(2.dp))
                BasicText(note, style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Box(Modifier.width(StageWidth).clearAndSetSemantics { }, contentAlignment = Alignment.CenterEnd) {
            TvStageBlock(request, manage, texts)
        }
    }
}

/** What the list says instead of cards: none, or why none came, with a button where one helps. */
@Composable
private fun ListNote(
    top: Dp,
    title: String,
    body: String?,
    action: Triple<String, PleyIcon, () -> Unit>?,
    memory: FocusMemory,
    initial: Boolean,
) {
    Column(Modifier.padding(start = PleyTv.Gutter, top = top + 24.dp, end = TvSafeArea.Horizontal)) {
        BasicText(title, style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
        if (body != null) {
            Spacer(Modifier.height(8.dp))
            BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), modifier = Modifier.widthIn(max = 520.dp))
        }
        if (action != null) {
            Spacer(Modifier.height(20.dp))
            val (label, icon, onClick) = action
            TvPillButton(label, onClick = onClick, icon = icon, modifier = Modifier.rememberedFocus(memory, NOTE_KEY, initial = initial))
        }
    }
}

/** Where the cards go while the first answer comes: still blocks. No spinner. */
@Composable
private fun CardSkeletons(top: Dp) {
    Column(Modifier.padding(start = PleyTv.Gutter, top = top + 8.dp, end = TvSafeArea.Horizontal), verticalArrangement = Arrangement.spacedBy(CardGap)) {
        repeat(4) { Box(Modifier.fillMaxWidth().height(CardHeight).background(PleyColors.Surface2, PleyShape.Control)) }
    }
}

/** The list's top, under the tabs; lower with the managers' filters, or the line about signing in again. */
private val ListTop = 128.dp
private val ListTopLow = 178.dp

private val CardHeight = 88.dp
private val CardGap = 10.dp
private val CardPitch = CardHeight + CardGap
private val PosterWidth = 48.dp
private val PosterHeight = 72.dp
private val StageWidth = 260.dp

/** A focused card grows a little: it is wide, and its neighbours are close. */
private const val CARD_SCALE = 1.02f

/** Cards from the end at which the next page is asked for. */
private const val NEAR_END = 5

private const val CARD_TYPE = "pley.requests.card"

/** The page's elements in its [FocusMemory]: a card by its request, the note's button, Sign in again. */
private fun cardKey(id: Long) = "card/$id"
private const val NOTE_KEY = "note"
private const val SIGN_IN_KEY = "signin"

internal const val TV_REQUESTS = "pley.tv.requests"
internal const val TV_REQUESTS_LIST = "pley.tv.requests.list"
internal const val TV_REQUESTS_CARD = "pley.tv.requests.card."
internal const val TV_REQUESTS_TAB = "pley.tv.requests.tab."
internal const val TV_REQUESTS_KIND = "pley.tv.requests.kind"
internal const val TV_REQUESTS_TITLE = "pley.tv.requests.title"
internal const val TV_REQUESTS_SIGN_IN = "pley.tv.requests.signin"
