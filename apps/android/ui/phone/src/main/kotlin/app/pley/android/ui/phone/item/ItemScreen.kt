package app.pley.android.ui.phone.item

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.ItemKind
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.CastTitle
import app.pley.android.screens.item.ItemEvent
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.ItemUiState
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.HubRow
import app.pley.android.ui.phone.kit.OfflineBanner
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.requests.ItemRequestBlock
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass

/**
 * An item's page on phones and tablets (design spec §4.3, §4.4): a movie,
 * show, season or episode, as the web shows it. Artists, albums and tracks
 * open their music pages instead, and an extra its movie's or show's.
 */
@Composable
internal fun ItemScreen(itemId: Long) {
    val model: ItemViewModel = viewModel(factory = ItemViewModel.factory(itemId))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                ItemEvent.NothingToPlay -> shell.toasts.show(resources.getString(R.string.item_nothing_to_play))
                is ItemEvent.Failed -> shell.report(event.error)
            }
        }
    }
    when (val s = state) {
        ItemUiState.Loading -> ItemSkeleton(onBack = navigator::back)
        ItemUiState.NotFound -> Column(Modifier.fillMaxSize()) {
            PleyTopAppBar(title = "", onBack = navigator::back)
            EmptyState(
                title = stringResource(R.string.item_not_found),
                body = stringResource(R.string.item_not_found_body),
                icon = PleyIcons.CircleAlert,
            )
        }
        is ItemUiState.Failed -> Column(Modifier.fillMaxSize()) {
            PleyTopAppBar(title = "", onBack = navigator::back)
            ErrorState(stringResource(R.string.item_failed), s.error, onRetry = model::retry)
        }
        is ItemUiState.Elsewhere -> LaunchedEffect(s.key) {
            navigator.back()
            navigator.open(s.key)
        }
        is ItemUiState.Ready -> {
            ReportLoad(s.status)
            LaunchedEffect(Unit) {
                withFrameNanos { }
                model.onShown()
            }
            ItemPageContent(s.page, model, navigator)
        }
    }
}

/**
 * The page, with its [downloads] (phones and tablets) for Download and the
 * episodes' buttons: the app's own, or a test's.
 */
@Composable
internal fun ItemPageContent(
    page: ItemPage,
    model: ItemViewModel,
    navigator: Navigator,
    downloads: ItemDownloads? = rememberItemDownloads(),
) {
    CompositionLocalProvider(LocalItemDownloads provides downloads) {
        ItemPageBody(page, model, navigator)
    }
}

/** The page: the header over the backdrop, then its sections, under a bar that turns solid as it scrolls. */
@Composable
private fun ItemPageBody(page: ItemPage, model: ItemViewModel, navigator: Navigator) {
    val metrics = LocalPhoneMetrics.current
    val list = rememberLazyListState()
    val skeleton = rememberSkeleton(animated = true)
    val actions = remember(model, navigator) { PageActions(model, navigator) }
    val titles = RowTitles(
        extras = stringResource(R.string.item_extras),
        cast = castTitle(page.castTitle),
        related = stringResource(R.string.item_more_like_this),
        season = page.context?.seasonTitle?.let { stringResource(R.string.item_more_episodes, it) } ?: stringResource(R.string.item_episodes),
    )
    val resources = LocalResources.current
    val rows = remember(page, titles, resources) { phoneRows(page, titles, resources) }
    // What requests say of it, read once the page has drawn.
    val request by model.request.collectAsStateWithLifecycle()
    // The sections keep clear of the bars and the cutout at the sides, as the rows do.
    val sides = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd)
    Box(Modifier.fillMaxSize()) {
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().testTag(ITEM_PAGE),
            contentPadding = PaddingValues(bottom = 32.dp),
        ) {
            item(key = "header", contentType = "header") {
                ItemHeader(page, actions) { request?.let { ItemRequestBlock(it) } }
            }
            item(key = "tracks", contentType = "tracks") { TrackSelects(page, actions, sides) }
            val seasons = page.seasons
            if (seasons != null && page.kind != ItemKind.Episode) {
                item(key = "seasons", contentType = "seasons") { SeasonPicker(seasons, actions, sides) }
                val episodes = seasons.episodes
                if (episodes == null) {
                    items(EPISODE_SKELETONS, key = { "episode-skeleton-$it" }, contentType = { "episode-skeleton" }) {
                        EpisodeRowSkeleton(skeleton, sides)
                    }
                } else {
                    items(episodes, key = { "episode-${it.item.id}" }, contentType = { "episode" }) { episode ->
                        EpisodeRow(episode, page.origin, actions, sides)
                    }
                    if (episodes.isEmpty()) {
                        item(key = "no-episodes") {
                            Text(
                                stringResource(R.string.item_no_episodes),
                                style = MaterialTheme.typography.bodyMedium,
                                color = PleyColors.Muted,
                                modifier = sides.padding(vertical = 12.dp),
                            )
                        }
                    }
                }
            }
            rows.forEach { row ->
                item(key = row.model.key, contentType = row.model.kind) {
                    HubRow(
                        row = row.model,
                        onOpen = { row.open(it.id, actions) },
                        onLongPress = row.longPress?.let { long -> { long(it.id, actions) } },
                        skeleton = skeleton,
                        modifier = Modifier.padding(top = PleyPhone.SectionGap),
                    )
                }
            }
            if (!page.details.isEmpty) {
                item(key = "details", contentType = "details") { DetailsSection(page, actions, sides) }
            }
            if (page.mediaInfo.isNotEmpty()) {
                item(key = "media", contentType = "media") { MediaInfoSection(page.mediaInfo, sides) }
            }
        }
        ItemTopBar(page, list, onBack = navigator::back, onMore = { navigator.open(PleyKey.ItemActions(page.id)) })
    }
}

/** The bar over the page, with More. */
@Composable
private fun ItemTopBar(page: ItemPage, list: LazyListState, onBack: () -> Unit, onMore: () -> Unit) {
    BackdropTopBar(page.title, list, onBack) {
        PleyIconButton(PleyIcons.Ellipsis, stringResource(R.string.item_more_actions), onMore)
    }
}

/**
 * The bar over a page that opens on a backdrop (design spec §3.1): the
 * item page's and a Discover title's. Transparent over the backdrop with
 * a 96 dp scrim behind back and [actions]. It turns [bg][PleyColors.Bg] as
 * the header's title comes up to it, so that title slides under a solid
 * bar, and shows [title] itself only once the header's has gone under it:
 * the two never show at once. Read while drawing only.
 */
@Composable
internal fun BackdropTopBar(title: String, list: LazyListState, onBack: () -> Unit, actions: @Composable RowScope.() -> Unit = {}) {
    val density = LocalDensity.current
    val metrics = LocalPhoneMetrics.current
    val titleTop = itemTitleTop(itemBackdropHeight(metrics.widthClass != WidthClass.Compact))
    val barBottom = WindowInsets.safeDrawing.only(WindowInsetsSides.Top).getTop(density) + with(density) { BarHeight.toPx() }
    // The scroll at which the header's title reaches the bar's bottom edge.
    val meets = with(density) { titleTop.toPx() } - barBottom
    val fade = with(density) { BAR_FADE.toPx() }
    val under = with(density) { TITLE_UNDER.toPx() }
    fun scrolled(): Float = if (list.firstVisibleItemIndex > 0) Float.POSITIVE_INFINITY else list.firstVisibleItemScrollOffset.toFloat()
    fun solid(): Float = ((scrolled() - (meets - fade)) / fade).coerceIn(0f, 1f)
    fun titleShown(): Float = ((scrolled() - (meets + under)) / fade).coerceIn(0f, 1f)
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .fillMaxWidth()
                .drawBehind {
                    drawRect(Brush.verticalGradient(0f to PleyColors.Bg.copy(alpha = 0.6f), 1f to Color.Transparent, endY = 96.dp.toPx()))
                    val a = solid()
                    if (a > 0f) drawRect(PleyColors.Bg.copy(alpha = a))
                    if (a >= 1f) drawLine(PleyColors.Surface4, Offset(0f, size.height), Offset(size.width, size.height), 1.dp.toPx())
                }
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top)),
        ) {
            Row(
                Modifier.fillMaxWidth().height(BarHeight).padding(start = 4.dp + metrics.insetStart, end = 4.dp + metrics.insetEnd),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PleyIconButton(PleyIcons.ArrowLeft, stringResource(R.string.kit_back), onBack)
                Text(
                    title,
                    style = MaterialTheme.typography.headlineSmall,
                    color = PleyColors.Fg,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).padding(horizontal = 4.dp).graphicsLayer { alpha = titleShown() },
                )
                actions()
            }
        }
        OfflineBanner()
    }
}

/** The loading page (design spec §8): the backdrop's block, the title's and the button's, shimmering together. */
@Composable
internal fun ItemSkeleton(onBack: () -> Unit) {
    val skeleton = rememberSkeleton(animated = true)
    val metrics = LocalPhoneMetrics.current
    val loading = stringResource(R.string.item_loading)
    Box(Modifier.fillMaxSize().testTag(ITEM_SKELETON).semantics { contentDescription = loading }) {
        Column(Modifier.fillMaxSize()) {
            Box(Modifier.fillMaxWidth().height(itemBackdropHeight(metrics.widthClass != WidthClass.Compact)).skeleton(skeleton, RectangleShape))
            Column(Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd, top = 16.dp, bottom = 16.dp)) {
                Bar(skeleton, 0.7f, 36.dp)
                Spacer(Modifier.height(12.dp))
                Bar(skeleton, 0.45f, 16.dp)
                Spacer(Modifier.height(20.dp))
                Bar(skeleton, 1f, 48.dp, PleyShape.Control)
                Spacer(Modifier.height(24.dp))
                Bar(skeleton, 1f, 16.dp)
                Spacer(Modifier.height(8.dp))
                Bar(skeleton, 0.9f, 16.dp)
                Spacer(Modifier.height(8.dp))
                Bar(skeleton, 0.6f, 16.dp)
            }
        }
        Row(
            Modifier
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top))
                .height(BarHeight)
                .padding(start = 4.dp + metrics.insetStart, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PleyIconButton(PleyIcons.ArrowLeft, stringResource(R.string.kit_back), onBack)
        }
    }
}

@Composable
private fun Bar(skeleton: SkeletonState, fraction: Float, height: Dp, shape: Shape = PleyShape.Badge) {
    Box(Modifier.fillMaxWidth(fraction).height(height).skeleton(skeleton, shape))
}

/** The title of the cast row. */
@Composable
internal fun castTitle(title: CastTitle): String =
    stringResource(if (title == CastTitle.GuestStars) R.string.item_guest_stars else R.string.item_cast)

/** The backdrop's height on compact widths (design spec §4.3). */
internal val BackdropHeight = 300.dp

/** The bar over the page, under the status bar. */
private val BarHeight = 64.dp

/** How much scrolling the bar's background and its title each take to fade in. */
private val BAR_FADE = 48.dp

/** How far under the bar the header's title goes before the bar's own title begins to show. */
private val TITLE_UNDER = 32.dp

private const val EPISODE_SKELETONS = 4

internal const val ITEM_SKELETON = "pley.item.skeleton"
internal const val ITEM_PAGE = "pley.item.page"
