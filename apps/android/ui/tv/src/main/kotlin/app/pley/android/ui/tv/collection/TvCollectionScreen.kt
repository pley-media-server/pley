package app.pley.android.ui.tv.collection

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.LibraryKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.collection.CollectionDeps
import app.pley.android.screens.collection.CollectionEvent
import app.pley.android.screens.collection.CollectionPage
import app.pley.android.screens.collection.CollectionViewModel
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.tileActions
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.library.TvMessage
import app.pley.android.ui.tv.library.TvPosterGrid
import app.pley.android.ui.tv.library.TvTiles
import app.pley.android.ui.tv.library.rememberTvGridState
import app.pley.android.ui.tv.library.tvCount
import app.pley.android.ui.tv.library.tvGridHero
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import kotlinx.coroutines.delay

/**
 * A collection's page on TV (design spec §4.9): the item page's top, a
 * backdrop with the text column at x = 96 ("Collection" and its TMDB or
 * Smart badge, the title, what it holds in words, the summary, then Play
 * with focus and Shuffle), and its titles in a grid below, which scrolls
 * up over the backdrop as focus moves down, the backdrop dimming under it.
 */
@Composable
internal fun TvCollectionScreen(key: PleyKey.Collection) {
    val context = LocalContext.current
    val vm: CollectionViewModel<ArtworkTileModel> = viewModel {
        CollectionViewModel(key.collectionId, CollectionDeps.of(screensGraph()), TvTiles(context))
    }
    val state by vm.state.collectAsStateWithLifecycle()
    var message by remember { mutableStateOf<UiText?>(null) }
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            message = when (event) {
                CollectionEvent.NothingToPlay -> UiText.res(R.string.tv_collection_nothing_to_play)
                is CollectionEvent.Failed -> errorSentence(event.error)
            }
        }
    }
    LaunchedEffect(message) {
        if (message != null) {
            delay(MESSAGE_MS)
            message = null
        }
    }
    val page = state.valueOrNull
    when {
        page != null -> CollectionContent(page, vm, message)
        state is Loadable.Failed -> TvMessage(
            title = failedTitle((state as Loadable.Failed).error, stringResource(R.string.tv_collection_failed)),
            body = failedBody((state as Loadable.Failed).error),
            action = stringResource(R.string.tv_library_retry),
            onAction = vm::retry,
            top = 58.dp,
        )
        else -> Loading()
    }
}

@Composable
private fun CollectionContent(page: CollectionPage<ArtworkTileModel>, vm: CollectionViewModel<ArtworkTileModel>, message: UiText?) {
    val navigator = LocalTvNavigator.current
    val actions = tileActions()
    val memory = rememberFocusMemory()
    val skeleton = rememberSkeleton(animated = false)
    val grid = rememberTvGridState()
    val heroPx = with(androidx.compose.ui.platform.LocalDensity.current) { HeroHeight.toPx() }
    Box(Modifier.fillMaxSize()) {
        val image = remember(page.backdropUrl, page.posterUrl, page.origin) { ArtworkImage.of(page.backdropUrl ?: page.posterUrl, page.origin) }
        TvBackdrop(
            image = image,
            title = page.title,
            scrim = BackdropScrim.Item,
            modifier = Modifier.drawWithContent {
                drawContent()
                // The grid passes over the backdrop: it darkens as the top scrolls away (one rect, drawn).
                val scrolled = if (grid.firstVisibleItemIndex > 0) 1f else (grid.firstVisibleItemScrollOffset / heroPx).coerceIn(0f, 1f)
                if (scrolled > 0f) drawRect(PleyColors.Bg.copy(alpha = scrolled * 0.92f))
            },
        )
        TvPosterGrid(
            total = page.tiles.size,
            tileAt = { page.tiles.getOrNull(it) },
            keyAt = { page.itemIds.getOrNull(it) ?: -(it + 1L) },
            kind = TileKind.LibraryGrid,
            onOpen = { position, _ -> vm.opens(position)?.let(navigator::open) },
            onActions = { tile -> actions.open(tile.id, continueWatching = false) },
            memory = memory,
            memoryPrefix = "collection",
            skeleton = skeleton,
            state = grid,
            initialFocus = false,
            contentTop = 58.dp,
            heroHeight = HeroHeight,
            header = {
                item(key = "hero", span = { GridItemSpan(maxLineSpan) }, contentType = "hero") {
                    Hero(page, vm, message, memory)
                }
            },
            modifier = Modifier.fillMaxSize(),
        )
    }
}

/** The text column of the page's top, and the heading of the titles under it. */
@Composable
private fun Hero(page: CollectionPage<ArtworkTileModel>, vm: CollectionViewModel<ArtworkTileModel>, message: UiText?, memory: FocusMemory) {
    val resources = LocalResources.current
    // Focus here takes the grid back to its start, gliding as a page of rows goes back to its hero.
    Column(Modifier.height(HeroHeight).tvGridHero().padding(top = 58.dp).widthIn(max = TextColumn)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(PleyIcons.Layers, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            BasicText(stringResource(R.string.tv_collection_label), style = PleyType.Tv.Label.copy(color = PleyColors.Muted))
            if (page.automatic) QualityBadge(stringResource(R.string.tv_collection_tmdb), TileStyle.Tv)
            if (page.smart) QualityBadge(stringResource(R.string.tv_collection_smart), TileStyle.Tv)
        }
        Spacer(Modifier.height(10.dp))
        BasicText(
            page.title,
            style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.semantics { heading() },
        )
        Spacer(Modifier.height(12.dp))
        val noun = if (page.kind == LibraryKind.Shows) R.plurals.tv_library_count_shows else R.plurals.tv_library_count_movies
        BasicText(
            (listOfNotNull(tvCount(resources, noun, page.count), page.libraryName) + page.notes).joinToString("  ·  "),
            style = PleyType.Tv.Label.copy(color = PleyColors.Muted),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        page.summary?.let {
            Spacer(Modifier.height(12.dp))
            BasicText(
                it,
                style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg.copy(alpha = 0.85f)),
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(Modifier.weight(1f))
        Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap), verticalAlignment = Alignment.Top) {
            TvPillButton(
                stringResource(R.string.tv_collection_play),
                onClick = { vm.play(shuffle = false) },
                icon = PleyIcons.Play,
                enabled = page.canPlay,
                modifier = if (page.canPlay) Modifier.rememberedFocus(memory, "play", initial = true) else Modifier,
            )
            TvRoundButton(
                stringResource(R.string.tv_collection_shuffle),
                PleyIcons.Shuffle,
                onClick = { vm.play(shuffle = true) },
                enabled = page.canShuffle,
                modifier = if (page.canPlay) Modifier else Modifier.initialFocus(),
            )
            if (message != null) {
                BasicText(
                    message.asString(),
                    style = PleyType.Tv.Label.copy(color = PleyColors.DangerText),
                    modifier = Modifier.padding(start = 12.dp, top = 14.dp).semantics { liveRegion = LiveRegionMode.Polite },
                )
            }
        }
        Spacer(Modifier.height(PleyTv.ButtonLabelRoom + 12.dp))
        if (page.tiles.isEmpty()) {
            BasicText(stringResource(R.string.tv_collection_empty), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
        }
    }
}

@Composable
private fun Loading() {
    val skeleton = rememberSkeleton(animated = false)
    Column(Modifier.fillMaxSize().padding(start = PleyTv.Gutter, top = 58.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Box(Modifier.size(120.dp, 18.dp).skeleton(skeleton, PleyShape.Badge))
        Box(Modifier.size(360.dp, 44.dp).skeleton(skeleton, PleyShape.Badge))
        Box(Modifier.size(240.dp, 18.dp).skeleton(skeleton, PleyShape.Badge))
        Box(Modifier.width(TextColumn).height(70.dp).skeleton(skeleton, PleyShape.Badge))
        Spacer(Modifier.height(20.dp))
        Box(Modifier.size(128.dp, PleyTv.ButtonHeight).skeleton(skeleton, PleyShape.Control))
        Spacer(Modifier.fillMaxWidth())
    }
}

/** The page's top, to the grid's first row of wells (design spec §4.3: what is below starts at 452). */
private val HeroHeight = 452.dp - 16.dp

/** Text stays in the backdrop's 85 % scrim: x 96 to 560. */
private val TextColumn = PleyTv.TextColumnEnd - PleyTv.Gutter

private const val MESSAGE_MS = 4_000L
