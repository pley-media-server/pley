package app.pley.android.ui.tv.item

import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.PrimaryAction
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.ItemRequestState
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.kit.tvRowColumnHero
import app.pley.android.ui.tv.requests.TvItemRequestLine

/**
 * The top of the TV item page (design spec §4.3, mockup tv-movie), in the
 * 464 dp text column at x = 96: the title (the show's, for an episode or
 * season) from y = 40, an episode's context
 * line, the metadata line (and under it a movie's or show's [request],
 * its stage), genres, three lines of summary (More shows it
 * all when it runs longer), the credits, then the buttons: Resume or
 * Play, From start, Request more seasons (a show with seasons to ask
 * for), Watched, Audio and subtitles, My List, Add to playlist, More.
 * Focus starts on Resume or Play; holding Select on it opens the item's
 * actions. Down from the buttons goes to [down] (the episode to watch, or a show's season posters).
 * The title and the summary have more room than the column
 * ([HeroColumn], [SummaryColumn]); a long title has up to three lines, growing down,
 * before it is cut. What follows the buttons shows whole above the
 * screen's bottom edge ([heroFit]): a show's season posters with their
 * names, the episodes, or the first row with its names. For that the
 * room under the buttons closes up first, then the summary gives up
 * lines, down to one, and the credits go before its second line does.
 * Where the row that follows is Cast & Crew, the credits line is left
 * to it, which names the same people.
 */
@Composable
internal fun TvHero(
    page: ItemPage,
    focus: ItemPageFocus,
    memory: FocusMemory,
    buttons: HeroButtons,
    down: FocusRequester?,
    actions: TvPageActions,
    onTracks: () -> Unit,
    onMore: () -> Unit,
    onPrimaryActions: () -> Unit,
    request: ItemRequestState? = null,
) {
    val places = remember { HeroPlaces() }
    val genres = (page.genres + page.partOf.map { stringResource(R.string.tv_item_part_of, it.title) }).joinToString(" · ")
    // Who made it and who is in it: named by the Cast & Crew row when that is what follows, not twice.
    val credits = if (castFollows(page)) null else creditsLine(page)
    val fit = heroFit(page, request, genres.isNotEmpty(), credits != null)
    Column(
        Modifier
            .fillMaxWidth()
            .shorterBy(fit.closer)
            .onPlaced { places.hero = it }
            .onFocusChanged { if (it.hasFocus) focus.area = Area.Hero }
            .tvRowColumnHero()
            .focusGroup()
            .padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = HeroTop, bottom = HeroBottom),
    ) {
        Box(Modifier.widthIn(max = HeroColumn).heightIn(min = TitleBox), contentAlignment = Alignment.BottomStart) {
            BasicText(heroTitle(page), style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg), maxLines = TITLE_LINES, overflow = TextOverflow.Ellipsis)
        }
        contextLine(page)?.let {
            Spacer(Modifier.height(8.dp))
            BasicText(it, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
        }
        Spacer(Modifier.height(12.dp))
        TvMetaLine(page.meta)
        request?.let { state ->
            state.request?.let { asked ->
                Spacer(Modifier.height(10.dp))
                TvItemRequestLine(asked, state.manage)
            }
        }
        // How far a resumed title was played: "1h 33m left", as on Home.
        page.primary.timeLeft.takeIf { page.primary.kind == PrimaryAction.Kind.Resume && it.isNotEmpty() }?.let { left ->
            Spacer(Modifier.height(8.dp))
            BasicText(left, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 1)
        }
        if (genres.isNotEmpty()) {
            Spacer(Modifier.height(6.dp))
            BasicText(genres, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
        }
        page.summary?.let { summary ->
            Spacer(Modifier.height(SummaryGap))
            HeroSummary(summary, buttons, fit.lines.coerceAtLeast(1))
        }
        credits?.takeIf { fit.credits }?.let {
            Spacer(Modifier.height(8.dp))
            BasicText(it, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
        }
        Spacer(Modifier.height(ButtonsGap))
        HeroButtonsRow(page, memory, buttons, down, actions, places, focus, onTracks, onMore, onPrimaryActions, request)
    }
}

/** Where the hero and its focusable parts were placed, for scrolling back to them. */
private class HeroPlaces {
    var hero: LayoutCoordinates? = null

    /** [part]'s top inside the hero, in px. */
    fun topOf(part: LayoutCoordinates): Float {
        val root = hero ?: return 0f
        return if (root.isAttached && part.isAttached) root.localPositionOf(part, Offset.Zero).y else 0f
    }
}

/**
 * The summary, plain text that never takes focus. When it runs past its
 * lines, More offers it all ([HeroButtons.summaryCut]).
 */
@Composable
private fun HeroSummary(summary: String, buttons: HeroButtons, lines: Int) {
    BasicText(
        summary,
        style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg.copy(alpha = 0.85f)),
        maxLines = lines,
        overflow = TextOverflow.Ellipsis,
        onTextLayout = { buttons.summaryCut = it.hasVisualOverflow },
        modifier = Modifier.widthIn(max = SummaryColumn),
    )
}

/**
 * How the hero makes room for what follows its buttons: the summary's
 * [lines], whether the [credits] line shows, and how much [closer] what
 * follows comes to the buttons.
 */
internal class HeroFit(val lines: Int, val credits: Boolean, val closer: Dp)

/**
 * The hero's fit: the summary's three lines and the credits where what
 * follows the buttons still shows whole above the screen's bottom edge
 * ([followsHeight]). Where it wouldn't, the room under the buttons closes
 * up first, by [CloserMost] at the most; then the summary gives up its
 * third line; then the credits go, before its second; one line stays.
 * Worked out from what the hero shows over and under the summary (the
 * title's lines and the summary's are measured), so the page is laid out
 * once.
 */
@Composable
private fun heroFit(page: ItemPage, request: ItemRequestState?, genres: Boolean, credits: Boolean): HeroFit {
    val follows = followsHeight(page) ?: return HeroFit(SUMMARY_LINES, credits, 0.dp)
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val title = heroTitle(page)
    val titleLines = remember(title, measurer, density) {
        val width = with(density) { HeroColumn.roundToPx() }
        measurer.measure(title, PleyType.Tv.HeroTitle, maxLines = TITLE_LINES, constraints = Constraints(maxWidth = width)).lineCount
    }
    // The lines the summary has to show: a short one asks for less room.
    val summary = page.summary
    val wanted = remember(summary, measurer, density) {
        val width = with(density) { SummaryColumn.roundToPx() }
        if (summary == null) 0 else measurer.measure(summary, PleyType.Tv.BodyLarge, maxLines = SUMMARY_LINES, constraints = Constraints(maxWidth = width)).lineCount
    }
    val resumed = page.primary.kind == PrimaryAction.Kind.Resume && page.primary.timeLeft.isNotEmpty()
    val over = HeroTop + maxOf(TitleBox, TitleLine * titleLines) +
        (if (contextLine(page) != null) ContextRoom else 0.dp) + MetaRoom +
        (if (request?.request != null) RequestRoom else 0.dp) +
        (if (resumed) CaptionRoom else 0.dp) +
        (if (genres) CaptionRoom else 0.dp) +
        (if (summary != null) SummaryGap else 0.dp)
    val under = ButtonsGap + PleyTv.ButtonHeight + PleyTv.ButtonLabelRoom + HeroBottom + PleyTv.RowGap + follows
    return fitIn(PleyTv.CanvasHeight - over - under, wanted, credits)
}

/**
 * The fit ([HeroFit]) of a summary of [wanted] lines and the [credits]
 * line in [room], which closing up under the buttons makes larger.
 */
internal fun fitIn(room: Dp, wanted: Int, credits: Boolean): HeroFit {
    fun needs(lines: Int, withCredits: Boolean) = SummaryLine * lines + if (withCredits) CaptionRoom else 0.dp
    fun fit(lines: Int, withCredits: Boolean): HeroFit? {
        val closer = (needs(lines, withCredits) - room).coerceAtLeast(0.dp)
        return if (closer <= CloserMost) HeroFit(lines, withCredits, closer) else null
    }
    for (lines in wanted downTo 1) {
        // The credits stay while the summary has two lines with them.
        if (credits && (lines > 1 || wanted == 1)) fit(lines, true)?.let { return it }
        if (!credits || lines <= 2) fit(lines, false)?.let { return it }
    }
    // Nothing fits (a long title, a request, a time left): what follows is as near as it comes.
    if (wanted == 0 && credits) fit(0, true)?.let { return it }
    val least = wanted.coerceAtMost(1)
    return HeroFit(least, false, (needs(least, false) - room).coerceIn(0.dp, CloserMost))
}

/**
 * Takes [by] off the height it reports: what follows comes that much
 * nearer, into the empty room under the hero's buttons.
 */
private fun Modifier.shorterBy(by: Dp): Modifier =
    if (by <= 0.dp) {
        this
    } else {
        layout { measurable, constraints ->
            val placeable = measurable.measure(constraints)
            layout(placeable.width, (placeable.height - by.roundToPx()).coerceAtLeast(0)) { placeable.place(0, 0) }
        }
    }

/**
 * How much of what follows the buttons is to show above the screen's
 * bottom edge, with the page at rest: a show's season posters with their
 * names and episode counts; the episodes with their captions, under their
 * tabs or their season's name; else the first row (Extras, Cast & Crew)
 * with its header and its captions. `null` when nothing follows.
 */
private fun followsHeight(page: ItemPage): Dp? {
    val seasons = page.seasons
    val header = PleyTv.RowHeaderHeight + PleyTv.HeaderToTiles
    val captions = PleyTv.TilesToCaption + PleyTv.CaptionHeight
    return when {
        seasonPosterRow(page) -> TileKind.Poster.tvHeight + captions
        seasons != null -> (if (seasons.seasons.size > 1 && !seasons.posters) TabHeight + TabsToTiles else header) + TileKind.Episode.tvHeight + captions
        page.extras.isNotEmpty() -> header + TileKind.Landscape.tvHeight + captions
        castFollows(page) -> header + TileKind.Person.tvHeight + captions
        else -> null
    }
}

/** Whether Cast & Crew is the row right under the buttons: a movie's page without extras. */
private fun castFollows(page: ItemPage): Boolean =
    page.seasons == null && page.extras.isEmpty() && (page.cast.isNotEmpty() || page.details.crew.isNotEmpty())

/** "Directed by Lena Okafor · Starring Idris Vale, Maren Holt": labels in [muted][PleyColors.Muted]. */
@Composable
private fun creditsLine(page: ItemPage): androidx.compose.ui.text.AnnotatedString? {
    val makers = page.details.crew.firstOrNull { it.role == CreditRole.Director }
        ?: page.details.crew.firstOrNull { it.role == CreditRole.Creator }
    val stars = page.cast.take(STARRING)
    if (makers == null && stars.isEmpty()) return null
    val made = makers?.let { stringResource(if (it.role == CreditRole.Director) R.string.tv_item_directed_by else R.string.tv_item_credits_created_by) }
    val starring = stringResource(R.string.tv_item_starring)
    return buildAnnotatedString {
        if (makers != null && made != null) {
            withStyle(SpanStyle(color = PleyColors.Muted)) { append("$made ") }
            append(makers.people.take(2).joinToString(", ") { it.name })
        }
        if (stars.isNotEmpty()) {
            if (makers != null) withStyle(SpanStyle(color = PleyColors.Subtle)) { append(" · ") }
            withStyle(SpanStyle(color = PleyColors.Muted)) { append("$starring ") }
            append(stars.joinToString(", ") { it.name })
        }
    }
}

/** An episode's "S2 · E5 · Glasshouse"; a season's own title under its show's. */
private fun contextLine(page: ItemPage): String? = when (page.kind) {
    ItemKind.Episode -> listOfNotNull(page.context?.episodeLabel, page.title).joinToString(" · ")
    ItemKind.Season -> page.title.takeIf { page.context?.showTitle != null }
    else -> null
}

@Composable
private fun HeroButtonsRow(
    page: ItemPage,
    memory: FocusMemory,
    buttons: HeroButtons,
    down: FocusRequester?,
    actions: TvPageActions,
    places: HeroPlaces,
    focus: ItemPageFocus,
    onTracks: () -> Unit,
    onMore: () -> Unit,
    onPrimaryActions: () -> Unit,
    request: ItemRequestState?,
) {
    val primary = page.primary
    val unavailable = primary.kind == PrimaryAction.Kind.Unavailable
    var placed by remember { mutableStateOf<LayoutCoordinates?>(null) }
    Row(
        Modifier
            .onPlaced { placed = it }
            .onFocusChanged { if (it.hasFocus) placed?.let { p -> focus.heroElementY = places.topOf(p) } }
            .focusProperties { if (down != null) this.down = down },
        horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap),
        verticalAlignment = Alignment.Top,
    ) {
        TvPillButton(
            label = if (unavailable) stringResource(R.string.tv_item_unavailable) else primaryLabel(primary),
            onClick = { actions.model.play(fromStart = false) },
            icon = if (unavailable) null else PleyIcons.Play,
            enabled = !unavailable,
            onLongClick = onPrimaryActions,
            modifier = Modifier
                .testTag(TestTags.ITEM_PLAY)
                .rememberedFocus(memory, "play", initial = !unavailable)
                .focusRequester(buttons.play),
        )
        if (page.fromStart) {
            TvRoundButton(
                label = stringResource(R.string.tv_item_from_start),
                icon = PleyIcons.RotateCcw,
                onClick = { actions.model.play(fromStart = true) },
                modifier = Modifier.rememberedFocus(memory, "fromStart"),
            )
        }
        // The title's Discover page, its season picker open.
        if (request != null && request.more) {
            TvRoundButton(
                label = stringResource(ScreensR.string.requests_more_seasons),
                icon = PleyIcons.Plus,
                onClick = { actions.open(request.picker) },
                modifier = Modifier.rememberedFocus(memory, "requestMore"),
            )
        }
        TvRoundButton(
            label = stringResource(if (page.watched) R.string.tv_item_mark_unwatched else R.string.tv_item_mark_watched),
            icon = if (page.watched) PleyIcons.EyeOff else PleyIcons.CircleCheck,
            onClick = actions.model::toggleWatched,
            modifier = Modifier.rememberedFocus(memory, "watched", initial = unavailable),
        )
        if (page.tracks != null) {
            TvRoundButton(
                label = stringResource(R.string.tv_item_tracks),
                icon = PleyIcons.Captions,
                onClick = onTracks,
                modifier = Modifier.rememberedFocus(memory, "tracks").focusRequester(buttons.tracks).testTag(TV_TRACKS_BUTTON),
            )
        }
        page.myList?.let { inList ->
            TvRoundButton(
                label = stringResource(if (inList) R.string.tv_item_in_my_list else R.string.tv_item_add_to_my_list),
                icon = if (inList) PleyIcons.BookmarkCheck else PleyIcons.BookmarkPlus,
                onClick = actions.model::toggleMyList,
                modifier = Modifier.rememberedFocus(memory, "myList").testTag(TV_MY_LIST_BUTTON),
            )
        }
        page.trailer?.let { trailer ->
            TvRoundButton(
                label = stringResource(R.string.tv_item_trailer),
                icon = PleyIcons.Film,
                onClick = { actions.model.play(trailer, fromStart = true) },
                modifier = Modifier.rememberedFocus(memory, "trailer"),
            )
        }
        TvRoundButton(
            label = stringResource(R.string.tv_item_add_to_playlist),
            icon = PleyIcons.ListPlus,
            onClick = { actions.open(PleyKey.AddToPlaylist(listOf(page.id))) },
            modifier = Modifier.rememberedFocus(memory, "playlist"),
        )
        TvRoundButton(
            label = stringResource(R.string.tv_item_more),
            icon = PleyIcons.Ellipsis,
            onClick = onMore,
            modifier = Modifier.rememberedFocus(memory, "more").focusRequester(buttons.more),
        )
    }
}

/** "Resume", "Play S1 · E3", … */
@Composable
internal fun primaryLabel(primary: PrimaryAction): String {
    val episode = primary.episode
    return when {
        primary.kind == PrimaryAction.Kind.Resume && episode != null -> stringResource(R.string.tv_item_resume_episode, episode)
        primary.kind == PrimaryAction.Kind.Resume -> stringResource(R.string.tv_item_resume)
        episode != null -> stringResource(R.string.tv_item_play_episode, episode)
        else -> stringResource(R.string.tv_item_play)
    }
}

/**
 * The hero's top, the title's: high on the page, so that what follows the
 * buttons (a show's season posters, the episodes) shows above the
 * screen's bottom edge.
 */
private val HeroTop = 40.dp

/** The title's box at the least: a title of one line. */
private val TitleBox = 44.dp

/** A line of the title, of the summary, and of the captions (time left, genres, credits) with the gap over each. */
private val TitleLine = 44.dp
private val SummaryLine = 22.dp
private val CaptionRoom = 25.dp

/** The line under the title of an episode's or a season's page, the metadata line and a request's line, each with the gap over it. */
private val ContextRoom = 30.dp
private val MetaRoom = 32.dp
private val RequestRoom = 30.dp

/** Over the summary, over the buttons, and under the hero. */
private val SummaryGap = 10.dp
private val ButtonsGap = 22.dp
private val HeroBottom = 8.dp

/**
 * The most what follows comes nearer the buttons ([HeroFit.closer]): the
 * room for a round button's label and 8 dp stay between them.
 */
private val CloserMost = PleyTv.RowGap

/** The text column, x 96 to 560. */
internal val TextColumn = PleyTv.TextColumnEnd - PleyTv.Gutter

/**
 * The room the title has, x 96 to 736, over the backdrop's darkened side:
 * wider than the lines under it, so a long title is seldom cut.
 */
internal val HeroColumn = 640.dp

/** The room the summary has, x 96 to 656: wider than the text column, so its three lines say more, short of the title's. */
internal val SummaryColumn = 560.dp

private const val TITLE_LINES = 3

private const val SUMMARY_LINES = 3

/** Names after "Starring". */
private const val STARRING = 3

internal const val TV_TRACKS_BUTTON = "pley.tv.item.tracks"

internal const val TV_MY_LIST_BUTTON = "pley.tv.item.myList"
