package app.pley.android.ui.phone.livetv

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.common.resolve
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveGroup
import app.pley.android.model.LiveProgram
import app.pley.android.screens.livetv.ChannelPages
import app.pley.android.screens.livetv.LivePage
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.screens.livetv.LiveTvEvent
import app.pley.android.screens.livetv.LiveTvViewModel
import app.pley.android.screens.livetv.LiveView
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyChip
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.flow.distinctUntilChanged

/**
 * Live TV on phones and tablets (the web's LiveTvPage):
 * "Live TV" with a button that switches between the guide and the
 * channels; the category in a picker (Favourites, Recently watched, All
 * channels, the groups); a search of names, numbers and what's on. The
 * guide is channels down and time across; the channels are cards. Both
 * load a page of channels at a time as they scroll, and open back at the
 * channel last watched. A channel plays at a tap; a programme's details
 * open in a sheet, with Watch and the favourite.
 */
@Composable
internal fun PhoneLiveTvScreen() {
    val vm: LiveTvViewModel = viewModel(factory = LiveTvViewModel.Factory)
    val page by vm.state.collectAsStateWithLifecycle()
    val channels by vm.channels.collectAsStateWithLifecycle()
    val now by vm.now.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    var details by remember { mutableStateOf<Pair<LiveChannel, LiveProgram>?>(null) }
    var picking by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    // Search let go first: back from the player, the field would take focus again and open the keyboard.
    val watch: (LiveChannel) -> Unit = { channel ->
        keyboard?.hide()
        focus.clearFocus(force = true)
        vm.watch(channel)
    }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                is LiveTvEvent.Failed -> shell.toasts.show(UiText.res(R.string.livetv_favorite_failed, errorSentence(event.error)).resolve(resources))
            }
        }
    }
    ReportLoad(
        when {
            channels.error != null -> Loadable.Failed(channels.error!!, Unit)
            channels.total != null -> Loadable.Ready(Unit, fresh = true)
            else -> null
        },
    )
    val grid = rememberLazyGridState()
    Column(Modifier.fillMaxSize().testTag(LIVE_TV_PAGE)) {
        PleyTopAppBar(
            title = stringResource(R.string.livetv_title),
            onBack = navigator::back,
            scrolled = { page.view == LiveView.Channels && grid.canScrollBackward },
            actions = {
                val guide = page.view == LiveView.Guide
                PleyIconButton(
                    if (guide) PleyIcons.LayoutGrid else PleyIcons.CalendarDays,
                    stringResource(if (guide) R.string.livetv_show_channels else R.string.livetv_show_guide),
                    onClick = { vm.setView(if (guide) LiveView.Channels else LiveView.Guide) },
                    modifier = Modifier.testTag(LIVE_TV_VIEW),
                )
            },
        )
        if (!page.allowed) {
            Message(stringResource(R.string.livetv_not_allowed_title), stringResource(R.string.livetv_not_allowed_body))
            return@Column
        }
        Filters(
            page,
            onQuery = vm::setQuery,
            onPick = { picking = true },
            onGroup = {
                keyboard?.hide()
                focus.clearFocus(force = true)
                vm.setCategory(LiveCategory.Group(it.name))
            },
        )
        Box(Modifier.weight(1f).fillMaxWidth()) {
            val total = channels.total
            when {
                total == null && channels.error != null -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                    ErrorState(stringResource(R.string.livetv_failed), channels.error!!, onRetry = vm::retry)
                }
                total == null -> Box(Modifier.fillMaxSize())
                total == 0 -> Empty(page)
                page.view == LiveView.Guide -> PhoneGuide(
                    vm = vm,
                    channels = channels,
                    category = page.listed,
                    origin = page.origin,
                    now = now,
                    onWatch = watch,
                    onDetails = { channel, program -> details = channel to program },
                )
                else -> ChannelCards(vm, channels, page, now, grid, onWatch = watch, onDetails = { channel, program -> details = channel to program })
            }
        }
    }
    if (picking) {
        CategorySheet(
            groups = page.groups,
            selected = page.category,
            onSelect = {
                picking = false
                vm.setCategory(it)
            },
            onDismiss = { picking = false },
        )
    }
    details?.let { (channel, program) ->
        ProgramSheet(
            channel = channel,
            program = program,
            origin = page.origin,
            now = now,
            onWatch = {
                details = null
                watch(channel)
            },
            onFavorite = {
                vm.toggleFavorite(channel)
                details = channel.copy(favorite = !channel.favorite) to program
            },
            onDismiss = { details = null },
        )
    }
}

@Composable
private fun Message(title: String, body: String) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(top = 32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        EmptyState(title = title, body = body, icon = PleyIcons.TvMinimal)
    }
}

/** A category or search without channels: why, in its own words. */
@Composable
private fun Empty(page: LivePage) {
    when {
        page.query.isNotBlank() -> Message(stringResource(R.string.livetv_no_match_title), stringResource(R.string.livetv_no_match_search, page.query.trim()))
        page.category == LiveCategory.Favorites -> Message(stringResource(R.string.livetv_none_favorites_title), stringResource(R.string.livetv_none_favorites_body))
        page.category == LiveCategory.Recent -> Message(stringResource(R.string.livetv_none_recent_title), stringResource(R.string.livetv_none_recent_body))
        page.category is LiveCategory.Group -> Message(stringResource(R.string.livetv_no_match_title), stringResource(R.string.livetv_no_match_group))
        else -> Message(stringResource(R.string.livetv_none_title), stringResource(R.string.livetv_none_body))
    }
}

/** What a category is called: "Favourites", "UK · Sport". */
@Composable
internal fun categoryName(category: LiveCategory, page: LivePage): String = when (category) {
    LiveCategory.All -> stringResource(R.string.livetv_category_all)
    LiveCategory.Favorites -> stringResource(R.string.livetv_category_favorites)
    LiveCategory.Recent -> stringResource(R.string.livetv_category_recent)
    is LiveCategory.Group -> LiveTv.groupName(page.groups?.groups?.firstOrNull { it.name == category.name }, category.name)
}

/**
 * The category's button (its icon, name and count, a chevron: it opens the
 * picker), then the search, which looks among every channel and names the
 * groups it matches (a tap opens one).
 */
@Composable
private fun Filters(page: LivePage, onQuery: (String) -> Unit, onPick: () -> Unit, onGroup: (LiveGroup) -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val focus = LocalFocusManager.current
    val count = when (val c = page.category) {
        LiveCategory.All -> page.groups?.total
        LiveCategory.Favorites -> page.groups?.favorites
        LiveCategory.Recent -> page.groups?.recent
        is LiveCategory.Group -> page.groups?.groups?.firstOrNull { it.name == c.name }?.count
    }
    Column(Modifier.fillMaxWidth().padding(start = metrics.marginStart, end = metrics.marginEnd, bottom = 8.dp)) {
        Row(
            Modifier
                .fillMaxWidth()
                .clip(PleyShape.Control)
                .clickable(role = Role.Button, onClick = onPick)
                .padding(vertical = 6.dp)
                .testTag(LIVE_TV_CATEGORY),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // While a search is typed, what shows is its results from every channel, not the category's (the web's).
            Icon(if (page.searching) PleyIcons.Search else categoryIcon(page.category), contentDescription = null, size = 20.dp, tint = PleyColors.AccentBright)
            Spacer(Modifier.width(10.dp))
            Text(
                if (page.searching) stringResource(R.string.livetv_results_for, page.query.trim()) else categoryName(page.category, page),
                style = MaterialTheme.typography.titleMedium,
                color = PleyColors.Fg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (count != null && !page.searching) {
                Text("  ${formatCount(count)}", style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle, maxLines = 1)
            }
            Spacer(Modifier.width(4.dp))
            Icon(PleyIcons.ChevronDown, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            Spacer(Modifier.weight(1f))
        }
        Spacer(Modifier.height(6.dp))
        Row(
            Modifier
                .fillMaxWidth()
                .height(44.dp)
                .background(PleyColors.Surface3, PleyShape.Pill)
                .padding(start = 14.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(PleyIcons.Search, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(10.dp))
            val label = stringResource(R.string.livetv_search)
            Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                if (page.query.isEmpty()) {
                    Text(label, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                BasicTextField(
                    value = page.query,
                    onValueChange = onQuery,
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodyMedium.copy(color = PleyColors.Fg),
                    cursorBrush = SolidColor(PleyColors.AccentBright),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { focus.clearFocus() }),
                    modifier = Modifier.fillMaxWidth().semantics { contentDescription = label }.testTag(LIVE_TV_SEARCH),
                )
            }
            if (page.query.isNotEmpty()) {
                PleyIconButton(PleyIcons.X, stringResource(R.string.livetv_search_clear), onClick = { onQuery("") }, tint = PleyColors.Muted)
            } else {
                Spacer(Modifier.width(10.dp))
            }
        }
        if (page.found.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().padding(top = 8.dp).horizontalScroll(rememberScrollState()).testTag(LIVE_TV_FOUND_GROUPS),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(stringResource(R.string.livetv_found_groups).uppercase(), style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle)
                for (g in page.found) {
                    PleyChip("${LiveTv.groupName(g, g.name)}  ${formatCount(g.count)}", selected = false, onClick = { onGroup(g) })
                }
            }
        }
    }
}

/** A category's icon: a star, the clock, the grid, a group's TV. */
internal fun categoryIcon(category: LiveCategory) = when (category) {
    LiveCategory.Favorites -> PleyIcons.Star
    LiveCategory.Recent -> PleyIcons.History
    LiveCategory.All -> PleyIcons.LayoutGrid
    is LiveCategory.Group -> PleyIcons.TvMinimal
}

/**
 * The channels as cards, as many across as fit (one on a phone held
 * upright), a page loaded at a time as they scroll, opening at the
 * channel last watched.
 */
@Composable
private fun ChannelCards(
    vm: LiveTvViewModel,
    channels: ChannelPages,
    page: LivePage,
    now: Long,
    grid: LazyGridState,
    onWatch: (LiveChannel) -> Unit,
    onDetails: (LiveChannel, LiveProgram) -> Unit,
) {
    val metrics = LocalPhoneMetrics.current
    val landing by vm.landing.collectAsStateWithLifecycle()
    val skeleton = rememberSkeleton(animated = true)
    Landing(landing, page) { index -> if (index != null) grid.scrollToItem(index) else grid.scrollToItem(0) }
    LaunchedEffect(grid) {
        snapshotFlow { grid.layoutInfo.visibleItemsInfo.let { (it.firstOrNull()?.index ?: 0) to (it.lastOrNull()?.index ?: 0) } }
            .distinctUntilChanged()
            .collect { (first, last) -> vm.showingRows(first, last) }
    }
    LazyVerticalGrid(
        columns = GridCells.Adaptive(CARD_MIN),
        state = grid,
        modifier = Modifier.fillMaxSize().testTag(LIVE_TV_CHANNELS),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 4.dp, bottom = 32.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        items(count = channels.total ?: 0, key = { index -> channels[index]?.id ?: -index - 1L }, contentType = { "channel" }) { index ->
            val channel = channels[index]
            if (channel == null) {
                Box(Modifier.fillMaxWidth().height(CARD_HEIGHT).skeleton(skeleton, PleyShape.Card))
            } else {
                ChannelCard(
                    channel,
                    LiveTv.channelName(channel, page.listed),
                    page.origin,
                    now,
                    onWatch = { onWatch(channel) },
                    onFavorite = { vm.toggleFavorite(channel) },
                    onDetails = { onDetails(channel, it) },
                )
            }
        }
    }
}

/**
 * Where a list opens: at [landing]'s channel, once per category, search
 * and return from the player ([scrollTo] with its index, or `null` for
 * the top).
 */
@Composable
internal fun Landing(landing: app.pley.android.screens.livetv.Landing?, page: LivePage, scrollTo: suspend (Int?) -> Unit) {
    val placed = remember { mutableStateOf<Any?>(null) }
    LaunchedEffect(landing, page.category, page.query) {
        val l = landing ?: return@LaunchedEffect
        if (l.category != page.category || l.query != page.query.trim()) return@LaunchedEffect
        if (placed.value == l) return@LaunchedEffect
        placed.value = l
        scrollTo(l.index)
    }
}

/**
 * A channel: its logo (number in its corner), name and star, what's on
 * with how far it is and when it ends, and what's next. The card plays the
 * channel; what's on and what's next open their details.
 */
@Composable
private fun ChannelCard(
    channel: LiveChannel,
    name: String,
    origin: String,
    now: Long,
    onWatch: () -> Unit,
    onFavorite: () -> Unit,
    onDetails: (LiveProgram) -> Unit,
) {
    val clock = rememberClock()
    val on = channel.onAt(now)
    val next = channel.nextAt(now)
    val watch = if (on != null) {
        stringResource(R.string.livetv_watch_with_now, listOfNotNull(channel.number, name).joinToString(" "), on.title)
    } else {
        stringResource(R.string.livetv_watch, listOfNotNull(channel.number, name).joinToString(" "))
    }
    Row(
        Modifier
            .fillMaxWidth()
            .height(CARD_HEIGHT)
            .clip(PleyShape.Card)
            .background(PleyColors.Surface)
            .clickable(role = Role.Button, onClickLabel = watch, onClick = onWatch)
            .padding(10.dp)
            .testTag(LIVE_TV_CHANNEL + channel.id),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            ChannelTile(channel, origin, Modifier.size(width = 88.dp, height = 56.dp), name = true, style = MaterialTheme.typography.labelMedium)
            channel.number?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.labelSmall,
                    color = PleyColors.Fg,
                    modifier = Modifier.padding(3.dp).clip(PleyShape.Badge).background(PleyColors.OnArt).padding(horizontal = 4.dp, vertical = 1.dp),
                )
            }
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(name, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (on != null) {
                Row(
                    Modifier.clickable(role = Role.Button) { onDetails(on) }.padding(vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    BadgeOf(on, Modifier.padding(end = 6.dp))
                    Text(on.title, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ProgramProgress(on, now, Modifier.weight(1f))
                    Spacer(Modifier.width(8.dp))
                    Text(clock(on.stop), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
                }
            } else {
                Text(
                    stringResource(if (channel.hasGuide) R.string.livetv_nothing_now else R.string.livetv_no_guide),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Subtle,
                    maxLines = 1,
                )
            }
            if (next != null) {
                Text(
                    stringResource(R.string.livetv_next, clock(next.start), next.title),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Subtle,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 2.dp).clickable(role = Role.Button) { onDetails(next) },
                )
            }
        }
        PleyIconButton(
            if (channel.favorite) PleyIcons.Star else PleyIcons.StarOutline,
            stringResource(if (channel.favorite) R.string.livetv_favorite_remove else R.string.livetv_favorite_add),
            onClick = onFavorite,
            tint = if (channel.favorite) PleyColors.Accent else PleyColors.Muted,
        )
    }
}

/** Cards at least this wide: one column on a phone held upright, two or three on tablets. */
private val CARD_MIN = 320.dp
private val CARD_HEIGHT = 96.dp

internal const val LIVE_TV_PAGE = "pley.live"
internal const val LIVE_TV_VIEW = "pley.live.view"
internal const val LIVE_TV_CATEGORY = "pley.live.category"
internal const val LIVE_TV_SEARCH = "pley.live.search"
internal const val LIVE_TV_FOUND_GROUPS = "pley.live.found.groups"
internal const val LIVE_TV_CHANNELS = "pley.live.channels"
internal const val LIVE_TV_CHANNEL = "pley.live.channel."
