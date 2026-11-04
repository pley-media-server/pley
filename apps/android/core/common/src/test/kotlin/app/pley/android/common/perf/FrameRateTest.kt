package app.pley.android.common.perf

import android.app.Activity
import android.app.Application
import android.content.Context
import android.content.Intent
import android.view.View
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.android.controller.ActivityController

/**
 * The frame-rate counter's switch: off at first, the device's choice kept
 * and read back, a launch's extra for that run only, and the box in each
 * started window while it's on.
 */
@RunWith(RobolectricTestRunner::class)
class FrameRateTest {
    private lateinit var app: Application

    @Before
    fun start() {
        app = RuntimeEnvironment.getApplication()
        FrameRate.reset(app)
        FrameRate.follow(app, shells = emptySet())
    }

    @After
    fun end() {
        FrameRate.reset(app)
    }

    private fun stored(): Boolean = app.getSharedPreferences("frame_rate", Context.MODE_PRIVATE).getBoolean("on", false)

    private fun box(activity: Activity): View? = activity.window.decorView.findViewWithTag(FrameRate.BOX_TAG)

    private fun intent(frames: Boolean?) = Intent(app, Activity::class.java).apply { if (frames != null) putExtra(FrameRate.EXTRA, frames) }

    @Test
    fun itsOffUntilSwitchedOnAndTheDevicesChoiceIsKept() {
        assertFalse(FrameRate.on.value)
        FrameRate.show(app, true)
        assertTrue(FrameRate.on.value)
        assertTrue(stored())
        FrameRate.show(app, false)
        assertFalse(FrameRate.on.value)
        assertFalse(stored())
    }

    @Test
    fun theDevicesChoiceIsReadBackAtTheNextStart() {
        app.getSharedPreferences("frame_rate", Context.MODE_PRIVATE).edit().putBoolean("on", true).commit()
        FrameRate.load(app)
        assertTrue(FrameRate.on.value)
    }

    @Test
    fun aChoiceMadeBeforeTheReadFinishesStands() {
        app.getSharedPreferences("frame_rate", Context.MODE_PRIVATE).edit().putBoolean("on", true).commit()
        FrameRate.reset(app)
        FrameRate.show(app, false)
        app.getSharedPreferences("frame_rate", Context.MODE_PRIVATE).edit().putBoolean("on", true).commit()
        FrameRate.load(app)
        assertFalse(FrameRate.on.value)
    }

    @Test
    fun aLaunchsExtraIsForThatRunAndANewChoiceEndsIt() {
        FrameRate.show(app, true)
        FrameRate.read(intent(frames = false))
        assertFalse(FrameRate.on.value)
        // Kept as the device's choice all along.
        assertTrue(stored())
        FrameRate.read(intent(frames = null))
        assertFalse(FrameRate.on.value)
        FrameRate.show(app, true)
        assertTrue(FrameRate.on.value)
    }

    @Test
    fun anActivityStartedWithTheExtraTurnsItOn() {
        val controller = Robolectric.buildActivity(Activity::class.java, intent(frames = true)).setup()
        assertTrue(FrameRate.on.value)
        assertFalse(stored())
        assertNotNull(box(controller.get()))
        controller.pause().stop().destroy()
    }

    @Test
    fun theBoxIsInEachStartedWindowWhileItsOn() {
        val controller: ActivityController<Activity> = Robolectric.buildActivity(Activity::class.java).setup()
        val activity = controller.get()
        assertNull(box(activity))
        FrameRate.show(app, true)
        val shown = box(activity)
        assertNotNull(shown)
        assertFalse(shown!!.isFocusable)
        assertEquals(View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS, shown.importantForAccessibility)
        // Stopped, its window isn't watched; started again, it is.
        controller.pause().stop()
        assertNull(box(activity))
        controller.restart().resume()
        assertNotNull(box(activity))
        FrameRate.show(app, false)
        assertNull(box(activity))
        controller.pause().stop().destroy()
    }

    @Test
    fun aRouteIsNamedWithoutItsIds() {
        assertEquals("Item", FrameRate.nameOf(Route.Item(itemId = 4)))
        assertEquals("Home", FrameRate.nameOf(Route.Home))
    }

    private sealed interface Route {
        data object Home : Route

        data class Item(val itemId: Long) : Route
    }
}
