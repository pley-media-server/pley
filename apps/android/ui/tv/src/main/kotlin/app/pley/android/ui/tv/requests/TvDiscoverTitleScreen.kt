package app.pley.android.ui.tv.requests

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverSeason
import app.pley.android.model.SeasonState
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.Ask
import app.pley.android.screens.requests.DiscoverTitleViewModel
import app.pley.android.screens.requests.MainAction
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.SeasonPicker
import app.pley.android.screens.requests.TitleBadge
import app.pley.android.screens.requests.TitleUiState
import app.pley.android.screens.requests.askable
import app.pley.android.screens.requests.downloadProgress
import app.pley.android.screens.requests.requestOf
import app.pley.android.screens.requests.stagePill
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvMetaLine
import app.pley.android.ui.tv.item.TvTextDialog
import app.pley.android.ui.tv.item.TvToastHost
import app.pley.android.ui.tv.item.TvToastState
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.LocalTvRowColumn
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.focusRow
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.settings.TvSettingRow
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.theme.LocalTvTier

/**
 * A movie or show found on TMDB, on TV (packages/tv's title.tsx), over
 * its backdrop (its poster at the right without one): the hero with its
 * facts, network and genres, tagline and overview, and its one main
 * action, with Your requests where that takes no focus, and Overview when
 * the overview is cut; then its seasons, each with its state or its
 * request's stage, and its cast. Select on a season that may be asked for
 * opens Which seasons? at it; [PleyKey.DiscoverTitle.pick] opens it with
 * the page. Focus starts on the main action.
 */
@Composable
internal fun TvDiscoverTitleScreen(key: PleyKey.DiscoverTitle) {
    TvDiscoverTitleScreen(viewModel { DiscoverTitleViewModel(key, RequestsDeps.of(screensGraph())) })
}

@Composable
internal fun TvDiscoverTitleScreen(vm: DiscoverTitleViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val texts = rememberRequestTexts()
    val toasts = remember { TvToastState() }
    TvRequestEvents(vm.events, texts, toasts) {
        when (val s = vm.state.value) {
            is TitleUiState.Ready -> s.summary
            is TitleUiState.Problem -> s.summary
            TitleUiState.Loading -> null
        }
    }
    Box(Modifier.fillMaxSize().testTag(TV_TITLE_PAGE)) {
        when (val s = state) {
            TitleUiState.Loading -> TitleSkeleton()
            is TitleUiState.Problem -> if (s.problem == RequestProblem.NotFound) {
                TvRequestsNotice(
                    title = stringResource(ScreensR.string.requests_not_found),
                    body = stringResource(ScreensR.string.requests_not_found_body),
                    icon = PleyIcons.Compass,
                    action = Triple(stringResource(R.string.tv_requests_back), PleyIcons.ArrowLeft) { navigator.back() },
                )
            } else {
                TvRequestMessage(s.problem, s.summary, texts, stringResource(ScreensR.string.requests_title_failed), s.origin, onRetry = vm::retry)
            }
            is TitleUiState.Ready -> {
                // Read again and refused, the page keeps what it showed and says why (the shell says so when the server is away).
                s.problem?.takeIf { it !is RequestProblem.Offline }?.let { problem ->
                    LaunchedEffect(problem) { toasts.show(texts.problem(problem, s.summary), error = true) }
                }
                TitlePage(s, vm, texts)
            }
        }
        TvToastHost(toasts)
    }
}


@Composable
private fun TitlePage(page: TitleUiState.Ready, vm: DiscoverTitleViewModel, texts: RequestTexts) {
    val navigator = LocalTvNavigator.current
    val rail = LocalTvRailFocus.current
    val density = LocalDensity.current
    val tier = LocalTvTier.current
    val detail = page.detail
    val memory = rememberFocusMemory()
    val main = remember { FocusRequester() }
    val picker by vm.picker.collectAsStateWithLifecycle()
    var inHero by remember { mutableStateOf(true) }
    var overview by remember { mutableStateOf(false) }
    var overviewCut by remember { mutableStateOf(false) }
    var signIn by remember { mutableStateOf(false) }
    var heroPx by remember { mutableIntStateOf(0) }
    val heroHeight = maxOf(with(density) { heroPx.toDp() }, HeroMin)
    val infoAlpha by animateFloatAsState(if (inHero) 0f else 1f, PleyMotion.reveal(), label = "info")
    val backdrop = remember(detail.backdropUrl, page.origin, tier) {
        ArtworkImage.of(detail.backdropUrl, page.origin)?.copy(maxWidth = ArtworkImage.backdropCap(tier))
    }
    val handlers = remember(vm) {
        TvTitleHandlers(
            request = vm::request,
            more = { vm.openPicker() },
            tellMeToo = vm::tellMeToo,
            stopFollowing = vm::stopFollowing,
            open = { navigator.open(PleyKey.Item(it)) },
            signInAgain = { signIn = true },
        )
    }
    val seasons = remember(detail) { detail.seasons.orEmpty().sortedWith(compareBy<DiscoverSeason> { it.number == 0 }.thenBy { it.number }) }
    val pickable = canPick(detail, page.action)

    Box(Modifier.fillMaxSize()) {
        TvBackdrop(backdrop, detail.title, scrim = BackdropScrim.Item)
        // No backdrop on TMDB: the poster stands at the right of the hero instead.
        if (backdrop == null && inHero) {
            val poster = remember(detail.posterUrl, page.origin) { ArtworkImage.of(detail.posterUrl, page.origin) }
            if (poster != null) {
                Artwork(poster, detail.title, Modifier.offset(PosterX, HeroTop).size(PosterWidth, PosterHeight).background(PleyColors.Surface2, PleyShape.Control))
            }
        }
        TitleInfo(detail, texts, Modifier.graphicsLayer { alpha = infoAlpha })
        TvRowColumn(heroHeight = heroHeight, modifier = Modifier.fillMaxSize().testTag(TV_TITLE_ROWS)) {
            item(HERO_KEY, contentType = HERO_KEY) {
                Hero(
                    page = page,
                    texts = texts,
                    handlers = handlers,
                    memory = memory,
                    main = main,
                    onFocused = { inHero = true },
                    onCut = { overviewCut = it },
                    overviewCut = overviewCut,
                    onOverview = { overview = true },
                    modifier = Modifier
                        .heightIn(min = HeroMin)
                        .onSizeChanged { heroPx = it.height }
                        .graphicsLayer { alpha = 1f - infoAlpha },
                )
            }
            if (seasons.isNotEmpty()) {
                item(SEASONS_KEY, contentType = SEASONS_KEY) {
                    SeasonsRow(seasons, page, texts, memory, onFocus = { inHero = false }) { season ->
                        if (pickable && askable(season, detail)) vm.openPicker(season.number)
                    }
                }
            }
            if (detail.cast.isNotEmpty()) {
                item(CAST_KEY, contentType = CAST_KEY) {
                    val tiles = remember(detail.cast, page.origin) {
                        detail.cast.mapIndexed { i, member ->
                            ArtworkTileModel(
                                id = i.toLong(),
                                title = member.name,
                                subtitle = member.character.orEmpty(),
                                image = ArtworkImage.of(member.photoUrl, page.origin),
                                description = listOfNotNull(member.name, member.character?.takeIf { it.isNotBlank() }).joinToString(", "),
                            )
                        }
                    }
                    // TMDB's people have no page here: the row is there to read.
                    TvRow(
                        rowKey = CAST_KEY,
                        title = stringResource(ScreensR.string.requests_cast),
                        tiles = tiles,
                        kind = TileKind.Person,
                        memory = memory,
                        onOpen = { },
                        onFocus = { inHero = false },
                    )
                }
            }
            item(CREDIT_KEY, contentType = CREDIT_KEY) { TvTmdbCredit() }
        }
        SeasonPickerPanel(picker, detail, page, texts, vm, returnTo = main)
        if (overview) {
            TvTextDialog(
                title = detail.title,
                text = listOfNotNull(detail.tagline?.takeIf { it.isNotBlank() }, detail.overview).joinToString("\n\n"),
                onDismiss = { overview = false },
            )
        }
        if (signIn) TvSignInAgainDialog(page.origin, onDismiss = { signIn = false }, returnTo = main)
    }
    // A page with nothing to focus (it can't be asked for, and has no rows): the rail, so the remote is never dead.
    val nothing = seasons.isEmpty() && detail.cast.isEmpty() && !takesFocus(page.action) && !overviewCut && page.summary?.requestsEntry != true
    LaunchedEffect(nothing) {
        if (nothing) {
            withFrameNanos { }
            rail?.focusCurrent()
        }
    }
}

/**
 * The hero (title.tsx's TitleHero), in the 464 dp text column at x = 96:
 * the title bottom-aligned in the logo's box, its facts, network and
 * genres, the tagline, three lines of overview, then the main action.
 * After it, Your requests when the main action takes no focus, and
 * Overview when the overview runs past its lines.
 */
@Composable
private fun Hero(
    page: TitleUiState.Ready,
    texts: RequestTexts,
    handlers: TvTitleHandlers,
    memory: FocusMemory,
    main: FocusRequester,
    onFocused: () -> Unit,
    onCut: (Boolean) -> Unit,
    overviewCut: Boolean,
    onOverview: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val navigator = LocalTvNavigator.current
    val column = LocalTvRowColumn.current
    val group = remember { FocusRequester() }
    if (column != null) {
        DisposableEffect(column) {
            column.rows[HERO_KEY] = group
            onDispose { if (column.rows[HERO_KEY] === group) column.rows.remove(HERO_KEY) }
        }
    }
    val detail = page.detail
    val seasons = detail.seasons.orEmpty().count { it.number > 0 }
    val seasonsLine = if (detail.kind == TitleKind.Show && seasons > 0) pluralStringResource(ScreensR.plurals.requests_seasons, seasons, seasons) else null
    val kind = texts.kind(detail.kind)
    Column(
        modifier
            .fillMaxWidth()
            .focusRequester(group)
            .onFocusChanged {
                if (it.hasFocus) {
                    onFocused()
                    column?.focusedRow = HERO_KEY
                }
            }
            .focusGroup()
            .padding(start = PleyTv.Gutter, top = HeroTop, end = TvSafeArea.Horizontal, bottom = 8.dp)
            .testTag(TV_TITLE_HERO),
    ) {
        Box(Modifier.width(TextColumn).heightIn(min = TitleBox), contentAlignment = Alignment.BottomStart) {
            BasicText(
                detail.title,
                style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.semantics { heading() },
            )
        }
        Spacer(Modifier.height(12.dp))
        TvMetaLine(remember(detail, seasonsLine, kind) { titleMeta(detail, seasonsLine, kind) })
        val facts = (listOfNotNull(detail.network?.takeIf { it.isNotBlank() }) + detail.genres).joinToString(" · ")
        if (facts.isNotEmpty()) {
            Spacer(Modifier.height(6.dp))
            BasicText(facts, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
        }
        detail.tagline?.takeIf { it.isNotBlank() }?.let {
            Spacer(Modifier.height(10.dp))
            BasicText(
                it,
                style = PleyType.Tv.Body.copy(color = PleyColors.Fg.copy(alpha = 0.75f), fontStyle = FontStyle.Italic),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.width(TextColumn),
            )
        }
        if (detail.overview.isNotBlank()) {
            Spacer(Modifier.height(10.dp))
            BasicText(
                detail.overview,
                style = PleyType.Tv.BodyLarge.copy(color = SummaryColor),
                maxLines = OVERVIEW_LINES,
                overflow = TextOverflow.Ellipsis,
                onTextLayout = { onCut(it.hasVisualOverflow) },
                modifier = Modifier.width(TextColumn),
            )
        }
        Spacer(Modifier.height(22.dp))
        TvTitleActions(
            action = page.action,
            kind = detail.kind,
            allowance = page.allowance,
            manage = page.manage,
            texts = texts,
            on = handlers,
            mainModifier = Modifier.focusRequester(main).rememberedFocus(memory, "main", initial = true),
        ) { claimed ->
            val summary = page.summary
            if (!claimed && summary != null && summary.requestsEntry) {
                TvRoundButton(
                    label = stringResource(if (summary.manage) ScreensR.string.requests_manage else ScreensR.string.requests_yours),
                    icon = PleyIcons.ListVideo,
                    onClick = { navigator.open(PleyKey.Requests) },
                    modifier = Modifier.rememberedFocus(memory, "requests", initial = true),
                )
            }
            if (overviewCut) {
                TvRoundButton(
                    label = stringResource(R.string.tv_requests_overview),
                    icon = PleyIcons.Ellipsis,
                    onClick = onOverview,
                    modifier = Modifier.rememberedFocus(memory, "overview", initial = !claimed && summary?.requestsEntry != true),
                )
            }
        }
    }
}

/**
 * The seasons, specials last, as posters: each with its badge (In
 * library, Partly in library, Requested, Declined) and under it its
 * request's stage, else its state ("10 episodes", "Airs Nov 4, 2026").
 */
@Composable
private fun SeasonsRow(
    seasons: List<DiscoverSeason>,
    page: TitleUiState.Ready,
    texts: RequestTexts,
    memory: FocusMemory,
    onFocus: () -> Unit,
    onSelect: (DiscoverSeason) -> Unit,
) {
    val detail = page.detail
    val tiles = remember(seasons, detail, page.origin, texts, page.manage) {
        seasons.map { season ->
            val request = requestOf(season, detail)
            val line = if (request != null && season.state == SeasonState.Requested) texts.pill(stagePill(request, page.manage)) else texts.seasonState(season)
            val badge = seasonBadge(season)?.let { texts.badge(it) }
            val name = texts.seasonName(season)
            ArtworkTileModel(
                id = season.number.toLong(),
                title = name,
                subtitle = line,
                // The season's poster, else the show's.
                image = ArtworkImage(listOfNotNull(season.posterUrl, detail.posterUrl).mapNotNull { ArtworkImage.of(it, page.origin) }.flatMap { it.candidates }),
                progress = request?.takeIf { season.state == SeasonState.Requested }?.let(::downloadProgress) ?: 0f,
                badges = listOfNotNull(badge),
                description = listOfNotNull(name, badge, line).joinToString(", "),
            )
        }
    }
    val byNumber = remember(seasons) { seasons.associateBy { it.number.toLong() } }
    TvRow(
        rowKey = SEASONS_KEY,
        title = stringResource(ScreensR.string.requests_seasons_heading),
        tiles = tiles,
        kind = TileKind.Poster,
        memory = memory,
        onOpen = { tile -> byNumber[tile.id]?.let(onSelect) },
        onFocus = { onFocus() },
    )
}

/** A season's badge: what it is for this profile; none where it's still to ask for. */
private fun seasonBadge(season: DiscoverSeason): TitleBadge? = when {
    season.state == SeasonState.Library -> TitleBadge.InLibrary
    season.state == SeasonState.Partial -> TitleBadge.PartlyInLibrary
    season.state == SeasonState.Requested -> TitleBadge.Requested
    season.declined -> TitleBadge.Declined
    else -> null
}

/** The block over the rows once focus leaves the hero: the title, its facts, two lines of overview. */
@Composable
private fun TitleInfo(detail: DiscoverDetail, texts: RequestTexts, modifier: Modifier = Modifier) {
    val seasons = detail.seasons.orEmpty().count { it.number > 0 }
    val seasonsLine = if (detail.kind == TitleKind.Show && seasons > 0) pluralStringResource(ScreensR.plurals.requests_seasons, seasons, seasons) else null
    val kind = texts.kind(detail.kind)
    Column(modifier.padding(start = PleyTv.Gutter, top = 28.dp, end = TvSafeArea.Horizontal).width(TextColumn)) {
        BasicText(detail.title, style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
        Spacer(Modifier.height(10.dp))
        TvMetaLine(remember(detail, seasonsLine, kind) { titleMeta(detail, seasonsLine, kind) })
        if (detail.overview.isNotBlank()) {
            Spacer(Modifier.height(8.dp))
            BasicText(detail.overview, style = PleyType.Tv.Body.copy(color = SummaryColor), maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

/**
 * Which seasons? (title.tsx's SeasonPicker), a side panel: the title
 * under its name, every season with its line, those that may be asked
 * for with a tick box, then the limits when more are ticked than may be
 * asked for (the first line marked), and Request N seasons. Focus starts
 * on the season it was opened at, else the first that may be asked for,
 * and stays in the panel; Back closes it, focus back on [returnTo].
 */
@Composable
private fun SeasonPickerPanel(
    picker: SeasonPicker?,
    detail: DiscoverDetail,
    page: TitleUiState.Ready,
    texts: RequestTexts,
    vm: DiscoverTitleViewModel,
    returnTo: FocusRequester,
) {
    var last by remember { mutableStateOf(picker) }
    if (picker != null) last = picker
    val shown = last
    // Closed, focus goes back once the panel has let go of it (it keeps focus while it is open).
    var wasOpen by remember { mutableStateOf(false) }
    LaunchedEffect(picker != null) {
        if (picker != null) {
            wasOpen = true
        } else if (wasOpen) {
            wasOpen = false
            withFrameNanos { }
            withFrameNanos { }
            runCatching { returnTo.requestFocus() }
        }
    }
    TvSidePanel(visible = picker != null, onDismiss = vm::closePicker, modifier = Modifier.testTag(TV_PICKER)) {
        if (shown == null) return@TvSidePanel
        val list = rememberLazyListState()
        val first = remember { FocusRequester() }
        val start = shown.start?.takeIf { n -> shown.seasons.any { it.askable && it.season.number == n } }
            ?: shown.seasons.firstOrNull { it.askable }?.season?.number
        LaunchedEffect(picker != null) {
            if (picker != null) {
                withFrameNanos { }
                withFrameNanos { }
                list.focusRow(first)
            }
        }
        Column(Modifier.padding(start = 16.dp, top = 8.dp, bottom = 12.dp)) {
            BasicText(stringResource(ScreensR.string.requests_picker_title), style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 2)
            BasicText(detail.title, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        LazyColumn(
            state = list,
            modifier = Modifier
                .weight(1f)
                .focusProperties { onExit = { if (picker != null) cancelFocusChange() } }
                .focusGroup(),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            items(shown.seasons, key = { it.season.number }, contentType = { "season" }) { row ->
                val number = row.season.number
                val ticked = number in shown.picked
                TvSettingRow(
                    label = texts.seasonName(row.season),
                    value = texts.seasonState(row.season),
                    enabled = row.askable,
                    onClick = { vm.toggle(number) },
                    focusRequester = if (number == start) first else null,
                    modifier = Modifier.testTag(TV_PICKER_SEASON + number),
                    trailing = if (row.askable) ({ focused -> Tick(ticked, focused) }) else null,
                )
            }
            val lines = if (shown.overLimit || shown.left?.let { it <= 0 } == true) texts.allowance(page.allowance, TitleKind.Show) else emptyList()
            if (lines.isNotEmpty()) {
                item(key = "limits", contentType = "limits") {
                    Column(Modifier.padding(start = 16.dp, top = 12.dp, end = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        lines.forEachIndexed { i, line ->
                            Row(verticalAlignment = Alignment.Top) {
                                // More ticked than may be asked for: the line saying how many may stands out.
                                if (shown.overLimit && i == 0) {
                                    Icon(PleyIcons.CircleAlert, contentDescription = null, size = 14.dp, tint = PleyColors.DangerText, modifier = Modifier.padding(top = 2.dp))
                                    Spacer(Modifier.width(6.dp))
                                }
                                BasicText(line, style = PleyType.Tv.Caption.copy(color = if (shown.overLimit && i == 0) PleyColors.Fg else PleyColors.Muted))
                            }
                        }
                    }
                }
            }
            item(key = "ask", contentType = "ask") {
                Box(Modifier.padding(start = 16.dp, top = 16.dp, bottom = 8.dp)) {
                    TvPillButton(
                        label = texts.pickerButton(shown),
                        onClick = vm::askPicked,
                        icon = if (shown.count > 0) PleyIcons.Plus else null,
                        enabled = shown.ready,
                        modifier = Modifier.testTag(TV_PICKER_ASK),
                    )
                }
            }
        }
    }
}

/** A tick box at a picker row's end, in the row's colours: filled with a check when ticked, an edge when not. */
@Composable
private fun Tick(on: Boolean, focused: Boolean) {
    val fill = when {
        !on -> Color.Transparent
        focused -> PleyColors.OnAccent
        else -> PleyColors.Accent
    }
    val edge = if (on) fill else if (focused) PleyColors.OnAccent else PleyColors.Control
    Box(
        Modifier.size(20.dp).background(fill, PleyShape.Badge).border(1.5.dp, edge, PleyShape.Badge),
        contentAlignment = Alignment.Center,
    ) {
        if (on) Icon(PleyIcons.Check, contentDescription = null, size = 14.dp, tint = if (focused) PleyColors.Accent else PleyColors.OnAccent)
    }
}

/** The loading page: still blocks where the hero's text and button will be. No spinner. */
@Composable
private fun TitleSkeleton() {
    Column(Modifier.fillMaxSize().padding(start = PleyTv.Gutter, top = 82.dp)) {
        Block(320.dp, 40.dp)
        Spacer(Modifier.height(14.dp))
        Block(220.dp, 18.dp)
        Spacer(Modifier.height(12.dp))
        Block(160.dp, 14.dp)
        Spacer(Modifier.height(16.dp))
        Block(464.dp, 15.dp)
        Spacer(Modifier.height(7.dp))
        Block(440.dp, 15.dp)
        Spacer(Modifier.height(7.dp))
        Block(300.dp, 15.dp)
        Spacer(Modifier.height(27.dp))
        Box(Modifier.size(140.dp, PleyTv.ButtonHeight).background(PleyColors.Surface2, PleyShape.Control))
    }
}

@Composable
private fun Block(width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp) {
    Box(Modifier.size(width, height).background(PleyColors.Surface2, PleyShape.Badge))
}

/** Whether a show's seasons may be picked to ask for now: Request, or Request more seasons, even at the limit (the picker says so). */
internal fun canPick(detail: DiscoverDetail, action: MainAction): Boolean {
    if (detail.kind != TitleKind.Show || detail.seasons.orEmpty().none { askable(it, detail) }) return false
    return (action is MainAction.Request && action.ask != Ask.NoAccess) || (action is MainAction.Partial && (action.more == Ask.Yes || action.more == Ask.Limit))
}

/** Whether [action] draws a button that takes focus. */
private fun takesFocus(action: MainAction): Boolean = when (action) {
    MainAction.SignInAgain, is MainAction.Mine, is MainAction.Following -> true
    is MainAction.Open -> action.itemId != null
    is MainAction.TellMeToo -> action.ask != Ask.NoAccess
    is MainAction.Partial -> action.itemId != null || action.more == Ask.Yes
    is MainAction.Request -> action.ask == Ask.Yes
    is MainAction.Restricted, is MainAction.Declined -> false
}

/** The hero's top: the logo box at y = 52. */
private val HeroTop = 52.dp

/** The logo art's box; the title sits at its bottom. */
private val TitleBox = 72.dp

/** A short hero still puts the first row's header where its captions start under the screen's foot rather than cut at it. */
private val HeroMin = 372.dp

/** The text column: x 96 to 560. */
private val TextColumn = 464.dp
private val SummaryColor = PleyColors.Fg.copy(alpha = 0.85f)

/** Without a backdrop, the poster at the right of the hero. */
private val PosterX = 664.dp
private val PosterWidth = 200.dp
private val PosterHeight = 300.dp

private const val OVERVIEW_LINES = 3

private const val HERO_KEY = "pley.title.hero"
private const val SEASONS_KEY = "seasons"
private const val CAST_KEY = "cast"
private const val CREDIT_KEY = "pley.title.credit"

internal const val TV_TITLE_PAGE = "pley.tv.title"
internal const val TV_TITLE_ROWS = "pley.tv.title.rows"
internal const val TV_TITLE_HERO = "pley.tv.title.hero"
internal const val TV_PICKER = "pley.tv.title.picker"
internal const val TV_PICKER_SEASON = "pley.tv.title.picker.season."
internal const val TV_PICKER_ASK = "pley.tv.title.picker.ask"
