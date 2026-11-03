package app.pley.android.ui.phone.item

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.model.ItemKind
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.MetaPart
import app.pley.android.screens.item.PrimaryAction
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.cast.CastTitle
import app.pley.android.ui.phone.kit.ActionButton
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.StarRating
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass

/**
 * The top of an item page (design spec §4.3, mockups phone-movie and
 * phone-show): the backdrop fading into the page, the breadcrumb and title
 * over its fade, the metadata line, what is left of a title being watched,
 * genres and collections, Play or Resume across the width, the actions
 * (From start, Watched, Playlist, Trailer), what requests say of it
 * ([requests]: its request's stage, Request more seasons), the tagline and
 * a four-line summary, and the profile's rating. From 600 dp the poster
 * stands beside it.
 */
@Composable
internal fun ItemHeader(page: ItemPage, actions: PageActions, requests: @Composable () -> Unit = {}) {
    val metrics = LocalPhoneMetrics.current
    val tier = LocalPhoneDeviceTier.current
    val wide = metrics.widthClass != WidthClass.Compact
    val backdropHeight = itemBackdropHeight(wide)
    val backdrop = remember(page.item, page.origin, tier) { ArtworkCandidates.backdrop(page.item, page.origin, tier) }
    Box(Modifier.fillMaxWidth()) {
        Artwork(
            image = backdrop,
            title = page.context?.showTitle ?: page.title,
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
        Column(Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd, top = backdropHeight - TitleOverlap)) {
            if (wide) {
                Row(verticalAlignment = Alignment.Top) {
                    val poster = remember(page.item, page.origin) { ArtworkCandidates.poster(page.item, page.origin) }
                    Artwork(
                        image = poster,
                        title = page.title,
                        titleStyle = MaterialTheme.typography.titleSmall,
                        modifier = Modifier
                            .width(WidePosterWidth)
                            .aspectRatio(2f / 3f)
                            .clip(PleyShape.TilePhone),
                    )
                    Spacer(Modifier.width(24.dp))
                    Column(Modifier.widthIn(max = TextColumnMax)) { HeaderText(page, actions, fullWidthPlay = false, requests) }
                }
            } else {
                HeaderText(page, actions, fullWidthPlay = true, requests)
            }
        }
    }
}

/**
 * The backdrop's height: 300 dp, 380 from 600 dp wide, but never more than
 * [BACKDROP_MAX_SHARE] of the window's height, so a phone on its side
 * shows Play without scrolling.
 */
@Composable
internal fun itemBackdropHeight(wide: Boolean): Dp {
    val spec = if (wide) WideBackdropHeight else BackdropHeight
    val window = with(LocalDensity.current) { LocalWindowInfo.current.containerSize.height.toDp() }
    return if (window > 0.dp) minOf(spec, window * BACKDROP_MAX_SHARE) else spec
}

/** Where the header's text starts on a page whose backdrop is [backdropHeight] tall. */
internal fun itemTitleTop(backdropHeight: Dp): Dp = backdropHeight - TitleOverlap

private const val BACKDROP_MAX_SHARE = 0.62f

@Composable
private fun HeaderText(page: ItemPage, actions: PageActions, fullWidthPlay: Boolean, requests: @Composable () -> Unit) {
    Breadcrumb(page, actions)
    Text(
        page.title,
        style = MaterialTheme.typography.displaySmall,
        color = PleyColors.Fg,
        modifier = Modifier.semantics { heading() },
    )
    Spacer(Modifier.height(10.dp))
    MetaLine(page.meta)
    // How much is left of a title being watched, as the TV shows it: words, no bar.
    val primary = page.primary
    if (primary.kind == PrimaryAction.Kind.Resume && primary.timeLeft.isNotEmpty()) {
        Spacer(Modifier.height(8.dp))
        Text(primary.timeLeft, style = MaterialTheme.typography.labelLarge, color = PleyColors.Fg)
    }
    if (page.genres.isNotEmpty()) {
        Spacer(Modifier.height(6.dp))
        Text(page.genres.joinToString(" · "), style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
    }
    page.partOf.forEach { collection ->
        Text(
            stringResource(R.string.item_part_of, collection.title),
            style = MaterialTheme.typography.labelMedium,
            color = PleyColors.AccentBright,
            modifier = Modifier
                .padding(top = 4.dp)
                .clickable(role = Role.Button) { actions.openCollection(collection.id) }
                .padding(vertical = 4.dp),
        )
    }
    Spacer(Modifier.height(18.dp))
    PlayBlock(page, actions, fullWidthPlay)
    Spacer(Modifier.height(12.dp))
    ActionRow(page, actions)
    requests()
    if (page.tagline != null || page.summary != null) Spacer(Modifier.height(12.dp))
    page.tagline?.let { tagline ->
        Text(
            tagline,
            style = MaterialTheme.typography.bodyLarge.copy(fontStyle = FontStyle.Italic),
            color = PleyColors.Fg.copy(alpha = 0.75f),
            modifier = Modifier.padding(bottom = 6.dp),
        )
    }
    page.summary?.let { Summary(it, page.id) }
    if (page.rateable) {
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.item_your_rating), style = MaterialTheme.typography.labelLarge, color = PleyColors.Muted)
            Spacer(Modifier.width(8.dp))
            StarRating(value = page.userRating, onChange = actions::rate, starSize = 22.dp)
        }
    }
    Spacer(Modifier.height(8.dp))
}

/** An episode's show and season, a season's show: links above the title. */
@Composable
private fun Breadcrumb(page: ItemPage, actions: PageActions) {
    val context = page.context ?: return
    val show = context.showTitle
    if (show != null) {
        Text(
            show,
            style = MaterialTheme.typography.titleMedium,
            color = PleyColors.Fg.copy(alpha = 0.9f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .clickable(enabled = context.showId != null, role = Role.Button) { context.showId?.let(actions::openItem) }
                .padding(vertical = 2.dp),
        )
    }
    if (page.kind == ItemKind.Episode) {
        val line = context.episodeLongLabel ?: context.seasonTitle
        if (line != null) {
            Text(
                line,
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier
                    .clickable(enabled = context.seasonId != null, role = Role.Button) { context.seasonId?.let(actions::openItem) }
                    .padding(vertical = 2.dp),
            )
        }
    }
    Spacer(Modifier.height(2.dp))
}

/**
 * The metadata line (design spec §2.6): "2021 · PG-13 · 2h 31m · ★ 8.1 ·
 * 4K DV P8.1 Atmos", "·" in [subtle][PleyColors.Subtle]; it wraps.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun MetaLine(parts: List<MetaPart>, modifier: Modifier = Modifier) {
    if (parts.isEmpty()) return
    FlowRow(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
        itemVerticalAlignment = Alignment.CenterVertically,
    ) {
        parts.forEachIndexed { i, part ->
            if (i > 0) Text("·", style = MaterialTheme.typography.labelMedium, color = PleyColors.Subtle)
            when (part) {
                is MetaPart.Text -> Text(part.text, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
                is MetaPart.ContentRating -> QualityBadge(part.rating, TileStyle.Phone)
                is MetaPart.Rating -> Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(PleyIcons.Star, contentDescription = null, size = 14.dp, tint = PleyColors.Accent)
                    Spacer(Modifier.width(4.dp))
                    Text(part.rating, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg)
                }
                is MetaPart.Badges -> Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    part.badges.forEach { QualityBadge(it, TileStyle.Phone) }
                }
            }
        }
    }
}

/**
 * Play or Resume across the width, or "Unavailable", and Download under
 * it for a movie or an episode; what is left of a title being watched is
 * under the metadata.
 */
@Composable
private fun PlayBlock(page: ItemPage, actions: PageActions, fullWidth: Boolean) {
    val primary = page.primary
    val width = if (fullWidth) Modifier.fillMaxWidth() else Modifier.widthIn(min = 220.dp)
    if (primary.kind == PrimaryAction.Kind.Unavailable) {
        TonalButton(stringResource(R.string.item_unavailable), onClick = {}, enabled = false, modifier = width.height(48.dp))
        return
    }
    Row(if (fullWidth) Modifier.fillMaxWidth() else Modifier, verticalAlignment = Alignment.CenterVertically) {
        PrimaryButton(
            text = primaryLabel(primary),
            onClick = { actions.play(fromStart = false) },
            icon = PleyIcons.Play,
            modifier = (if (fullWidth) Modifier.weight(1f) else Modifier.widthIn(min = 220.dp)).testTag(TestTags.ITEM_PLAY),
        )
        // Movies, episodes and extras cast to another device too, from where the profile left off.
        if (page.kind == ItemKind.Movie || page.kind == ItemKind.Episode || page.kind == ItemKind.Extra) {
            val tracks = page.tracks
            CastTitle(page.id, tracks?.versionId?.takeIf { tracks.versions.size > 1 })
        }
    }
    if (page.kind == ItemKind.Movie || page.kind == ItemKind.Episode) {
        Spacer(Modifier.height(8.dp))
        DownloadButton(page.id, page.title, width)
    }
}

/** "Resume", "Play S1 · E3", … */
@Composable
internal fun primaryLabel(primary: PrimaryAction): String {
    val episode = primary.episode
    return when {
        primary.kind == PrimaryAction.Kind.Resume && episode != null -> stringResource(R.string.item_resume_episode, episode)
        primary.kind == PrimaryAction.Kind.Resume -> stringResource(R.string.item_resume)
        episode != null -> stringResource(R.string.item_play_episode, episode)
        else -> stringResource(R.string.item_play)
    }
}

/**
 * The actions under Play (design spec §3.3): From start, Watched, My List,
 * Playlist and Trailer, sharing the width (a quarter each at least), as in
 * the mockup.
 */
@Composable
private fun ActionRow(page: ItemPage, actions: PageActions) {
    val watchedLabel = stringResource(if (page.watched) R.string.item_mark_unwatched else R.string.item_mark_watched)
    val listLabel = stringResource(if (page.myList == true) R.string.item_in_my_list else R.string.item_not_in_my_list)
    val items = buildList {
        if (page.fromStart) add(HeaderAction(PleyIcons.RotateCcw, stringResource(R.string.item_from_start)) { actions.play(fromStart = true) })
        add(
            HeaderAction(
                if (page.watched) PleyIcons.CircleCheck else PleyIcons.Check,
                stringResource(R.string.item_watched),
                selected = page.watched,
                state = watchedLabel,
            ) { actions.toggleWatched() },
        )
        page.myList?.let { inList ->
            add(
                HeaderAction(
                    if (inList) PleyIcons.BookmarkCheck else PleyIcons.BookmarkPlus,
                    stringResource(R.string.item_my_list),
                    selected = inList,
                    state = listLabel,
                    tag = ITEM_MY_LIST,
                ) { actions.toggleMyList() },
            )
        }
        add(HeaderAction(PleyIcons.ListPlus, stringResource(R.string.item_playlist)) { actions.addToPlaylist(page.id) })
        page.trailer?.let { trailer -> add(HeaderAction(PleyIcons.Film, stringResource(R.string.item_trailer)) { actions.play(trailer) }) }
    }
    Row(Modifier.fillMaxWidth()) {
        items.forEach { action ->
            Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                ActionButton(
                    icon = action.icon,
                    label = action.label,
                    onClick = action.onClick,
                    selected = action.selected,
                    // The whole slot: a large font's label has the room before it shrinks.
                    modifier = Modifier
                        .fillMaxWidth()
                        .then(action.state?.let { s -> Modifier.semantics { stateDescription = s } } ?: Modifier)
                        .then(action.tag?.let { Modifier.testTag(it) } ?: Modifier),
                )
            }
        }
        repeat((ACTION_SLOTS - items.size).coerceAtLeast(0)) { Spacer(Modifier.weight(1f)) }
    }
}

/** A button of the action row: [selected] tints it, [state] is what screen readers hear it is. */
private class HeaderAction(
    val icon: PleyIcon,
    val label: String,
    val selected: Boolean = false,
    val state: String? = null,
    val tag: String? = null,
    val onClick: () -> Unit,
)

/** The summary in four lines, "More" to read it all (Summary of shared.tsx). */
@Composable
internal fun Summary(text: String, key: Long) {
    var expanded by rememberSaveable(key) { mutableStateOf(false) }
    var overflows by remember(text) { mutableStateOf(false) }
    Text(
        text,
        style = MaterialTheme.typography.bodyLarge,
        color = PleyColors.Fg.copy(alpha = 0.85f),
        maxLines = if (expanded) Int.MAX_VALUE else SUMMARY_LINES,
        overflow = TextOverflow.Ellipsis,
        onTextLayout = { if (!expanded) overflows = it.hasVisualOverflow || (it.lineCount > 0 && it.isLineEllipsized(it.lineCount - 1)) },
    )
    if (overflows || expanded) {
        PleyTextButton(
            stringResource(if (expanded) R.string.item_read_less else R.string.item_read_more),
            onClick = { expanded = !expanded },
            modifier = Modifier.padding(start = 0.dp),
        )
    }
}

/** The summary's lines before "More" (design spec §4.3). */
const val SUMMARY_LINES = 4

private const val ACTION_SLOTS = 4

internal const val ITEM_MY_LIST = "pley.item.myList"

/** The backdrop from 600 dp. */
private val WideBackdropHeight = 380.dp

/** How far the title block reaches up into the backdrop's fade. */
private val TitleOverlap = 104.dp

/** The poster beside the header from 600 dp ("poster from 600 dp"). */
internal val WidePosterWidth = 180.dp

/** The widest the header's text gets on tablets. */
internal val TextColumnMax = 720.dp
