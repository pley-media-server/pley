package app.pley.android.ui.tv.shell

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.common.errorSentence
import app.pley.android.data.Loadable
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.network.OfflineException
import app.pley.android.ui.tv.R
import kotlinx.coroutines.delay

/**
 * Whether what the TV shows came from what was saved, the server out of
 * reach: the phone's offline banner (design spec §3.4). Screens [report]
 * their loads; one the server couldn't answer raises it, a fresh one
 * lowers it. Its events bring the screens back on their own once the
 * server answers again.
 *
 * A TV has no corner free of controls on every screen (a library's tabs
 * sit at the top right, rows of tiles at the bottom), so the notice is
 * said for a few seconds ([announcing]), and the rail's avatar keeps a
 * mark while it holds.
 */
@Stable
internal class TvOfflineState {
    var offline by mutableStateOf(false)
        private set

    /** The notice shows: from when the server was found out of reach, until [announced]. */
    var announcing by mutableStateOf(false)
        private set

    /**
     * A screen's [load], [before] it the one it reported last. A failure to
     * reach the server raises it; a fresh answer lowers it only after the
     * same screen's failure, as a fresh one may be the cache's of a minute
     * ago.
     */
    fun report(load: Loadable<*>, before: Loadable<*>?) {
        when {
            load is Loadable.Failed && load.error is OfflineException -> if (!offline) {
                offline = true
                announcing = true
            }
            load is Loadable.Ready && load.fresh && before is Loadable.Failed -> answered()
        }
    }

    /** The server answers again (its events connected, or a screen's fresh answer after a failure). */
    fun answered() {
        offline = false
        announcing = false
    }

    /** The notice has been shown its time. */
    fun announced() {
        announcing = false
    }
}

/** The shell's; outside it (a screen on its own, in tests) one nobody shows. */
internal val LocalTvOffline = staticCompositionLocalOf { TvOfflineState() }

/** A failed load's title: [title], or "Can't reach your server" when the server answered nothing. */
@Composable
internal fun failedTitle(error: Throwable, title: String): String =
    if (error is OfflineException) stringResource(R.string.tv_shell_offline_title) else title

/** Why a load failed, and what to check when the server answered nothing. */
@Composable
internal fun failedBody(error: Throwable): String =
    if (error is OfflineException && !error.blocked) {
        stringResource(R.string.tv_shell_offline_body, errorSentence(error).asString())
    } else {
        errorSentence(error).asString()
    }

/** Reports [load] to the shell's [TvOfflineState] whenever it changes, with the one before. */
@Composable
internal fun ReportOffline(load: Loadable<*>?) {
    val offline = LocalTvOffline.current
    val before = remember { arrayOfNulls<Loadable<*>>(1) }
    LaunchedEffect(load, offline) {
        if (load == null || load is Loadable.Loading) return@LaunchedEffect
        offline.report(load, before[0])
        before[0] = load
    }
}

/**
 * "Pley is offline" at the top right, where TV toasts are (design spec
 * §2.10), for [ANNOUNCE_MS] once the server is found out of reach: the
 * server-off icon and the line on surface3. Never focusable, as there is
 * nothing to do but wait; announced when it shows.
 */
@Composable
internal fun BoxScope.TvOfflineNotice(state: TvOfflineState) {
    LaunchedEffect(state.announcing) {
        if (state.announcing) {
            delay(ANNOUNCE_MS)
            state.announced()
        }
    }
    AnimatedVisibility(
        visible = state.announcing,
        modifier = Modifier.align(Alignment.TopEnd).padding(top = TvSafeArea.Vertical, end = TvSafeArea.Horizontal),
        enter = fadeIn(PleyMotion.reveal()),
        exit = fadeOut(PleyMotion.reveal()),
    ) {
        Row(
            Modifier
                .background(PleyColors.Surface3, PleyShape.Pill)
                .padding(horizontal = 20.dp, vertical = 10.dp)
                .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }
                .testTag(TAG_OFFLINE),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(PleyIcons.ServerOff, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
            BasicText(stringResource(R.string.tv_shell_offline), style = PleyType.Tv.Label.copy(color = PleyColors.Fg))
        }
    }
}

internal const val TAG_OFFLINE = "tv.offline"

/** How long the notice shows: as long as an error's toast (design spec §2.10). */
internal const val ANNOUNCE_MS = 6_000L

/**
 * The rail avatar's mark while the server is out of reach: the server-off
 * icon on a surface3 disc ringed in the page's colour, astride its lower
 * right edge as a status badge is.
 */
@Composable
internal fun BoxScope.OfflineMark() {
    Box(
        Modifier
            .align(Alignment.BottomEnd)
            .offset(x = 6.dp, y = 6.dp)
            .size(16.dp)
            .background(PleyColors.Bg, PleyShape.Circle)
            .padding(2.dp)
            .background(PleyColors.Surface3, PleyShape.Circle)
            .testTag(TAG_OFFLINE_MARK),
        contentAlignment = Alignment.Center,
    ) {
        Icon(PleyIcons.ServerOff, contentDescription = null, size = 9.dp, tint = PleyColors.Fg)
    }
}

internal const val TAG_OFFLINE_MARK = "tv.offline.mark"
