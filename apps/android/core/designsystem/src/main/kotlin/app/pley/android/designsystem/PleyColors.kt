package app.pley.android.designsystem

import androidx.compose.ui.graphics.Color

/**
 * pley's colours (design spec §1.1): the web's (apps/web/src/index.css),
 * dark only, with four native changes for contrast: `subtle` is lighter,
 * its old value is now [Disabled] and [Control], and [DangerText] is new.
 * The phone builds its Material 3 scheme from these and the TV its
 * tv-material one; neither defines a colour of its own.
 *
 * Contrast ratios are against [Bg] unless said otherwise.
 */
object PleyColors {
    /** The window, painted once as the window background. */
    val Bg = Color(0xFF0B0D10)

    /** The expanded rail, settings groups. */
    val Surface = Color(0xFF12151A)

    /** Tile wells, inputs, dialogs, panels, skeletons. */
    val Surface2 = Color(0xFF191D24)

    /** Unfocused buttons and chips, the current page in lists. */
    val Surface3 = Color(0xFF222731)

    /** Dividers, hairlines, the progress track. The web's `--color-border`. */
    val Surface4 = Color(0xFF2A303B)

    /** Decorative borders only (badges on [Bg]), 1.9:1: never the only edge of a control. The web's `--color-border-strong`. */
    val Outline = Color(0xFF3A4250)

    /** Edges of interactive things: input outlines, empty PIN dots, outlined pills. 4.2:1, 3.2 on [Surface3]. */
    val Control = Color(0xFF6C7584)

    /** Primary text and icons, 16.3:1. Text is never pure white. */
    val Fg = Color(0xFFE9EBEF)

    /** Secondary text, 8.2:1 (7.1 on [Surface2]). */
    val Muted = Color(0xFFA1A9B6)

    /** Tertiary text: dates, separators, 6.4:1. */
    val Subtle = Color(0xFF8B95A5)

    /** Disabled text and icons only, never information (4.2:1). The web's `--color-subtle`. */
    val Disabled = Color(0xFF6C7584)

    /** TV focus fills; phone primary actions; progress; checks (9.1:1). */
    val Accent = Color(0xFF38BDF8)

    /** The TV focus ring; text links on the phone; a pressed accent (11.7:1). The web's `--color-accent-hover`. */
    val AccentBright = Color(0xFF7DD3FC)

    /** Content on [Accent] (8.8:1) and [AccentBright] (11.3:1). The web's `--color-accent-fg`. */
    val OnAccent = Color(0xFF03131F)

    /** The phone's primary container: [Accent] at 20 % over [Bg]. */
    val PrimaryContainer = Color(0xFF14303E)

    /** The logo, the TV banner and the sign-in screens' brand gradient. */
    val BrandLight = Color(0xFFBAE6FD)

    /** Brand gradients only: never behind text (3.4:1 with [Fg]). */
    val BrandDeep = Color(0xFF0284C7)

    /** The logo's back poster. */
    val BrandFar = Color(0xFF0C4A6E)

    /** The logo's tile. */
    val BrandTile = Color(0xFF0A1622)

    /** "Signed in", "On this network" dots; a request ready to watch (6.9:1 on [Surface]). */
    val Success = Color(0xFF3FB27F)

    /** Waiting on someone: a request waiting for approval (9.4:1 on [Surface]). */
    val Amber = Color(0xFFF2B347)

    /** Destructive fills, with [Bg] content (5.0:1). Never text on surfaces: that is [DangerText]. */
    val Danger = Color(0xFFE5484D)

    /** Error text: 7.0:1, 6.1 on [Surface2], 5.4 on [Surface3]. */
    val DangerText = Color(0xFFFF6B70)

    /** Informational notices. */
    val Info = Color(0xFF4C9AFF)

    /** QR codes only. */
    val QrDark = Color(0xFF000000)

    /** QR codes only. */
    val QrLight = Color(0xFFFFFFFF)

    /** Badges, the watched tick and round buttons on art: [Bg] at 80 %. */
    val OnArt = Bg.copy(alpha = 0.8f)

    /** The progress track over art: black at 60 %. */
    val ProgressTrackOnArt = Color.Black.copy(alpha = 0.6f)

    /** The hairline inside a tile's well, so dark art doesn't dissolve into [Bg]: white at 6 %. */
    val TileHairline = Color.White.copy(alpha = 0.06f)

    /** Behind dialogs, sheets and panels: [Bg] at 60 %. */
    val Scrim = Bg.copy(alpha = 0.6f)

    /** The web's name for [Surface4]. */
    val Border get() = Surface4

    /** The web's name for [Outline]. */
    val BorderStrong get() = Outline

    /** The web's name for [AccentBright]. */
    val AccentHover get() = AccentBright

    /** The web's name for [OnAccent]. */
    val AccentFg get() = OnAccent
}
