package app.pley.android.ui.phone.remote

import app.pley.android.common.UiText
import app.pley.android.data.OtherDevice
import app.pley.android.data.RemoteRepository
import app.pley.android.data.RemoteSession
import app.pley.android.model.CastRequest
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.network.ApiException
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.ui.phone.R
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Play on…'s cast on its way, which outlives the chooser and the activity
 * being made again: one at a time, and what it comes to goes to the chooser
 * open then, or to a toast with none open.
 */
class PlayOnCastsTest {
    @get:Rule val main = MainDispatcherRule()

    private val request = CastRequest(itemId = 101)

    /** A remote whose casts wait for [gate], then are refused with [refusal] when there is one. */
    private class Remote : RemoteRepository {
        var gate: CompletableDeferred<Unit>? = null
        var refusal: Exception? = null
        val sent = mutableListOf<String>()
        override val sessions: StateFlow<List<RemoteSession>> = MutableStateFlow(emptyList())
        override val devices: StateFlow<List<OtherDevice>> = MutableStateFlow(emptyList())
        override val clock: () -> Long = { 0L }
        override suspend fun refresh(): List<RemoteSession> = emptyList()
        override suspend fun command(sessionId: String, command: PlayerCommandRequest) = Unit
        override suspend fun stop(sessionId: String) = Unit
        override suspend fun cast(receiverId: String, request: CastRequest) {
            gate?.await()
            refusal?.let { throw it }
            sent += receiverId
        }
        override val castNeedsPin: Boolean = false
        override fun here(player: Any, sessionId: String?) = Unit
    }

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
    fun aRefusalGoesToTheChooserOpenWhenItComesWhicheverOpeningSentIt() = runTest {
        val casts = PlayOnCasts()
        val remote = Remote().apply {
            gate = CompletableDeferred()
            refusal = ApiException(409, "cast_failed", "the device didn't play it")
        }
        val first = PlayOnCasts.nextOpening()
        casts.opened(first)
        casts.cast(remote, "cast:abc", "cast:abc", "Bedroom Chromecast", request, from = first)
        // Closed, then opened again, before the device answers.
        casts.closed(first)
        val again = PlayOnCasts.nextOpening()
        casts.opened(again)
        remote.gate!!.complete(Unit)
        val failed = UiText.res(R.string.play_on_failed, "Bedroom Chromecast")
        assertEquals(CastOutcome.Refused("cast:abc", failed), casts.forChooser.value)
        casts.took(casts.forChooser.value!!)
        assertNull(casts.forChooser.value)
    }

    @Test
    fun withNoChooserOpenItIsAToastAndOneClosedBeforeShowingItPassesItOn() = runTest {
        val casts = PlayOnCasts()
        val remote = Remote().apply { refusal = ApiException(409, "cast_failed", "the device didn't play it") }
        val failed = UiText.res(R.string.play_on_failed, "Bedroom Chromecast")
        casts.cast(remote, "cast:abc", "cast:abc", "Bedroom Chromecast", request, from = PlayOnCasts.nextOpening())
        assertEquals(CastNews.Toast(failed), casts.told.first())

        // Shown to a chooser that closed before it read it: a toast says it instead.
        val opening = PlayOnCasts.nextOpening()
        casts.opened(opening)
        casts.cast(remote, "cast:abc", "cast:abc", "Bedroom Chromecast", request, from = opening)
        assertEquals(CastOutcome.Refused("cast:abc", failed), casts.forChooser.value)
        casts.closed(opening)
        assertNull(casts.forChooser.value)
        assertEquals(CastNews.Toast(failed), casts.told.first())
    }

    @Test
    fun aCastThatWentIsFollowedAndClosesOnlyTheOpeningItWasSentFrom() = runTest {
        val casts = PlayOnCasts()
        val remote = Remote()
        val opening = PlayOnCasts.nextOpening()
        casts.opened(opening)
        casts.cast(remote, "cast:abc", "cast:abc", "Bedroom Chromecast", request, from = opening)
        assertEquals(CastNews.Sent("cast:abc", "Bedroom Chromecast"), casts.told.first())
        assertEquals(CastOutcome.Sent("cast:abc", opening), casts.forChooser.value)
        casts.took(casts.forChooser.value!!)

        // Sent from an opening that closed, another open now: that one stays.
        remote.gate = CompletableDeferred()
        casts.cast(remote, "cast:abc", "cast:abc", "Bedroom Chromecast", request, from = opening)
        casts.closed(opening)
        casts.opened(PlayOnCasts.nextOpening())
        remote.gate!!.complete(Unit)
        assertEquals(CastNews.Sent("cast:abc", "Bedroom Chromecast"), casts.told.first())
        assertNull(casts.forChooser.value)
        assertEquals(listOf("cast:abc", "cast:abc"), remote.sent)
    }
}
