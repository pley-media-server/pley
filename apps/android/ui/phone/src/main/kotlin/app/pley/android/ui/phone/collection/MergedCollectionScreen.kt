package app.pley.android.ui.phone.collection

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.GridDensity
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.collection.CollectionDeps
import app.pley.android.screens.collection.CollectionEvent
import app.pley.android.screens.collection.MergedCollectionPage
import app.pley.android.screens.collection.MergedCollectionViewModel
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.MediaCard
import app.pley.android.ui.phone.kit.PleyTopAppBar
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
 * A Home row of several libraries' collections of one title, on the phone
 * (the web's MergedCollectionPage): "Collection", the title, how many
 * movies and shows, the libraries they come from (each opens its own
 * collection), Play and Shuffle, then every title in the row's order.
 */
@Composable
internal fun MergedCollectionScreen(key: PleyKey.MergedCollection) {
    val tiles = rememberPhoneTiles()
    val vm: MergedCollectionViewModel<ArtworkTileModel> = viewModel {
        MergedCollectionViewModel(key.collectionIds, CollectionDeps.of(screensGraph()), tiles)
    }
    val state by vm.state.collectAsStateWithLifecycle()
    val shell = LocalPhoneShell.current
    val navigator = LocalPhoneNavigator.current
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
    val metrics = LocalPhoneMetrics.current
    val skeleton = rememberSkeleton(animated = true)
    val page = state.valueOrNull
    val available = metrics.contentWidth - metrics.margin * 2
    val columns = libraryColumns(GridDensity.Medium, TileKind.Poster, available)
    val width = cellWidth(columns, available)
    Column(Modifier.fillMaxSize()) {
        PleyTopAppBar(title = "", onBack = navigator::back)
        val failed = state as? Loadable.Failed
        if (page == null && failed != null) {
            ErrorState(stringResource(R.string.collection_failed), failed.error, onRetry = vm::retry)
            return@Column
        }
        LazyVerticalGrid(
            columns = EqualCells(columns),
            state = rememberItemGridState(),
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
            horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            item(key = "header", span = { GridItemSpan(maxLineSpan) }, contentType = "header") {
                Header(page, onPlay = vm::play, onOpen = navigator::open)
            }
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
}

@Composable
private fun Header(page: MergedCollectionPage<ArtworkTileModel>?, onPlay: (Boolean) -> Unit, onOpen: (PleyKey) -> Unit) {
    val resources = LocalResources.current
    val skeleton = rememberSkeleton(animated = true)
    Column(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 12.dp)) {
        if (page == null) {
            Box(Modifier.size(180.dp, 32.dp).skeleton(skeleton, PleyShape.Badge))
            Spacer(Modifier.height(12.dp))
            Box(Modifier.size(120.dp, 16.dp).skeleton(skeleton, PleyShape.Badge))
            return@Column
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(PleyIcons.Layers, contentDescription = null, size = 16.dp, tint = PleyColors.Muted)
            Text(stringResource(R.string.collection_label), style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
        }
        Spacer(Modifier.height(8.dp))
        Text(
            page.title,
            style = MaterialTheme.typography.displaySmall.copy(lineBreak = LineBreak.Heading),
            color = PleyColors.Fg,
            modifier = Modifier.semantics { heading() },
        )
        Spacer(Modifier.height(8.dp))
        // The counts, then each library, which opens its own collection.
        val counts = listOfNotNull(
            page.movies.takeIf { it > 0 }?.let { count(resources, R.plurals.library_count_movies, it) },
            page.shows.takeIf { it > 0 }?.let { count(resources, R.plurals.library_count_shows, it) },
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), itemVerticalAlignment = Alignment.CenterVertically) {
            val parts = counts.map { it to null } + page.sources.map { it.libraryName to PleyKey.Collection(it.collectionId) }
            parts.forEachIndexed { at, (words, opens) ->
                if (at > 0) Text("·", style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
                Text(
                    words,
                    style = MaterialTheme.typography.labelMedium,
                    color = if (opens != null) PleyColors.Fg else PleyColors.Muted,
                    modifier = if (opens != null) {
                        Modifier.clickable(role = Role.Button) { onOpen(opens) }.padding(vertical = 4.dp)
                    } else {
                        Modifier
                    },
                )
            }
        }
        Spacer(Modifier.height(20.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            PrimaryButton(
                stringResource(R.string.collection_play),
                onClick = { onPlay(false) },
                icon = PleyIcons.Play,
                enabled = page.canPlay,
                modifier = Modifier.weight(1f),
            )
            TonalButton(
                stringResource(R.string.collection_shuffle),
                onClick = { onPlay(true) },
                icon = PleyIcons.Shuffle,
                enabled = page.canShuffle,
                modifier = Modifier.height(48.dp),
            )
        }
    }
}
