package app.pley.android.ui.phone.music

import androidx.compose.animation.EnterExitState
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.ui.LocalNavAnimatedContentScope
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.PhoneNavigation
import app.pley.android.ui.phone.shell.Placeholder
import app.pley.android.ui.phone.shell.RISES

/**
 * The phone's artist, album and book pages and Now Playing (Artist,
 * Album, Book, NowPlaying), added to the app's back stacks. Now Playing opens over the
 * tab it was opened from, coming up from the mini player, which hides
 * while it shows.
 */
fun EntryProviderScope<NavKey>.musicEntries() {
    entry<PleyKey.Artist> { key -> WithGraph(key) { ArtistScreen(key.itemId) } }
    entry<PleyKey.Album> { key -> WithGraph(key) { AlbumScreen(key.itemId, key.trackId) } }
    entry<PleyKey.Book> { key -> WithGraph(key) { BookScreen(key.itemId) } }
    entry<PleyKey.NowPlaying>(metadata = mapOf(RISES to true)) { key ->
        TellsItHasDrawn()
        HeldWhileLeaving { WithGraph(key) { NowPlayingScreen() } }
    }
}

/**
 * [content] at the height it had while it leaves: closing Now Playing
 * brings the mini player back at once under it, which would take the
 * bar's height off the page as it fades out and lay it out again, its
 * cover smaller and its controls moving (or, on a tall car's screen, in
 * its other layout). It keeps its height, top-aligned, and fades out over
 * the bar.
 */
@Composable
private fun HeldWhileLeaving(content: @Composable () -> Unit) {
    val transition = LocalNavAnimatedContentScope.current.transition
    val held = remember { IntArray(1) }
    Box(
        Modifier.layout { measurable, constraints ->
            val leaving = transition.targetState == EnterExitState.PostExit
            if (!leaving && constraints.hasBoundedHeight) held[0] = constraints.maxHeight
            val height = if (leaving && held[0] > 0) held[0] else constraints.maxHeight
            val placeable = measurable.measure(constraints.copy(minHeight = height, maxHeight = height))
            layout(placeable.width, constraints.constrainHeight(height)) { placeable.placeRelative(0, 0) }
        },
    ) { content() }
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
