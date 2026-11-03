package app.pley.android.ui.tv.kit

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.Interaction
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
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
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
import androidx.compose.ui.graphics.drawscope.DrawScope
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
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
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
 * Under a held key the look changes evenly over the time between moves
 * (rule 5, [KeyRepeatGate.GLIDE_MS]).
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
 * Right stays meaningful at the row's end.
 */
@Composable
fun TvSeeAllTile(
    kind: TileKind,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    focusModifier: Modifier = Modifier,
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
            Icon(PleyIcons.ArrowRight, contentDescription = null, size = 32.dp, tint = PleyColors.Fg)
            BasicText(label, style = PleyType.Tv.Label.copy(color = PleyColors.Fg, textAlign = TextAlign.Center))
        }
    }
}

/**
 * A focusable well of [kind]'s size with the TV's focus look, and a
 * caption under it that [caption] draws (told whether it is emphasized:
 * focused, and not quiet). In a row sliding or a grid gliding under a held
 * key the tile keeps its resting look ([TileQuiet], [LocalTileQuiet]).
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
    val quiet = LocalTileQuiet.current
    val tier = LocalTvTier.current
    val reduced = reducedMotion()
    val scope = rememberCoroutineScope()
    val look = remember(gate, quiet) { TileFocusLook(scope, gate, quiet) }
    look.reduced = reduced
    look.tier = tier
    val presses = remember(look) { TilePresses(look) }
    Column(modifier.width(kind.tvWidth).focusedOnTop(look)) {
        Box(
            focusModifier
                .onFocusChanged { look.focus(it.isFocused) }
                .menuKey(onLongClick)
                .combinedClickable(
                    interactionSource = presses,
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
                    drawFocusRing(kind, look.shown(), look.scale())
                },
        ) {
            well()
        }
        Spacer(Modifier.height(TileStyle.Tv.captionGap))
        caption(look.emphasized)
    }
}

/**
 * A tile's focus, animated outside composition: [progress] (0 at rest, 1
 * focused), the quiet and the press are read only in the layer and while
 * drawing, so an animation frame never recomposes. [focused] switches once
 * per change, for the tile's drawing order, and [emphasized], for the
 * caption's weight, once at either end of a slide as well.
 */
@Stable
internal class TileFocusLook(private val scope: CoroutineScope, private val gate: KeyRepeatGate, private val quiet: TileQuiet?) {
    var focused by mutableStateOf(false)
        private set

    /** Whether the caption is bold: focused, and not quiet. Tiles without focus don't read the quiet at all. */
    val emphasized: Boolean by derivedStateOf { focused && quiet?.on != true }

    val progress = Animatable(0f)

    /** The press's scale, made at the first press: most tiles are never pressed. */
    private var pressed by mutableStateOf<Animatable<Float, AnimationVector1D>?>(null)
    var reduced = false
    var tier = Tier.T2

    fun focus(on: Boolean) {
        if (on == focused) return
        focused = on
        val target = if (on) 1f else 0f
        // T0 animates the focused tile only (design spec §6); a tile focus leaves while quiet
        // has shown nothing of it. A held key's move changes the look evenly over the time to
        // the next, so focus glides from tile to tile rather than pulsing.
        val snap = reduced || (!on && (tier == Tier.T0 || quiet?.on == true))
        val spec: FiniteAnimationSpec<Float> = when {
            gate.holding -> KeyRepeatGate.glide()
            on -> PleyMotion.focus()
            else -> PleyMotion.unfocus()
        }
        scope.launch {
            if (snap) {
                progress.snapTo(target)
            } else {
                progress.animateTo(target, spec)
            }
        }
    }

    fun press(down: Boolean) {
        val target = if (down) PleyFocus.TILE_PRESSED_SCALE / PleyFocus.TILE_SCALE else 1f
        val scale = pressed ?: Animatable(1f).also { pressed = it }
        scope.launch { scale.animateTo(target, PleyMotion.press(reduced)) }
    }

    /** How much of the focus look shows: [progress], none of it while quiet. */
    fun shown(): Float {
        val p = progress.value
        return if (p == 0f || quiet == null) p else p * (1f - quiet.amount)
    }

    /** The well's scale now: up to [PleyFocus.TILE_SCALE], times the press. */
    fun scale(): Float = (1f + (PleyFocus.TILE_SCALE - 1f) * shown()) * (pressed?.value ?: 1f)
}

/**
 * A tile's presses, heard as they happen rather than collected: no
 * coroutine per tile waits for them, and nothing else reads them.
 */
private class TilePresses(private val look: TileFocusLook) : MutableInteractionSource {
    override val interactions: Flow<Interaction> = emptyFlow()

    override suspend fun emit(interaction: Interaction) = hear(interaction)

    override fun tryEmit(interaction: Interaction): Boolean {
        hear(interaction)
        return true
    }

    private fun hear(interaction: Interaction) {
        if (interaction is PressInteraction) look.press(interaction is PressInteraction.Press)
    }
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

/**
 * The focus ring around a resting well of [kind] whose top left is at
 * [topLeft]: the one a row or grid draws still while its tiles are quiet
 * ([TileQuiet]). [stroke] is the ring's, made once rather than each frame.
 */
internal fun DrawScope.drawStillRing(kind: TileKind, topLeft: Offset, alpha: Float, stroke: Stroke) {
    if (alpha <= 0f) return
    val offset = PleyFocus.RingOffset.toPx()
    val width = kind.tvWidth.roundToPx().toFloat()
    val height = kind.tvHeight.roundToPx().toFloat()
    val radius = if (kind.round) minOf(width, height) / 2f + offset else PleyShape.TileTvRadius.toPx() + offset
    drawRoundRect(
        color = PleyColors.AccentBright.copy(alpha = alpha.coerceIn(0f, 1f)),
        topLeft = Offset(topLeft.x - offset, topLeft.y - offset),
        size = Size(width + 2 * offset, height + 2 * offset),
        cornerRadius = CornerRadius(radius, radius),
        style = stroke,
    )
}
