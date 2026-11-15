package app.pley.android.ui.phone.remote

import app.pley.android.common.UiText
import app.pley.android.testing.MainDispatcherRule
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Play on…'s cast on its way, which outlives the chooser and the activity being made again. */
class PlayOnCastsTest {
    @get:Rule val main = MainDispatcherRule()

    @Test
    fun oneCastStartsAtATimeAndSaysOnWhichDevice() = runTest {
        val casts = PlayOnCasts()
        val gate = CompletableDeferred<Unit>()
        assertTrue(casts.start("cast:abc") { gate.await() })
        assertEquals("cast:abc", casts.starting.value)
        // Another, from a chooser opened again meanwhile, waits its turn: it doesn't run.
        var second = false
        assertFalse(casts.start("app-12") { second = true })
        assertFalse(second)
        gate.complete(Unit)
        assertNull(casts.starting.value)
        assertTrue(casts.start("app-12") { second = true })
        assertTrue(second)
        assertNull(casts.starting.value)
    }

    @Test
    fun whatItTellsIsKeptUntilTheHostReadsIt() = runTest {
        val casts = PlayOnCasts()
        // Told while no host listens, as while the activity is made again.
        casts.tell(CastNews.Sent("cast:abc", "Bedroom Chromecast"))
        casts.tell(CastNews.Refused(UiText.Raw("Bedroom Chromecast didn't play it.")))
        assertEquals(
            listOf(CastNews.Sent("cast:abc", "Bedroom Chromecast"), CastNews.Refused(UiText.Raw("Bedroom Chromecast didn't play it."))),
            casts.told.take(2).toList(),
        )
        casts.tell(CastNews.Sent("app-12", "Living room TV"))
        assertEquals(CastNews.Sent("app-12", "Living room TV"), casts.told.first())
    }
}
