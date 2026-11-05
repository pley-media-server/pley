package app.pley.android.ui.tv.collection

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.collection.CollectionsViewModel
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.screensGraph
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.library.TvMessage
import app.pley.android.ui.tv.library.TvTiles
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * The rail's Collections (design spec §2.1): every movie and show
 * library's collections, a row each in the server's order of libraries,
 * under "Collections". The first collection has focus, or the one focused
 * last when coming back.
 */
@Composable
internal fun TvCollectionsScreen(@Suppress("UNUSED_PARAMETER") key: PleyKey.Collections) {
    val navigator = app.pley.android.ui.tv.shell.LocalTvNavigator.current
    val context = LocalContext.current
    val vm: CollectionsViewModel<ArtworkTileModel> = viewModel {
        CollectionsViewModel(null, LibraryDeps.of(screensGraph()), TvTiles(context))
    }
    val state by vm.state.collectAsStateWithLifecycle()
    val memory = rememberFocusMemory()
    val shelves = state.valueOrNull?.filter { it.collections.valueOrNull?.isNotEmpty() == true }
    Box(Modifier.fillMaxSize()) {
        when {
            shelves == null && state is Loadable.Failed -> TvMessage(
                title = failedTitle((state as Loadable.Failed).error, stringResource(R.string.tv_library_collections_failed)),
                body = failedBody((state as Loadable.Failed).error),
                action = stringResource(R.string.tv_library_retry),
                onAction = vm::retry,
                top = RowsTop,
            )
            shelves != null && shelves.isEmpty() && state.valueOrNull?.all { it.collections !is Loadable.Loading } == true -> TvMessage(
                title = stringResource(R.string.tv_collections_none),
                body = stringResource(R.string.tv_collections_none_body),
                top = RowsTop,
            )
            shelves != null -> TvRowColumn(
                modifier = Modifier.fillMaxSize(),
                headerY = RowsTop,
                contentPadding = PaddingValues(top = RowsTop, bottom = 180.dp),
            ) {
                shelves.forEachIndexed { at, shelf ->
                    val tiles = shelf.collections.valueOrNull.orEmpty()
                    item(key = "library:${shelf.libraryId}", contentType = "shelf") {
                        TvRow(
                            rowKey = "library:${shelf.libraryId}",
                            title = shelf.libraryName,
                            tiles = tiles.map { it.tile },
                            kind = TileKind.Poster,
                            onOpen = { tile -> navigator.open(PleyKey.Collection(tile.id)) },
                            memory = memory,
                            initialFocus = at == 0,
                        )
                    }
                }
            }
        }
        BasicText(
            stringResource(R.string.tv_collections_title),
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
            modifier = Modifier.padding(start = PleyTv.Gutter, top = TvSafeArea.Vertical).semantics { heading() },
        )
    }
}

private val RowsTop = 80.dp
