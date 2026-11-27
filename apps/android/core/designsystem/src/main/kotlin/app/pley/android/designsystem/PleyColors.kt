package app.pley.android.designsystem

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import app.pley.android.designsystem.theme.LocalPleyPalette
import app.pley.android.designsystem.theme.PleyPalette

/**
 * Pley's colors (design spec §1.1) in the theme shown: the palette of
 * [LocalPleyPalette], which the shells provide from this device's pick
 * (docs/THEMES.md). Each is a token of packages/themes/themes.json, or made
 * of them; the phone builds its Material 3 scheme from these and the TV its
 * tv-material one, and neither defines a color of its own.
 *
 * Read in composition: what draws later (a draw block, a click) takes the
 * color read where it was made, which a pick makes again.
 */
object PleyColors {
    /** The palette itself, for what keeps several colors or hands them on. */
    val palette: PleyPalette
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current

    /** The window, painted once as the window background. `bg`. */
    val Bg: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.bg

    /** The expanded rail, settings groups. `surface`. */
    val Surface: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.surface

    /** Tile wells, inputs, dialogs, panels, skeletons. `surface-2`. */
    val Surface2: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.surface2

    /** Unfocused buttons and chips, the current page in lists. `surface-3`. */
    val Surface3: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.surface3

    /** Dividers, hairlines, the progress track. `border`. */
    val Surface4: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.border

    /** Decorative borders only (badges on [Bg]): never the only edge of a control. `border-strong`. */
    val Outline: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.borderStrong

    /** Edges of interactive things: input outlines, empty PIN dots, outlined pills ([PleyPalette.control]). */
    val Control: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.control

    /** Primary text and icons. `fg`. */
    val Fg: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.fg

    /** Secondary text. `muted`. */
    val Muted: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.muted

    /** Tertiary text: dates, separators, hints. `subtle`. */
    val Subtle: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.subtle

    /** Disabled text and icons only, never information ([PleyPalette.disabled]). */
    val Disabled: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.disabled

    /** TV focus fills; phone primary actions; progress; checks. `accent`. */
    val Accent: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.accent

    /** The TV focus ring; text links on the phone; a pressed accent. `accent-hover`. */
    val AccentBright: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.accentHover

    /** Content on [Accent] and [AccentBright]. `accent-fg`. */
    val OnAccent: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.accentFg

    /** The phone's primary container: [Accent] at 20 % over [Bg]. */
    val PrimaryContainer: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.primaryContainer

    /** The logo's front poster, from this to [Brand]; the TV banner and the sign-in screens' brand gradient. `brand-light`. */
    val BrandLight: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.brandLight

    /** The logo's front poster, from [BrandLight] to this. `brand`. */
    val Brand: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.brand

    /** The logo's middle poster; brand gradients, never behind text. `brand-deep`. */
    val BrandDeep: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.brandDeep

    /** The logo's back poster. `brand-far`. */
    val BrandFar: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.brandFar

    /** The logo's tile. `brand-tile`. */
    val BrandTile: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.brandTile

    /** "Signed in", "On this network" dots; a request ready to watch. `success`. */
    val Success: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.success

    /** Waiting on someone: a request waiting for approval. `warning`, with [Bg] words on it. */
    val Warning: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.warning

    /** Destructive fills, with [DangerFg] content. `danger`. */
    val Danger: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.danger

    /** Words and icons on a [Danger] fill. `danger-fg`. */
    val DangerFg: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.dangerFg

    /** Error text: `danger`, which reads on every surface. */
    val DangerText: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.danger

    /** Informational notices. `info`. */
    val Info: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.info

    /** Shadows. `shade`. */
    val Shade: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.shade

    /** Badges, the watched tick and round buttons on art: [Bg] at 80 %. */
    val OnArt: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.onArt

    /** The hairline inside a tile's well, so art the page's color doesn't dissolve into it: [Fg] at 6 %. */
    val TileHairline: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.tileHairline

    /** Behind dialogs, sheets and panels: [Shade], dimmer. */
    val Scrim: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.scrim

    /** The web's name for [Surface4]. */
    val Border: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.border

    /** The web's name for [Outline]. */
    val BorderStrong: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.borderStrong

    /** The web's name for [AccentBright]. */
    val AccentHover: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.accentHover

    /** The web's name for [OnAccent]. */
    val AccentFg: Color
        @Composable @ReadOnlyComposable
        get() = LocalPleyPalette.current.accentFg

    /** QR codes only, in every theme: a scanner reads dark on light. */
    val QrDark = Color(0xFF000000)

    /** QR codes only. */
    val QrLight = Color(0xFFFFFFFF)

    /** The progress track over art: black at 60 %, on the picture in every theme. */
    val ProgressTrackOnArt = Color.Black.copy(alpha = 0.6f)
}
