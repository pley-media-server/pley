package app.pley.android.ui.tv.music

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.shell.Placeholder

/**
 * The TV's screens for artists, albums, books and Now Playing (Artist,
 * Album, Book, NowPlaying), added to the app's back stack. Music plays on behind other
 * screens (a media session service); video never does.
 */
fun EntryProviderScope<NavKey>.musicEntries() {
    entry<PleyKey.Artist> { key -> WithGraph(key) { TvArtistScreen(key.itemId) } }
    entry<PleyKey.Album> { key -> WithGraph(key) { TvAlbumScreen(key.itemId, key.trackId) } }
    entry<PleyKey.Book> { key -> WithGraph(key) { TvBookScreen(key.itemId) } }
    entry<PleyKey.NowPlaying> { key -> WithGraph(key) { TvNowPlayingScreen() } }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
