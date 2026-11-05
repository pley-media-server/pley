package app.pley.android.player.session

import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.testing.player.session.FakePlaybackController
import app.pley.android.testing.player.session.FakePlaybackServices
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The player screens' fake: it can show every field of the state, and records what the screen asks. */
@RunWith(RobolectricTestRunner::class)
class FakePlaybackControllerTest {
    @Test
    fun `everything fills every field of the state`() {
        val defaults = PlayerUiState()
        val filled = FakePlaybackController.everything()
        val fields = PlayerUiState::class.java.declaredFields.filter { !java.lang.reflect.Modifier.isStatic(it.modifiers) }
        for (field in fields) {
            field.isAccessible = true
            assertNotEquals("${field.name} is left as it was", field.get(defaults), field.get(filled))
        }
    }

    @Test
    fun `it shows what it is given and records the commands`() {
        val fake = FakePlaybackController()
        fake.set(FakePlaybackController.everything())
        fake.update { it.copy(phase = PlayerPhase.Error("No", canRetry = true)) }
        assertEquals(PlayerPhase.Error("No", canRetry = true), fake.state.value.phase)
        fake.seekTo(5_000)
        fake.selectSubtitle(null)
        fake.selectQuality("4m")
        fake.dismissNotice()
        assertNull(fake.state.value.notice)
        fake.dismissRemoteNotice()
        assertNull(fake.state.value.remoteNotice)
        assertEquals(listOf("seekTo", "selectSubtitle", "selectQuality", "dismissNotice", "dismissRemoteNotice"), fake.calls.map { it.name })
        assertEquals(listOf(5_000L), fake.calls("seekTo").single().args)
        assertEquals(3_000L, fake.saveState().positionMs)
        assertEquals("4m", fake.saveState().quality)

        val services = FakePlaybackServices()
        val key = services.prepare(PlayRequest(7))
        assertEquals(listOf(PlayRequest(7)), services.requests)
        assertEquals(true, services.controller(key) is FakePlaybackController)
    }
}
