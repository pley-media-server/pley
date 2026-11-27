package app.pley.android.ui.phone.kit

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.LayoutModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.unit.Constraints
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.theme.LocalPleyPalette
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * A card's press (design spec §3.2): while [interactions] hold a press the
 * art grows to 1.04 and a 2 dp [accent][PleyColors.Accent] ring follows
 * its [shape] just outside it. The press's progress is read only in the
 * layer and while drawing, so pressing a card never recomposes it or its
 * row. With [reduced] motion it snaps. Put it outside the art's clip.
 */
internal fun Modifier.cardPress(interactions: InteractionSource, shape: Shape, reduced: Boolean): Modifier =
    this then CardPressElement(interactions, shape, reduced)

private data class CardPressElement(
    val interactions: InteractionSource,
    val shape: Shape,
    val reduced: Boolean,
) : ModifierNodeElement<CardPressNode>() {
    override fun create() = CardPressNode(interactions, shape, reduced)

    override fun update(node: CardPressNode) = node.update(interactions, shape, reduced)

    override fun InspectorInfo.inspectableProperties() {
        name = "cardPress"
    }
}

private class CardPressNode(
    private var interactions: InteractionSource,
    private var shape: Shape,
    private var reduced: Boolean,
) : Modifier.Node(), LayoutModifierNode, DrawModifierNode, CompositionLocalConsumerModifierNode {
    private val progress = Animatable(0f)
    private var watching: Job? = null
    private var ringOutline: Outline? = null
    private var ringSize = Size.Unspecified

    override val shouldAutoInvalidate: Boolean get() = false

    override fun onAttach() = watch()

    fun update(interactions: InteractionSource, shape: Shape, reduced: Boolean) {
        val again = interactions != this.interactions
        val reshaped = shape != this.shape
        this.interactions = interactions
        this.shape = shape
        this.reduced = reduced
        if (again && isAttached) watch()
        if (reshaped) {
            ringOutline = null
            invalidateDraw()
        }
    }

    private fun watch() {
        watching?.cancel()
        watching = coroutineScope.launch {
            val held = mutableSetOf<PressInteraction.Press>()
            interactions.interactions.collect { interaction ->
                when (interaction) {
                    is PressInteraction.Press -> held += interaction
                    is PressInteraction.Release -> held -= interaction.press
                    is PressInteraction.Cancel -> held -= interaction.press
                    else -> return@collect
                }
                val target = if (held.isEmpty()) 0f else 1f
                launch {
                    if (reduced) progress.snapTo(target) else progress.animateTo(target, PleyMotion.press())
                }
            }
        }
    }

    override fun MeasureScope.measure(measurable: Measurable, constraints: Constraints): MeasureResult {
        val placeable = measurable.measure(constraints)
        return layout(placeable.width, placeable.height) {
            placeable.placeWithLayer(0, 0) {
                // Read here, so a press redraws the layer without measuring or composing.
                val s = 1f + (PleyFocus.PHONE_PRESSED_SCALE - 1f) * progress.value
                scaleX = s
                scaleY = s
            }
        }
    }

    override fun ContentDrawScope.draw() {
        drawContent()
        val p = progress.value
        if (p <= 0f) return
        val stroke = PleyFocus.PhonePressRing.toPx()
        // The ring's center runs half a stroke outside the art, so it hugs the edge.
        val ring = Size(size.width + stroke, size.height + stroke)
        val outline = ringOutline?.takeIf { ringSize == ring } ?: shape.createOutline(ring, layoutDirection, this).also {
            ringOutline = it
            ringSize = ring
        }
        translate(-stroke / 2f, -stroke / 2f) {
            drawOutline(outline, currentValueOf(LocalPleyPalette).accent, alpha = p.coerceIn(0f, 1f), style = Stroke(stroke))
        }
    }

    override fun onDetach() {
        watching = null
    }
}
