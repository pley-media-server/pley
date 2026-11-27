package app.pley.android.ui.phone.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import app.pley.android.designsystem.LocalReducedMotion
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.theme.Appearance
import app.pley.android.designsystem.theme.LocalPleyPalette
import app.pley.android.designsystem.theme.PleyPalette

/**
 * Material 3 in pley's tokens (design spec §1.1, §1.2, §1.4), in [palette]:
 * this device's theme ([Appearance], docs/THEMES.md), or a player's dark
 * twin of it ([PleyPalette.dark]), with no dynamic color. Phones and
 * tablets only; the TV builds tv-material's theme from the same tokens.
 * Nothing here defines a color, size or duration of its own.
 *
 * It also provides [LocalPleyPalette], [LocalReducedMotion] (the system's
 * "Remove animations") and the page's [PhoneMetrics] for the window's
 * width.
 */
@Composable
fun PhoneTheme(palette: PleyPalette = Appearance.palette, content: @Composable () -> Unit) {
    val reduced = LocalReducedMotion.current ?: PleyMotion.systemReducedMotion(LocalContext.current)
    val colors = remember(palette) { phoneColors(palette) }
    CompositionLocalProvider(LocalPleyPalette provides palette) {
        MaterialTheme(colorScheme = colors, typography = PhoneTypography, shapes = PhoneShapes) {
            CompositionLocalProvider(
                LocalReducedMotion provides reduced,
                LocalPhoneMetrics provides rememberPhoneMetrics(),
                content = content,
            )
        }
    }
}

/**
 * [palette] for what is drawn below, in Material's roles too: the shell's
 * choice between the profile's pick and Midnight outside a profile
 * ([Appearance.paletteFor]).
 */
@Composable
fun PhonePalette(palette: PleyPalette, content: @Composable () -> Unit) {
    val colors = remember(palette) { phoneColors(palette) }
    CompositionLocalProvider(LocalPleyPalette provides palette) {
        MaterialTheme(colorScheme = colors, typography = PhoneTypography, shapes = PhoneShapes, content = content)
    }
}

/** The Material roles of design spec §1.1 in [p], every one set: Material's defaults are a light purple scheme. */
fun phoneColors(p: PleyPalette): ColorScheme = darkColorScheme(
    primary = p.accent,
    onPrimary = p.accentFg,
    primaryContainer = p.primaryContainer,
    onPrimaryContainer = p.accentHover,
    inversePrimary = p.brandDeep,
    secondary = p.surface3,
    onSecondary = p.fg,
    secondaryContainer = p.surface3,
    onSecondaryContainer = p.fg,
    tertiary = p.brandLight,
    onTertiary = p.accentFg,
    tertiaryContainer = p.surface3,
    onTertiaryContainer = p.fg,
    background = p.bg,
    onBackground = p.fg,
    surface = p.bg,
    onSurface = p.fg,
    surfaceVariant = p.surface2,
    onSurfaceVariant = p.muted,
    surfaceTint = p.bg,
    inverseSurface = p.fg,
    inverseOnSurface = p.bg,
    error = p.danger,
    onError = p.dangerFg,
    errorContainer = p.surface3,
    onErrorContainer = p.danger,
    outline = p.control,
    outlineVariant = p.border,
    scrim = p.shade,
    surfaceBright = p.surface3,
    surfaceDim = p.bg,
    surfaceContainerLowest = p.bg,
    surfaceContainerLow = p.surface,
    surfaceContainer = p.surface2,
    surfaceContainerHigh = p.surface3,
    surfaceContainerHighest = p.border,
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
