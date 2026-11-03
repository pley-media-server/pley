package app.pley.android.ui.phone.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.rememberImagePrefetcher
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.model.LibraryKind
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import app.pley.android.model.HubKeys
import app.pley.android.screens.library.CollectionTile
import app.pley.android.screens.library.LibraryUiState
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.library.TileRow
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.HubRow
import app.pley.android.ui.phone.kit.HubRowModel
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.PRESS_ROOM
import app.pley.android.ui.phone.kit.cardPress
import app.pley.android.ui.phone.kit.rememberItemGridState
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * The Recommended tab (apps/web's RecommendedTab): the library's hubs as
 * rows (Continue Watching as stills, the rest as posters), then its
 * collections. The collections row's title opens the Collections tab.
 */
@Composable
internal fun RecommendedPane(state: LibraryUiState<ArtworkTileModel>, vm: LibraryViewModel<ArtworkTileModel>, skeleton: SkeletonState) {
    val navigator = LocalPhoneNavigator.current
    val recommended = state.recommended
    val rows = recommended.valueOrNull
    when {
        rows == null && recommended is Loadable.Failed ->
            Scrolling { ErrorState(stringResource(R.string.library_recommended_failed), recommended.error, onRetry = vm::retry) }
        rows != null && rows.isEmpty() -> Scrolling {
            EmptyState(
                title = stringResource(R.string.library_recommended_empty_title),
                body = stringResource(R.string.library_recommended_empty_body),
                icon = PleyIcons.Sparkles,
            )
        }
        else -> {
            val collectionsTitle = stringResource(R.string.library_tab_collections)
            val models = remember(rows, collectionsTitle) {
                rows?.map { it.model(collectionsTitle) } ?: List(2) { HubRowModel("loading$it", "", TileKind.Poster, null) }
            }
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(top = 16.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(PleyPhone.SectionGap - PRESS_ROOM * 2),
            ) {
                items(models, key = { it.key }, contentType = { "row" }) { model ->
                    val row = rows?.firstOrNull { it.key == model.key }
                    HubRow(
                        row = model,
                        onOpen = { tile -> row?.let { r -> r.opens.getOrNull(r.tiles.indexOf(tile))?.let(navigator::open) } },
                        skeleton = skeleton,
                        onLongPress = if (row?.key == TileRow.COLLECTIONS) {
                            null
                        } else {
                            { tile -> navigator.open(PleyKey.ItemActions(tile.id, continueWatching = HubKeys.isContinueWatching(model.key))) }
                        },
                        onTitleClick = if (row?.key == TileRow.COLLECTIONS) {
                            { vm.select(LibraryTab.Collections) }
                        } else {
                            row?.seeAll?.let { view -> { vm.seeAll(view) } }
                        },
                    )
                }
            }
        }
    }
}

private fun TileRow<ArtworkTileModel>.model(collections: String) = HubRowModel(
    key = key,
    title = title ?: collections,
    kind = when (look) {
        TileLook.Landscape -> TileKind.Landscape
        TileLook.Square -> TileKind.Square
        else -> TileKind.Poster
    },
    items = tiles,
)

/**
 * The Collections tab, and the Collections page (apps/web's CollectionsTab):
 * how many there are, then their posters with the count on each, titled
 * and counted ("12 movies").
 */
@Composable
internal fun CollectionsPane(
    collections: Loadable<List<CollectionTile<ArtworkTileModel>>>,
    kind: LibraryKind?,
    onRetry: () -> Unit,
    skeleton: SkeletonState,
) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val resources = LocalResources.current
    val list = collections.valueOrNull
    when {
        list == null && collections is Loadable.Failed ->
            Scrolling { ErrorState(stringResource(R.string.library_collections_failed), collections.error, onRetry = onRetry) }
        list != null && list.isEmpty() -> Scrolling {
            EmptyState(
                title = stringResource(R.string.library_collections_empty_title),
                body = stringResource(if (kind == LibraryKind.Shows) R.string.library_collections_empty_shows else R.string.library_collections_empty_movies),
                icon = PleyIcons.Layers,
            )
        }
        else -> {
            val available = metrics.contentWidth - metrics.margin * 2
            val columns = libraryColumns(app.pley.android.data.GridDensity.Medium, TileKind.Poster, available)
            val width = cellWidth(columns, available)
            // About a screen composed ahead, and the posters of the next 1.5 screens asked for (the count has none).
            val grid = rememberItemGridState()
            val density = LocalDensity.current
            val images = remember(list) { listOf(null) + list.orEmpty().map { it.tile.image } }
            rememberImagePrefetcher(
                state = grid,
                images = images,
                widthPx = with(density) { width.roundToPx() },
                heightPx = with(density) { (width / TileKind.Poster.aspectRatio).roundToPx() },
                tier = LocalPhoneDeviceTier.current,
            )
            LazyVerticalGrid(
                columns = EqualCells(columns),
                state = grid,
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
                verticalArrangement = Arrangement.spacedBy(16.dp),
                userScrollEnabled = list != null,
            ) {
                if (list == null) {
                    items(12, contentType = { "skeleton" }) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Phone, width = width) }
                } else {
                    item(key = "count", span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }, contentType = "count") {
                        Text(
                            count(resources, R.plurals.library_count_collections, list.size),
                            style = MaterialTheme.typography.labelMedium,
                            color = PleyColors.Muted,
                        )
                    }
                    items(list, key = { it.id }, contentType = { "collection" }) { c ->
                        CollectionCard(c, onClick = { navigator.open(PleyKey.Collection(c.id)) }, modifier = Modifier.fillMaxWidth())
                    }
                }
            }
        }
    }
}

/**
 * A collection's card (the web's CollectionCard): its poster with the
 * number of titles in a badge at the top right, then its title and
 * "12 movies". Pressed like any card.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun CollectionCard(collection: CollectionTile<ArtworkTileModel>, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val interactions = remember { MutableInteractionSource() }
    val model = collection.tile
    val shape = TileStyle.Phone.shape(TileKind.Poster)
    Column(modifier.combinedClickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onClick)) {
        Box(Modifier.cardPress(interactions, shape, reducedMotion()).fillMaxWidth().aspectRatio(TileKind.Poster.aspectRatio)) {
            ArtworkTile(model, TileKind.Poster, TileStyle.Phone, Modifier.fillMaxSize())
            QualityBadge(
                collection.count.toString(),
                TileStyle.Phone,
                onArt = true,
                modifier = Modifier.align(Alignment.TopEnd).padding(6.dp).clearAndSetSemantics { },
            )
        }
        Spacer(Modifier.height(TileStyle.Phone.captionGap))
        TileCaption(model.title, model.subtitle, TileStyle.Phone)
    }
}
