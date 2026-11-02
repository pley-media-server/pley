package app.pley.android.ui.tv.requests

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.DiscoverSource
import app.pley.android.ui.tv.shell.Placeholder

/**
 * The TV's requests (Discover, a list's or search's grid of TMDB's
 * titles, a title's page, the requests), added to the app's back stack.
 */
fun EntryProviderScope<NavKey>.requestsEntries() {
    entry<PleyKey.Discover> { key -> WithGraph(key) { TvDiscoverScreen() } }
    entry<PleyKey.DiscoverList> { key -> WithGraph(key) { TvDiscoverGridScreen(DiscoverSource.Row(key.list)) } }
    entry<PleyKey.DiscoverResults> { key -> WithGraph(key) { TvDiscoverGridScreen(DiscoverSource.Words(key.query)) } }
    entry<PleyKey.DiscoverTitle> { key -> WithGraph(key) { TvDiscoverTitleScreen(key) } }
    entry<PleyKey.Requests> { key -> WithGraph(key) { TvRequestsScreen() } }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
