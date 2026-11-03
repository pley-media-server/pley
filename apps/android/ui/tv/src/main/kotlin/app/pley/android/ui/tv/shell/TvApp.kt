package app.pley.android.ui.tv.shell

import androidx.activity.compose.BackHandler
import androidx.activity.compose.ReportDrawn
import androidx.activity.compose.ReportDrawnWhen
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import androidx.navigation3.runtime.NavEntry
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.scene.SinglePaneSceneStrategy
import androidx.navigation3.ui.NavDisplay
import app.pley.android.common.TestTags
import app.pley.android.common.Tier
import app.pley.android.common.perf.StartupSignals
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.reducedMotion
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.account.accountEntries
import app.pley.android.ui.tv.home.homeEntries
import app.pley.android.ui.tv.item.LocalTvTileActions
import app.pley.android.ui.tv.item.TvTileActionsPanel
import app.pley.android.ui.tv.item.TvTileActionsState
import app.pley.android.ui.tv.item.itemEntries
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvScrolling
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.library.libraryEntries
import app.pley.android.ui.tv.music.musicEntries
import app.pley.android.ui.tv.onboarding.onboardingEntries
import app.pley.android.ui.tv.livetv.liveTvEntries
import app.pley.android.ui.tv.playlists.playlistsEntries
import app.pley.android.ui.tv.search.searchEntries
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filterNotNull

/**
 * The TV app (Android TV, Google TV, Fire TV): the theme, the D-pad rules
 * that hold everywhere (the held-key gate, where scrolling puts focus),
 * the start gate, the rail and the one back stack whose screens each area
 * adds (its <Area>Entries). Screens not built yet show [placeholder].
 *
 * [tier] is the device's ([app.pley.android.common.DeviceProfile]); [onExit]
 * leaves the app, the last step of Back. [opened] is an item the home
 * screen's Watch Next row asked for: once the app is past signing in and
 * "Who's watching?", its page opens and [onOpened] plays it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TvApp(
    source: TvShellSource,
    onExit: () -> Unit,
    tier: Tier = Tier.T2,
    reducedMotion: Boolean? = null,
    placeholder: @Composable (NavKey) -> Unit = { Placeholder(it) },
    opened: Flow<Long?> = emptyFlow(),
    onOpened: (Long) -> Unit = {},
) {
    // Settings → Display → Reduce motion, on top of the system's "Remove animations".
    val chosen by source.reduceMotion.collectAsStateWithLifecycle(false)
    TvTheme(tier, reducedMotion ?: true.takeIf { chosen }) {
        val gate = remember { KeyRepeatGate() }
        val root = remember { TvScrolling.root() }
        CompositionLocalProvider(LocalKeyRepeatGate provides gate, LocalBringIntoViewSpec provides root) {
            // No background: the window's is pley's (one full-screen fill, not two).
            Box(Modifier.fillMaxSize().keyRepeatGate(gate).testTag(TestTags.TV_ROOT)) {
                // Known already (read as the activity started), the first screen is in the first frame.
                val start by produceState(source.startFactsNow()?.let(::startKey), source) {
                    if (value == null) value = startKey(source.startFacts())
                }
                start?.let { TvShell(it, source, onExit, placeholder, opened, onOpened) }
            }
        }
        KeepUpdated(source)
    }
}

@Composable
private fun TvShell(
    start: PleyKey,
    source: TvShellSource,
    onExit: () -> Unit,
    placeholder: @Composable (NavKey) -> Unit,
    opened: Flow<Long?>,
    onOpened: (Long) -> Unit,
) {
    val backStack = rememberNavBackStack(start)
    val libraries by source.libraries.collectAsStateWithLifecycle(emptyList())
    val profile by source.profile.collectAsStateWithLifecycle(null)
    // Read after the first frame, as the phone's mini player is: music never holds up the start.
    val music by produceState(false, source) {
        withFrameNanos { }
        source.music.collect { value = it }
    }
    // Live TV too: its summary is a snapshot, read in a moment.
    val liveTv by produceState(false, source) {
        withFrameNanos { }
        source.liveTv.collect { value = it }
    }
    val items = remember(libraries, music, liveTv) { railItems(libraries, nowPlaying = music, liveTv = liveTv) }
    val content = remember { FocusRequester() }
    val rail = remember { RailState(content) }
    val offline = remember { TvOfflineState() }
    val navigator = remember(backStack) { TvNavigator(backStack, rail, onExit) }
    val tileActions = remember(backStack) { TvTileActionsState(content) { backStack.lastOrNull() } }
    val pages = remember(rail) { RootUnderPages { rail.shown } }
    val root = navigator.root ?: start
    rail.current = railCurrent(root, items + SettingsItem)
    // Signing in and choosing a profile are full screen.
    rail.shown = root !is PleyKey.Servers && root !is PleyKey.Link && root !is PleyKey.SignIn && root !is PleyKey.Profiles
    val curtain = rememberCurtain(backStack.lastOrNull())
    ReturnFocus(rail, content, pages)
    // The app is fully drawn (Android's time to full display) once Home has its
    // rows and pictures; a start elsewhere (sign-in, "Who's watching?") is at once.
    if (start == PleyKey.Home) {
        val homeDrawn by StartupSignals.homeDrawn.collectAsStateWithLifecycle()
        ReportDrawnWhen { homeDrawn }
    } else {
        ReportDrawn()
    }
    // The server's events connected again: it answers, and the offline mark goes.
    LaunchedEffect(source, offline) { source.serverAnswered.collect { offline.answered() } }
    // A refused sign-in (the server ended it, or forgot it): that server's sign-in again, as on phones.
    LaunchedEffect(source, navigator) {
        source.signedOut.collect { issuer -> if (rail.shown) navigator.restart(PleyKey.Link(issuer)) }
    }

    // A Watch Next program: its page, and it plays, once past signing in and choosing a profile.
    LaunchedEffect(opened, navigator) {
        combine(opened, snapshotFlow { rail.shown }) { id, shown -> id.takeIf { shown } }.filterNotNull().collect { id ->
            navigator.open(PleyKey.Item(id))
            onOpened(id)
        }
    }

    // The last resort of Back: below every screen's own (they are added later, so they come first).
    BackHandler { navigator.back() }

    // The rail's page again: back to where focus was on it. Another page: its own initial focus.
    val select: (PleyKey) -> Unit = { key ->
        if (navigator.root == key && backStack.size == 1) rail.backToContent() else navigator.restart(key)
    }
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier
                .fillMaxSize()
                .focusRequester(content)
                .onFocusChanged { rail.contentFocused = it.hasFocus }
                // No focusRestorer here: it would hand focus back to a page that is leaving.
                // The rail saves and restores the page's focus itself.
                .railOnLeftStopOnRight(if (rail.shown) rail else null, stopRight = false)
                .focusGroup()
                .drawWithContent {
                    drawContent()
                    // The open rail dims the page behind it, at once.
                    if (rail.hasFocus) drawRect(RailDim)
                    val a = curtain.value
                    if (a > 0f) drawRect(PleyColors.Bg.copy(alpha = a))
                },
        ) {
            CompositionLocalProvider(
                LocalTvNavigator provides navigator,
                LocalTvRailFocus provides rail.takeIf { it.shown },
                LocalTvOffline provides offline,
                LocalTvTileActions provides tileActions,
            ) {
                NavDisplay(
                    backStack = backStack,
                    onBack = { navigator.back() },
                    entryDecorators = listOf(
                        rememberSaveableStateHolderNavEntryDecorator(),
                        rememberViewModelStoreNavEntryDecorator(),
                        rememberRootPageDecorator(backStack, pages),
                    ),
                    // The rail's page stays under the pages opened on it (RootUnderPages).
                    sceneStrategies = listOf(pages, SinglePaneSceneStrategy()),
                    // Screens change under the curtain, not with sliding or fading pages.
                    transitionSpec = { EnterTransition.None togetherWith ExitTransition.None },
                    popTransitionSpec = { EnterTransition.None togetherWith ExitTransition.None },
                    predictivePopTransitionSpec = { EnterTransition.None togetherWith ExitTransition.None },
                    entryProvider = entryProvider(fallback = { key -> NavEntry(key) { placeholder(it) } }) {
                        onboardingEntries()
                        homeEntries()
                        libraryEntries()
                        itemEntries()
                        searchEntries()
                        playlistsEntries()
                        liveTvEntries()
                        accountEntries()
                        musicEntries()
                    },
                )
            }
            // Signing in and choosing a profile say for themselves when the server can't be reached.
            if (rail.shown) TvOfflineNotice(offline)
        }
        // A tile's actions, over the page they were asked on.
        TvTileActionsPanel(tileActions, page = backStack.lastOrNull(), navigator = navigator)
        if (rail.shown) {
            TvRail(
                items = items,
                profile = profile,
                rail = rail,
                onSelect = { select(it.key) },
                onProfile = { select(PleyKey.Profiles()) },
                offline = offline.offline,
            )
        }
    }
}

/** The page under the open rail: [bg][PleyColors.Bg] at 50 %. */
private val RailDim = PleyColors.Bg.copy(alpha = 0.5f)

/**
 * Another activity (the player) had the window: when it comes back, focus
 * goes back to where it was on the page, or to the page's first element
 * when that one is gone (Resume that became Play). Left to Compose, it
 * lands in the rail. Not when focus was in the rail already.
 */
@Composable
private fun ReturnFocus(rail: RailState, content: FocusRequester, pages: RootUnderPages) {
    val window = LocalWindowInfo.current
    LaunchedEffect(window, rail, content, pages) {
        var inRail = false
        snapshotFlow { window.isWindowFocused }.drop(1).collect { focused ->
            if (!focused) {
                inRail = rail.hasFocus
                if (!inRail) {
                    pages.saveFocus()
                    runCatching { content.saveFocusedChild() }
                }
            } else if (!inRail) {
                withFrameNanos { }
                rail.backToContent()
            }
        }
    }
}

/**
 * Screen changes (design spec §5.1): the new screen shows under a
 * [bg][PleyColors.Bg] rectangle that fades out over 110 ms. One rectangle,
 * drawn over the content: no full-screen layer fades. None with reduced
 * motion.
 */
@Composable
private fun rememberCurtain(key: Any?): Animatable<Float, *> {
    val reduced = reducedMotion()
    val curtain = remember { Animatable(0f) }
    val first = remember { BooleanArray(1) { true } }
    LaunchedEffect(key) {
        if (first[0] || reduced) {
            first[0] = false
            return@LaunchedEffect
        }
        curtain.snapTo(1f)
        curtain.animateTo(0f, tween(PleyMotion.SCREEN_MS / 2, easing = LinearEasing))
    }
    return curtain
}

/** Server events while the app is started, never in the background. */
@Composable
private fun KeepUpdated(source: TvShellSource) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(source, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { source.keepUpdated() }
    }
}


/**
 * A screen that isn't built yet: its name, a line about the app, and a
 * focused button, as every TV screen starts with focus (D-pad rule 1).
 */
@Composable
fun Placeholder(key: NavKey) {
    val navigator = LocalTvNavigator.current
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(start = PleyTv.Gutter, top = 58.dp, end = 48.dp)
            .testTag(TestTags.PLACEHOLDER),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        BasicText(
            stringResource(R.string.tv_shell_placeholder_title, key.javaClass.simpleName),
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
        )
        BasicText(
            stringResource(R.string.tv_shell_placeholder_body),
            style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Muted),
            modifier = Modifier.widthIn(max = 464.dp),
        )
        val next = if (key == PleyKey.Home) PleyKey.Libraries else PleyKey.Search()
        TvPillButton(
            label = stringResource(R.string.tv_shell_placeholder_next),
            onClick = { navigator.open(next) },
            modifier = Modifier.padding(top = 16.dp).initialFocus(),
        )
    }
}
