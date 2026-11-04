package app.pley.android.common.perf

import android.annotation.SuppressLint
import android.app.Activity
import android.app.Application
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.util.Log
import android.util.TypedValue
import android.view.Display
import android.view.FrameMetrics
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.Window
import android.widget.FrameLayout
import android.widget.TextView
import androidx.annotation.MainThread
import androidx.annotation.VisibleForTesting
import androidx.core.content.edit
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import kotlin.math.ceil
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * The frame-rate counter, for judging smoothness on a device in a release
 * build (a debug build's Compose is too slow to judge by), as Samsung's
 * profiling build does. While it's on, each window the app shows reports
 * its frames (the window's frame metrics, on a thread of its own), and
 * every half second something moves, a line goes to the log under [TAG]
 * ("fps=58 longest=21.4 missed=2 screen=Home") and a small box in the
 * window's top corner says the same: frames a second, the longest frame in
 * milliseconds, and how many missed their deadline. At rest it says
 * nothing new, as nothing is drawn.
 *
 * It's switched on in Diagnostics, kept on the device, or for one run by
 * the launch intent's [EXTRA] (`--ez frames true`, or `false` to keep it
 * off). Off, nothing of it runs: no thread, no listener, no box.
 *
 * Its switch is a file of its own rather than the app's settings: the
 * windows need it before the data side is made, and it belongs to the
 * device, not to a profile.
 */
object FrameRate {
    /** The log's tag. */
    const val TAG = "PleyFrames"

    /** The launch intent's extra that turns it on or off for this run. */
    const val EXTRA = "frames"

    /** The box's view tag. */
    const val BOX_TAG = "pley.frames"

    private const val FILE = "frame_rate"
    private const val KEY_ON = "on"

    private val state = MutableStateFlow(false)

    /** Whether it's on: the device's choice, or this run's ([EXTRA]). */
    val on: StateFlow<Boolean> = state.asStateFlow()

    /** The device's choice, once read or made; guarded by this object. */
    private var stored = false
    private var storedKnown = false

    /** This run's choice ([EXTRA]), which wins over the device's until a new one is made. */
    private var forced: Boolean? = null

    /**
     * Where the app's shell is: its route, set as it navigates, named in
     * the lines of the windows of [watch]'s shells ("Home" for `Home`,
     * "Item" for `Item(itemId=4)`). Only read when a line is written.
     */
    @Volatile
    var route: Any? = null

    private var shells: Set<Class<out Activity>> = emptySet()

    /** The started activities, each with its watch while it's on; main thread only. */
    private val started = LinkedHashMap<Activity, Watch?>()

    /** The watches' thread and its handler, while any watch runs; main thread only. */
    private var thread: HandlerThread? = null
    private var handler: Handler? = null

    private val main by lazy { Handler(Looper.getMainLooper()) }

    /**
     * Follows the app's activities, watching each started one's window
     * while it's on, and reads the device's choice off the main thread.
     * [shells] are the activities whose lines name [route].
     */
    fun watch(app: Application, shells: Set<Class<out Activity>>) {
        follow(app, shells)
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch { load(app) }
    }

    /** Follows the app's activities without reading the device's choice. */
    internal fun follow(app: Application, shells: Set<Class<out Activity>>) {
        this.shells = shells
        app.registerActivityLifecycleCallbacks(Windows)
    }

    /** The device's choice, kept; it ends this run's ([EXTRA]). */
    fun show(context: Context, on: Boolean) {
        synchronized(this) {
            stored = on
            storedKnown = true
            forced = null
        }
        update()
        prefs(context).edit { putBoolean(KEY_ON, on) }
    }

    /** This run's choice, from [intent]'s [EXTRA] when it has one. */
    fun read(intent: Intent?) {
        // Any app may start the launchers, with extras that don't unparcel.
        val asked = runCatching { intent?.takeIf { it.hasExtra(EXTRA) }?.getBooleanExtra(EXTRA, false) }.getOrNull() ?: return
        synchronized(this) { forced = asked }
        update()
    }

    /** Reads the device's choice; one made meanwhile ([show]) stands. */
    internal fun load(context: Context) {
        val saved = prefs(context).getBoolean(KEY_ON, false)
        synchronized(this) {
            if (storedKnown) return
            stored = saved
            storedKnown = true
        }
        update()
    }

    /** Forgets every choice and every activity, for tests. */
    @VisibleForTesting
    internal fun reset(context: Context) {
        synchronized(this) {
            stored = false
            storedKnown = false
            forced = null
        }
        route = null
        update()
        started.clear()
        prefs(context).edit(commit = true) { clear() }
    }

    /** "Home" for a route `Home`, "Item" for `Item(itemId=4)`: its name without its ids. */
    internal fun nameOf(route: Any): String = route.toString().substringBefore('(')

    private fun prefs(context: Context) = context.applicationContext.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    private fun update() {
        synchronized(this) { state.value = forced ?: stored }
        if (Looper.myLooper() == Looper.getMainLooper()) refresh() else main.post { refresh() }
    }

    /** Starts or stops the started activities' watches as [on] says. */
    @MainThread
    private fun refresh() {
        val on = state.value
        for ((activity, watch) in started.entries.toList()) {
            if (on && watch == null) {
                started[activity] = Watch(activity, handler()).also { it.start() }
            } else if (!on && watch != null) {
                watch.stop()
                started[activity] = null
            }
        }
        if (!on) {
            thread?.quitSafely()
            thread = null
            handler = null
        }
    }

    @MainThread
    private fun handler(): Handler = handler ?: run {
        val t = HandlerThread("pley-frames").also { it.start() }
        thread = t
        Handler(t.looper).also { handler = it }
    }

    /** The name a line gives [activity]'s window: the shell's route, or the activity's own. */
    private fun screenOf(activity: Activity): String {
        val shown = route.takeIf { activity.javaClass in shells }
        return shown?.let(::nameOf) ?: activity.javaClass.simpleName
    }

    private object Windows : Application.ActivityLifecycleCallbacks {
        override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {
            if (savedInstanceState == null) read(activity.intent)
        }

        override fun onActivityStarted(activity: Activity) {
            started[activity] = if (state.value) Watch(activity, handler()).also { it.start() } else null
        }

        override fun onActivityStopped(activity: Activity) {
            started.remove(activity)?.stop()
        }

        override fun onActivityDestroyed(activity: Activity) {
            started.remove(activity)?.stop()
        }

        override fun onActivityResumed(activity: Activity) = Unit

        override fun onActivityPaused(activity: Activity) = Unit

        override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit
    }

    /** One window's frames, counted on [handler]'s thread, told in the log and in its box. */
    private class Watch(private val activity: Activity, private val handler: Handler) : Window.OnFrameMetricsAvailableListener {
        private val tally = FrameTally()
        private val display: Display? = displayOf(activity)
        private val box = boxFor(activity)

        @Volatile
        private var watching = false

        /** Before Android 12, which doesn't say a frame's deadline: the display's refresh, read as each window opens (a TV switches modes for a video). */
        private var interval = FrameTally.intervalNanos(60f)

        private val close = Runnable {
            if (watching) tally.close()?.let(::tell)
        }

        @MainThread
        fun start() {
            watching = true
            (activity.window.decorView as? ViewGroup)?.addView(box)
            activity.window.addOnFrameMetricsAvailableListener(this, handler)
        }

        @MainThread
        fun stop() {
            watching = false
            // Never added when the window had no renderer.
            runCatching { activity.window.removeOnFrameMetricsAvailableListener(this) }
            handler.removeCallbacks(close)
            (box.parent as? ViewGroup)?.removeView(box)
        }

        override fun onFrameMetricsAvailable(window: Window, metrics: FrameMetrics, dropCountSinceLastInvocation: Int) {
            // The window's first frame builds it: no measure of how it moves.
            if (!watching || metrics.getMetric(FrameMetrics.FIRST_DRAW_FRAME) == 1L) return
            if (!tally.open) {
                handler.postDelayed(close, tally.windowMs)
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) interval = FrameTally.intervalNanos(display?.refreshRate ?: 60f)
            }
            tally.lost(dropCountSinceLastInvocation)
            val deadline = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) metrics.getMetric(FrameMetrics.DEADLINE) else interval
            tally.add(metrics.getMetric(FrameMetrics.TOTAL_DURATION), deadline)
        }

        private fun tell(reading: FrameReading) {
            Log.i(TAG, "${reading.line} screen=${screenOf(activity)}")
            box.post {
                box.text = reading.label
                box.setTextColor(
                    when (reading.grade) {
                        FrameReading.Grade.Smooth -> SMOOTH
                        FrameReading.Grade.Some -> SOME
                        FrameReading.Grade.Many -> MANY
                    },
                )
            }
        }
    }

    /** The display whose refresh is a frame's deadline, before Android 12 only: later ones say it with each frame. */
    private fun displayOf(activity: Activity): Display? {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) return null
        @Suppress("DEPRECATION")
        return runCatching { activity.windowManager.defaultDisplay }.getOrNull()
    }

    /**
     * The box: white on translucent black where pages put nothing. On a TV,
     * at the top right in the band above the safe area's top edge, which
     * the pages leave empty (their tabs and buttons start below it); on a
     * phone, at the top centre under the status bar, between the bar's
     * title and its buttons. Its width is fixed at its longest text's, so a
     * new reading redraws it without laying the window out again. It takes
     * no focus, touch or reader's attention. Its words are units, as the
     * log's are, so they aren't translated.
     */
    @SuppressLint("SetTextI18n")
    private fun boxFor(activity: Activity): TextView {
        val dp = activity.resources.displayMetrics.density
        val tv = (activity.resources.configuration.uiMode and Configuration.UI_MODE_TYPE_MASK) == Configuration.UI_MODE_TYPE_TELEVISION
        return TextView(activity).apply {
            tag = BOX_TAG
            typeface = Typeface.MONOSPACE
            // Short enough to fit above a TV's safe area, and above a phone's top bar's title.
            setTextSize(TypedValue.COMPLEX_UNIT_SP, if (tv) 12f else 11f)
            setTextColor(Color.WHITE)
            maxLines = 1
            gravity = Gravity.CENTER
            val padH = (8 * dp).toInt()
            val padV = ((if (tv) 3 else 2) * dp).toInt()
            setPadding(padH, padV, padH, padV)
            background = GradientDrawable().apply {
                setColor(BACKGROUND)
                cornerRadius = 6 * dp
            }
            text = "-- fps"
            isFocusable = false
            isClickable = false
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
            val width = ceil(paint.measureText(WIDEST)).toInt() + 2 * padH
            val corner = Gravity.TOP or if (tv) Gravity.END else Gravity.CENTER_HORIZONTAL
            layoutParams = FrameLayout.LayoutParams(width, ViewGroup.LayoutParams.WRAP_CONTENT, corner).apply {
                // A TV's band above its safe area (TvSafeArea: 27 dp down, 48 in); a phone's status bar until its insets say.
                topMargin = ((if (tv) 3 else 24) * dp).toInt()
                marginEnd = if (tv) (48 * dp).toInt() else 0
            }
            if (!tv) {
                ViewCompat.setOnApplyWindowInsetsListener(this) { view, insets ->
                    val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
                    (view.layoutParams as? FrameLayout.LayoutParams)?.let { params ->
                        params.topMargin = bars.top
                        view.layoutParams = params
                    }
                    insets
                }
            }
        }
    }

    /** The box's widest text, which sets its width. */
    private const val WIDEST = "000 fps  000.0 ms  00 late"

    private const val BACKGROUND = 0xB3000000.toInt()
    private const val SMOOTH = 0xFF4ADE80.toInt()
    private const val SOME = 0xFFFBBF24.toInt()
    private const val MANY = 0xFFF87171.toInt()
}
