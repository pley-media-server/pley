package app.pley.android.ui.phone.music

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.PhoneNavigation
import app.pley.android.ui.phone.shell.Placeholder
import app.pley.android.ui.phone.shell.RISES

/**
 * The phone's artist and album pages and Now Playing (Artist, Album,
 * NowPlaying), added to the app's back stacks. Now Playing opens over the
 * tab it was opened from, coming up from the mini player, which hides
 * while it shows.
 */
fun EntryProviderScope<NavKey>.musicEntries() {
    entry<PleyKey.Artist> { key -> WithGraph(key) { ArtistScreen(key.itemId) } }
    entry<PleyKey.Album> { key -> WithGraph(key) { AlbumScreen(key.itemId, key.trackId) } }
    entry<PleyKey.NowPlaying>(metadata = mapOf(RISES to true)) { key ->
        TellsItHasDrawn()
        WithGraph(key) { NowPlayingScreen() }
    }
}

/**
 * Tells the shell Now Playing has drawn, and when it has gone: the mini player leaves as it comes, not before. Counted,
 * as another tab's Now Playing fading out goes after this one has come.
 */
@Composable
private fun TellsItHasDrawn() {
    val navigation = LocalPhoneNavigator.current as? PhoneNavigation ?: return
    DisposableEffect(navigation) {
        navigation.nowPlayingsDrawn++
        onDispose { navigation.nowPlayingsDrawn-- }
    }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
