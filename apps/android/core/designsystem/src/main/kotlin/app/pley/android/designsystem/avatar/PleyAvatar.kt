package app.pley.android.designsystem.avatar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.image.ArtworkBox
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkLoad
import kotlin.math.pow

/**
 * How every avatar draws a profile, on phones and TVs alike, as the web's
 * Avatar.tsx and every other app (docs/THEMES.md, "Profile colors";
 * packages/tv/test/fixtures/avatar.json pins it). These are the profile's
 * colors, not the theme's: they stay under every theme and accent.
 */
object ProfileLook {
    /** The colors profiles can choose, and the fallback picked from the name. */
    val colors: List<Color> = listOf(
        Color(0xFF38BDF8), Color(0xFFA78BFA), Color(0xFFF472B6), Color(0xFFFB923C),
        Color(0xFFFACC15), Color(0xFF4ADE80), Color(0xFF2DD4BF), Color(0xFFF87171),
    )

    /** The initial's two colors: Midnight's background on light circles, white on dark ones. */
    val InkDark = Color(0xFF0B0D10)
    val InkLight = Color(0xFFFFFFFF)

    private val Hex = Regex("^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$")

    /**
     * The circle's color: the profile's [chosen] one ("#e07a5f", "#abc", any
     * case); without a valid one, one of [colors] picked from [name], its
     * code points folded as h = (h * 31 + code point) mod 2^32.
     */
    fun color(name: String, chosen: String?): Color {
        parse(chosen)?.let { return it }
        var hash = 0L
        var i = 0
        while (i < name.length) {
            val point = name.codePointAt(i)
            hash = (hash * 31 + point) and 0xFFFFFFFFL
            i += Character.charCount(point)
        }
        return colors[(hash % colors.size).toInt()]
    }

    /** The initial's color on [circle]: [InkDark] or [InkLight], whichever has WCAG's higher contrast with it (dark on a tie). */
    fun ink(circle: Color): Color {
        val on = luminance(circle) + 0.05
        val dark = on / (luminance(InkDark) + 0.05)
        val light = (luminance(InkLight) + 0.05) / on
        return if (dark >= light) InkDark else InkLight
    }

    /** The trimmed name's first character (a whole code point: an emoji, a Chinese name) in capitals; "?" for a blank name. */
    fun initial(name: String): String {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return "?"
        return String(Character.toChars(trimmed.codePointAt(0))).uppercase()
    }

    /** "#rgb" or "#rrggbb" as a color; `null` for anything else. */
    private fun parse(hex: String?): Color? {
        val digits = hex?.let { Hex.matchEntire(it) }?.groupValues?.get(1) ?: return null
        val full = if (digits.length == 3) digits.map { "$it$it" }.joinToString("") else digits
        return Color(0xFF000000 or full.toLong(16))
    }

    /** WCAG's relative luminance, from the color's 8-bit sRGB channels. */
    private fun luminance(color: Color): Double {
        val argb = color.toArgb()
        fun linear(shift: Int): Double {
            val c = (argb shr shift and 0xFF) / 255.0
            return if (c <= 0.04045) c / 12.92 else ((c + 0.055) / 1.055).pow(2.4)
        }
        return 0.2126 * linear(16) + 0.7152 * linear(8) + 0.0722 * linear(0)
    }
}

/**
 * A profile's avatar, [size] across: its [picture]; while that loads, and
 * without one or when it fails, a circle in the profile's [color] with the
 * name's initial on it ([ProfileLook]). [style] sets the initial's font,
 * and [ProfileLook.ink] its color. Decoration for screen readers: the name
 * is always said beside it.
 */
@Composable
fun PleyAvatar(name: String, color: String?, picture: ArtworkImage?, size: Dp, style: TextStyle, modifier: Modifier = Modifier) {
    val fill = remember(name, color) { ProfileLook.color(name, color) }
    val load = remember(picture) { ArtworkLoad() }
    Box(
        modifier.size(size).clip(PleyShape.Circle).background(fill).clearAndSetSemantics {},
        contentAlignment = Alignment.Center,
    ) {
        // The initial until the picture is in: alone while it loads or when it fails, under it as it fades in.
        if (picture == null || !load.loaded) {
            BasicText(ProfileLook.initial(name), style = style.copy(color = ProfileLook.ink(fill)), maxLines = 1)
        }
        if (picture != null) ArtworkBox(picture, load, Modifier.size(size))
    }
}
