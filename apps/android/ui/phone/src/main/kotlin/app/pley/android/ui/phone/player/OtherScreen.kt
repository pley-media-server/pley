package app.pley.android.ui.phone.player

import android.app.Activity
import android.content.Context
import android.content.pm.ActivityInfo
import android.content.pm.ApplicationInfo
import android.view.Display
import androidx.activity.compose.LocalActivity
import androidx.car.app.connection.CarConnection
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import app.pley.android.designsystem.text.LocalMinTextSize
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.rememberPhoneMetrics
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Whether a window shows on a screen other than the device's own: the
 * car's, where Android Auto shows Pley while parked, or another display
 * the phone drives. Such a screen keeps its orientation and has no picture
 * in picture: the players don't turn it and don't shrink. On a car's
 * ([isCar]) they are car-sized ([CarSized]) and pause as Android Auto
 * covers them when the car moves off.
 */
object OtherScreen {
    fun shows(activity: Activity): Boolean = ContextCompat.getDisplayOrDefault(activity).displayId != Display.DEFAULT_DISPLAY

    /**
     * [activity] shows on a car's screen: another screen while Android Auto
     * projects ([CarConnection], as Google tells parked apps to check).
     * Until Android Auto has answered, another screen counts as a car's:
     * an app it opens starts before the answer comes. A debuggable build
     * counts the developer options' simulated displays as cars' too, so the
     * car's layouts can be tried on an emulator, where Android Auto can't run.
     */
    fun isCar(activity: Activity): Boolean {
        if (!shows(activity)) return false
        if (projection.value != false) return true
        val debuggable = activity.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0
        return debuggable && ContextCompat.getDisplayOrDefault(activity).name.startsWith(SIMULATED_DISPLAY)
    }

    /** How Android names the displays its developer options simulate ("Overlay #1"). */
    private const val SIMULATED_DISPLAY = "Overlay #"

    /** Whether Android Auto projects now; `null` until it says. */
    private val projection = MutableStateFlow<Boolean?>(null)

    /** Whether Android Auto projects, as it says it: what is drawn follows it ([onCarScreen]). */
    val projected: StateFlow<Boolean?> get() = projection

    /** A build that never opens on a car's screen (the store build): no other screen is a car's. */
    fun neverACar() {
        projection.value = false
    }

    /** Follows Android Auto's connection for the app's life: the Application calls it once. */
    fun follow(context: Context) {
        CarConnection(context.applicationContext).type.observeForever { type ->
            projection.value = type == CarConnection.CONNECTION_TYPE_PROJECTION
        }
    }
}

/**
 * Whether what is composed shows on a car's screen ([OtherScreen.isCar]): asked again as Android Auto answers (a window
 * on another screen counts as a car's until it has) and as the window moves to another screen, the phone's as Android
 * Auto goes, which changes its configuration.
 */
@Composable
internal fun onCarScreen(): Boolean {
    val activity = LocalActivity.current
    val projected by OtherScreen.projected.collectAsState()
    val configuration = LocalConfiguration.current
    return remember(activity, projected, configuration) { activity != null && OtherScreen.isCar(activity) }
}

/**
 * A player's window turned for its screen: landscape, either way up, on a phone's own; a tablet's and another screen's
 * left as they are, as turning a car's would letterbox the picture. Asked again as the window moves, to the phone's
 * screen as the car goes.
 */
internal fun Activity.turnForScreen() {
    // In picture in picture or split screen the configuration is the window's, not the screen's: a tablet's small window
    // would pass for a phone's, and the tablet would turn as it leaves. Asked again once the window is whole. Another
    // screen's window (a car's, whatever its windowing) is never turned.
    if (isInPictureInPictureMode || (isInMultiWindowMode && !OtherScreen.shows(this))) return
    val phone = resources.configuration.smallestScreenWidthDp < TABLET_DP && !OtherScreen.shows(this)
    val wanted = if (phone) ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE else ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
    if (requestedOrientation != wanted) requestedOrientation = wanted
}

/** The smallest width, in dp, from which a screen is a tablet's. */
private const val TABLET_DP = 600

/** The player draws for a car's screen ([CarSized]): fewer, larger controls. */
internal val LocalCarScreen = staticCompositionLocalOf { false }

/**
 * [content] as a car's screen wants it when [car] (Google's car app
 * quality criteria): a third larger, so the player's 48 dp buttons are
 * 64 dp there, and no text under 24 sp. A screen arm's length away, used
 * at a glance, with no room for a phone's small print.
 */
@Composable
internal fun CarSized(car: Boolean, content: @Composable () -> Unit) {
    // Already car-sized by the page around it (the whole app is, on a car's screen): not again.
    if (!car || LocalCarScreen.current) {
        content()
        return
    }
    val density = LocalDensity.current
    val type = MaterialTheme.typography
    // The car's screen sets its text's size, as Android Auto's own does: the phone's font size setting would push it out
    // of the car's layouts, which already hold no text under 24 sp.
    val larger = remember(density) { Density(density.density * CAR_SCALE, fontScale = 1f) }
    val smallest = (CAR_TEXT.value / CAR_SCALE).sp
    val readable = remember(type) { type.forCar(smallest) }
    // The theme's styles are raised here; text in a size of its own (a tile's caption, a badge) reads the floor itself.
    CompositionLocalProvider(LocalDensity provides larger, LocalCarScreen provides true, LocalMinTextSize provides smallest) {
        // The page's widths and margins as the car's dp have them, not the phone's: measured again inside.
        CompositionLocalProvider(LocalPhoneMetrics provides rememberPhoneMetrics()) {
            MaterialTheme(colorScheme = MaterialTheme.colorScheme, typography = readable, shapes = MaterialTheme.shapes, content = content)
        }
    }
}

/**
 * A menu's or a sheet's [content], car-sized as the page that opened it.
 * Such a window starts again from the screen's own density, so a page's
 * [CarSized] doesn't reach it; its text styles do, already raised.
 */
@Composable
internal fun CarSizedWindow(content: @Composable () -> Unit) {
    if (!LocalCarScreen.current) {
        content()
        return
    }
    val density = LocalDensity.current
    val larger = remember(density) { Density(density.density * CAR_SCALE, fontScale = 1f) }
    CompositionLocalProvider(LocalDensity provides larger, content = content)
}

/** How much larger the player draws on a car's screen: 48 dp to 64. */
internal const val CAR_SCALE = 64f / 48f

/** The smallest text on a car's screen. */
private val CAR_TEXT = 24.sp

/** How far the car's controls keep from the screen's edges and from each other, once scaled: 24 dp. */
internal val CarGap = (24f / CAR_SCALE).dp

/**
 * The theme's styles on a car's screen: body text and labels at [floor]
 * (24 sp once scaled), and titles a step above each other from there, so
 * a list's title still stands over its second line and a page's heading
 * over both. Raised to the floor alone, a phone's 16 sp list title, 14 sp
 * body and 12 sp caption would all read the same. Line heights grow with
 * their text; a style already larger keeps its size.
 */
internal fun Typography.forCar(floor: TextUnit): Typography {
    fun TextStyle.grown(to: Float = floor.value): TextStyle {
        val least = maxOf(to, floor.value)
        if (!fontSize.isSpecified || fontSize.value >= least) return this
        val ratio = if (lineHeight.isSpecified) lineHeight.value / fontSize.value else 1.25f
        return copy(fontSize = least.sp, lineHeight = (least * ratio).sp)
    }
    return Typography(
        displayLarge = displayLarge.grown(),
        displayMedium = displayMedium.grown(),
        displaySmall = displaySmall.grown(),
        headlineLarge = headlineLarge.grown(CAR_HEADLINE_MEDIUM),
        headlineMedium = headlineMedium.grown(CAR_HEADLINE_MEDIUM),
        headlineSmall = headlineSmall.grown(CAR_HEADLINE_SMALL),
        titleLarge = titleLarge.grown(CAR_TITLE_LARGE),
        titleMedium = titleMedium.grown(CAR_TITLE_MEDIUM),
        titleSmall = titleSmall.grown(),
        bodyLarge = bodyLarge.grown(),
        bodyMedium = bodyMedium.grown(),
        bodySmall = bodySmall.grown(),
        labelLarge = labelLarge.grown(),
        labelMedium = labelMedium.grown(),
        labelSmall = labelSmall.grown(),
    )
}

/** The car's titles, in sp before [CAR_SCALE]: 26.7, 30, 34 and 38 sp on its screen, over body text's 24. */
private const val CAR_TITLE_MEDIUM = 20f
private const val CAR_TITLE_LARGE = 22.5f
private const val CAR_HEADLINE_SMALL = 25.5f
private const val CAR_HEADLINE_MEDIUM = 28.5f
