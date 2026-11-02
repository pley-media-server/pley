package app.pley.android.player.music

import android.content.Intent
import android.view.KeyEvent
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaNotification
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The remote's Stop key pauses the music; the notification swiped away still ends it. */
@OptIn(markerClass = [UnstableApi::class])
@RunWith(RobolectricTestRunner::class)
class StopKeyTest {
    private var pauses = 0
    private val stop = StopKey { pauses++ }

    private fun button(code: Int, action: Int = KeyEvent.ACTION_DOWN, repeat: Int = 0) =
        Intent(Intent.ACTION_MEDIA_BUTTON).putExtra(Intent.EXTRA_KEY_EVENT, KeyEvent(0, 0, action, code, repeat))

    @Test
    fun theStopKeyPausesOnceAndIsHandled() {
        assertTrue(stop.handle(button(KeyEvent.KEYCODE_MEDIA_STOP)))
        assertTrue(stop.handle(button(KeyEvent.KEYCODE_MEDIA_STOP, repeat = 1)))
        assertTrue(stop.handle(button(KeyEvent.KEYCODE_MEDIA_STOP, KeyEvent.ACTION_UP)))
        assertEquals(1, pauses)
    }

    @Test
    fun theNotificationSweptAwayAndOtherKeysAreMedia3s() {
        val dismissed = button(KeyEvent.KEYCODE_MEDIA_STOP).putExtra(MediaNotification.NOTIFICATION_DISMISSED_EVENT_KEY, true)
        assertFalse(stop.handle(dismissed))
        assertFalse(stop.handle(button(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE)))
        assertFalse(stop.handle(button(KeyEvent.KEYCODE_MEDIA_NEXT)))
        assertFalse(stop.handle(Intent(Intent.ACTION_MEDIA_BUTTON)))
        assertEquals(0, pauses)
    }
}
