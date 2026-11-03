package app.pley.android.ui.tv.shell

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import app.pley.android.data.RequestNotice
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyElevation
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.reducedMotion
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.requests.rememberRequestTexts
import kotlinx.coroutines.delay

/**
 * Requests' news over the pages, one at a time, as the Samsung and LG
 * apps have it (packages/tv's notice.tsx): on a TV, what became ready to
 * watch, the only news the data module tells there. A card at the bottom
 * right with the title's poster, the news, Open and Close.
 *
 * It shows once the remote has rested [NOTICE_IDLE_MS], the app is in
 * front (never over the player, a window of its own), the page is [free]
 * for it (not the open rail, a panel, a dialog, a field being typed in)
 * and something on the page has focus, which it saves in [content]. On
 * screen it is off the queue ([TvShellSource.dismiss]), so newer news
 * can't push it out. Focus stays in it. Close, Back, Up or Down close it
 * and focus goes back where it was, as it does when [NOTICE_SHOWN_MS] pass
 * untouched; Open opens the title where it is in a library, else the
 * requests ([onOpen]). An OK in its first [OK_GUARD_MS] was meant for the
 * page: it does nothing.
 */
@Composable
internal fun BoxScope.TvRequestNotices(
    source: TvShellSource,
    content: FocusRequester,
    free: () -> Boolean,
    onOpen: (PleyKey) -> Unit,
) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    // Read after the first frame, while the app is started.
    val queue by produceState(emptyList<TvNotice>(), source, lifecycle) {
        withFrameNanos { }
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { source.notices.collect { value = it } }
    }
    val gate = LocalKeyRepeatGate.current
    val isFree by rememberUpdatedState(free)
    var shown by remember { mutableStateOf<TvNotice?>(null) }
    // One just closed (a key, often): the next waits for the remote to rest from it too.
    val closed = remember { BooleanArray(1) }
    val next = queue.firstOrNull()
    LaunchedEffect(next?.notice?.key, shown == null) {
        if (next == null || shown != null) return@LaunchedEffect
        if (closed[0]) {
            closed[0] = false
            delay(NOTICE_IDLE_MS)
        }
        while (true) {
            val quiet = gate.quietFor()
            val ready = quiet >= NOTICE_IDLE_MS && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) && isFree()
            // Where focus is on the page now, which it goes back to.
            if (ready && runCatching { content.saveFocusedChild() }.getOrDefault(false)) break
            delay(if (quiet < NOTICE_IDLE_MS) NOTICE_IDLE_MS - quiet else NOTICE_POLL_MS)
        }
        source.dismiss(next.notice)
        shown = next
    }
    val notice = shown ?: return
    key(notice.notice.key) {
        NoticeCard(
            notice = notice,
            onClose = { open ->
                closed[0] = true
                if (open) {
                    // The page it opens has its own initial focus.
                    shown = null
                    onOpen((notice.notice as? RequestNotice.Ready)?.itemId?.let { PleyKey.Item(it) } ?: PleyKey.Requests)
                } else {
                    runCatching { if (!content.restoreFocusedChild()) content.requestFocus() }
                    shown = null
                }
            },
        )
    }
}

/**
 * The card: 360 wide, [surface2][PleyColors.Surface2] at 96 % with a
 * [surface4][PleyColors.Surface4] hairline along its top, radius 8; the
 * poster at 40 by 60, or a green tick without one, then the news in three
 * lines at most, and Open with Close at its foot. It fades in over 150 ms
 * (at once with reduced motion). [onClose] is told once, with whether it
 * opens.
 */
@Composable
private fun BoxScope.NoticeCard(notice: TvNotice, onClose: (open: Boolean) -> Unit) {
    val texts = rememberRequestTexts()
    val text = remember(notice, texts) { texts.notice(notice.notice) }
    val openFocus = remember { FocusRequester() }
    // Closing: focus may leave the card then, and nothing else closes it.
    val closing = remember { BooleanArray(1) }
    var armed by remember { mutableStateOf(false) }
    var touches by remember { mutableIntStateOf(0) }
    val close = { open: Boolean ->
        if (!closing[0]) {
            closing[0] = true
            onClose(open)
        }
    }
    BackHandler { close(false) }
    LaunchedEffect(Unit) {
        runCatching { openFocus.requestFocus() }
        delay(OK_GUARD_MS)
        armed = true
    }
    // Untouched, it goes by itself; every key restarts its time.
    LaunchedEffect(touches) {
        delay(NOTICE_SHOWN_MS)
        close(false)
    }
    val reduced = reducedMotion()
    val alpha = remember { Animatable(if (reduced) 1f else 0f) }
    LaunchedEffect(Unit) { alpha.animateTo(1f, PleyMotion.reveal()) }
    Column(
        Modifier
            .align(Alignment.BottomEnd)
            .padding(end = TvSafeArea.Horizontal, bottom = NoticeBottom)
            .width(NoticeWidth)
            .graphicsLayer {
                this.alpha = alpha.value
                shape = PleyShape.Dialog
                clip = true
            }
            .background(PleyColors.Surface2.copy(alpha = 0.96f))
            .drawBehind { drawRect(PleyColors.Surface4, size = Size(size.width, PleyElevation.Level2Hairline.toPx())) }
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                touches++
                if (event.key == Key.DirectionUp || event.key == Key.DirectionDown) {
                    close(false)
                    true
                } else {
                    false
                }
            }
            .focusProperties { onExit = { if (!closing[0]) cancelFocusChange() } }
            .focusGroup()
            .padding(16.dp)
            .testTag(TV_REQUEST_NOTICE),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            val poster = notice.poster
            if (poster != null) {
                Artwork(poster, "", Modifier.size(PosterWidth, PosterHeight).clip(PleyShape.TileTv).background(PleyColors.Surface3))
                Spacer(Modifier.width(14.dp))
            }
            Row(Modifier.weight(1f)) {
                // The poster says which title it is; without one, a tick says it's good news.
                if (poster == null) {
                    Icon(PleyIcons.CircleCheck, contentDescription = null, size = 20.dp, tint = PleyColors.Success)
                    Spacer(Modifier.width(10.dp))
                }
                BasicText(
                    text,
                    style = PleyType.Tv.Label.copy(color = PleyColors.Fg),
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                )
            }
        }
        Spacer(Modifier.height(14.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End)) {
            TvTextButton(
                stringResource(ScreensR.string.requests_open),
                onClick = { if (armed) close(true) },
                modifier = Modifier.focusRequester(openFocus),
            )
            TvTextButton(stringResource(R.string.tv_requests_close), onClick = { if (armed) close(false) })
        }
    }
}

/** A notice waits for the remote to rest this long, so it never takes a press meant for the page. */
internal const val NOTICE_IDLE_MS = 1_500L

/** Untouched this long, a notice goes by itself. */
internal const val NOTICE_SHOWN_MS = 10_000L

/** OK this soon after a notice took focus was meant for what had it before: it does nothing. */
internal const val OK_GUARD_MS = 600L

/** How often a notice that waits for the page looks again. */
private const val NOTICE_POLL_MS = 250L

private val NoticeWidth = 360.dp

/** Where the Samsung and LG apps put it: above the safe area's foot, clear of a row's captions. */
private val NoticeBottom = 51.dp

private val PosterWidth = 40.dp
private val PosterHeight = 60.dp

internal const val TV_REQUEST_NOTICE = "pley.tv.notice"
