package app.pley.android.ui.phone.search

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.model.LiveChannel
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.requests.RequestsEntryViewModel
import app.pley.android.screens.screensGraph
import app.pley.android.screens.search.SearchDeps
import app.pley.android.screens.search.SearchGroup
import app.pley.android.screens.search.SearchGroupKind
import app.pley.android.screens.search.SearchResultsState
import app.pley.android.screens.search.SearchTiles
import app.pley.android.screens.search.SearchUiState
import app.pley.android.screens.search.SearchViewModel
import app.pley.android.screens.search.TmdbSection
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.MediaCard
import app.pley.android.ui.phone.kit.OfflineBanner
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.rememberHubRowState
import app.pley.android.ui.phone.kit.rowCardWidth
import app.pley.android.ui.phone.library.PhoneTiles
import app.pley.android.ui.phone.livetv.ChannelTile
import app.pley.android.ui.phone.livetv.ProgramProgress
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.delay

/**
 * Search on the phone (design spec §4.5; the web's SearchPage): a search
 * bar docked at the top with the keyboard up when the tab opens empty.
 * Before typing, the way to Discover where requests are open, and the
 * profile's recent searches with Clear; while typing, the results as
 * rows, grouped and counted (People, Live TV, Movies, Shows…), the ones
 * shown staying while the next load, then what TMDB has that no library
 * has. A channel found plays at once.
 */
@Composable
internal fun PhoneSearchScreen(key: PleyKey.Search) {
    val resources = LocalResources.current
    val vm: SearchViewModel<ArtworkTileModel> = viewModel {
        SearchViewModel(key.query, SearchDeps.of(screensGraph()), phoneSearchTiles(PhoneTiles(resources)))
    }
    val entries: RequestsEntryViewModel = viewModel(factory = RequestsEntryViewModel.factory)
    val state by vm.state.collectAsStateWithLifecycle()
    val tmdb by vm.tmdb.collectAsStateWithLifecycle()
    val entry by entries.entry.collectAsStateWithLifecycle()
    val field = rememberTextFieldState(key.query)
    LaunchedEffect(field, vm) {
        snapshotFlow { field.text.toString() }.collect(vm::onQueryChange)
    }
    SearchContent(
        state = state,
        field = field,
        onSubmit = vm::submit,
        onRecent = { query ->
            field.setTextAndPlaceCursorAtEnd(query)
            vm.searchAgain(query)
        },
        onClearRecent = vm::clearRecent,
        open = vm::open,
        onRetry = vm::retry,
        onWatch = vm::watch,
        tmdb = tmdb,
        discover = entry.discover,
    )
}

/** The phone's search tiles: library tiles for items, a circle with the photo for people. */
internal fun phoneSearchTiles(items: PhoneTiles): SearchTiles<ArtworkTileModel> = SearchTiles.of(items) { person, origin ->
    ArtworkTileModel(id = person.id, title = person.name, image = ArtworkImage.of(person.photoUrl, origin), description = person.name)
}

@Composable
internal fun SearchContent(
    state: SearchUiState<ArtworkTileModel>,
    field: TextFieldState,
    onSubmit: () -> Unit,
    onRecent: (String) -> Unit,
    onClearRecent: () -> Unit,
    open: (SearchGroupKind, Long) -> PleyKey?,
    onRetry: () -> Unit = {},
    onWatch: (LiveChannel) -> Unit = {},
    tmdb: TmdbSection = TmdbSection.Hidden,
    /** Discover shows: its way in heads the page before typing. */
    discover: Boolean = false,
) {
    val navigator = LocalPhoneNavigator.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focusManager = LocalFocusManager.current
    // Scrolling the results puts the keyboard away.
    val hideOnScroll = remember(keyboard) {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (source == NestedScrollSource.UserInput && available.y != 0f) keyboard?.hide()
                return Offset.Zero
            }
        }
    }
    Column(Modifier.fillMaxSize().testTag(SEARCH_PAGE)) {
        SearchBar(
            field = field,
            onSubmit = {
                onSubmit()
                keyboard?.hide()
                focusManager.clearFocus()
            },
            autoFocus = state.query.isEmpty(),
        )
        OfflineBanner()
        Box(Modifier.weight(1f).fillMaxWidth().nestedScroll(hideOnScroll)) {
            when (val results = state.results) {
                SearchResultsState.Idle -> Recent(state.recent, onRecent, onClearRecent, discover = discover)
                is SearchResultsState.Loading -> Skeletons()
                is SearchResultsState.Empty -> if (tmdb == TmdbSection.Hidden) {
                    Message(
                        title = stringResource(R.string.search_none_title, results.query),
                        body = stringResource(R.string.search_none_body),
                        icon = PleyIcons.Search,
                    )
                } else {
                    OnlyOnTmdb(results.query, tmdb)
                }
                is SearchResultsState.Failed -> Column(Modifier.fillMaxSize().padding(top = 48.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    ErrorState(stringResource(R.string.search_failed), results.error, onRetry = onRetry)
                }
                is SearchResultsState.Ready -> Results(
                    results,
                    tmdb = tmdb,
                    onOpen = { group, id -> open(group, id)?.let(navigator::open) },
                    onActions = { id -> navigator.open(PleyKey.ItemActions(id)) },
                    onWatch = onWatch,
                )
            }
        }
    }
}

/** The search bar docked at the top, under the status bar: [SearchField] for the libraries. */
@Composable
private fun SearchBar(field: TextFieldState, onSubmit: () -> Unit, autoFocus: Boolean) {
    SearchField(
        field = field,
        label = stringResource(R.string.search_field),
        onSubmit = onSubmit,
        autoFocus = autoFocus,
        modifier = Modifier.windowInsetsPadding(WindowInsets.statusBars),
    )
}

/**
 * A search pill: 56 dp in [surface3][PleyColors.Surface3] with the search
 * glyph, the field saying [label] while empty, and Clear. The keyboard's
 * action is Search. [autoFocus] raises the keyboard the first time the
 * page shows. Search's bar, and Discover's.
 */
@Composable
internal fun SearchField(
    field: TextFieldState,
    label: String,
    onSubmit: () -> Unit,
    autoFocus: Boolean,
    modifier: Modifier = Modifier,
    tag: String = SEARCH_FIELD,
) {
    val metrics = LocalPhoneMetrics.current
    val focus = remember { FocusRequester() }
    var focusedOnce by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        if (autoFocus && !focusedOnce) runCatching { focus.requestFocus() }
        focusedOnce = true
    }
    Row(
        modifier
            .fillMaxWidth()
            .padding(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 8.dp)
            .height(56.dp)
            .background(PleyColors.Surface3, PleyShape.Pill)
            .padding(start = 16.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.Search, contentDescription = null, size = 22.dp, tint = PleyColors.Muted)
        Spacer(Modifier.width(12.dp))
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (field.text.isEmpty()) {
                Text(label, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            BasicTextField(
                state = field,
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(focus)
                    .semantics { contentDescription = label }
                    .testTag(tag),
                textStyle = MaterialTheme.typography.bodyLarge.copy(color = PleyColors.Fg),
                cursorBrush = SolidColor(PleyColors.AccentBright),
                lineLimits = TextFieldLineLimits.SingleLine,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, imeAction = ImeAction.Search),
                onKeyboardAction = { onSubmit() },
            )
        }
        if (field.text.isNotEmpty()) {
            PleyIconButton(PleyIcons.X, stringResource(R.string.search_clear_field), onClick = { field.clearText() }, tint = PleyColors.Muted)
        } else {
            Spacer(Modifier.width(12.dp))
        }
    }
}

/** Before typing: the way to Discover where requests are open, then the recent searches with Clear, or what search finds. */
@Composable
private fun Recent(recent: List<String>, onRecent: (String) -> Unit, onClear: () -> Unit, discover: Boolean) {
    if (recent.isEmpty()) {
        Column(Modifier.fillMaxSize()) {
            if (discover) DiscoverEntry()
            Message(stringResource(R.string.search_empty_title), stringResource(R.string.search_empty_body), icon = PleyIcons.Search)
        }
        return
    }
    val metrics = LocalPhoneMetrics.current
    LazyColumn(Modifier.fillMaxSize().testTag(SEARCH_RECENT), contentPadding = PaddingValues(bottom = 24.dp)) {
        if (discover) {
            item(key = "discover", contentType = "discover") { DiscoverEntry() }
        }
        item(key = "header", contentType = "header") {
            Row(
                Modifier.fillMaxWidth().padding(start = metrics.marginStart, end = metrics.marginEnd - 8.dp, top = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    stringResource(R.string.search_recent),
                    style = MaterialTheme.typography.titleLarge,
                    color = PleyColors.Fg,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                PleyTextButton(stringResource(R.string.search_recent_clear), onClick = onClear)
            }
        }
        items(recent, key = { it }, contentType = { "recent" }) { query ->
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .clickable(role = Role.Button) { onRecent(query) }
                    .padding(start = metrics.marginStart, end = metrics.marginEnd),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(PleyIcons.History, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
                Spacer(Modifier.width(16.dp))
                Text(query, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            }
        }
    }
}

/**
 * The results: a line saying what they are for (with a spinner while newer
 * ones load), then a row per group, Live TV's after People (the web's
 * order), then what TMDB has ([tmdb]).
 */
@Composable
private fun Results(
    results: SearchResultsState.Ready<ArtworkTileModel>,
    tmdb: TmdbSection,
    onOpen: (SearchGroupKind, Long) -> Unit,
    onActions: (Long) -> Unit,
    onWatch: (LiveChannel) -> Unit,
) {
    val metrics = LocalPhoneMetrics.current
    val list = rememberLazyListState()
    LazyColumn(
        state = list,
        modifier = Modifier.fillMaxSize().testTag(SEARCH_RESULTS),
        contentPadding = PaddingValues(top = 8.dp, bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(PleyPhone.SectionGap),
    ) {
        item(key = "for", contentType = "for") {
            Row(Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    stringResource(R.string.search_results_for, results.query),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (results.loading) {
                    Spacer(Modifier.width(8.dp))
                    DelayedSpinner(stringResource(R.string.search_loading), size = 16.dp)
                }
            }
        }
        val people = results.groups.takeWhile { it.kind == SearchGroupKind.People }
        val rest = results.groups.drop(people.size)
        items(people, key = { it.kind.name }, contentType = { it.kind.tileKind }) { group ->
            ResultRow(group, onOpen = { onOpen(group.kind, it) }, onActions = null)
        }
        if (results.channels.isNotEmpty()) {
            item(key = "channels", contentType = "channels") {
                ChannelRow(results.channels, results.origin, onWatch)
            }
        }
        items(rest, key = { it.kind.name }, contentType = { it.kind.tileKind }) { group ->
            ResultRow(group, onOpen = { onOpen(group.kind, it) }, onActions = onActions)
        }
        if (tmdb != TmdbSection.Hidden) {
            item(key = "tmdb", contentType = "tmdb") { OnTmdb(results.query, tmdb) }
        }
    }
}

/** A group's name and count, read as one heading. */
@Composable
private fun RowHeader(name: String, count: Int) {
    val metrics = LocalPhoneMetrics.current
    val spoken = pluralStringResource(R.plurals.search_group_count, count, name, count.toString())
    Row(
        Modifier
            .padding(start = metrics.marginStart, end = metrics.marginEnd)
            .heightIn(min = 32.dp)
            .clearAndSetSemantics {
                contentDescription = spoken
                heading()
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(name, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg)
        Spacer(Modifier.width(8.dp))
        Text(count.toString(), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
    }
}

/**
 * The Live TV channels found (the web's ChannelResults): cards with the
 * logo, number and name, and what's on with how far it is (the group
 * without a guide). A card plays its channel.
 */
@Composable
private fun ChannelRow(channels: List<LiveChannel>, origin: String, onWatch: (LiveChannel) -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val now by produceState(System.currentTimeMillis() / 1000) {
        while (true) {
            delay(NOW_TICK_MS)
            value = System.currentTimeMillis() / 1000
        }
    }
    Column(Modifier.fillMaxWidth()) {
        RowHeader(stringResource(R.string.search_group_live), channels.size)
        Spacer(Modifier.height(PleyPhone.HeaderToContent - PRESS_ROOM))
        LazyRow(
            state = rememberHubRowState(),
            modifier = Modifier.fillMaxWidth().testTag(SEARCH_ROW + SEARCH_CHANNELS),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = PRESS_ROOM, bottom = PRESS_ROOM),
            horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
        ) {
            items(channels, key = { it.id }, contentType = { "channel" }) { channel ->
                ChannelCard(channel, origin, now, onClick = { onWatch(channel) })
            }
        }
    }
}

@Composable
private fun ChannelCard(channel: LiveChannel, origin: String, now: Long, onClick: () -> Unit) {
    val on = channel.onAt(now)
    val title = listOfNotNull(channel.number, channel.name).joinToString(" ")
    val watch = if (on != null) {
        stringResource(R.string.livetv_watch_with_now, title, on.title)
    } else {
        stringResource(R.string.livetv_watch, title)
    }
    Row(
        Modifier
            .width(CHANNEL_CARD_WIDTH)
            .clip(PleyShape.Card)
            .background(PleyColors.Surface)
            .clickable(role = Role.Button, onClickLabel = watch, onClick = onClick)
            .padding(10.dp)
            .testTag(SEARCH_CHANNEL + channel.id),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ChannelTile(channel, origin, Modifier.size(width = 72.dp, height = 44.dp), well = PleyColors.Surface3)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                channel.number?.let {
                    Text(it, style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle, maxLines = 1)
                    Spacer(Modifier.width(6.dp))
                }
                Text(channel.name, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (on != null) {
                Text(on.title, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.height(4.dp))
                ProgramProgress(on, now, Modifier.fillMaxWidth())
            } else if (channel.group.isNotEmpty()) {
                Text(channel.group, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** One group: its name and count, then its cards in a row that scrolls sideways. */
@Composable
private fun ResultRow(group: SearchGroup<ArtworkTileModel>, onOpen: (Long) -> Unit, onActions: ((Long) -> Unit)?) {
    val metrics = LocalPhoneMetrics.current
    val kind = group.kind.tileKind
    val width = rowCardWidth(kind, metrics.contentWidth - metrics.margin * 2)
    Column(Modifier.fillMaxWidth()) {
        RowHeader(groupName(group.kind), group.count)
        Spacer(Modifier.height(PleyPhone.HeaderToContent - PRESS_ROOM))
        LazyRow(
            state = rememberHubRowState(),
            modifier = Modifier.fillMaxWidth().testTag(SEARCH_ROW + group.kind.name),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = PRESS_ROOM, bottom = PRESS_ROOM),
            horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
        ) {
            items(group.tiles, key = { it.id }, contentType = { kind }) { tile ->
                MediaCard(
                    model = tile,
                    kind = kind,
                    onClick = { onOpen(tile.id) },
                    onLongClick = onActions?.let { { it(tile.id) } },
                    modifier = Modifier.width(width),
                )
            }
        }
    }
}

/** Two rows of skeleton cards while the first answer comes. */
@Composable
private fun Skeletons() {
    val skeleton = rememberSkeleton(animated = true)
    val metrics = LocalPhoneMetrics.current
    Column(Modifier.fillMaxSize().padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(PleyPhone.SectionGap)) {
        repeat(2) {
            SkeletonRow(skeleton, TileKind.Poster, metrics.marginStart, rowCardWidth(TileKind.Poster, metrics.contentWidth - metrics.margin * 2))
        }
    }
}

@Composable
private fun SkeletonRow(skeleton: SkeletonState, kind: TileKind, margin: androidx.compose.ui.unit.Dp, width: androidx.compose.ui.unit.Dp) {
    Column(Modifier.fillMaxWidth()) {
        Box(Modifier.padding(horizontal = margin).size(120.dp, 22.dp).background(PleyColors.Surface2, PleyShape.Badge))
        Spacer(Modifier.height(PleyPhone.HeaderToContent))
        Row(Modifier.padding(horizontal = margin), horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap)) {
            repeat(4) { SkeletonTile(skeleton, kind, TileStyle.Phone, width = width) }
        }
    }
}

@Composable
private fun Message(title: String, body: String, icon: PleyIcon? = null) {
    Column(Modifier.fillMaxSize().padding(top = 48.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        EmptyState(title = title, body = body, icon = icon)
    }
}

@Composable
private fun groupName(kind: SearchGroupKind): String = stringResource(
    when (kind) {
        SearchGroupKind.People -> R.string.search_group_people
        SearchGroupKind.Movies -> R.string.search_group_movies
        SearchGroupKind.Shows -> R.string.search_group_shows
        SearchGroupKind.Seasons -> R.string.search_group_seasons
        SearchGroupKind.Episodes -> R.string.search_group_episodes
        SearchGroupKind.Artists -> R.string.search_group_artists
        SearchGroupKind.Albums -> R.string.search_group_albums
        SearchGroupKind.Tracks -> R.string.search_group_tracks
    },
)

/** How a group's cards are drawn: people as circles, the rest by their look. */
internal val SearchGroupKind.tileKind: TileKind
    get() = if (this == SearchGroupKind.People) {
        TileKind.Person
    } else {
        when (look) {
            TileLook.Landscape -> TileKind.Landscape
            TileLook.Square -> TileKind.Square
            else -> TileKind.Poster
        }
    }

/** Room above and below cards for a pressed card's growth and ring (the kit's rows keep the same). */
private val PRESS_ROOM = 6.dp

internal const val SEARCH_PAGE = "pley.search"
internal const val SEARCH_FIELD = "pley.search.field"
internal const val SEARCH_RECENT = "pley.search.recent"
internal const val SEARCH_RESULTS = "pley.search.results"
internal const val SEARCH_ROW = "pley.search.row."
internal const val SEARCH_CHANNELS = "Channels"
internal const val SEARCH_CHANNEL = "pley.search.channel."

/** How wide a channel found is drawn (the web's 16 rem). */
private val CHANNEL_CARD_WIDTH = 256.dp

/** How often what's on a channel found is looked at again. */
private const val NOW_TICK_MS = 30_000L
