package app.pley.android.ui.phone.requests

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.shell.Placeholder

/**
 * The phone's request screens (Discover, a title found on TMDB, Your
 * requests and the managers' Requests), added to the app's back stacks.
 * They open above Search's and Me's screens, and from a request's notice.
 */
fun EntryProviderScope<NavKey>.requestsEntries() {
    entry<PleyKey.Discover> { key -> WithGraph(key) { PhoneDiscoverScreen(key) } }
    entry<PleyKey.DiscoverTitle> { key -> WithGraph(key) { PhoneDiscoverTitleScreen(key) } }
    entry<PleyKey.Requests> { key -> WithGraph(key) { PhoneRequestsScreen() } }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
