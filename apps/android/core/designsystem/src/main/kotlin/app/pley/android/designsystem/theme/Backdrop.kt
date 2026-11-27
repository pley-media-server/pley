package app.pley.android.designsystem.theme

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Shader
import android.graphics.drawable.Drawable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.core.graphics.createBitmap
import androidx.core.graphics.drawable.toDrawable
import androidx.core.view.WindowCompat
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * The themes' backdrops (docs/THEMES.md; the web's `--glow` and `--sidebar`
 * in index.css): Twilight's sunset along the foot of the window,
 * Moonlight's moon and Nightshade's light at its top right, Sunshine's
 * bubbles, and Sunshine's rainbow sidebar. Each is drawn once, small, into
 * a picture that the window's background stretches (filtered, a linear
 * gradient stays one): the window draws it as it draws its color, so
 * nothing scrolling or taking focus draws it again.
 */
object Backdrop {
    /** The longer side of a glow's picture, in pixels: its gradients are soft, so a stretched small picture is them. */
    private const val SIDE = 192

    /** Whether [theme] has a glow behind its pages. */
    fun hasGlow(theme: PleyTheme): Boolean = layers(theme).isNotEmpty()

    /**
     * [theme]'s page for a window [width] by [height]: its `bg`, and its glow
     * over it where it has one, as a picture of the window's shape.
     */
    fun page(theme: PleyTheme, bg: Color, width: Int, height: Int): Bitmap? {
        val layers = layers(theme)
        if (layers.isEmpty() || width <= 0 || height <= 0) return null
        val scale = SIDE.toFloat() / max(width, height)
        val w = (width * scale).roundToInt().coerceAtLeast(1)
        val h = (height * scale).roundToInt().coerceAtLeast(1)
        val bitmap = createBitmap(w, h)
        val canvas = Canvas(bitmap)
        canvas.drawColor(bg.toArgb())
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { isDither = true }
        // CSS lists the top layer first: drawn from the last.
        for (layer in layers.asReversed()) {
            paint.shader = layer.shader(w.toFloat(), h.toFloat())
            canvas.drawRect(0f, 0f, w.toFloat(), h.toFloat(), paint)
        }
        return bitmap
    }

    /**
     * Sunshine's sidebar (the web's `--sidebar`): a soft rainbow from the top
     * down, drawn over the sidebar's own `surface`; none for the others.
     */
    fun sidebar(theme: PleyTheme): List<Pair<Float, Color>>? = if (theme != PleyTheme.Sunshine) {
        null
    } else {
        listOf(
            0f to Color(255, 99, 146).copy(alpha = 0.22f),
            0.2f to Color(255, 158, 64).copy(alpha = 0.22f),
            0.4f to Color(255, 214, 0).copy(alpha = 0.24f),
            0.6f to Color(74, 214, 140).copy(alpha = 0.22f),
            0.8f to Color(56, 182, 255).copy(alpha = 0.22f),
            1f to Color(150, 100, 255).copy(alpha = 0.22f),
        )
    }

    /** One of a glow's gradients, in CSS's terms, its color fading to clear at [reach] of its ray. */
    private sealed interface Layer {
        fun shader(w: Float, h: Float): Shader
    }

    /**
     * `radial-gradient(rx ry at x y, color, transparent reach)`: an ellipse
     * [rx] by [ry] (shares of the width and height) centered at [x], [y];
     * `circle` (rx 0) reaches the farthest corner, as CSS's default size.
     */
    private class Radial(val rx: Float, val ry: Float, val x: Float, val y: Float, val color: Color, val reach: Float) : Layer {
        override fun shader(w: Float, h: Float): Shader {
            val cx = x * w
            val cy = y * h
            val (radiusX, radiusY) = if (rx == 0f) {
                val far = listOf(hypot(cx, cy), hypot(w - cx, cy), hypot(cx, h - cy), hypot(w - cx, h - cy)).max()
                far to far
            } else {
                rx * w to ry * h
            }
            // Clear in the same color, so the fade is the color thinning, as CSS blends premultiplied.
            val shader = RadialGradient(cx, cy, radiusX, intArrayOf(color.toArgb(), color.copy(alpha = 0f).toArgb()), floatArrayOf(0f, reach), Shader.TileMode.CLAMP)
            if (radiusY != radiusX) shader.setLocalMatrix(Matrix().apply { setScale(1f, radiusY / radiusX, cx, cy) })
            return shader
        }
    }

    /** `linear-gradient(to bottom, …)`: [stops] from the top (0) to the bottom (1). */
    private class Downward(val stops: List<Pair<Float, Color>>) : Layer {
        override fun shader(w: Float, h: Float): Shader =
            LinearGradient(0f, 0f, 0f, h, stops.map { it.second.toArgb() }.toIntArray(), stops.map { it.first }.toFloatArray(), Shader.TileMode.CLAMP)
    }

    /** The web's `--glow` of each theme, top layer first. */
    private fun layers(theme: PleyTheme): List<Layer> = when (theme) {
        PleyTheme.Twilight -> listOf(
            Downward(listOf(0f to Color(190, 60, 130, 0), 0.35f to Color(190, 60, 130, 0), 0.72f to Color(190, 60, 130).copy(alpha = 0.08f), 1f to Color(255, 122, 72).copy(alpha = 0.12f))),
            Radial(1.1f, 0.55f, 0.5f, 1.18f, Color(255, 138, 76).copy(alpha = 0.26f), 0.65f),
            Radial(0.6f, 0.5f, 0.96f, 1.06f, Color(244, 114, 182).copy(alpha = 0.26f), 0.7f),
            Radial(0.55f, 0.45f, 0.04f, 1.1f, Color(168, 85, 247).copy(alpha = 0.24f), 0.7f),
        )
        PleyTheme.Moonlight -> listOf(Radial(0.55f, 0.4f, 0.92f, -0.06f, Color(245, 214, 140).copy(alpha = 0.07f), 0.7f))
        PleyTheme.Nightshade -> listOf(Radial(0.7f, 0.45f, 0.88f, -0.08f, Color(95, 211, 154).copy(alpha = 0.07f), 0.7f))
        PleyTheme.Sunshine -> listOf(
            Radial(0f, 0f, 0.34f, 0.1f, Color(255, 99, 146).copy(alpha = 0.24f), 0.2f),
            Radial(0f, 0f, 0.96f, 0.08f, Color(56, 182, 255).copy(alpha = 0.28f), 0.22f),
            Radial(0f, 0f, 0.9f, 0.92f, Color(255, 196, 0).copy(alpha = 0.32f), 0.24f),
            Radial(0f, 0f, 0.42f, 0.96f, Color(74, 214, 140).copy(alpha = 0.26f), 0.22f),
            Radial(0f, 0f, 0.68f, 0.48f, Color(150, 100, 255).copy(alpha = 0.12f), 0.22f),
        )
        else -> emptyList()
    }
}

/**
 * The window under a shell's pages in [palette]: its background the
 * theme's page, glow and all ([Backdrop]), and on a phone the system's
 * bars' icons dark on a light theme. Set again as a pick or the window's
 * size changes, in the frame that draws the pick.
 */
@Composable
fun ThemedWindow(palette: PleyPalette) {
    val view = LocalView.current
    val size = LocalWindowInfo.current.containerSize
    val background = remember(palette, size) {
        Backdrop.page(palette.theme, palette.bg, size.width, size.height)
    }
    val applied = remember { arrayOfNulls<Any>(2) }
    SideEffect {
        if (applied[0] === palette && applied[1] === background) return@SideEffect
        applied[0] = palette
        applied[1] = background
        val window = view.context.activity()?.window ?: return@SideEffect
        val drawable: Drawable = background?.toDrawable(view.resources)?.apply { isFilterBitmap = true } ?: palette.bg.toArgb().toDrawable()
        window.setBackgroundDrawable(drawable)
        WindowCompat.getInsetsController(window, view).apply {
            isAppearanceLightStatusBars = palette.light
            isAppearanceLightNavigationBars = palette.light
        }
    }
}

private tailrec fun Context.activity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.activity()
    else -> null
}
