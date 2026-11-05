package app.pley.android.ui.phone.requests

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.model.CastMember
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverSeason
import app.pley.android.model.SeasonState
import app.pley.android.model.TitleKind
import app.pley.android.model.format.formatRuntime
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.item.MetaPart
import app.pley.android.screens.requests.Allowance
import app.pley.android.screens.requests.DiscoverTitleViewModel
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.SeasonPicker
import app.pley.android.screens.requests.TitleUiState
import app.pley.android.screens.requests.requestOf
import app.pley.android.screens.requests.stagePill
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.item.BackdropTopBar
import app.pley.android.ui.phone.item.ItemSkeleton
import app.pley.android.ui.phone.item.MetaLine
import app.pley.android.ui.phone.item.SectionTitle
import app.pley.android.ui.phone.item.Summary
import app.pley.android.ui.phone.item.TextColumnMax
import app.pley.android.ui.phone.item.WidePosterWidth
import app.pley.android.ui.phone.item.itemBackdropHeight
import app.pley.android.ui.phone.item.itemTitleTop
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.rowCardWidth
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass

/**
 * A title found on TMDB, laid out as a
 * library's title page is: the backdrop fading into the page with the
 * poster, the title, year · certification · runtime or seasons · rating,
 * the genres; the one main action; the tagline and overview. Then the
 * requests for it with their actions, the seasons with each one's state
 * (and its request's stage), the cast with their photos, and TMDB's
 * credit. Request on a show opens the season picker.
 */
@Composable
internal fun PhoneDiscoverTitleScreen(key: PleyKey.DiscoverTitle) {
    val vm: DiscoverTitleViewModel = viewModel { DiscoverTitleViewModel(key, RequestsDeps.of(screensGraph())) }
    val state by vm.state.collectAsStateWithLifecycle()
    val picker by vm.picker.collectAsStateWithLifecycle()
    val approving by vm.approving.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val texts = rememberRequestTexts()
    RequestEvents(vm.events, texts) {
        when (val s = state) {
            is TitleUiState.Ready -> s.summary
            is TitleUiState.Problem -> s.summary
            TitleUiState.Loading -> null
        }
    }
    when (val s = state) {
        TitleUiState.Loading -> ItemSkeleton(onBack = navigator::back)
        is TitleUiState.Problem -> Column(Modifier.fillMaxSize().testTag(TITLE_PAGE)) {
            PleyTopAppBar(title = "", onBack = navigator::back)
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                RequestProblemState(s.problem, s.summary, texts, stringResource(R.string.requests_title_failed), onRetry = vm::retry)
            }
        }
        is TitleUiState.Ready -> {
            TitlePage(s, texts, vm)
            picker?.let { open ->
                SeasonPickerSheet(
                    picker = open,
                    allowance = s.allowance,
                    busy = s.busy,
                    texts = texts,
                    onToggle = vm::toggle,
                    onRequest = vm::askPicked,
                    onDismiss = vm::closePicker,
                )
            }
            approving?.let { dialog -> ApproveDialogView(dialog, texts, onApprove = vm::approve, onDismiss = vm::closeApprove) }
        }
    }
}

@Composable
private fun TitlePage(page: TitleUiState.Ready, texts: RequestTexts, vm: DiscoverTitleViewModel) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val list = rememberLazyListState()
    val detail = page.detail
    val sides = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd)
    val handlers = remember(vm, navigator) {
        TitleActionHandlers(
            request = vm::request,
            more = vm::openPicker,
            tellMeToo = vm::tellMeToo,
            stopFollowing = vm::stopFollowing,
            open = { navigator.open(PleyKey.Item(it)) },
        )
    }
    val act = rememberRequestActions(texts) { request, action, reason -> vm.act(request, action, reason) }
    Box(Modifier.fillMaxSize()) {
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().testTag(TITLE_PAGE),
            contentPadding = PaddingValues(bottom = 32.dp),
        ) {
            item(key = "header", contentType = "header") { TitleHeader(page, texts, handlers, onRetry = vm::retry) }
            if (detail.requests.isNotEmpty()) {
                item(key = "requests-title", contentType = "section-title") {
                    Box(sides.padding(top = PleyPhone.SectionGap, bottom = PleyPhone.HeaderToContent)) {
                        SectionTitle(stringResource(if (page.manage) R.string.requests_manage else R.string.requests_yours))
                    }
                }
                items(detail.requests, key = { "request-${it.id}" }, contentType = { "request" }) { request ->
                    RequestCard(
                        request = request,
                        origin = page.origin,
                        manage = page.manage,
                        busy = page.busy,
                        texts = texts,
                        onOpen = null,
                        onAction = { action -> act(request, action) },
                        modifier = sides.padding(bottom = 8.dp).widthIn(max = REQUEST_CARD_MAX),
                        poster = false,
                    )
                }
            }
            val seasons = detail.seasons.orEmpty().sortedWith(compareBy<DiscoverSeason> { it.number == 0 }.thenBy { it.number })
            if (detail.kind == TitleKind.Show && seasons.isNotEmpty()) {
                item(key = "seasons-title", contentType = "section-title") {
                    Box(sides.padding(top = PleyPhone.SectionGap, bottom = 4.dp)) { SectionTitle(stringResource(R.string.requests_seasons_heading)) }
                }
                items(seasons, key = { "season-${it.number}" }, contentType = { "season" }) { season ->
                    SeasonRow(season, detail, page.manage, page.origin, texts, sides)
                }
            }
            if (detail.cast.isNotEmpty()) {
                item(key = "cast", contentType = "cast") { CastRow(detail.cast, page.origin, Modifier.padding(top = PleyPhone.SectionGap)) }
            }
            item(key = "credit", contentType = "credit") { TmdbCredit(sides.padding(top = PleyPhone.SectionGap)) }
        }
        BackdropTopBar(detail.title, list, onBack = navigator::back)
    }
}

/**
 * The top of the page: the backdrop and its fade, the poster with the
 * title, the metadata line and the genres beside it; under them (beside
 * the poster from 600 dp) the main action, a line when the page couldn't
 * be read again, the tagline and the overview.
 */
@Composable
private fun TitleHeader(page: TitleUiState.Ready, texts: RequestTexts, on: TitleActionHandlers, onRetry: () -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val tier = LocalPhoneDeviceTier.current
    val wide = metrics.widthClass != WidthClass.Compact
    val backdropHeight = itemBackdropHeight(wide)
    val detail = page.detail
    val backdrop = remember(detail.backdropUrl, detail.posterUrl, page.origin, tier) {
        (ArtworkImage.of(detail.backdropUrl, page.origin) ?: ArtworkImage.of(detail.posterUrl, page.origin))
            ?.copy(maxWidth = ArtworkImage.backdropCap(tier))
    }
    Box(Modifier.fillMaxWidth().testTag(TITLE_HEADER)) {
        Artwork(
            image = backdrop,
            title = detail.title,
            modifier = Modifier
                .fillMaxWidth()
                .height(backdropHeight)
                .drawWithCache {
                    val fade = Brush.verticalGradient(
                        0f to Color.Transparent,
                        0.45f to PleyColors.Bg.copy(alpha = 0.35f),
                        1f to PleyColors.Bg,
                        startY = size.height * 0.4f,
                        endY = size.height,
                    )
                    onDrawWithContent {
                        drawContent()
                        drawRect(fade)
                    }
                },
        )
        Column(Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd, top = itemTitleTop(backdropHeight))) {
            if (wide) {
                Row(verticalAlignment = Alignment.Top) {
                    Poster(detail, page.origin, WidePosterWidth)
                    Spacer(Modifier.width(24.dp))
                    Column(Modifier.widthIn(max = TextColumnMax)) {
                        TitleLines(detail, texts, large = true)
                        HeaderBody(page, texts, on, onRetry)
                    }
                }
            } else {
                Row(verticalAlignment = Alignment.Top) {
                    Poster(detail, page.origin, PosterWidth)
                    Spacer(Modifier.width(16.dp))
                    Column(Modifier.weight(1f)) { TitleLines(detail, texts, large = false) }
                }
                HeaderBody(page, texts, on, onRetry)
            }
        }
    }
}

@Composable
private fun Poster(detail: DiscoverDetail, origin: String, width: Dp) {
    Artwork(
        image = remember(detail.posterUrl, origin) { ArtworkImage.of(detail.posterUrl, origin) },
        title = detail.title,
        titleStyle = MaterialTheme.typography.labelMedium,
        modifier = Modifier
            .width(width)
            .aspectRatio(2f / 3f)
            .clip(PleyShape.TilePhone),
    )
}

/** The title, and its original one where it differs; the metadata line; the genres. */
@Composable
private fun TitleLines(detail: DiscoverDetail, texts: RequestTexts, large: Boolean) {
    Text(
        detail.title,
        style = if (large) MaterialTheme.typography.displaySmall else MaterialTheme.typography.headlineSmall,
        color = PleyColors.Fg,
        modifier = Modifier.semantics { heading() },
    )
    detail.originalTitle?.takeIf { it.isNotBlank() && !it.equals(detail.title, ignoreCase = true) }?.let { original ->
        Text(original, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = Modifier.padding(top = 2.dp))
    }
    Spacer(Modifier.height(10.dp))
    val seasons = detail.seasons.orEmpty().count { it.number > 0 }
    val seasonsLine = if (detail.kind == TitleKind.Show && seasons > 0) pluralStringResource(R.plurals.requests_seasons, seasons, seasons) else null
    MetaLine(remember(detail, seasonsLine, texts) { metaOf(detail, seasonsLine, texts) })
    if (detail.genres.isNotEmpty()) {
        Spacer(Modifier.height(6.dp))
        Text(detail.genres.joinToString(" · "), style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
    }
}

/**
 * The line under a title (as a library's page has it): its year, its
 * certification boxed, its runtime ("1h 52m") or how many seasons it has,
 * TMDB's rating after a star; without a year, whether it's a movie or a
 * show.
 */
private fun metaOf(detail: DiscoverDetail, seasons: String?, texts: RequestTexts): List<MetaPart> = buildList {
    add(MetaPart.Text(detail.year?.toString() ?: texts.kind(detail.kind)))
    detail.certification?.takeIf { it.isNotBlank() }?.let { add(MetaPart.ContentRating(it)) }
    val length = if (detail.kind == TitleKind.Show) seasons else detail.runtimeMinutes?.takeIf { it > 0 }?.let { formatRuntime(it * 60_000L, texts.words) }
    length?.takeIf { it.isNotEmpty() }?.let { add(MetaPart.Text(it)) }
    detail.rating?.takeIf { it > 0 }?.let { add(MetaPart.Rating(texts.rating(it))) }
}

@Composable
private fun HeaderBody(page: TitleUiState.Ready, texts: RequestTexts, on: TitleActionHandlers, onRetry: () -> Unit) {
    val detail = page.detail
    Spacer(Modifier.height(18.dp))
    TitleActions(page.action, detail.kind, page.allowance, page.manage, page.busy, texts, on)
    // Shown from before: the server didn't answer this time (out of reach, the offline banner says so).
    page.problem?.takeIf { it !is RequestProblem.Offline }?.let { problem ->
        Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(PleyIcons.CircleAlert, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(8.dp))
            Text(texts.problem(problem, page.summary), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, modifier = Modifier.weight(1f))
            if (problem.retry) PleyTextButton(stringResource(R.string.requests_retry), onClick = onRetry)
        }
    }
    val tagline = detail.tagline?.takeIf { it.isNotBlank() }
    if (tagline != null || detail.overview.isNotBlank()) Spacer(Modifier.height(16.dp))
    tagline?.let {
        Text(
            it,
            style = MaterialTheme.typography.bodyLarge.copy(fontStyle = FontStyle.Italic),
            color = PleyColors.Fg.copy(alpha = 0.75f),
            modifier = Modifier.padding(bottom = 6.dp),
        )
    }
    if (detail.overview.isNotBlank()) Summary(detail.overview, detail.tmdbId)
}

/**
 * One season: its poster, its name, and how it stands for this account
 * ("In library", "4 of 10 episodes in library", "Airs Nov 4, 2026",
 * "10 episodes"), or the stage of the request asking for it.
 */
@Composable
private fun SeasonRow(season: DiscoverSeason, detail: DiscoverDetail, manage: Boolean, origin: String, texts: RequestTexts, modifier: Modifier) {
    val request = requestOf(season, detail)
    val name = texts.seasonName(season)
    Row(
        modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp)
            .semantics(mergeDescendants = true) {}
            .testTag(TITLE_SEASON + season.number),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Artwork(
            image = remember(season.posterUrl, origin) { ArtworkImage.of(season.posterUrl, origin) },
            title = name,
            titleStyle = MaterialTheme.typography.labelSmall,
            modifier = Modifier
                .width(SeasonPosterWidth)
                .aspectRatio(2f / 3f)
                .clip(PleyShape.TilePhone),
        )
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(name, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg)
            if (request != null && season.state == SeasonState.Requested) {
                StagePillChip(stagePill(request, manage), texts)
            } else {
                // In a library green with a check, declined red: the rest in muted words.
                val mark = when {
                    season.declined -> PleyIcons.TicketX to PleyColors.DangerText
                    season.state == SeasonState.Library -> PleyIcons.CircleCheck to PleyColors.Success
                    else -> null
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    mark?.let { (icon, tint) ->
                        Icon(icon, contentDescription = null, size = 16.dp, tint = tint)
                        Spacer(Modifier.width(6.dp))
                    }
                    Text(texts.seasonState(season), style = MaterialTheme.typography.bodyMedium, color = mark?.second ?: PleyColors.Muted)
                }
            }
        }
    }
}

/** The cast with their photos, round, as a library's page shows them; TMDB's people have no page here. */
@Composable
private fun CastRow(cast: List<CastMember>, origin: String, modifier: Modifier = Modifier) {
    val metrics = LocalPhoneMetrics.current
    val width = rowCardWidth(TileKind.Person, metrics.contentWidth - metrics.margin * 2)
    val tiles = remember(cast, origin) {
        cast.mapIndexed { i, member ->
            ArtworkTileModel(
                id = i.toLong(),
                title = member.name,
                subtitle = member.character.orEmpty(),
                image = ArtworkImage.of(member.photoUrl, origin),
                description = listOfNotNull(member.name, member.character?.takeIf { it.isNotBlank() }).joinToString(", "),
            )
        }
    }
    Column(modifier.fillMaxWidth().testTag(TITLE_CAST)) {
        Box(Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd)) { SectionTitle(stringResource(R.string.requests_cast)) }
        Spacer(Modifier.height(PleyPhone.HeaderToContent))
        LazyRow(
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd),
            horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
        ) {
            itemsIndexed(tiles, key = { i, _ -> i }, contentType = { _, _ -> TileKind.Person }) { _, tile ->
                Column(Modifier.width(width).semantics(mergeDescendants = true) {}) {
                    ArtworkTile(tile, TileKind.Person, TileStyle.Phone, Modifier.fillMaxWidth().aspectRatio(TileKind.Person.aspectRatio))
                    Spacer(Modifier.height(TileStyle.Phone.captionGap))
                    TileCaption(tile.title, tile.subtitle, TileStyle.Phone)
                }
            }
        }
    }
}

/**
 * The season picker ("Which seasons?"): every season,
 * specials last, those that may be asked for ticked lowest first up to
 * what's left; the rest show their state and can't be ticked. Ticking
 * more than may be asked for disables Request and shows the allowance in
 * the danger colour.
 */
@Composable
private fun SeasonPickerSheet(
    picker: SeasonPicker,
    allowance: Allowance,
    busy: Boolean,
    texts: RequestTexts,
    onToggle: (Int) -> Unit,
    onRequest: () -> Unit,
    onDismiss: () -> Unit,
) {
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.requests_picker_title), modifier = Modifier.testTag(TITLE_PICKER)) {
        Column(
            Modifier
                .weight(1f, fill = false)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp),
        ) {
            picker.seasons.forEach { row ->
                CheckRow(
                    label = texts.seasonName(row.season),
                    hint = texts.seasonState(row.season),
                    checked = row.season.number in picker.picked,
                    enabled = row.askable,
                    modifier = Modifier.testTag(TITLE_PICKER_SEASON + row.season.number),
                    onToggle = { onToggle(row.season.number) },
                )
            }
        }
        Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            val lines = texts.allowance(allowance, TitleKind.Show)
            if (lines.isNotEmpty()) {
                Column {
                    lines.forEach {
                        Text(it, style = MaterialTheme.typography.bodySmall, color = if (picker.overLimit) PleyColors.DangerText else PleyColors.Muted)
                    }
                }
            }
            PrimaryButton(
                texts.pickerButton(picker),
                onClick = onRequest,
                enabled = picker.ready && !busy,
                modifier = Modifier.fillMaxWidth().testTag(TITLE_PICKER_REQUEST),
            )
        }
    }
}

/** The poster beside the title on phones. */
private val PosterWidth = 104.dp

private val SeasonPosterWidth = 48.dp

internal const val TITLE_PAGE = "pley.discover.title"
internal const val TITLE_HEADER = "pley.discover.title.header"
internal const val TITLE_SEASON = "pley.discover.title.season."
internal const val TITLE_CAST = "pley.discover.title.cast"
internal const val TITLE_PICKER = "pley.discover.title.picker"
internal const val TITLE_PICKER_SEASON = "pley.discover.title.picker.season."
internal const val TITLE_PICKER_REQUEST = "pley.discover.title.picker.request"
