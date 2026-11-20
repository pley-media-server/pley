package app.pley.android.ui.phone.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalContext
import app.pley.android.designsystem.LocalReducedMotion
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType

/**
 * Material 3 in pley's tokens (design spec §1.1, §1.2, §1.4): dark only,
 * as the web, with no dynamic color. Phones and tablets only; the TV
 * builds tv-material's theme from the same tokens. Nothing here defines a
 * color, size or duration of its own.
 *
 * It also provides [LocalReducedMotion] (the system's "Remove animations")
 * and the page's [PhoneMetrics] for the window's width.
 */
@Composable
fun PhoneTheme(content: @Composable () -> Unit) {
    val reduced = LocalReducedMotion.current ?: PleyMotion.systemReducedMotion(LocalContext.current)
    MaterialTheme(colorScheme = PhoneColors, typography = PhoneTypography, shapes = PhoneShapes) {
        CompositionLocalProvider(
            LocalReducedMotion provides reduced,
            LocalPhoneMetrics provides rememberPhoneMetrics(),
            content = content,
        )
    }
}

/** The Material roles of design spec §1.1, every one set: Material's defaults are a light purple scheme. */
val PhoneColors: ColorScheme = darkColorScheme(
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
    onTertiaryContainer = PleyColors.Fg,
    background = PleyColors.Bg,
    onBackground = PleyColors.Fg,
    surface = PleyColors.Bg,
    onSurface = PleyColors.Fg,
    surfaceVariant = PleyColors.Surface2,
    onSurfaceVariant = PleyColors.Muted,
    surfaceTint = PleyColors.Bg,
    inverseSurface = PleyColors.Fg,
    inverseOnSurface = PleyColors.Bg,
    error = PleyColors.Danger,
    onError = PleyColors.Bg,
    errorContainer = PleyColors.Surface3,
    onErrorContainer = PleyColors.DangerText,
    outline = PleyColors.Control,
    outlineVariant = PleyColors.Surface4,
    scrim = PleyColors.Bg,
    surfaceBright = PleyColors.Surface3,
    surfaceDim = PleyColors.Bg,
    surfaceContainerLowest = PleyColors.Bg,
    surfaceContainerLow = PleyColors.Surface,
    surfaceContainer = PleyColors.Surface2,
    surfaceContainerHigh = PleyColors.Surface3,
    surfaceContainerHighest = PleyColors.Surface4,
)

/** pley's phone type as Material's roles (design spec §1.2): Roboto at 400, 500 and 700 only. */
val PhoneTypography: Typography = Typography(
    displayLarge = PleyType.Phone.DisplaySmall,
    displayMedium = PleyType.Phone.DisplaySmall,
    displaySmall = PleyType.Phone.DisplaySmall,
    headlineLarge = PleyType.Phone.HeadlineMedium,
    headlineMedium = PleyType.Phone.HeadlineMedium,
    headlineSmall = PleyType.Phone.HeadlineSmall,
    titleLarge = PleyType.Phone.TitleLarge,
    titleMedium = PleyType.Phone.TitleMedium,
    titleSmall = PleyType.Phone.TitleSmall,
    bodyLarge = PleyType.Phone.BodyLarge,
    bodyMedium = PleyType.Phone.BodyMedium,
    bodySmall = PleyType.Phone.BodySmall,
    labelLarge = PleyType.Phone.LabelLarge,
    labelMedium = PleyType.Phone.LabelMedium,
    labelSmall = PleyType.Phone.LabelSmall,
)

/**
 * Material's shape scale from pley's radii (design spec §1.4): badges 4,
 * tiles 6, fields 8, cards 12, dialogs 16, sheets 20 (top corners only,
 * which the sheet itself asks for).
 */
val PhoneShapes: Shapes = Shapes(
    extraSmall = PleyShape.Badge,
    small = RoundedCornerShape(PleyShape.FieldRadius),
    medium = PleyShape.Card,
    large = PleyShape.Dialog,
    extraLarge = RoundedCornerShape(PleyShape.SheetRadius),
)
