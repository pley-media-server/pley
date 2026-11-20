package app.pley.android.ui.tv.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Density
import kotlin.math.min
import androidx.tv.material3.ColorScheme
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Shapes
import androidx.tv.material3.Typography
import androidx.tv.material3.darkColorScheme
import app.pley.android.common.Tier
import app.pley.android.designsystem.LocalReducedMotion
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType

/**
 * The TV's look: tv-material's theme built from pley's tokens (design spec
 * §1.1, §1.2, §1.4), plus the device's [tier] and whether to move less.
 *
 * The TV never uses Compose Material 3: the two libraries each have a
 * MaterialTheme, and mixing them mixes colors, type and shapes
 * (https://developer.android.com/training/tv/playback/compose). tv-material
 * starts from its light scheme and the phone's type scale, so every value
 * is set here, none left to its defaults.
 *
 * [reducedMotion] is pley's own "Reduce motion" switch; `null` leaves it to
 * the system's "Remove animations".
 */
@Composable
fun TvTheme(
    tier: Tier = Tier.T2,
    reducedMotion: Boolean? = null,
    content: @Composable () -> Unit,
) {
    val reduced = reducedMotion ?: PleyMotion.systemReducedMotion(LocalContext.current)
    CompositionLocalProvider(LocalDensity provides tvCanvasDensity()) {
        MaterialTheme(colorScheme = TvColors, shapes = TvShapes, typography = TvTypography) {
            CompositionLocalProvider(
                LocalTvTier provides tier,
                LocalReducedMotion provides reduced,
                content = content,
            )
        }
    }
}

/**
 * The density that makes the design's 960 × 540 dp canvas
 * ([PleyTv.CanvasWidth] by [PleyTv.CanvasHeight]) fill the screen, whatever
 * density the TV reports. Most 1080p TVs and sticks report 320 dpi, which is
 * that already; boxes that report 240 or 213 would otherwise show every
 * screen smaller or larger. On a screen that isn't 16:9 the canvas fits
 * the tighter side and grows along the other. The TV's own text size still
 * applies, up to [MAX_FONT_SCALE], so captions keep fitting their tiles.
 */
@Composable
internal fun tvCanvasDensity(): Density {
    val system = LocalDensity.current
    val window = LocalWindowInfo.current.containerSize
    return remember(window, system) { tvCanvasDensity(window.width, window.height, system.fontScale) ?: system }
}

/** [tvCanvasDensity] for a [width] by [height] px window; `null` before the window is measured. */
internal fun tvCanvasDensity(width: Int, height: Int, fontScale: Float): Density? {
    if (width <= 0 || height <= 0) return null
    val scale = min(width / PleyTv.CanvasWidth.value, height / PleyTv.CanvasHeight.value)
    return Density(scale, fontScale.coerceAtMost(MAX_FONT_SCALE))
}

/** The most the TV's text size setting enlarges text. */
internal const val MAX_FONT_SCALE = 1.3f

/**
 * The device's tier (architecture.md, design spec §6): T0 swaps backdrops
 * at once and asks for smaller pictures.
 */
val LocalTvTier = staticCompositionLocalOf { Tier.T2 }

/**
 * The scheme of design spec §1.1, the phone's roles with `border` as the
 * focus ring's color and no tint on surfaces.
 */
internal val TvColors: ColorScheme = darkColorScheme(
    primary = PleyColors.Accent,
    onPrimary = PleyColors.OnAccent,
    primaryContainer = PleyColors.PrimaryContainer,
    onPrimaryContainer = PleyColors.AccentBright,
    inversePrimary = PleyColors.BrandDeep,
    secondary = PleyColors.Surface3,
    onSecondary = PleyColors.Fg,
    secondaryContainer = PleyColors.Surface3,
    onSecondaryContainer = PleyColors.Fg,
    tertiary = PleyColors.BrandLight,
    onTertiary = PleyColors.OnAccent,
    tertiaryContainer = PleyColors.Surface3,
    onTertiaryContainer = PleyColors.BrandLight,
    background = PleyColors.Bg,
    onBackground = PleyColors.Fg,
    surface = PleyColors.Bg,
    onSurface = PleyColors.Fg,
    surfaceVariant = PleyColors.Surface2,
    onSurfaceVariant = PleyColors.Muted,
    surfaceTint = Color.Transparent,
    inverseSurface = PleyColors.Fg,
    inverseOnSurface = PleyColors.Bg,
    error = PleyColors.Danger,
    onError = PleyColors.Bg,
    errorContainer = PleyColors.Surface3,
    onErrorContainer = PleyColors.DangerText,
    border = PleyColors.AccentBright,
    borderVariant = PleyColors.Control,
    scrim = PleyColors.Bg,
)

/** tv-material's roles set to pley's TV type (design spec §1.2). */
internal val TvTypography = Typography(
    displayLarge = PleyType.Tv.HeroTitle,
    displayMedium = PleyType.Tv.HeroTitle,
    displaySmall = PleyType.Tv.PageTitle,
    headlineLarge = PleyType.Tv.PageTitle,
    headlineMedium = PleyType.Tv.PageTitle,
    headlineSmall = PleyType.Tv.Section,
    titleLarge = PleyType.Tv.Section,
    titleMedium = PleyType.Tv.Title,
    titleSmall = PleyType.Tv.Label,
    bodyLarge = PleyType.Tv.BodyLarge,
    bodyMedium = PleyType.Tv.Body,
    bodySmall = PleyType.Tv.Caption,
    labelLarge = PleyType.Tv.Label,
    labelMedium = PleyType.Tv.Caption,
    labelSmall = PleyType.Tv.Badge,
)

/** Badges 4, fields and tiles 8, surfaces 12, dialogs and panels 16, sheets 20 (design spec §1.4). */
internal val TvShapes = Shapes(
    extraSmall = RoundedCornerShape(PleyShape.BadgeRadius),
    small = RoundedCornerShape(PleyShape.FieldRadius),
    medium = RoundedCornerShape(PleyShape.CardRadius),
    large = RoundedCornerShape(PleyShape.DialogRadius),
    extraLarge = RoundedCornerShape(PleyShape.SheetRadius),
)
