package app.pley.android.ui.tv.kit

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.layout
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.tvTileSize
import app.pley.android.ui.tv.theme.LocalTvTier
import app.pley.android.common.Tier
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * A tile on TV (design spec §2.3, D-pad rule 8): the artwork tile
 * (:core:designsystem's [ArtworkTile]) with focus. Our own composable, not
 * tv-material's Card, which recomposes on every frame of its animation and
 * draws its glow with a shadow layer.
 *
 * Focused, the well grows to 1.08 (in its layer, never recomposing), gets
 * the accent ring drawn around it, and nothing else: no glow; the caption,
 * outside the scaled layer, turns bold. Select opens; holding Select calls
 * [onLongClick], the item's actions (rule 6), which are also buttons on its
 * page; so does the remote's Menu key, the way Fire TV apps open them.
 * While a key repeats the tile snaps to its focus look (rule 5).
 *
 * [focusModifier] goes on the focusable itself: a FocusRequester,
 * [initialFocus] or [rememberedFocus].
 */
@Composable
fun TvTile(
    model: ArtworkTileModel,
    kind: TileKind,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    onLongClick: (() -> Unit)? = null,
    focusModifier: Modifier = Modifier,
) {
    TvTileFrame(
        kind = kind,
        onClick = onClick,
        onLongClick = onLongClick,
        modifier = modifier,
        focusModifier = focusModifier,
        caption = { focused -> TileCaption(model.title, model.subtitle, TileStyle.Tv, emphasized = focused) },
    ) {
        ArtworkTile(model, kind, TileStyle.Tv, Modifier.tvTileSize(kind))
    }
}

/**
 * The last tile of a row that has more (design spec §2.4): a well of the
 * row's tile size with an arrow and "See all", focused like any tile so
 * Right stays meaningful at the row's end. A grid that reads on when
 * asked ends in one too, with its own [icon] (Show more, Retry).
 */
@Composable
fun TvSeeAllTile(
    kind: TileKind,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    focusModifier: Modifier = Modifier,
    icon: PleyIcon = PleyIcons.ArrowRight,
) {
    TvTileFrame(kind, onClick, null, modifier, focusModifier, caption = { }) {
        Column(
            Modifier
                .tvTileSize(kind)
                .background(PleyColors.Surface2, TileStyle.Tv.shape(kind))
                .semantics { contentDescription = label },
            verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(icon, contentDescription = null, size = 32.dp, tint = PleyColors.Fg)
            BasicText(label, style = PleyType.Tv.Label.copy(color = PleyColors.Fg, textAlign = TextAlign.Center))
        }
    }
}

/**
 * A focusable well of [kind]'s size with the TV's focus look, and a
 * caption under it that [caption] draws (told whether the tile has focus).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TvTileFrame(
    kind: TileKind,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    focusModifier: Modifier = Modifier,
    caption: @Composable (focused: Boolean) -> Unit,
    well: @Composable () -> Unit,
) {
    val gate = LocalKeyRepeatGate.current
    val tier = LocalTvTier.current
    val reduced = reducedMotion()
    val scope = rememberCoroutineScope()
    val look = remember(gate) { TileFocusLook(scope, gate) }
    look.reduced = reduced
    look.tier = tier
    val interactions = remember { MutableInteractionSource() }
    LaunchedEffect(interactions) {
        interactions.interactions.collect { if (it is PressInteraction) look.press(it is PressInteraction.Press) }
    }
    Column(modifier.width(kind.tvWidth).focusedOnTop(look)) {
        Box(
            focusModifier
                .onFocusChanged { look.focus(it.isFocused) }
                .menuKey(onLongClick)
                .combinedClickable(
                    interactionSource = interactions,
                    indication = null,
                    onLongClick = onLongClick,
                    onClick = onClick,
                )
                .graphicsLayer {
                    val s = look.scale()
                    scaleX = s
                    scaleY = s
                }
                .drawWithContent {
                    drawContent()
                    drawFocusRing(kind, look.progress.value, look.scale())
                },
        ) {
            well()
        }
        Spacer(Modifier.height(TileStyle.Tv.captionGap))
        caption(look.focused)
    }
}

/**
 * A tile's focus, animated outside composition: [progress] (0 at rest, 1
 * focused) and the press are read only in the layer and while drawing, so
 * an animation frame never recomposes. [focused] switches once per change,
 * for the caption's weight and the tile's drawing order.
 */
@Stable
internal class TileFocusLook(private val scope: CoroutineScope, private val gate: KeyRepeatGate) {
    var focused by mutableStateOf(false)
        private set
    val progress = Animatable(0f)
    private val pressed = Animatable(1f)
    var reduced = false
    var tier = Tier.T2

    fun focus(on: Boolean) {
        if (on == focused) return
        focused = on
        val target = if (on) 1f else 0f
        // Held keys snap; T0 animates the focused tile only (design spec §6).
        val snap = reduced || gate.holding || (!on && tier == Tier.T0)
        scope.launch {
            if (snap) {
                progress.snapTo(target)
            } else {
                progress.animateTo(target, if (on) PleyMotion.focus() else PleyMotion.unfocus())
            }
        }
    }

    fun press(down: Boolean) {
        val target = if (down) PleyFocus.TILE_PRESSED_SCALE / PleyFocus.TILE_SCALE else 1f
        scope.launch { pressed.animateTo(target, PleyMotion.press(reduced)) }
    }

    /** The well's scale now: up to [PleyFocus.TILE_SCALE], times the press. */
    fun scale(): Float = (1f + (PleyFocus.TILE_SCALE - 1f) * progress.value) * pressed.value

}

/**
 * The remote's Menu key calls [onActions] as it comes up, once however
 * long it is held; without actions the key goes on to the screen.
 */
internal fun Modifier.menuKey(onActions: (() -> Unit)?): Modifier =
    if (onActions == null) {
        this
    } else {
        onKeyEvent { event ->
            if (event.key != Key.Menu) return@onKeyEvent false
            if (event.type == KeyEventType.KeyUp) onActions()
            true
        }
    }

/** The focused tile draws over its neighbours and their captions (zIndex 1), decided while placing, not composing. */
private fun Modifier.focusedOnTop(look: TileFocusLook): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints)
    layout(placeable.width, placeable.height) {
        placeable.place(0, 0, zIndex = if (look.focused) 1f else 0f)
    }
}

/**
 * The focus ring (design spec §1.4) around a well drawn at [scale]: a 3 dp
 * accentBright stroke centred 4.5 dp outside the scaled edge, so drawn in
 * the well's own (unscaled) units it is divided by the scale.
 */
internal fun ContentDrawScope.drawFocusRing(kind: TileKind, alpha: Float, scale: Float) {
    if (alpha <= 0f) return
    val s = scale.coerceAtLeast(0.01f)
    val offset = PleyFocus.RingOffset.toPx() / s
    val stroke = PleyFocus.RingStroke.toPx() / s
    val radius = if (kind.round) size.minDimension / 2f + offset else PleyShape.TileTvRadius.toPx() + offset
    drawRoundRect(
        color = PleyColors.AccentBright.copy(alpha = alpha.coerceIn(0f, 1f)),
        topLeft = Offset(-offset, -offset),
        size = Size(size.width + 2 * offset, size.height + 2 * offset),
        cornerRadius = CornerRadius(radius, radius),
        style = Stroke(stroke),
    )
}
