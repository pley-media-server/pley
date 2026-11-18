package app.pley.android.ui.tv.collection

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
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
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.collection.CollectionDeps
import app.pley.android.screens.collection.CollectionEvent
import app.pley.android.screens.collection.MergedCollectionPage
import app.pley.android.screens.collection.MergedCollectionViewModel
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.tileActions
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.library.TvChip
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
 * A Home row of several libraries' collections of one title, on TV (the
 * web's MergedCollectionPage): at x = 96, "Collection", the title, how
 * many movies and shows, Play with focus and Shuffle, then a chip for
 * each library they come from, which opens that library's collection;
 * every title in the row's order in a grid below.
 */
@Composable
internal fun TvMergedCollectionScreen(key: PleyKey.MergedCollection) {
    val context = LocalContext.current
    val vm: MergedCollectionViewModel<ArtworkTileModel> = viewModel {
        MergedCollectionViewModel(key.collectionIds, CollectionDeps.of(screensGraph()), TvTiles(context))
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
        page != null -> MergedContent(page, vm, message)
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
private fun MergedContent(page: MergedCollectionPage<ArtworkTileModel>, vm: MergedCollectionViewModel<ArtworkTileModel>, message: UiText?) {
    val navigator = LocalTvNavigator.current
    val actions = tileActions()
    val memory = rememberFocusMemory()
    val skeleton = rememberSkeleton(animated = false)
    TvPosterGrid(
        total = page.tiles.size,
        tileAt = { page.tiles.getOrNull(it) },
        keyAt = { page.itemIds.getOrNull(it) ?: -(it + 1L) },
        kind = TileKind.LibraryGrid,
        onOpen = { position, _ -> vm.opens(position)?.let(navigator::open) },
        onActions = { tile -> actions.open(tile.id, continueWatching = false) },
        memory = memory,
        memoryPrefix = "merged",
        skeleton = skeleton,
        state = rememberTvGridState(),
        initialFocus = false,
        contentTop = 58.dp,
        heroHeight = MergedHeroHeight,
        header = {
            item(key = "hero", span = { GridItemSpan(maxLineSpan) }, contentType = "hero") {
                MergedHero(page, vm, message, memory, onOpen = navigator::open)
            }
        },
        modifier = Modifier.fillMaxSize(),
    )
}

/** The page's top: the words, Play and Shuffle, and the libraries' chips. */
@Composable
private fun MergedHero(
    page: MergedCollectionPage<ArtworkTileModel>,
    vm: MergedCollectionViewModel<ArtworkTileModel>,
    message: UiText?,
    memory: FocusMemory,
    onOpen: (PleyKey) -> Unit,
) {
    val resources = LocalResources.current
    Column(Modifier.height(MergedHeroHeight).tvGridHero().padding(top = 58.dp).widthIn(max = PleyTv.TextColumnEnd - PleyTv.Gutter)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(PleyIcons.Layers, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            BasicText(stringResource(R.string.tv_collection_label), style = PleyType.Tv.Label.copy(color = PleyColors.Muted))
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
        val counts = listOfNotNull(
            page.movies.takeIf { it > 0 }?.let { tvCount(resources, R.plurals.tv_library_count_movies, it) },
            page.shows.takeIf { it > 0 }?.let { tvCount(resources, R.plurals.tv_library_count_shows, it) },
        )
        BasicText(
            counts.joinToString("  ·  "),
            style = PleyType.Tv.Label.copy(color = PleyColors.Muted),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
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
        Spacer(Modifier.height(PleyTv.ButtonLabelRoom + 8.dp))
        // Each library's own collection, to open on its own.
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            BasicText(stringResource(R.string.tv_collection_from), style = PleyType.Tv.Label.copy(color = PleyColors.Muted))
            page.sources.forEach { source ->
                TvChip(
                    label = source.libraryName,
                    onClick = { onOpen(PleyKey.Collection(source.collectionId)) },
                    modifier = Modifier.widthIn(max = 220.dp).rememberedFocus(memory, "library/${source.collectionId}"),
                )
            }
        }
        Spacer(Modifier.height(20.dp))
    }
}

/** The page's top, down to the grid's first row of wells: no backdrop, so shorter than a collection's. */
private val MergedHeroHeight = 360.dp
