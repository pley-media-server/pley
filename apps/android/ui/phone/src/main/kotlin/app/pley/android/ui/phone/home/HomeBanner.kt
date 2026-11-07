package app.pley.android.ui.phone.home

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.collectIsDraggedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.layout.layout
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.home.Featured
import app.pley.android.screens.home.HomeInfo
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * Home's banner on phones (the web's HomeBanner): the featured titles'
 * backdrops full width under the top bar, one at a time, each with why it
 * is there ("New in Movies"), its name, year · length or seasons · rating ·
 * genres, its plot, Play (Resume once started), More info and My List.
 * Swiped between; it turns to the next by itself every [TURN_MS], the dash
 * under it filling as the time runs, except while a finger is on it, the
 * page is scrolled away from it, or the system asks for less motion.
 */
@Composable
internal fun HomeBanner(
    featured: List<Featured>,
    origin: String,
    visible: () -> Boolean,
    onPlay: (HomeInfo) -> Unit,
    onInfo: (HomeInfo) -> Unit,
    onList: (HomeInfo) -> Unit,
    modifier: Modifier = Modifier,
) {
    val count = featured.size
    val pager = rememberPagerState { count }
    val dragged by pager.interactionSource.collectIsDraggedAsState()
    val calm = reducedMotion()
    val turn = remember { Animatable(0f) }
    val height = bannerHeight()
    val label = stringResource(R.string.home_banner_label)
    val scope = rememberCoroutineScope()

    // Each title's turn: the dash fills, then the next comes. A finger on it, or the page scrolled
    // away, starts the turn over once it is let go.
    LaunchedEffect(pager.settledPage, dragged, count, calm) {
        turn.snapTo(0f)
        if (count < 2 || calm || dragged) return@LaunchedEffect
        snapshotFlow { visible() }.first { it }
        turn.animateTo(1f, tween(TURN_MS, easing = LinearEasing))
        pager.animateScrollToPage((pager.settledPage + 1) % count)
    }

    Box(
        modifier
            .fillMaxWidth()
            // The first row rises into the banner's faded bottom: the page takes it as that much shorter.
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints)
                layout(placeable.width, placeable.height - RowsOverlap.roundToPx()) { placeable.place(0, 0) }
            }
            .height(height)
            .semantics { contentDescription = label },
    ) {
        HorizontalPager(
            state = pager,
            key = { featured[it].info.item.id },
            beyondViewportPageCount = 1,
            modifier = Modifier.fillMaxSize().testTag(BANNER_TAG),
        ) { page ->
            Slide(featured[page], origin, onPlay, onInfo, onList)
        }
        if (count > 1) {
            Turns(
                count = count,
                current = pager.currentPage,
                progress = { turn.value },
                titles = featured.map { it.info.title },
                onSelect = { page -> scope.launch { pager.animateScrollToPage(page) } },
                modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = TurnsBottom),
            )
        }
    }
}

/** One title: its backdrop fading into the page, and its words and buttons over the fade. */
@Composable
private fun Slide(
    featured: Featured,
    origin: String,
    onPlay: (HomeInfo) -> Unit,
    onInfo: (HomeInfo) -> Unit,
    onList: (HomeInfo) -> Unit,
) {
    val info = featured.info
    val tier = LocalPhoneDeviceTier.current
    val metrics = LocalPhoneMetrics.current
    val backdrop = remember(info.item, origin, tier) { ArtworkCandidates.backdrop(info.item, origin, tier) }
    Box(Modifier.fillMaxSize()) {
        Artwork(
            image = backdrop,
            title = info.title,
            // A picture of what the heading names: screen readers skip it.
            modifier = Modifier.fillMaxSize().clearAndSetSemantics { }.drawWithCache {
                // The top keeps the bar's icons readable; the bottom melts into the page under the words.
                val top = Brush.verticalGradient(0f to Color.Black.copy(alpha = 0.55f), 1f to Color.Transparent, endY = size.height * 0.22f)
                val bottom = Brush.verticalGradient(
                    0f to Color.Transparent,
                    0.45f to PleyColors.Bg.copy(alpha = 0.75f),
                    0.85f to PleyColors.Bg,
                    startY = size.height * 0.3f,
                    endY = size.height,
                )
                onDrawWithContent {
                    drawContent()
                    drawRect(Color.Black.copy(alpha = 0.18f))
                    drawRect(top)
                    drawRect(bottom)
                }
            },
        )
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .padding(start = metrics.marginStart, end = metrics.marginEnd, bottom = WordsBottom),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                featured.label.asString().uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(letterSpacing = 2.4.sp, shadow = SoftShadow),
                color = PleyColors.Fg.copy(alpha = 0.85f),
                maxLines = 1,
            )
            Spacer(Modifier.height(10.dp))
            Text(
                info.title,
                style = titleStyle(info.title),
                color = Color.White,
                textAlign = TextAlign.Center,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.semantics { heading() },
            )
            val facts = remember(info) { facts(info) }
            if (facts.isNotEmpty()) {
                Spacer(Modifier.height(10.dp))
                Text(
                    facts,
                    style = MaterialTheme.typography.bodyMedium.copy(shadow = SoftShadow),
                    color = PleyColors.Fg.copy(alpha = 0.85f),
                    textAlign = TextAlign.Center,
                    maxLines = 2,
                )
            }
            if (info.summary.isNotEmpty()) {
                Spacer(Modifier.height(8.dp))
                Text(
                    info.summary,
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Fg.copy(alpha = 0.75f),
                    textAlign = TextAlign.Center,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.widthIn(max = 520.dp),
                )
            }
            Spacer(Modifier.height(18.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                PrimaryButton(
                    text = stringResource(if (info.inProgress) R.string.item_resume else R.string.item_play),
                    onClick = { onPlay(info) },
                    icon = PleyIcons.Play,
                )
                TonalButton(
                    text = stringResource(R.string.home_banner_more_info),
                    onClick = { onInfo(info) },
                    icon = PleyIcons.Info,
                    modifier = Modifier.heightIn(min = 48.dp),
                )
                if (info.listable && info.complete) ListButton(info, onList)
            }
        }
    }
}

/** My List: a ring with + to put the title on it, ✓ once it is. */
@Composable
private fun ListButton(info: HomeInfo, onList: (HomeInfo) -> Unit) {
    val label = stringResource(if (info.inList) R.string.item_action_remove_from_my_list else R.string.item_action_add_to_my_list)
    // A 40 dp ring in a 48 dp touch target: smaller than the buttons beside it, as easy to hit.
    Box(
        Modifier
            .size(48.dp)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClickLabel = label) { onList(info) }
            .semantics { contentDescription = label; selected = info.inList },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(40.dp)
                .background(Color.Black.copy(alpha = 0.25f), CircleShape)
                .border(1.5.dp, Color.White.copy(alpha = 0.45f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                if (info.inList) PleyIcons.Check else PleyIcons.Plus,
                contentDescription = null,
                size = 18.dp,
                tint = if (info.inList) PleyColors.Accent else Color.White,
            )
        }
    }
}

/** The dashes under the banner: one per title, the current one longer and filling as its turn runs. */
@Composable
private fun Turns(
    count: Int,
    current: Int,
    progress: () -> Float,
    titles: List<String>,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        repeat(count) { page ->
            val on = page == current
            val width by animateDpAsState(if (on) 40.dp else 16.dp, label = "turn")
            val label = stringResource(R.string.home_banner_show, titles[page])
            Box(
                Modifier
                    .height(24.dp)
                    .clickable(role = Role.Tab, onClickLabel = label) { onSelect(page) }
                    .semantics { contentDescription = label; selected = on },
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    Modifier
                        .size(width = width, height = 4.dp)
                        .clip(CircleShape)
                        .background(Color.White.copy(alpha = 0.28f))
                        .then(
                            if (on) {
                                Modifier.drawWithContent {
                                    drawContent()
                                    val p = progress()
                                    drawRect(Color.White, size = size.copy(width = size.width * (if (p > 0f) p else 1f)))
                                }
                            } else {
                                Modifier
                            },
                        ),
                )
            }
        }
    }
}

/** Year · length or seasons · rating · genres: the parts there are. */
private fun facts(info: HomeInfo): String = buildList {
    info.meta.year.takeIf { it.isNotEmpty() }?.let(::add)
    info.meta.runtime.takeIf { it.isNotEmpty() }?.let(::add)
    info.meta.contentRating?.let(::add)
    info.meta.rating?.let { add("★ $it") }
    addAll(info.genres.take(2))
}.joinToString("  ·  ")

/** Longer names come smaller, so a long one never pushes the rest off. */
@Composable
private fun titleStyle(title: String) = when {
    title.length > 40 -> MaterialTheme.typography.headlineMedium
    title.length > 22 -> MaterialTheme.typography.headlineLarge
    else -> MaterialTheme.typography.displaySmall
}.copy(fontWeight = FontWeight.ExtraBold, lineHeight = 1.05.em, shadow = TitleShadow)

/** The banner's height: about three quarters of a portrait phone, never under 440 dp. */
@Composable
private fun bannerHeight(): Dp {
    val window = with(LocalDensity.current) { LocalWindowInfo.current.containerSize.height.toDp() }
    return if (window > 0.dp) maxOf(window * 0.72f, MinHeight).coerceAtMost(MaxHeight) else MinHeight
}

private val TitleShadow = Shadow(Color.Black.copy(alpha = 0.45f), Offset(0f, 2f), blurRadius = 24f)
private val SoftShadow = Shadow(Color.Black.copy(alpha = 0.6f), Offset(0f, 1f), blurRadius = 8f)

private val MinHeight = 440.dp
private val MaxHeight = 720.dp

/** The words sit above the dashes, which sit above where the first row starts. */
private val WordsBottom = 44.dp
private val TurnsBottom = 6.dp

/** How far the first row rises into the banner, under its dashes. */
private val RowsOverlap = 28.dp

/** How long each title stays before the next comes: the web's. */
private const val TURN_MS = 9_000

internal const val BANNER_TAG = "home.banner"
