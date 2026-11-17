package app.pley.android.ui.phone.library

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.data.GridDensity
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
import app.pley.android.screens.PleyKey
import app.pley.android.screens.collection.CategoryChoice
import app.pley.android.screens.collection.CollectionGroup
import app.pley.android.screens.collection.CollectionsView
import app.pley.android.screens.library.CollectionTile
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.PleyChip
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.cardPress
import app.pley.android.ui.phone.kit.rememberItemGridState
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/** What the Collections tab's filters change; `null` where a page has none. */
internal class CollectionFilters(
    val category: (CategoryChoice) -> Unit,
    val clear: () -> Unit,
)

/**
 * The Collections tab, and the Collections page (apps/web's CollectionsTab):
 * the Category chip and how many there are, then their posters
 * with the count on each, titled and counted ("12 movies"), under their
 * categories' headings when more than one shows.
 */
@Composable
internal fun CollectionsPane(
    collections: Loadable<CollectionsView<ArtworkTileModel>>,
    kind: LibraryKind?,
    onRetry: () -> Unit,
    skeleton: SkeletonState,
    filters: CollectionFilters? = null,
) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val view = collections.valueOrNull
    var sheet by rememberSaveable { mutableStateOf(false) }
    when {
        view == null && collections is Loadable.Failed ->
            Scrolling { ErrorState(stringResource(R.string.library_collections_failed), collections.error, onRetry = onRetry) }
        view != null && view.total == 0 -> Scrolling {
            EmptyState(
                title = stringResource(R.string.library_collections_empty_title),
                body = stringResource(if (kind == LibraryKind.Shows) R.string.library_collections_empty_shows else R.string.library_collections_empty_movies),
                icon = PleyIcons.Layers,
            )
        }
        else -> Column(Modifier.fillMaxSize()) {
            if (view != null) Toolbar(view, filters, onSheet = { sheet = true })
            val available = metrics.contentWidth - metrics.margin * 2
            val columns = libraryColumns(GridDensity.Medium, TileKind.Poster, available)
            val width = cellWidth(columns, available)
            // About a screen composed ahead, and the posters of the next 1.5 screens asked for (headings have none).
            val grid = rememberItemGridState()
            // Another category starts at the top, as a new sort or filter does on the Library tab.
            val category = view?.category?.toString()
            var shownCategory by rememberSaveable { mutableStateOf(category) }
            LaunchedEffect(category) {
                if (category != null && shownCategory != category) {
                    shownCategory = category
                    grid.scrollToItem(0)
                }
            }
            val density = LocalDensity.current
            val images = remember(view) {
                view?.groups.orEmpty().flatMap { g -> (if (view?.headed == true) listOf(null) else emptyList()) + g.collections.map { it.tile.image } }
            }
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
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 4.dp, bottom = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
                verticalArrangement = Arrangement.spacedBy(16.dp),
                userScrollEnabled = view != null,
            ) {
                if (view == null) {
                    items(12, contentType = { "skeleton" }) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Phone, width = width) }
                } else {
                    view.groups.forEach { group ->
                        if (view.headed) {
                            item(key = "heading/${group.key}", span = { GridItemSpan(maxLineSpan) }, contentType = "heading") {
                                GroupHeading(group)
                            }
                        }
                        items(group.collections, key = { it.id }, contentType = { "collection" }) { c ->
                            CollectionCard(c, onClick = { navigator.open(PleyKey.Collection(c.id)) }, modifier = Modifier.fillMaxWidth())
                        }
                    }
                }
            }
        }
    }
    if (sheet && view != null && filters != null) {
        CategorySheet(view, filters, onDismiss = { sheet = false })
    }
}

/** The Category chip when there are categories to choose, Clear filters, and the count. */
@Composable
private fun Toolbar(view: CollectionsView<ArtworkTileModel>, filters: CollectionFilters?, onSheet: () -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val resources = LocalResources.current
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (filters != null) {
            if (view.categoryOffered) {
                item(key = "category") {
                    PleyChip(
                        label = categoryLabel(view, view.category),
                        selected = view.filtered,
                        onClick = onSheet,
                        dropdown = true,
                    )
                }
            }
            if (view.filtered) {
                item(key = "clear") {
                    PleyChip(label = stringResource(R.string.library_clear_filters), selected = false, onClick = filters.clear, icon = PleyIcons.X)
                }
            }
        }
        item(key = "count") {
            Text(
                count(resources, R.plurals.library_count_collections, view.count),
                style = MaterialTheme.typography.labelMedium,
                color = PleyColors.Muted,
                modifier = Modifier.padding(start = 4.dp),
            )
        }
    }
}

/** A category's name over its collections, with how many there are. */
@Composable
private fun GroupHeading(group: CollectionGroup<ArtworkTileModel>) {
    Row(
        Modifier.padding(top = 8.dp).semantics(mergeDescendants = true) { heading() },
        verticalAlignment = Alignment.Bottom,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // A long name wraps beside its count, which stays inside the margins.
        Text(
            group.category?.title.orEmpty(),
            style = MaterialTheme.typography.titleMedium,
            color = PleyColors.Fg,
            modifier = Modifier.weight(1f, fill = false),
        )
        Text(digits(LocalResources.current, group.collections.size), style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
    }
}

/** The Category sheet: the choice in force has a check, and a choice closes the sheet. */
@Composable
private fun CategorySheet(view: CollectionsView<ArtworkTileModel>, filters: CollectionFilters, onDismiss: () -> Unit) {
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.library_collections_category)) {
        Choices(
            all = stringResource(R.string.library_collections_all_categories),
            values = view.choices.mapNotNull { it.choice },
            selected = view.category.takeIf { it != CategoryChoice.All },
            label = { choice -> view.choices.firstOrNull { it.choice == choice }?.category?.title.orEmpty() },
            onSelect = {
                filters.category(it ?: CategoryChoice.All)
                onDismiss()
            },
        )
    }
}

/** The Category chip's label: the category chosen, else "Category". */
@Composable
private fun categoryLabel(view: CollectionsView<ArtworkTileModel>, choice: CategoryChoice): String = when (choice) {
    CategoryChoice.All -> stringResource(R.string.library_collections_category)
    is CategoryChoice.Of -> view.choices.firstOrNull { it.choice == choice }?.category?.title ?: stringResource(R.string.library_collections_category)
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
                digits(LocalResources.current, collection.count),
                TileStyle.Phone,
                onArt = true,
                modifier = Modifier.align(Alignment.TopEnd).padding(6.dp).clearAndSetSemantics { },
            )
        }
        Spacer(Modifier.height(TileStyle.Phone.captionGap))
        TileCaption(model.title, model.subtitle, TileStyle.Phone)
    }
}
