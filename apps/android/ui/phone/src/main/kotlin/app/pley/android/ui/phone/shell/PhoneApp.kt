package app.pley.android.ui.phone.shell

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.ReportDrawn
import androidx.activity.compose.ReportDrawnWhen
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.ShortNavigationBarItemDefaults
import androidx.compose.material3.WideNavigationRailDefaults
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteDefaults
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteItem
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteScaffold
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteType
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import androidx.navigation3.runtime.NavBackStack
import androidx.navigation3.runtime.NavEntry
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberDecoratedNavEntries
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.scene.DialogSceneStrategy
import androidx.navigation3.scene.Scene
import androidx.navigation3.ui.NavDisplay
import app.pley.android.common.TestTags
import app.pley.android.common.perf.FrameRate
import app.pley.android.common.perf.StartupSignals
import app.pley.android.data.SessionState
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.CastFrom
import app.pley.android.screens.PleyKey
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.accountEntries
import app.pley.android.ui.phone.home.homeEntries
import app.pley.android.ui.phone.item.itemEntries
import app.pley.android.ui.phone.kit.FittedLabel
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.kit.LocalOfflineAction
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.OfflineAction
import app.pley.android.ui.phone.kit.ToastHost
import app.pley.android.ui.phone.library.libraryEntries
import app.pley.android.ui.phone.music.musicEntries
import app.pley.android.ui.phone.onboarding.onboardingEntries
import app.pley.android.ui.phone.livetv.liveTvEntries
import app.pley.android.ui.phone.playlists.playlistsEntries
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.CastsSent
import app.pley.android.ui.phone.remote.LocalPlayOn
import app.pley.android.ui.phone.remote.PlayOnHost
import app.pley.android.ui.phone.remote.PlayOnSheet
import app.pley.android.ui.phone.remote.RemoteSlot
import app.pley.android.ui.phone.requests.RequestsTabIcon
import app.pley.android.ui.phone.requests.requestsEntries
import app.pley.android.ui.phone.search.searchEntries
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The phone and tablet app (design spec §3.1, architecture.md).
 *
 * The first screen comes from [start], read from this device before the
 * first frame (no server: Servers; no tokens: signing in; otherwise
 * Home, drawn from what was saved). When the read hasn't finished by the
 * time the splash screen gives up, [awaitStart] waits for it behind a
 * blank page. Nothing touches the network, the Keystore or the [graph]'s
 * data before the first frame; after it, the shell listens for sign-outs,
 * follows live updates while the app is started, and warms up the
 * capability probe.
 *
 * Once in, the navigation bar (a rail from medium widths) switches
 * between Home, Libraries, Search, Playlists and Me, each with its own
 * saved back stack. Getting in is full screen.
 *
 * The window's background (the app's theme) is the page's colour, so
 * nothing here paints the whole screen again: each such layer is another
 * full screen of pixels on every frame. Test tags are resource ids as well,
 * which the benchmarks' journeys find things by ([TestTags]).
 */
@Composable
fun PhoneApp(
    start: PhoneStart?,
    awaitStart: suspend () -> PhoneStart,
    graph: () -> ScreensGraph,
) {
    PhoneTheme {
        Box(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }.testTag(TestTags.PHONE_ROOT)) {
            val known by produceState(start) { if (value == null) value = awaitStart() }
            known?.let { PhoneShell(it, graph) }
        }
    }
}

@Composable
internal fun PhoneShell(
    start: PhoneStart,
    graph: () -> ScreensGraph,
    onReady: (PhoneShellState, PhoneNavigation) -> Unit = { _, _ -> },
) {
    val activity = LocalActivity.current
    val first = remember { startKey(start) }
    val navigation = rememberPhoneNavigation(first) { activity?.finish() }
    val resources = LocalResources.current
    val shell = remember(navigation) { PhoneShellState(navigation, resources, start.server) }
    val graphs by rememberUpdatedState(graph)
    LaunchedEffect(shell) { onReady(shell, navigation) }
    // The screen the frame-rate counter's lines name, read outside composition.
    LaunchedEffect(navigation) {
        snapshotFlow { if (navigation.inside) navigation.stack()?.lastOrNull() else navigation.root.lastOrNull() }
            .collect { FrameRate.route = it }
    }
    AfterFirstFrame(shell, graphs)

    if (first == app.pley.android.screens.PleyKey.Home) {
        val homeDrawn by StartupSignals.homeDrawn.collectAsStateWithLifecycle()
        ReportDrawnWhen { homeDrawn }
    } else {
        ReportDrawn()
    }

    val tier = remember { graph().device.tier }
    val entries = remember { shellEntries() }
    val downloadsLabel = stringResource(R.string.downloads_offline_action)
    val offlineAction = remember(navigation, downloadsLabel) { OfflineAction(downloadsLabel) { navigation.open(PleyKey.Downloads) } }
    val playOn = remember(shell) {
        object : PlayOnHost {
            override val devices get() = shell.remoteDevices

            override fun playOn(target: CastTarget) = shell.playOn(target)
        }
    }
    CompositionLocalProvider(
        LocalPhoneNavigator provides navigation,
        LocalPhoneShell provides shell,
        LocalPlayOn provides playOn,
        LocalOffline provides shell.offlineState,
        LocalOfflineAction provides offlineAction.takeIf { navigation.inside },
        LocalPhoneDeviceTier provides tier,
    ) {
        if (navigation.inside) {
            key(navigation.generation) { Tabs(navigation, shell, entries) }
        } else {
            GettingIn(navigation, shell, entries)
        }
    }
}

/**
 * Work that waits for the first frame: the sign-in module's events
 * (signed out by the server) and the session (signed out before the shell
 * listened, as when this device can't read its tokens anymore), the
 * server in use, live updates while the app is started, the music
 * playing, what plays on the profile's other devices (the remote), what
 * they cast here and what the live player sent them, requests (and their
 * news, told while the app shows), and the capability probe. The graph's
 * parts are made off the main thread.
 */
@Composable
private fun AfterFirstFrame(shell: PhoneShellState, graph: () -> ScreensGraph) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val resources = LocalResources.current
    LaunchedEffect(shell) {
        withFrameNanos { }
        val g = withContext(Dispatchers.Default) { graph() }
        launch {
            val events = withContext(Dispatchers.Default) { g.auth.events }
            events.collect(shell::on)
        }
        launch {
            val session = withContext(Dispatchers.Default) { g.data.session.state }
            session.collect { if (it is SessionState.SignedOut) shell.signedOut(it.issuer) }
        }
        launch {
            val active = withContext(Dispatchers.Default) { g.data.servers.active }
            active.collect { server -> if (server != null) shell.server = server }
        }
        launch {
            val scans = withContext(Dispatchers.Default) { g.data.liveUpdates.scans }
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                scans.collect { shell.scanning = it.isNotEmpty() }
            }
        }
        launch {
            val music = withContext(Dispatchers.Default) { g.player.music.nowPlaying }
            music.collect { shell.nowPlaying = it }
        }
        launch {
            // The profile's other devices, for the remote: read while the app shows.
            val remote = withContext(Dispatchers.Default) { g.data.remote }
            shell.remote = remote
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                remote.devices.collect { shell.remoteDevices = it }
            }
        }
        launch {
            // What the profile's remotes cast here plays from now on: music says where it came from here, the
            // players say it themselves.
            val casts = withContext(Dispatchers.Default) { g.casts }
            casts.start()
            // One from another profile that couldn't switch this phone to it plays nothing: said here too.
            launch { casts.refused.collect { shell.toasts.show(CastFrom.switchFailed(resources)) } }
            casts.music.collect { shell.toasts.show(it.from.notice(resources)) }
        }
        launch {
            // Sent from the live player, which closed: said here, and the remote opens on the device.
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                CastsSent.pending.filterNotNull().collect { sent ->
                    CastsSent.said(sent)
                    shell.toasts.show(resources.getString(R.string.play_on_sent, sent.name))
                    shell.openRemote(sent.deviceId)
                }
            }
        }
        launch {
            // The server's events connected again: it answers, and the offline banner goes.
            val answered = withContext(Dispatchers.Default) { g.data.liveUpdates.answered }
            answered.collect { shell.answered() }
        }
        launch {
            // Requests' news, told once in and while the app shows: video plays in its own activity, so never over it.
            val requests = withContext(Dispatchers.Default) { g.data.requests.also { it.start() } }
            launch { requests.state.collect { shell.requests = RequestsEntry.of(it) } }
            val told = HashSet<Long>()
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                combine(requests.notices, snapshotFlow { shell.inside }) { notices, inside -> if (inside) notices else emptyList() }
                    .collect { notices ->
                        notices.forEach { notice ->
                            if (told.add(notice.key)) shell.tell(notice)
                            requests.dismiss(notice)
                        }
                    }
            }
        }
        launch(Dispatchers.Default) {
            runCatching { g.caps.capabilities.warmUp() }
        }
    }
}

/** Every screen the phone has, a placeholder for those not built yet, sheets marked as sheets. */
private fun shellEntries(): (NavKey) -> NavEntry<NavKey> {
    val provider = entryProvider(fallback = { key -> NavEntry(key) { Placeholder(it) } }) {
        onboardingEntries()
        homeEntries()
        libraryEntries()
        itemEntries()
        searchEntries()
        playlistsEntries()
        liveTvEntries()
        accountEntries()
        musicEntries()
        requestsEntries()
    }
    return { key -> asSheetIfOne(key, provider(key)) }
}

/** The steps of getting in, full screen, one back stack. */
@Composable
private fun GettingIn(navigation: PhoneNavigation, shell: PhoneShellState, entries: (NavKey) -> NavEntry<NavKey>) {
    Box(Modifier.fillMaxSize()) {
        Screens(
            entries = rememberDecoratedNavEntries(navigation.root, rememberDecorators(), entries),
            onBack = navigation::back,
        )
        ToastHost(
            shell.toasts,
            Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal)),
        )
    }
}

/** In: the navigation bar or rail, each tab's back stack, the remote's bar, the mini player and the toasts above the bar. */
@Composable
private fun Tabs(navigation: PhoneNavigation, shell: PhoneShellState, entries: (NavKey) -> NavEntry<NavKey>) {
    val stacks: Map<PhoneTab, NavBackStack<NavKey>> = PhoneTab.entries.associateWith { rememberNavBackStack(it.root) }
    DisposableEffect(stacks) {
        navigation.attach(stacks)
        onDispose { navigation.detach(stacks) }
    }
    val decorated: Map<PhoneTab, List<NavEntry<NavKey>>> = PhoneTab.entries.associateWith { tab ->
        rememberDecoratedNavEntries(stacks.getValue(tab), rememberDecorators(), entries)
    }
    val metrics = LocalPhoneMetrics.current
    val rail = metrics.rail
    val tab = navigation.tab
    val itemColors = ShortNavigationBarItemDefaults.colors(
        selectedIconColor = PleyColors.AccentBright,
        selectedTextColor = PleyColors.Fg,
        // No pill behind the chosen tab: its accent icon and bright label say it.
        selectedIndicatorColor = Color.Transparent,
        unselectedIconColor = PleyColors.Muted,
        unselectedTextColor = PleyColors.Muted,
    )
    NavigationSuiteScaffold(
        navigationItems = {
            PhoneTab.entries.forEach { item ->
                NavigationSuiteItem(
                    selected = item == tab,
                    onClick = { navigation.select(item) },
                    icon = {
                        val icon = @Composable { Icon(item.icon, contentDescription = null, tint = LocalContentColor.current) }
                        if (item == PhoneTab.Me) RequestsTabIcon(shell.requests, icon) else icon()
                    },
                    label = { FittedLabel(stringResource(item.label)) },
                    colors = itemColors,
                    modifier = Modifier.testTag(TAB_TAG + item.name),
                )
            }
        },
        navigationSuiteType = if (rail) NavigationSuiteType.WideNavigationRailCollapsed else NavigationSuiteType.ShortNavigationBarCompact,
        navigationSuiteColors = NavigationSuiteDefaults.colors(
            shortNavigationBarContainerColor = PleyColors.Bg,
            shortNavigationBarContentColor = PleyColors.Muted,
            wideNavigationRailColors = WideNavigationRailDefaults.colors(
                containerColor = PleyColors.Bg,
                contentColor = PleyColors.Muted,
            ),
        ),
        // The window's background shows through (PhoneApp).
        containerColor = Color.Transparent,
        contentColor = PleyColors.Fg,
    ) {
        val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
        Column(
            Modifier
                .fillMaxSize()
                .drawWithContent {
                    drawContent()
                    // The 1 dp hairline between the page and the bar (or the rail).
                    val hair = 1.dp.toPx()
                    if (rail) {
                        val x = if (rtl) size.width - hair / 2f else hair / 2f
                        drawLine(PleyColors.Surface4, Offset(x, 0f), Offset(x, size.height), hair)
                    } else {
                        drawLine(PleyColors.Surface4, Offset(0f, size.height - hair / 2f), Offset(size.width, size.height - hair / 2f), hair)
                    }
                },
        ) {
            Box(Modifier.weight(1f).fillMaxWidth()) {
                key(tab) {
                    Screens(entries = decorated.getValue(tab), onBack = navigation::back)
                }
                ToastHost(
                    shell.toasts,
                    Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom)),
                )
            }
            RemoteSlot(shell.remoteDevices, shell.remote, shell.server, shell.remoteAsked)
            MiniPlayerSlot(shell.nowPlaying)
        }
    }
    PlayOnSlot(shell)
    // Back from another tab's first screen goes to Home, then out of the app.
    BackHandler(enabled = tab != PhoneTab.Home && stacks.getValue(tab).size == 1) { navigation.select(PhoneTab.Home) }
}

/**
 * Play on…'s chooser while a page asked for it: sent, it closes, says
 * "Playing on" the device, and opens the remote on that device.
 */
@Composable
private fun PlayOnSlot(shell: PhoneShellState) {
    val target = shell.playingOn ?: return
    val remote = shell.remote ?: return
    val resources = LocalResources.current
    PlayOnSheet(
        target,
        shell.remoteDevices,
        remote,
        onCast = { device, name ->
            shell.playingOn = null
            shell.toasts.show(resources.getString(R.string.play_on_sent, name))
            shell.openRemote(device.deviceId)
        },
        onDismiss = { shell.playingOn = null },
    )
}

@Composable
private fun rememberDecorators() = listOf(
    rememberSaveableStateHolderNavEntryDecorator<NavKey>(),
    rememberViewModelStoreNavEntryDecorator<NavKey>(),
)

/**
 * One back stack's screens: sheets over the screen under them, dialogs as
 * dialogs, and Material's shared-axis X between screens (design spec §5.1),
 * following the finger on a predictive back. With reduced motion screens
 * swap in place.
 */
@Composable
private fun Screens(entries: List<NavEntry<NavKey>>, onBack: () -> Unit) {
    val reduced = reducedMotion()
    val strategies = remember { listOf(SheetSceneStrategy(), DialogSceneStrategy<NavKey>()) }
    NavDisplay(
        entries = entries,
        modifier = Modifier.fillMaxSize(),
        sceneStrategies = strategies,
        transitionSpec = { sharedAxis(forward = true, reduced) },
        popTransitionSpec = { sharedAxis(forward = false, reduced) },
        predictivePopTransitionSpec = { sharedAxis(forward = false, reduced) },
        onBack = onBack,
    )
}

/** Material's shared axis on X in pley's 220 ms: the old screen fades out over the first third, the new one in after it, both sliding 30 dp. */
private fun AnimatedContentTransitionScope<Scene<NavKey>>.sharedAxis(forward: Boolean, reduced: Boolean): ContentTransform {
    if (reduced) return EnterTransition.None togetherWith ExitTransition.None
    val move = tween<androidx.compose.ui.unit.IntOffset>(PleyMotion.SCREEN_MS, easing = FastOutSlowInEasing)
    val out = PleyMotion.SCREEN_MS / 3
    val sign = if (forward) 1 else -1
    val shift = { width: Int -> (width * SHIFT).toInt() }
    return (slideInHorizontally(move) { sign * shift(it) } + fadeIn(tween(PleyMotion.SCREEN_MS - out, delayMillis = out, easing = LinearEasing))) togetherWith
        (slideOutHorizontally(move) { -sign * shift(it) } + fadeOut(tween(out, easing = LinearEasing)))
}

/** 30 dp of a 412 dp phone, as a share of the width, so it needs no density here. */
private const val SHIFT = 0.073f

/** The test tag of each tab's item in the bar or rail: this and the tab's name. */
internal const val TAB_TAG = "pley.phone.tab."
