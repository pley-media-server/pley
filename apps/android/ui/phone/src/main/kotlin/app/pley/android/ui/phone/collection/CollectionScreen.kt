package app.pley.android.ui.phone.collection

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.resolve
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.rememberImagePrefetcher
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.SkeletonTile
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
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.MediaCard
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.kit.rememberItemGridState
import app.pley.android.ui.phone.library.EqualCells
import app.pley.android.ui.phone.library.cellWidth
import app.pley.android.ui.phone.library.count
import app.pley.android.ui.phone.library.libraryColumns
import app.pley.android.ui.phone.library.rememberPhoneTiles
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * A collection's page on the phone (design spec §4.9, the web's
 * CollectionPage): the backdrop fading into the page under a clear top
 * bar, "Collection" with its TMDB or Smart badge, the title, what it holds
 * in words, Play and Shuffle, the summary, then its titles in a grid.
 */
@Composable
internal fun CollectionScreen(key: PleyKey.Collection) {
    val tiles = rememberPhoneTiles()
    val vm: CollectionViewModel<ArtworkTileModel> = viewModel {
        CollectionViewModel(key.collectionId, CollectionDeps.of(screensGraph()), tiles)
    }
    val state by vm.state.collectAsStateWithLifecycle()
    val shell = LocalPhoneShell.current
    val nothing = stringResource(R.string.collection_nothing_to_play)
    LaunchedEffect(vm) {
        vm.events.collect { event ->
            when (event) {
                CollectionEvent.NothingToPlay -> shell.toasts.show(nothing)
                is CollectionEvent.Failed -> shell.report(event.error)
            }
        }
    }
    ReportLoad(state)
    CollectionContent(state, vm)
}

@Composable
private fun CollectionContent(state: Loadable<CollectionPage<ArtworkTileModel>>, vm: CollectionViewModel<ArtworkTileModel>) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val skeleton = rememberSkeleton(animated = true)
    val page = state.valueOrNull
    val available = metrics.contentWidth - metrics.margin * 2
    val columns = libraryColumns(app.pley.android.data.GridDensity.Medium, TileKind.Poster, available)
    val width = cellWidth(columns, available)
    // About a screen composed ahead, and the posters of the next 1.5 screens asked for (the header has none).
    val grid = rememberItemGridState()
    val density = LocalDensity.current
    val images = remember(page?.tiles) { listOf(null) + page?.tiles.orEmpty().map { it.image } }
    rememberImagePrefetcher(
        state = grid,
        images = images,
        widthPx = with(density) { width.roundToPx() },
        heightPx = with(density) { (width / TileKind.Poster.aspectRatio).roundToPx() },
        tier = LocalPhoneDeviceTier.current,
    )
    Box(Modifier.fillMaxSize()) {
        if (page == null && state is Loadable.Failed) {
            Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.statusBars).padding(top = 56.dp)) {
                ErrorState(stringResource(R.string.collection_failed), state.error, onRetry = vm::retry)
            }
        } else {
            LazyVerticalGrid(
                columns = EqualCells(columns),
                state = grid,
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
                horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                header(page, vm)
                if (page == null) {
                    items(6, contentType = { "skeleton" }) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Phone, width = width) }
                } else {
                    itemsIndexed(page.tiles, key = { _, tile -> tile.id }, contentType = { _, _ -> TileKind.Poster }) { position, tile ->
                        MediaCard(
                            model = tile,
                            kind = TileKind.Poster,
                            onClick = { vm.opens(position)?.let(navigator::open) },
                            onLongClick = { navigator.open(PleyKey.ItemActions(tile.id)) },
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
            }
        }
        // The clear top bar over the backdrop, with its scrim.
        Box(
            Modifier
                .fillMaxWidth()
                .height(TOP_SCRIM)
                .drawWithCache {
                    val brush = Brush.verticalGradient(listOf(PleyColors.Bg.copy(alpha = 0.6f), Color.Transparent))
                    onDrawBehind { drawRect(brush) }
                },
        )
        PleyIconButton(
            PleyIcons.ArrowLeft,
            stringResource(R.string.kit_back),
            onClick = navigator::back,
            modifier = Modifier
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top))
                .padding(start = 4.dp + metrics.insetStart, top = 4.dp),
        )
    }
}

private fun LazyGridScope.header(page: CollectionPage<ArtworkTileModel>?, vm: CollectionViewModel<ArtworkTileModel>) {
    item(key = "header", span = { GridItemSpan(maxLineSpan) }, contentType = "header") {
        Header(page, vm)
    }
}

@Composable
private fun Header(page: CollectionPage<ArtworkTileModel>?, vm: CollectionViewModel<ArtworkTileModel>) {
    val metrics = LocalPhoneMetrics.current
    val resources = LocalResources.current
    val skeleton = rememberSkeleton(animated = true)
    // The grid's side margins don't apply to the backdrop: it bleeds to the edges, under the insets too.
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .offset(x = (metrics.insetEnd - metrics.insetStart) / 2)
                .requiredWidth(metrics.pageWidth)
                .height(BACKDROP),
        ) {
            val image = remember(page?.backdropUrl, page?.posterUrl, page?.origin) {
                page?.let { ArtworkImage.of(it.backdropUrl ?: it.posterUrl, it.origin) }
            }
            Artwork(
                image = image,
                title = page?.title.orEmpty(),
                modifier = Modifier
                    .fillMaxSize()
                    .drawWithCache {
                        val fade = Brush.verticalGradient(0.35f to Color.Transparent, 1f to PleyColors.Bg)
                        onDrawWithContent {
                            drawContent()
                            drawRect(fade)
                        }
                    },
                alignment = androidx.compose.ui.BiasAlignment(0.4f, -0.6f),
            )
        }
        // The title overlaps the backdrop's fade.
        Column(Modifier.pullUp(OVERLAP).fillMaxWidth()) {
            if (page == null) {
                Box(Modifier.size(180.dp, 32.dp).skeleton(skeleton, PleyShape.Badge))
                Spacer(Modifier.height(12.dp))
                Box(Modifier.size(120.dp, 16.dp).skeleton(skeleton, PleyShape.Badge))
                return@Column
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(PleyIcons.Layers, contentDescription = null, size = 16.dp, tint = PleyColors.Muted)
                Text(stringResource(R.string.collection_label), style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
                if (page.automatic) QualityBadge(stringResource(R.string.collection_tmdb), TileStyle.Phone)
                if (page.smart) SmartBadge(stringResource(R.string.collection_smart))
            }
            Spacer(Modifier.height(8.dp))
            Text(
                page.title,
                style = MaterialTheme.typography.displaySmall.copy(lineBreak = LineBreak.Heading),
                color = PleyColors.Fg,
                modifier = Modifier.semantics { heading() },
            )
            Spacer(Modifier.height(8.dp))
            val noun = if (page.kind == LibraryKind.Shows) R.plurals.library_count_shows else R.plurals.library_count_movies
            val parts = listOfNotNull(count(resources, noun, page.count), page.libraryName) + page.noteTexts.map { it.resolve(resources) }
            Text(
                parts.joinToString(" · "),
                style = MaterialTheme.typography.labelMedium,
                color = PleyColors.Muted,
            )
            Spacer(Modifier.height(20.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                PrimaryButton(
                    stringResource(R.string.collection_play),
                    onClick = { vm.play(shuffle = false) },
                    icon = PleyIcons.Play,
                    enabled = page.canPlay,
                    modifier = Modifier.weight(1f),
                )
                TonalButton(
                    stringResource(R.string.collection_shuffle),
                    onClick = { vm.play(shuffle = true) },
                    icon = PleyIcons.Shuffle,
                    enabled = page.canShuffle,
                    modifier = Modifier.height(48.dp),
                )
            }
            page.summary?.let { summary ->
                Spacer(Modifier.height(20.dp))
                Summary(summary)
            }
            Spacer(Modifier.height(28.dp))
            Text(
                stringResource(if (page.kind == LibraryKind.Shows) R.string.collection_shows else R.string.collection_movies),
                style = MaterialTheme.typography.titleLarge,
                color = PleyColors.Fg,
                modifier = Modifier.semantics { heading() },
            )
            if (page.tiles.isEmpty()) {
                Spacer(Modifier.height(8.dp))
                Text(stringResource(R.string.collection_empty_movies), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            }
            Spacer(Modifier.height(4.dp))
        }
    }
}

/** The summary in bodyLarge, 4 lines with "More" to open the rest. */
@Composable
private fun Summary(text: String) {
    var open by rememberSaveable { mutableStateOf(false) }
    var overflows by remember { mutableStateOf(false) }
    Column {
        Text(
            text,
            style = MaterialTheme.typography.bodyLarge,
            color = PleyColors.Fg.copy(alpha = 0.85f),
            maxLines = if (open) Int.MAX_VALUE else 4,
            overflow = TextOverflow.Ellipsis,
            onTextLayout = { if (!open) overflows = it.hasVisualOverflow },
        )
        if (overflows || open) {
            PleyTextButton(stringResource(if (open) R.string.collection_less else R.string.collection_more), onClick = { open = !open })
        }
    }
}

/** "Smart": an accent-edged badge, the web's accent tone. */
@Composable
private fun SmartBadge(text: String) {
    Box(
        Modifier
            .border(1.dp, PleyColors.Accent.copy(alpha = 0.6f), PleyShape.Badge)
            .background(PleyColors.PrimaryContainer, PleyShape.Badge)
            .padding(horizontal = 6.dp, vertical = 1.dp),
    ) {
        Text(text, style = MaterialTheme.typography.labelSmall, color = PleyColors.AccentBright)
    }
}

/** Lays this out [by] higher than it would be, taking that much less room. */
private fun Modifier.pullUp(by: androidx.compose.ui.unit.Dp): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints)
    val shift = by.roundToPx()
    layout(placeable.width, (placeable.height - shift).coerceAtLeast(0)) { placeable.place(0, -shift) }
}

private val BACKDROP = 300.dp
private val OVERLAP = 56.dp
private val TOP_SCRIM = 96.dp
