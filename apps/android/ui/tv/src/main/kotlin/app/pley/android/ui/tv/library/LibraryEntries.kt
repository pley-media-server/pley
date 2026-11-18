package app.pley.android.ui.tv.library

import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.collection.TvCollectionScreen
import app.pley.android.ui.tv.collection.TvCollectionsScreen
import app.pley.android.ui.tv.collection.TvMergedCollectionScreen

/**
 * The TV's screens for library pages and collections, added to the app's
 * back stack: a library's page ([TvLibraryScreen]), the rail's
 * Collections ([TvCollectionsScreen]), a collection's page
 * ([TvCollectionScreen]) and a Home row's collections of one title
 * together ([TvMergedCollectionScreen]).
 */
fun EntryProviderScope<NavKey>.libraryEntries() {
    entry<PleyKey.Library> { key -> TvLibraryScreen(key) }
    entry<PleyKey.Collections> { key -> TvCollectionsScreen(key) }
    entry<PleyKey.Collection> { key -> TvCollectionScreen(key) }
    entry<PleyKey.MergedCollection> { key -> TvMergedCollectionScreen(key) }
}
