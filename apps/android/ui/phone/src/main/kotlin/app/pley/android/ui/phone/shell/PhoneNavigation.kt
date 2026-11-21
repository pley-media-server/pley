package app.pley.android.ui.phone.shell

import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableIntState
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.navigation3.runtime.NavBackStack
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.rememberNavBackStack
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey

/**
 * The phone's places and how they stack (architecture.md, "Navigation and
 * state"), as screens move through them ([Navigator]).
 *
 * Getting in (Servers, signing in) is one full-screen back stack, [root].
 * Once in, [root] holds just Home and each tab of the navigation bar keeps
 * its own back stack, so an item opened from Search is still there after
 * a look at Home. Every stack is saved with the activity (rotation,
 * process death); restarting (signing in or out) starts them over.
 */
@Stable
class PhoneNavigation internal constructor(
    internal val root: NavBackStack<NavKey>,
    private val selected: MutableState<PhoneTab>,
    private val generationState: MutableIntState,
    private val exit: () -> Unit,
) : Navigator {
    /** The tab shown while in. */
    val tab: PhoneTab get() = selected.value

    /** Whether the person is in (the tabs), rather than getting in. */
    val inside: Boolean get() = root.size == 1 && root[0] == PleyKey.Home

    /** Starts over with each change, so the tabs' stacks begin again after signing in. */
    internal val generation: Int get() = generationState.intValue

    /** The tabs' stacks while they are shown; set by the tabs' host. */
    internal var stacks: Map<PhoneTab, NavBackStack<NavKey>>? by mutableStateOf(null)

    /**
     * How many Now Playing pages are drawn: two for a moment, as one tab fades into another that has its own. The mini
     * player waits for one before it leaves: gone as soon as Now Playing was asked for, it left the page under it bare
     * for as long as Now Playing took to come.
     */
    internal var nowPlayingsDrawn: Int by mutableIntStateOf(0)

    /** Whether a Now Playing page has drawn ([nowPlayingsDrawn]). */
    internal val nowPlayingDrawn: Boolean get() = nowPlayingsDrawn > 0

    /** A screen to open above Home once the tabs' stacks exist (a restart onto it). */
    private var pending: PleyKey? = null

    /** The stack of [tab], if the tabs are shown. */
    internal fun stack(tab: PhoneTab = this.tab): NavBackStack<NavKey>? = stacks?.get(tab)

    /** Shows [tab]; choosing the tab already shown takes it back to its first screen. */
    fun select(tab: PhoneTab) {
        if (tab == this.tab) {
            stack(tab)?.let { stack -> while (stack.size > 1) stack.removeAt(stack.lastIndex) }
        } else {
            selected.value = tab
        }
    }

    override fun open(key: PleyKey) {
        if (key.isGettingIn) {
            if (inside) restart(key) else root.add(key)
            return
        }
        if (!inside) {
            // Who's watching? is a step of getting in; anything else means the person is in.
            if (key is PleyKey.Profiles) root.add(key) else restart(key)
            return
        }
        val tab = PhoneTab.rootOf(key)
        when {
            tab == null -> stack()?.add(key)
            key is PleyKey.Search && key.query.isNotEmpty() -> {
                selected.value = tab
                stack(tab)?.let { stack ->
                    Snapshot.withMutableSnapshot {
                        stack.clear()
                        stack.add(key)
                    }
                }
            }
            else -> select(tab)
        }
    }

    override fun back() {
        if (!inside) {
            if (root.size > 1) root.removeAt(root.lastIndex) else exit()
            return
        }
        val stack = stack()
        when {
            stack != null && stack.size > 1 -> stack.removeAt(stack.lastIndex)
            tab != PhoneTab.Home -> selected.value = PhoneTab.Home
            else -> exit()
        }
    }

    override fun restart(key: PleyKey) {
        Snapshot.withMutableSnapshot {
            if (key.isGettingIn) {
                root.clear()
                root.add(key)
                return@withMutableSnapshot
            }
            val tab = PhoneTab.rootOf(key)
            root.clear()
            root.add(PleyKey.Home)
            selected.value = tab ?: PhoneTab.Home
            pending = if (tab == null) key else null
            stacks = null
            generationState.intValue += 1
        }
    }

    /** The tabs' host made its stacks: keep them, and open what a restart asked for. */
    internal fun attach(made: Map<PhoneTab, NavBackStack<NavKey>>) {
        stacks = made
        pending?.let { key ->
            pending = null
            made.getValue(tab).add(key)
        }
    }

    internal fun detach(made: Map<PhoneTab, NavBackStack<NavKey>>) {
        if (stacks === made) stacks = null
    }
}

/**
 * The navigation, starting at [start] (the start gate's choice) the first
 * time and restored after that. [exit] leaves the app (Back from Home).
 */
@Composable
fun rememberPhoneNavigation(start: PleyKey, exit: () -> Unit): PhoneNavigation {
    val root = rememberNavBackStack(if (start.isGettingIn) start else PleyKey.Home)
    val tab = rememberSaveable { mutableStateOf(PhoneTab.Home) }
    val generation = rememberSaveable { mutableIntStateOf(0) }
    val leave by rememberUpdatedState(exit)
    return remember(root, tab, generation) { PhoneNavigation(root, tab, generation) { leave() } }
}

/** The phone's [Navigator], for the screens' entries. */
val LocalPhoneNavigator = staticCompositionLocalOf<Navigator> { error("No phone navigation here") }
