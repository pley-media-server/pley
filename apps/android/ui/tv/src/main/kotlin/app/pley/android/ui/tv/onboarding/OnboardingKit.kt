package app.pley.android.ui.tv.onboarding

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.Surface
import androidx.tv.material3.SurfaceDefaults
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.PleyMark
import app.pley.android.screens.onboarding.OnboardingSource
import app.pley.android.ui.tv.kit.OverlayShowing
import io.nayuki.qrcodegen.QrCode
import kotlinx.coroutines.delay
import kotlin.math.floor

/**
 * What the screens of getting in read and change: `null` (the app's, from
 * its graph) unless a test or a preview provides its own.
 */
val LocalOnboardingSource = staticCompositionLocalOf<OnboardingSource?> { null }

/** The pley mark on its tile and the wordmark, [height] tall (design spec §2.8: 40 on the link screen). */
@Composable
internal fun PleyLogo(modifier: Modifier = Modifier, height: Dp = 40.dp) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(height * 0.4f)) {
        Image(rememberVectorPainter(PleyMark.Tile), contentDescription = null, modifier = Modifier.size(height))
        BasicText(
            "Pley",
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg, fontSize = (height.value * 0.75f).sp, lineHeight = height.value.sp),
            modifier = Modifier.semantics { contentDescription = "Pley" },
        )
    }
}

/**
 * The sign-in screens' background (design spec §2.8): [bg][PleyColors.Bg]
 * with [brandDeep][PleyColors.BrandDeep] at 18 % glowing from the
 * bottom-right corner, gone by 60 % of the width. One gradient, drawn once.
 */
internal fun Modifier.brandGlow(): Modifier = drawWithCache {
    val brush = Brush.radialGradient(
        0f to PleyColors.BrandDeep.copy(alpha = 0.18f),
        1f to Color.Transparent,
        center = Offset(size.width, size.height),
        radius = size.width * 0.6f,
    )
    onDrawBehind { drawRect(brush) }
}

/**
 * The 8 dp [accent][PleyColors.Accent] dot of "Waiting for approval…": it
 * breathes between 40 % and 100 % over 2 s, redrawn twice a second only
 * (design spec §6); still with reduced motion.
 */
@Composable
internal fun BreathingDot(modifier: Modifier = Modifier, color: Color = PleyColors.Accent) {
    val alpha = remember { Animatable(1f) }
    LaunchedEffect(Unit) {
        // Steps of 500 ms: 100, 70, 40, 70 % and round again.
        val steps = floatArrayOf(1f, 0.7f, 0.4f, 0.7f)
        var i = 0
        while (true) {
            alpha.snapTo(steps[i % steps.size])
            i++
            delay(500)
        }
    }
    Box(
        modifier
            .size(8.dp)
            .drawBehind { drawCircle(color.copy(alpha = color.alpha * alpha.value)) },
    )
}

/**
 * A QR code of [text], black on white with its 4-module quiet zone, in a
 * box [size] wide: each module is a whole number of pixels, the code
 * centred in what is left (tv-addendum §4.2).
 */
@Composable
internal fun QrCodeImage(text: String, size: Dp, modifier: Modifier = Modifier) {
    val code = remember(text) { runCatching { QrCode.encodeText(text, QrCode.Ecc.MEDIUM) }.getOrNull() }
    Box(
        modifier
            .size(size)
            .drawWithCache {
                val qr = code
                val path = Path()
                var origin = Offset.Zero
                if (qr != null) {
                    val modules = qr.size + QUIET * 2
                    val cell = floor(this.size.minDimension / modules).coerceAtLeast(1f)
                    val side = cell * modules
                    origin = Offset(floor((this.size.width - side) / 2f), floor((this.size.height - side) / 2f))
                    for (y in 0 until qr.size) {
                        for (x in 0 until qr.size) {
                            if (qr.getModule(x, y)) {
                                path.addRect(androidx.compose.ui.geometry.Rect(Offset((x + QUIET) * cell, (y + QUIET) * cell), Size(cell, cell)))
                            }
                        }
                    }
                }
                onDrawBehind {
                    drawRect(PleyColors.QrLight)
                    translate(origin.x, origin.y) { drawPath(path, PleyColors.QrDark) }
                }
            },
    )
}

private const val QUIET = 4

/**
 * A dialog (design spec §2.10): a [surface2][PleyColors.Surface2] card,
 * radius 16, at most 560 wide, centred over a [scrim][PleyColors.Scrim];
 * [title] in tvTitle, [body] in tvBody muted, and its buttons
 * right-aligned. Back calls [onDismiss]. The buttons say where focus
 * starts (the safe choice). Focus stays on the buttons while it is open,
 * never reaching the page behind the scrim, and goes to [returnTo] (what
 * opened it) once it has closed.
 */
@Composable
internal fun TvDialog(
    title: String,
    body: String,
    onDismiss: () -> Unit,
    returnTo: FocusRequester? = null,
    buttons: @Composable () -> Unit,
) {
    BackHandler(onBack = onDismiss)
    OverlayShowing()
    val view = LocalView.current
    // Once the dialog's buttons are gone from the focus tree, after this frame.
    DisposableEffect(returnTo) { onDispose { returnTo?.let { r -> view.post { runCatching { r.requestFocus() } } } } }
    Box(Modifier.fillMaxSize().background(PleyColors.Scrim), contentAlignment = Alignment.Center) {
        Surface(
            modifier = Modifier.widthIn(max = 560.dp).focusProperties { onExit = { cancelFocusChange() } }.focusGroup(),
            shape = PleyShape.Dialog,
            colors = SurfaceDefaults.colors(containerColor = PleyColors.Surface2, contentColor = PleyColors.Fg),
        ) {
            Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
                BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
                Row(
                    Modifier.fillMaxWidth().padding(top = 16.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End),
                ) { buttons() }
            }
        }
    }
}

/** A status line: the breathing dot when [waiting], then [text] in tvLabel. */
@Composable
internal fun StatusLine(text: String, modifier: Modifier = Modifier, waiting: Boolean = false, style: TextStyle = PleyType.Tv.Label.copy(color = PleyColors.Muted)) {
    Row(modifier.heightIn(min = 20.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (waiting) BreathingDot()
        BasicText(text, style = style, maxLines = 2)
    }
}
