package app.pley.android.ui.phone.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.data.Loadable
import app.pley.android.data.map
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.collection.CollectionsViewModel
import app.pley.android.screens.library.LibraryDeps
import app.pley.android.screens.screensGraph
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.collection.CollectionScreen
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/**
 * The phone's screens for library pages and collections, added to the
 * app's back stacks: a library's page ([LibraryScreen]), a library's
 * collections, and a collection's page ([CollectionScreen]).
 */
fun EntryProviderScope<NavKey>.libraryEntries() {
    entry<PleyKey.Library> { key -> LibraryScreen(key) }
    entry<PleyKey.Collections> { key -> CollectionsScreen(key) }
    entry<PleyKey.Collection> { key -> CollectionScreen(key) }
}

/** One library's collections on a page of their own: "Collections", then their posters. */
@Composable
private fun CollectionsScreen(key: PleyKey.Collections) {
    val navigator = LocalPhoneNavigator.current
    val tiles = rememberPhoneTiles()
    val vm: CollectionsViewModel<ArtworkTileModel> = viewModel {
        CollectionsViewModel(key.libraryId, LibraryDeps.of(screensGraph()), tiles)
    }
    val state by vm.state.collectAsStateWithLifecycle()
    val shelf = (state as? Loadable.Ready)?.value?.firstOrNull()
    Column(Modifier.fillMaxSize()) {
        PleyTopAppBar(title = shelf?.libraryName ?: stringResource(R.string.collections_title), onBack = navigator::back)
        Box(Modifier.weight(1f).fillMaxWidth()) {
            CollectionsPane(
                collections = shelf?.collections ?: state.map { emptyList() },
                kind = shelf?.kind,
                onRetry = vm::retry,
                skeleton = rememberSkeleton(animated = true),
            )
        }
    }
}
