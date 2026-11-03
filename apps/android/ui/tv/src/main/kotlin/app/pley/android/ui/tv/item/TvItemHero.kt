package app.pley.android.ui.tv.item

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
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.PrimaryAction
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.kit.tvRowColumnHero

/**
 * The top of the TV item page (design spec §4.3, mockup tv-movie), in the
 * 464 dp text column at x = 96: the title (the show's, for an episode or
 * season) bottom-aligned in the logo's 72 dp box, an episode's context
 * line, the metadata line, genres, three lines of summary (More shows it
 * all when it runs longer), the credits, then the buttons: Resume or
 * Play, From start, Watched, Audio and subtitles, My List, Add to
 * playlist, More.
 * Focus starts on Resume or Play; holding Select on it opens the item's
 * actions. Down from the buttons goes to [down] (the episode to watch).
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
) {
    val places = remember { HeroPlaces() }
    Column(
        Modifier
            .fillMaxWidth()
            .onPlaced { places.hero = it }
            .onFocusChanged { if (it.hasFocus) focus.area = Area.Hero }
            .tvRowColumnHero()
            .padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = HeroTop, bottom = 8.dp),
    ) {
        Box(Modifier.width(TextColumn).heightIn(min = TitleBox), contentAlignment = Alignment.BottomStart) {
            BasicText(heroTitle(page), style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        contextLine(page)?.let {
            Spacer(Modifier.height(8.dp))
            BasicText(it, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
        }
        Spacer(Modifier.height(12.dp))
        TvMetaLine(page.meta)
        // How far a resumed title was played: "1h 33m left", as on Home.
        page.primary.timeLeft.takeIf { page.primary.kind == PrimaryAction.Kind.Resume && it.isNotEmpty() }?.let { left ->
            Spacer(Modifier.height(8.dp))
            BasicText(left, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 1)
        }
        val genres =(page.genres + page.partOf.map { stringResource(R.string.tv_item_part_of, it.title) }).joinToString(" · ")
        if (genres.isNotEmpty()) {
            Spacer(Modifier.height(6.dp))
            BasicText(genres, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
        }
        page.summary?.let { summary ->
            Spacer(Modifier.height(10.dp))
            HeroSummary(summary, buttons)
        }
        creditsLine(page)?.let { credits ->
            Spacer(Modifier.height(8.dp))
            BasicText(credits, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(TextColumn))
        }
        Spacer(Modifier.height(22.dp))
        HeroButtonsRow(page, memory, buttons, down, actions, places, focus, onTracks, onMore, onPrimaryActions)
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
private fun HeroSummary(summary: String, buttons: HeroButtons) {
    BasicText(
        summary,
        style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg.copy(alpha = 0.85f)),
        maxLines = SUMMARY_LINES,
        overflow = TextOverflow.Ellipsis,
        onTextLayout = { buttons.summaryCut = it.hasVisualOverflow },
        modifier = Modifier.width(TextColumn),
    )
}

/** "Directed by Lena Okafor · Starring Idris Vale, Maren Holt": labels in [muted][PleyColors.Muted]. */
@Composable
private fun creditsLine(page: ItemPage): androidx.compose.ui.text.AnnotatedString? {
    val makers = page.details.crew.firstOrNull { it.role == CreditRole.Director }
        ?: page.details.crew.firstOrNull { it.role == CreditRole.Creator }
    val stars = page.cast.take(STARRING)
    if (makers == null && stars.isEmpty()) return null
    val made = makers?.let { stringResource(if (it.role == CreditRole.Director) R.string.tv_item_directed_by else R.string.tv_item_created_by) }
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

/** The hero's top: the logo box at y = 52. */
private val HeroTop = 52.dp

/** The logo art's box, 320 by 72; the title sits at its bottom. */
private val TitleBox = 72.dp

/** The text column, x 96 to 560. */
internal val TextColumn = PleyTv.TextColumnEnd - PleyTv.Gutter

private const val SUMMARY_LINES = 3

/** Names after "Starring". */
private const val STARRING = 3

internal const val TV_TRACKS_BUTTON = "pley.tv.item.tracks"

internal const val TV_MY_LIST_BUTTON = "pley.tv.item.myList"
