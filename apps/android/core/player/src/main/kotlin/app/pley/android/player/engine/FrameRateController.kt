package app.pley.android.player.engine

import android.content.Context
import android.hardware.display.DisplayManager
import android.os.Handler
import android.os.Looper
import android.view.Display
import android.view.Window
import androidx.core.content.edit
import app.pley.android.caps.AndroidDisplayInfo
import app.pley.android.caps.DisplayState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.coroutines.resume

/** The window whose display mode the controller changes. */
interface DisplayModeHost {
    /** Names the display, for remembering timeouts per display. */
    val displayKey: String

    /** The display's modes, the one in use and the person's match-content choice. */
    fun read(): DisplayState?

    /** Asks for mode [modeId]; 0 hands the choice back to the system. */
    fun request(modeId: Int)

    /** Waits until the display runs [modeId]; `false` after [timeoutMs]. */
    suspend fun awaitMode(modeId: Int, timeoutMs: Long): Boolean
}

/** Timeouts and HDR types turned off per display, and the one-time hint, kept across playbacks. */
interface FrameRateMemory {
    suspend fun timeouts(displayKey: String): Int

    suspend fun setTimeouts(displayKey: String, count: Int)

    /** Display.HdrCapabilities types seen turned off in the system's settings ([DisplayState.turnedOffHdrTypes]). */
    suspend fun turnedOffHdrTypes(displayKey: String): Set<Int>

    suspend fun setTurnedOffHdrTypes(displayKey: String, types: Set<Int>)

    /** `true` the first time only: the hint is shown once. */
    suspend fun takeHint(): Boolean
}

/** What a match did. */
data class MatchOutcome(
    val decision: ModeDecision,
    /** The display changed to the mode asked for. */
    val switched: Boolean = false,
    /** It was asked to and didn't within the timeout. */
    val timedOut: Boolean = false,
    /** Show once where the device's own setting is. */
    val showHint: Boolean = false,
)

/**
 * Changes a TV's display mode to the video's frame rate before it plays
 * (TV addendum §5.5): it asks through the window's preferredDisplayModeId,
 * which works on every Android version and on Fire OS, then waits for the
 * display to report the mode (5 s at most) and about 2 s more for the TV
 * to settle. Two timeouts in a row on a display make Auto stop asking
 * there, and a hint is shown once. The mode stays through pauses; [clear]
 * gives the choice back when the player closes.
 */
class FrameRateController(
    private val host: DisplayModeHost,
    private val memory: FrameRateMemory,
    private val sdk: Int,
    private val switchTimeoutMs: Long = SWITCH_TIMEOUT_MS,
    private val settleMs: Long = SETTLE_MS,
) {
    /** The mode asked for, until [clear]. */
    var requestedModeId: Int = 0
        private set

    /** The display's name in [memory]. */
    val displayKey: String get() = host.displayKey

    /** The display as it is now. */
    fun read(): DisplayState? = host.read()

    /**
     * Matches the display to [request]; [onSwitching] runs once a change is
     * asked for, before the wait, with the refresh rate asked for.
     */
    suspend fun match(request: MatchRequest, onSwitching: (refreshRate: Float) -> Unit = {}): MatchOutcome {
        val key = host.displayKey
        val timeouts = memory.timeouts(key)
        val asked = request.copy(gaveUp = timeouts >= GIVE_UP_AFTER)
        return when (val decision = ModeChoice.decide(host.read(), asked, sdk)) {
            is ModeDecision.Keep -> MatchOutcome(decision, showHint = decision.reason == KeepReason.GaveUp && memory.takeHint())
            is ModeDecision.Switch -> {
                requestedModeId = decision.modeId
                host.request(decision.modeId)
                onSwitching(decision.refreshRate)
                if (host.awaitMode(decision.modeId, switchTimeoutMs)) {
                    if (timeouts != 0) memory.setTimeouts(key, 0)
                    delay(settleMs)
                    MatchOutcome(decision, switched = true)
                } else {
                    val count = timeouts + 1
                    memory.setTimeouts(key, count)
                    val hint = request.setting == RefreshRateMatching.Auto && count >= GIVE_UP_AFTER && memory.takeHint()
                    MatchOutcome(decision, timedOut = true, showHint = hint)
                }
            }
        }
    }

    /** Hands the display mode back to the system (0), if one was asked for. */
    fun clear() {
        if (requestedModeId != 0) {
            requestedModeId = 0
            host.request(0)
        }
    }

    companion object {
        const val SWITCH_TIMEOUT_MS = 5_000L
        const val SETTLE_MS = 2_000L
        const val GIVE_UP_AFTER = 2
    }
}

/** A player activity's window, on the display it shows on. */
class WindowModeHost(private val window: Window) : DisplayModeHost {
    private val context: Context = window.context
    private val displays: DisplayManager? = context.getSystemService(DisplayManager::class.java)
    private val main = Handler(Looper.getMainLooper())

    private val displayId: Int
        get() = window.decorView.display?.displayId ?: Display.DEFAULT_DISPLAY

    override val displayKey: String
        get() = displayKey(displays, displayId)

    override fun read(): DisplayState? = AndroidDisplayInfo(context) { displayId }.read()

    override fun request(modeId: Int) {
        val attributes = window.attributes
        if (attributes.preferredDisplayModeId != modeId) {
            attributes.preferredDisplayModeId = modeId
            window.attributes = attributes
        }
    }

    override suspend fun awaitMode(modeId: Int, timeoutMs: Long): Boolean {
        val manager = displays ?: return false
        val id = displayId
        fun running(): Boolean = manager.getDisplay(id)?.mode?.modeId == modeId
        if (running()) return true
        return withTimeoutOrNull(timeoutMs) {
            suspendCancellableCoroutine { continuation ->
                val listener = object : DisplayManager.DisplayListener {
                    override fun onDisplayAdded(displayId: Int) = Unit

                    override fun onDisplayRemoved(displayId: Int) = Unit

                    override fun onDisplayChanged(displayId: Int) {
                        if (displayId == id && running() && continuation.isActive) {
                            manager.unregisterDisplayListener(this)
                            continuation.resume(true)
                        }
                    }
                }
                manager.registerDisplayListener(listener, main)
                continuation.invokeOnCancellation { main.post { manager.unregisterDisplayListener(listener) } }
                // The change may have landed between the first look and the listener.
                if (running() && continuation.isActive) {
                    manager.unregisterDisplayListener(listener)
                    continuation.resume(true)
                }
            }
        } ?: false
    }
}

/** A display's name in [FrameRateMemory]: its id and name, "0:HDMI". */
fun displayKey(displays: DisplayManager?, displayId: Int): String =
    displays?.getDisplay(displayId)?.let { "${it.displayId}:${it.name}" } ?: "$displayId"

/** [FrameRateMemory] in the app's preferences, read and written off the main thread. */
class PreferencesFrameRateMemory(context: Context) : FrameRateMemory {
    private val app = context.applicationContext
    private val prefs by lazy { app.getSharedPreferences(FILE, Context.MODE_PRIVATE) }

    override suspend fun timeouts(displayKey: String): Int = withContext(Dispatchers.IO) { prefs.getInt(KEY_TIMEOUTS + displayKey, 0) }

    override suspend fun setTimeouts(displayKey: String, count: Int) = withContext(Dispatchers.IO) {
        prefs.edit { putInt(KEY_TIMEOUTS + displayKey, count) }
    }

    override suspend fun turnedOffHdrTypes(displayKey: String): Set<Int> = withContext(Dispatchers.IO) {
        prefs.getString(KEY_HDR_OFF + displayKey, null).orEmpty().split(',').mapNotNull { it.toIntOrNull() }.toSet()
    }

    override suspend fun setTurnedOffHdrTypes(displayKey: String, types: Set<Int>) = withContext(Dispatchers.IO) {
        prefs.edit { putString(KEY_HDR_OFF + displayKey, types.sorted().joinToString(",")) }
    }

    override suspend fun takeHint(): Boolean = withContext(Dispatchers.IO) {
        if (prefs.getBoolean(KEY_HINT, false)) {
            false
        } else {
            prefs.edit { putBoolean(KEY_HINT, true) }
            true
        }
    }

    private companion object {
        const val FILE = "pley_frame_rate"
        const val KEY_TIMEOUTS = "timeouts."
        const val KEY_HDR_OFF = "hdr_off."
        const val KEY_HINT = "hint_shown"
    }
}
