package app.pley.android.ui.tv.shell

import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.rememberLifecycleOwner
import androidx.navigation3.runtime.NavEntry
import androidx.navigation3.runtime.NavEntryDecorator
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.scene.OverlayScene
import androidx.navigation3.scene.Scene
import androidx.navigation3.scene.SceneStrategy
import androidx.navigation3.scene.SceneStrategyScope

/**
 * The rail's page under the pages opened on it, as the Samsung app keeps
 * its pages: Back to it shows it as it was, at once, focus on the element
 * that had it, rather than composing it again from its saved state.
 *
 * Navigation 3 composes an overlay scene's [OverlayScene.overlaidEntries]
 * under it, so the page on top is one over the back stack's first; the
 * pages between are composed again when Back reaches them, as before.
 * Only the first is kept, so a TV holds two pages at most, one more than
 * before, and a 1 GB stick (T0) as many as any. [RootPage] keeps it out
 * of the way meanwhile. Only while [enabled]: the rail's pages, not
 * signing in.
 */
internal class RootUnderPages(private val enabled: () -> Boolean) : SceneStrategy<NavKey> {
    /**
     * The back stack's first page, by its entry's content key, while this
     * keeps it ([rememberRootPageDecorator]). Every list of entries this
     * is asked about starts with it: the back stack, and the parts of it
     * Navigation 3 asks about next (what a scene covers, or goes back to).
     */
    var rootKey: Any? = null
        private set

    /** The kept page's focus group, while it is composed ([RootPage]). */
    internal var rootFocus: FocusRequester? = null

    /**
     * Before the shell keeps which element of its page has focus (another
     * activity takes the window): the kept page keeps which of its own
     * has it, as the shell's own keeping reaches one level only.
     */
    fun saveFocus() {
        runCatching { rootFocus?.saveFocusedChild() }
    }

    override fun SceneStrategyScope<NavKey>.calculateScene(entries: List<NavEntry<NavKey>>): Scene<NavKey>? {
        val root = entries.firstOrNull()
        rootKey = root?.contentKey.takeIf { enabled() }
        if (root == null || entries.size < 2 || rootKey == null) return null
        val top = entries.last()
        // A page opened from itself shows alone, as every page did.
        if (top.contentKey == root.contentKey) return null
        return PageOverRoot(top, root, entries.dropLast(1))
    }
}

/** [top] over [root] ([RootUnderPages]). */
private data class PageOverRoot(
    val top: NavEntry<NavKey>,
    val root: NavEntry<NavKey>,
    override val previousEntries: List<NavEntry<NavKey>>,
) : OverlayScene<NavKey> {
    override val key: Any get() = top.contentKey
    override val entries: List<NavEntry<NavKey>> = listOf(top)
    override val overlaidEntries: List<NavEntry<NavKey>> = listOf(root)
    override val content: @Composable () -> Unit = { top.Content() }
}

/**
 * Wraps the page [pages] keeps in [RootPage], covered while the back
 * stack ends on another page: one over it, or, while it goes, the next
 * rail page.
 */
@Composable
internal fun rememberRootPageDecorator(backStack: List<NavKey>, pages: RootUnderPages): NavEntryDecorator<NavKey> =
    remember(backStack, pages) {
        NavEntryDecorator { entry ->
            // Decided as the page first shows, and so until it goes: a page that turns into something
            // else as it leaves would be composed again for its last frames.
            val root = remember { if (entry.contentKey == pages.rootKey) backStack.firstOrNull() else null }
            if (root != null) {
                RootPage(covered = backStack.lastOrNull() != root, pages) { entry.Content() }
            } else {
                entry.Content()
            }
        }
    }

/**
 * The rail's page while pages show over it ([covered]): neither laid out
 * nor drawn, out of focus's reach, and stopped (its lifecycle CREATED),
 * so what it collects waits, and so do its Back handlers, which Back on
 * the page over it would reach otherwise. Covered, it lets focus go, as a
 * page that went would (the page over it takes it), keeping which of its
 * elements had it; shown again, by Back or by its rail entry, it gives
 * focus back to that element, or to its first. Shown for the first time
 * (made under another page), it starts on its own initial focus.
 */
@Composable
private fun RootPage(covered: Boolean, pages: RootUnderPages, content: @Composable () -> Unit) {
    val page = remember { FocusRequester() }
    DisposableEffect(pages, page) {
        pages.rootFocus = page
        onDispose { if (pages.rootFocus === page) pages.rootFocus = null }
    }
    val focusManager = LocalFocusManager.current
    val hidden by rememberUpdatedState(covered)
    var focused by remember { mutableStateOf(false) }
    var wasCovered by remember { mutableStateOf(covered) }
    // Whether focus was on it as it was covered, and so goes back to it.
    var kept by remember { mutableStateOf(false) }
    // At once, before the page over it is placed and takes focus. Not forced, so focus leaves as it
    // does by a key and the rows' restorers keep where it was; forced only if something held on to it.
    DisposableEffect(covered) {
        if (covered && focused) {
            kept = true
            runCatching { page.saveFocusedChild() }
            focusManager.clearFocus()
            if (focused) focusManager.clearFocus(force = true)
        }
        onDispose { }
    }
    LaunchedEffect(covered) {
        if (covered) {
            wasCovered = true
        } else if (wasCovered) {
            wasCovered = false
            // Once it is placed again.
            withFrameNanos { }
            if (!kept) {
                // Focus wasn't on it: it was made under the page over it (after process death, or for a
                // Watch Next program), and its own initial focus takes focus as it is first placed.
                // Only when that hasn't, the element that last had focus, or its first.
                withFrameNanos { }
                if (focused) return@LaunchedEffect
            }
            kept = false
            runCatching { if (!page.restoreFocusedChild()) page.requestFocus() }
        }
    }
    val lifecycle = rememberLifecycleOwner(if (covered) Lifecycle.State.CREATED else Lifecycle.State.RESUMED)
    CompositionLocalProvider(LocalLifecycleOwner provides lifecycle) {
        Box(
            Modifier
                .fillMaxSize()
                // Not placed: it costs no frame, and focus searches pass it by.
                .layout { measurable, constraints ->
                    if (hidden) {
                        layout(constraints.maxWidth, constraints.maxHeight) {}
                    } else {
                        val placeable = measurable.measure(constraints)
                        layout(placeable.width, placeable.height) { placeable.place(0, 0) }
                    }
                }
                // Nor read out while covered.
                .then(if (covered) Modifier.clearAndSetSemantics { } else Modifier)
                .onFocusChanged { focused = it.hasFocus }
                .focusRequester(page)
                .focusProperties {
                    // A request for focus inside it (its initial focus, say) waits until it shows.
                    onEnter = { if (hidden) cancelFocusChange() }
                    // Whatever focus leaves it for (the rail, a panel), it keeps which element had it, as
                    // the page's own group did before this one stood between it and the shell.
                    onExit = { runCatching { page.saveFocusedChild() } }
                }
                .focusGroup(),
        ) {
            content()
        }
    }
}
