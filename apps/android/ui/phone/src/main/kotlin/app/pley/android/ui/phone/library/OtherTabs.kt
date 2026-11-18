package app.pley.android.ui.phone.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.screens.PleyKey
import app.pley.android.model.HubKeys
import app.pley.android.screens.library.LibraryUiState
import app.pley.android.screens.library.LibraryViewModel
import app.pley.android.screens.library.TileLook
import app.pley.android.screens.library.TileRow
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.HubRow
import app.pley.android.ui.phone.kit.HubRowModel
import app.pley.android.ui.phone.kit.PRESS_ROOM
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/**
 * The Recommended tab (apps/web's RecommendedTab): the library's hubs as
 * rows (Continue Watching as stills, the rest as posters). A row's title
 * opens the collection it shows.
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
            val models = remember(rows) {
                rows?.map { it.model() } ?: List(2) { HubRowModel("loading$it", "", TileKind.Poster, null) }
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
                        onLongPress = { tile ->
                            navigator.open(PleyKey.ItemActions(tile.id, continueWatching = HubKeys.isContinueWatching(model.key)))
                        },
                        onTitleClick = row?.collection?.let { key -> { navigator.open(key) } }
                            ?: row?.seeAll?.let { view -> { vm.seeAll(view) } },
                    )
                }
            }
        }
    }
}

private fun TileRow<ArtworkTileModel>.model() = HubRowModel(
    key = key,
    title = title,
    kind = when (look) {
        TileLook.Landscape -> TileKind.Landscape
        TileLook.Square -> TileKind.Square
        else -> TileKind.Poster
    },
    items = tiles,
)
