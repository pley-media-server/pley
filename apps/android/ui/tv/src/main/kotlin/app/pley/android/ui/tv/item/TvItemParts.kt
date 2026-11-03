package app.pley.android.ui.tv.item

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Surface
import androidx.tv.material3.SurfaceDefaults
import app.pley.android.common.sentenceOf
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.screens.item.MetaPart
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.OverlayShowing
import app.pley.android.ui.tv.kit.TvTextButton
import kotlinx.coroutines.delay

/**
 * A row of a side panel or list (design spec §2.7): 44 tall, its [label]
 * in tvBody, a [checked] row with an [accent][PleyColors.Accent] check.
 * Focused, the row is an accent pill with [onAccent][PleyColors.OnAccent]
 * content; rows in panels never scale, which would clip them.
 */
@Composable
internal fun PanelRow(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    checked: Boolean = false,
    icon: PleyIcon? = null,
    supporting: String? = null,
    focusRequester: FocusRequester? = null,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val content = if (focused) PleyColors.OnAccent else PleyColors.Fg
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = PleyTv.ListRowHeight)
            .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
            .background(if (focused) PleyColors.Accent else androidx.compose.ui.graphics.Color.Transparent, PleyShape.Control)
            .semantics { selected = checked }
            .clickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = PleyTv.IconSize, tint = content)
            Spacer(Modifier.width(12.dp))
        }
        Column(Modifier.weight(1f)) {
            BasicText(label, style = PleyType.Tv.Body.copy(color = content), maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (supporting != null) {
                BasicText(
                    supporting,
                    style = PleyType.Tv.Caption.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (checked) {
            Spacer(Modifier.width(12.dp))
            Icon(PleyIcons.Check, contentDescription = null, size = PleyTv.IconSize, tint = if (focused) PleyColors.OnAccent else PleyColors.Accent)
        }
    }
}

/** A heading inside a side panel: "Audio", "Subtitles". */
@Composable
internal fun PanelHeading(text: String, modifier: Modifier = Modifier) {
    BasicText(text, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), modifier = modifier.padding(start = 16.dp, top = 16.dp, bottom = 8.dp))
}

/**
 * The focus ring (design spec §1.4) around something that isn't a tile:
 * a 3 dp [accentBright][PleyColors.AccentBright] stroke 4.5 dp outside it,
 * while [focused] says so (read while drawing).
 */
internal fun Modifier.focusRing(radius: androidx.compose.ui.unit.Dp, focused: () -> Boolean): Modifier = drawWithContent {
    drawContent()
    if (focused()) {
        val offset = PleyFocus.RingOffset.toPx()
        val r = radius.toPx() + offset
        drawRoundRect(
            PleyColors.AccentBright,
            topLeft = Offset(-offset, -offset),
            size = Size(size.width + 2 * offset, size.height + 2 * offset),
            cornerRadius = CornerRadius(r, r),
            style = Stroke(PleyFocus.RingStroke.toPx()),
        )
    }
}

/**
 * The metadata line on TV (design spec §2.6): tvLabel [muted][PleyColors.Muted]
 * with "·" in [subtle][PleyColors.Subtle], the content rating and quality
 * badges boxed, the rating after an accent star.
 */
@Composable
internal fun TvMetaLine(parts: List<MetaPart>, modifier: Modifier = Modifier) {
    if (parts.isEmpty()) return
    Row(modifier.widthIn(max = TextColumn), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        parts.forEachIndexed { i, part ->
            if (i > 0) BasicText("·", style = PleyType.Tv.Label.copy(color = PleyColors.Subtle))
            when (part) {
                is MetaPart.Text -> BasicText(part.text, style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 1)
                is MetaPart.ContentRating -> QualityBadge(part.rating, TileStyle.Tv)
                is MetaPart.Rating -> Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(PleyIcons.Star, contentDescription = null, size = 16.dp, tint = PleyColors.Accent)
                    Spacer(Modifier.width(4.dp))
                    BasicText(part.rating, style = PleyType.Tv.Label.copy(color = PleyColors.Fg))
                }
                is MetaPart.Badges -> Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    part.badges.forEach { QualityBadge(it, TileStyle.Tv) }
                }
            }
        }
    }
}

/** Toasts on TV (design spec §2.10): one pill at the top right, 3.5 s (errors 6 s, in dangerText). */
@Stable
internal class TvToastState {
    var message by mutableStateOf<Toast?>(null)
        private set

    fun show(text: String, error: Boolean = false) {
        message = Toast(text, error, System.nanoTime())
    }

    fun report(error: Throwable) {
        val sentence = sentenceOf(error)
        show(sentence, error = true)
    }

    internal fun clear(toast: Toast) {
        if (message == toast) message = null
    }

    data class Toast(val text: String, val error: Boolean, val at: Long)
}

@Composable
internal fun TvToastHost(state: TvToastState, modifier: Modifier = Modifier) {
    val toast = state.message
    LaunchedEffect(toast) {
        if (toast != null) {
            delay(if (toast.error) ERROR_TOAST_MS else TOAST_MS)
            state.clear(toast)
        }
    }
    Box(modifier.fillMaxSize().padding(horizontal = TvSafeArea.Horizontal, vertical = TvSafeArea.Vertical), contentAlignment = Alignment.TopEnd) {
        AnimatedVisibility(toast != null, enter = fadeIn(PleyMotion.reveal()), exit = fadeOut(PleyMotion.reveal())) {
            val shown = remember { mutableStateOf(toast) }
            if (toast != null) shown.value = toast
            val t = shown.value ?: return@AnimatedVisibility
            Box(
                Modifier
                    .widthIn(max = 420.dp)
                    .background(PleyColors.Surface3, PleyShape.Control)
                    .padding(horizontal = 20.dp, vertical = 12.dp)
                    .semantics { liveRegion = LiveRegionMode.Polite },
            ) {
                BasicText(t.text, style = PleyType.Tv.Label.copy(color = if (t.error) PleyColors.DangerText else PleyColors.Fg))
            }
        }
    }
}

private const val TOAST_MS = 3_500L
private const val ERROR_TOAST_MS = 6_000L

/**
 * A dialog (design spec §2.10): [surface2][PleyColors.Surface2], 16 dp
 * corners, at most 560 wide, centred over a [scrim][PleyColors.Scrim].
 * Close has focus; Back closes it too.
 */
@Composable
internal fun TvTextDialog(title: String?, text: String, onDismiss: () -> Unit) {
    BackHandler(onBack = onDismiss)
    OverlayShowing()
    val close = remember { FocusRequester() }
    val reduced = reducedMotion()
    LaunchedEffect(Unit) { runCatching { close.requestFocus() } }
    Box(Modifier.fillMaxSize().background(PleyColors.Scrim), contentAlignment = Alignment.Center) {
        AnimatedVisibility(true, enter = fadeIn(tween(if (reduced) 0 else PleyMotion.OVERLAY_MS))) {
            Surface(
                modifier = Modifier.widthIn(max = 560.dp),
                shape = PleyShape.Dialog,
                colors = SurfaceDefaults.colors(containerColor = PleyColors.Surface2, contentColor = PleyColors.Fg),
            ) {
                Column(Modifier.padding(28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    if (title != null) BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
                    BasicText(text, style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg.copy(alpha = 0.85f)))
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                        TvTextButton(androidx.compose.ui.res.stringResource(R.string.tv_item_close), onClick = onDismiss, modifier = Modifier.focusRequester(close))
                    }
                }
            }
        }
    }
}

/** A message page (design spec §8): the [title], [body], and one focused button. */
@Composable
internal fun TvMessage(title: String, body: String?, action: String, onAction: () -> Unit, icon: PleyIcon = PleyIcons.CircleAlert) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        delay(16)
        runCatching { focus.requestFocus() }
    }
    Column(
        Modifier.fillMaxSize().padding(start = app.pley.android.designsystem.PleyTv.Gutter, end = TvSafeArea.Horizontal),
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(icon, contentDescription = null, size = 40.dp, tint = PleyColors.Muted)
        Spacer(Modifier.size(16.dp))
        BasicText(title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg))
        if (body != null) {
            Spacer(Modifier.size(8.dp))
            BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), modifier = Modifier.widthIn(max = 464.dp))
        }
        Spacer(Modifier.size(24.dp))
        app.pley.android.ui.tv.kit.TvPillButton(action, onClick = onAction, modifier = Modifier.focusRequester(focus))
    }
}
