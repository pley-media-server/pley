package app.pley.android.designsystem.tile

import android.graphics.Bitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.theme.LocalPleyPalette
import app.pley.android.designsystem.theme.PleyPalette
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.concurrent.ConcurrentHashMap
import kotlin.math.ceil
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/**
 * The focused tile's glow (design spec §2.3, T1 and T2 only): a soft
 * accent halo [PleyFocus.GlowWidth] wide around the tile, at
 * [PleyFocus.GLOW_ALPHA] where it meets the tile and fading to nothing.
 *
 * It is one small alpha-only bitmap per tile kind and density, made at a
 * quarter of the resolution on a background thread the first time it is
 * needed (10 to 40 KB), then stretched and tinted when drawn: no shadow
 * layer, blur or render effect, which the TVs' GPUs can't afford. Not used
 * at all on T0: the shells don't add the modifier there.
 */
object TileGlow {
    private const val SCALE = 4

    private data class Key(val kind: TileKind, val density: Float)

    private val bitmaps = ConcurrentHashMap<Key, ImageBitmap>()

    /** The glow of [kind] at [density], or null while it is being made. */
    internal fun cached(kind: TileKind, density: Float): ImageBitmap? = bitmaps[Key(kind, density)]

    /** Makes the glows of [kinds] now, so the first focus doesn't wait: call it off the main thread. */
    fun warm(density: Density, kinds: Collection<TileKind> = TileKind.entries) {
        for (kind in kinds) bitmaps.getOrPut(Key(kind, density.density)) { render(kind, density.density) }
    }

    /**
     * The halo around [kind]'s tile as alpha, at a quarter of [density]'s
     * pixels: the tile's size plus the glow on every side.
     */
    internal fun render(kind: TileKind, density: Float): ImageBitmap {
        val glow = PleyFocus.GlowWidth.value
        val w = kind.tvWidth.value
        val h = kind.tvHeight.value
        val radius = if (kind.round) min(w, h) / 2f else app.pley.android.designsystem.PleyShape.TileTvRadius.value
        val pxPerDp = density / SCALE
        val bw = ceil((w + 2 * glow) * pxPerDp).toInt().coerceAtLeast(1)
        val bh = ceil((h + 2 * glow) * pxPerDp).toInt().coerceAtLeast(1)
        val pixels = IntArray(bw * bh)
        for (y in 0 until bh) {
            val py = (y + 0.5f) / pxPerDp - glow
            for (x in 0 until bw) {
                val px = (x + 0.5f) / pxPerDp - glow
                val d = outside(px, py, w, h, radius)
                val alpha = if (d <= 0f) {
                    PleyFocus.GLOW_ALPHA
                } else {
                    val t = 1f - d / glow
                    // Smoothstep: a soft shoulder near the tile, a long tail outwards.
                    if (t <= 0f) 0f else PleyFocus.GLOW_ALPHA * t * t * (3f - 2f * t)
                }
                pixels[y * bw + x] = (alpha * 255f + 0.5f).toInt() shl 24
            }
        }
        val bitmap = Bitmap.createBitmap(bw, bh, Bitmap.Config.ALPHA_8)
        bitmap.setPixels(pixels, 0, bw, 0, 0, bw, bh)
        return bitmap.asImageBitmap()
    }

    /** How far ([x], [y]) is outside a [w] by [h] rounded rectangle with corner [r]; 0 or less inside. */
    private fun outside(x: Float, y: Float, w: Float, h: Float, r: Float): Float {
        val qx = max(max(r - x, x - (w - r)), 0f)
        val qy = max(max(r - y, y - (h - r)), 0f)
        if (qx > 0f && qy > 0f) return hypot(qx, qy) - r
        return max(max(-x, x - w), max(-y, y - h))
    }

    internal suspend fun load(kind: TileKind, density: Float): ImageBitmap =
        bitmaps[Key(kind, density)] ?: withContext(Dispatchers.Default) {
            bitmaps.getOrPut(Key(kind, density)) { render(kind, density) }
        }
}

/**
 * Draws [kind]'s glow behind the content while [alpha] (read only while
 * drawing, so an animation redraws without recomposing) is above 0. Put
 * it outside the tile's clip, inside its focus scale.
 */
fun Modifier.tileGlow(kind: TileKind, alpha: () -> Float): Modifier = this then TileGlowElement(kind, alpha)

private data class TileGlowElement(val kind: TileKind, val alpha: () -> Float) : ModifierNodeElement<TileGlowNode>() {
    override fun create() = TileGlowNode(kind, alpha)

    override fun update(node: TileGlowNode) {
        node.kind = kind
        node.alpha = alpha
        node.invalidateDraw()
    }

    override fun InspectorInfo.inspectableProperties() {
        name = "tileGlow"
        properties["kind"] = kind
    }
}

private class TileGlowNode(var kind: TileKind, var alpha: () -> Float) : Modifier.Node(), DrawModifierNode, CompositionLocalConsumerModifierNode {
    // The accent's, made again after a pick changes it (the node redraws as the palette changes).
    private var tint: ColorFilter? = null
    private var tintOf: PleyPalette? = null
    private var loading = false

    override fun ContentDrawScope.draw() {
        val a = alpha()
        if (a > 0f) {
            val bitmap = TileGlow.cached(kind, density)
            if (bitmap != null) {
                val reach = PleyFocus.GlowWidth.roundToPx()
                drawImage(
                    image = bitmap,
                    dstOffset = IntOffset(-reach, -reach),
                    dstSize = IntSize(size.width.toInt() + 2 * reach, size.height.toInt() + 2 * reach),
                    alpha = a.coerceAtMost(1f),
                    colorFilter = tintFor(currentValueOf(LocalPleyPalette)),
                )
            } else if (!loading && isAttached) {
                loading = true
                val d = density
                coroutineScope.launch {
                    TileGlow.load(kind, d)
                    loading = false
                    invalidateDraw()
                }
            }
        }
        drawContent()
    }

    private fun tintFor(palette: PleyPalette): ColorFilter =
        tint?.takeIf { tintOf === palette } ?: ColorFilter.tint(palette.accent).also {
            tint = it
            tintOf = palette
        }
}
