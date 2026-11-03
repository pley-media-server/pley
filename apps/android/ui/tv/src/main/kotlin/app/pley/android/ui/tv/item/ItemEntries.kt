package app.pley.android.ui.tv.item

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.person.TvPersonScreen
import app.pley.android.ui.tv.shell.Placeholder

/**
 * The TV's item and person pages and an item's actions (Item, Person,
 * ItemActions), added to the app's back stack. Tiles show their actions
 * in a side panel over the page; ItemActions is their screen for what
 * opens them by key.
 */
fun EntryProviderScope<NavKey>.itemEntries() {
    entry<PleyKey.Item> { key -> WithGraph(key) { TvItemScreen(key.itemId) } }
    entry<PleyKey.Person> { key -> WithGraph(key) { TvPersonScreen(key.personId) } }
    entry<PleyKey.ItemActions> { key -> WithGraph(key) { TvItemActionsScreen(key.itemId, key.continueWatching) } }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
