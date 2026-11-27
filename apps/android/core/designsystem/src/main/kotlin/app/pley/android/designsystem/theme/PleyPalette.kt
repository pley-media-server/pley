package app.pley.android.designsystem.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.lerp

/**
 * The colors of [theme] in the accent picked over it, [pick] (`null`: the theme's own), every one the
 * app draws: the tokens of packages/themes/themes.json (docs/THEMES.md,
 * "Tokens"), each the accent's where the theme follows it, as the web's
 * `colorOf`, and a few the app makes of them. One instance per pick, made
 * once ([of]), so screens compare palettes by reference.
 *
 * Screens read it through PleyColors, from [LocalPleyPalette].
 */
@Immutable
class PleyPalette private constructor(val theme: PleyTheme, val pick: PleyAccent?) {
    private val own = ThemeTable.colors(theme)
    private val follows = ThemeTable.follows(theme)
    private val picked = pick?.let { ThemeTable.accent(it, theme.light) }.orEmpty()

    /** The color [token] has in this palette: the accent's where the theme follows it and the accent sets it, else the theme's own. */
    fun color(token: Token): Color = Color((picked[token]?.takeIf { token in follows } ?: own.getValue(token)))

    /** Whether the theme is light: the system's bars draw dark icons, and players draw in [dark]. */
    val light: Boolean get() = theme.light

    val bg = color(Token.Bg)
    val surface = color(Token.Surface)
    val surface2 = color(Token.Surface2)
    val surface3 = color(Token.Surface3)
    val border = color(Token.Border)
    val borderStrong = color(Token.BorderStrong)
    val fg = color(Token.Fg)
    val muted = color(Token.Muted)
    val subtle = color(Token.Subtle)
    val accent = color(Token.Accent)
    val accentHover = color(Token.AccentHover)
    val accentFg = color(Token.AccentFg)
    val success = color(Token.Success)
    val danger = color(Token.Danger)
    val dangerFg = color(Token.DangerFg)
    val warning = color(Token.Warning)
    val info = color(Token.Info)
    val shade = color(Token.Shade)
    val brandLight = color(Token.BrandLight)
    val brand = color(Token.Brand)
    val brandDeep = color(Token.BrandDeep)
    val brandFar = color(Token.BrandFar)
    val brandTile = color(Token.BrandTile)

    /**
     * Edges of things to press (input outlines, empty PIN dots, outlined
     * pills): the theme's `subtle`, which reads on every surface as WCAG
     * asks of a control's edge, where `border` is a hairline.
     */
    val control = subtle

    /** Disabled words and icons, never information: `subtle`, faded a fifth toward the page. */
    val disabled = lerp(bg, subtle, 0.8f)

    /** The phone's primary container: the accent at 20 % over the page. */
    val primaryContainer = accent.copy(alpha = 0.2f).compositeOver(bg)

    /** Badges, the watched tick and round buttons on art: the page at 80 %. */
    val onArt = bg.copy(alpha = 0.8f)

    /** Behind dialogs, sheets and panels: the theme's shade at 60 % of its own strength. */
    val scrim = shade.copy(alpha = shade.alpha * 0.6f)

    /** The hairline inside a tile's well, so art the page's color doesn't dissolve into it: the words' color at 6 %. */
    val tileHairline = fg.copy(alpha = 0.06f)

    /** The palette players and what is drawn over video take: a light theme's dark twin's, in the same accent; a dark theme's own. */
    val dark: PleyPalette get() = of(theme.twin, pick)

    override fun toString(): String = "PleyPalette(${theme.id}, ${pick?.id ?: "own"})"

    companion object {
        private val made = arrayOfNulls<PleyPalette>(PleyTheme.entries.size * (PleyAccent.entries.size + 1))

        /** The palette of [theme] in [accent] (`null`: its own), made once. */
        fun of(theme: PleyTheme, accent: PleyAccent?): PleyPalette {
            val index = theme.ordinal * (PleyAccent.entries.size + 1) + (accent?.ordinal?.plus(1) ?: 0)
            return made[index] ?: synchronized(made) { made[index] ?: PleyPalette(theme, accent).also { made[index] = it } }
        }

        /** Pley's own: Midnight in its own accent, what a device that picked nothing shows. */
        val Default: PleyPalette get() = of(PleyTheme.Default, null)
    }
}

/**
 * The palette what is drawn below takes. The shells provide this device's
 * pick ([Appearance]), the players its dark twin; Appearance's previews
 * each their own. Static: a pick recomposes everything below once, and a
 * color read costs no tracking while lists scroll.
 */
val LocalPleyPalette = staticCompositionLocalOf { PleyPalette.Default }
