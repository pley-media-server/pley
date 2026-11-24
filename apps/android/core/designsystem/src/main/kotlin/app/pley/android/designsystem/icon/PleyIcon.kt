package app.pley.android.designsystem.icon

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import java.util.concurrent.ConcurrentHashMap

/**
 * A Lucide icon: its SVG paths on the 24 by 24 grid. [vector] builds it at
 * a stroke width, once per width; [filled] glyphs are filled as well as
 * stroked, so their corners stay round.
 */
@Immutable
class PleyIcon(
    /** Lucide's name ("clapperboard"). */
    val name: String,
    val filled: Boolean,
    private vararg val paths: String,
) {
    private val vectors = ConcurrentHashMap<Float, ImageVector>(2)

    /** This icon stroked [strokeWidth] units wide (of 24), white: tint it when drawing. */
    fun vector(strokeWidth: Float = DEFAULT_STROKE): ImageVector = vectors.getOrPut(strokeWidth) {
        val builder = ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        for (path in paths) {
            builder.addPath(
                pathData = addPathNodes(path),
                pathFillType = PathFillType.NonZero,
                fill = if (filled) SolidColor(Color.White) else null,
                stroke = SolidColor(Color.White),
                strokeLineWidth = strokeWidth,
                strokeLineCap = StrokeCap.Round,
                strokeLineJoin = StrokeJoin.Round,
            )
        }
        builder.build()
    }

    override fun toString(): String = "PleyIcon($name)"

    companion object {
        /** Lucide's stroke: 2 units of 24. */
        const val DEFAULT_STROKE = 2f

        /**
         * The stroke for an icon drawn [size] tall, so lines keep about the
         * same weight on screen: 2 at 24 dp and 2.25 at 20 dp (the design
         * spec, §1.6), thinner for big glyphs, never under 1.5 or over 2.5.
         */
        fun strokeFor(size: Dp): Float = (2f + (24f - size.value) / 16f).coerceIn(1.5f, 2.5f)
    }
}

/**
 * Draws [icon] [size] square in [tint]. With a [contentDescription] it is
 * read out as an image; without one it is decoration and screen readers
 * skip it.
 */
@Composable
fun Icon(
    icon: PleyIcon,
    contentDescription: String?,
    modifier: Modifier = Modifier,
    size: Dp = 24.dp,
    tint: Color = PleyColors.Fg,
) {
    val painter = rememberVectorPainter(icon.vector(PleyIcon.strokeFor(size)))
    val filter = remember(tint) { ColorFilter.tint(tint) }
    val described = if (contentDescription != null) {
        Modifier.semantics {
            this.contentDescription = contentDescription
            role = Role.Image
        }
    } else {
        Modifier
    }
    Image(painter, contentDescription = null, modifier = modifier.then(described).size(size), colorFilter = filter)
}

/**
 * The Pley mark: three posters on a shelf, play cut out of the front one,
 * on its dark tile ([PleyMark.Tile]) or alone ([PleyMark.Glyph]).
 * apps/web/src/components/Logo.tsx.
 */
object PleyMark {
    private const val BACK = "M46 21.5h3a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3h-3z"
    private const val MIDDLE = "M36 18.5h4.5a3.5 3.5 0 0 1 3.5 3.5v20a3.5 3.5 0 0 1-3.5 3.5H36z"
    private const val FRONT = "M16 15.5h14a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H16a4 4 0 0 1-4-4v-25a4 4 0 0 1 4-4zM19 27v10a1 1 0 0 0 1.51.86l8.5-5a1 1 0 0 0 0-1.72l-8.5-5A1 1 0 0 0 19 27z"
    private const val TILE = "M15,0h34a15,15 0,0 1,15 15v34a15,15 0,0 1,-15 15h-34a15,15 0,0 1,-15 -15v-34a15,15 0,0 1,15 -15z"

    private fun gradient(): Brush = Brush.linearGradient(
        0f to PleyColors.BrandLight,
        1f to PleyColors.Accent,
        start = androidx.compose.ui.geometry.Offset(12f, 15f),
        end = androidx.compose.ui.geometry.Offset(34f, 49f),
    )

    private fun build(name: String, tile: Boolean): ImageVector {
        val builder = ImageVector.Builder(name, 64.dp, 64.dp, 64f, 64f)
        if (tile) builder.addPath(addPathNodes(TILE), fill = SolidColor(PleyColors.BrandTile))
        builder.addPath(addPathNodes(BACK), fill = SolidColor(PleyColors.BrandFar))
        builder.addPath(addPathNodes(MIDDLE), fill = SolidColor(PleyColors.BrandDeep))
        builder.addPath(addPathNodes(FRONT), pathFillType = PathFillType.EvenOdd, fill = gradient())
        return builder.build()
    }

    /** The mark on its rounded `brandTile` square, as app icons and the web's header show it. */
    val Tile: ImageVector by lazy { build("pley-mark-tile", tile = true) }

    /** The mark alone, for dark surfaces. */
    val Glyph: ImageVector by lazy { build("pley-mark", tile = false) }
}
