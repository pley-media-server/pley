@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.session

import app.pley.android.caps.DefaultPlaybackCapsPolicy
import app.pley.android.fixtures.FixtureIds
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerPhase
import app.pley.android.player.subtitles.ApiSubtitleFetcher
import app.pley.android.testing.caps.FakeDeviceCapabilities
import app.pley.android.testing.network.FakePleyApi
import app.pley.android.testing.player.session.FakeEngineLink
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestCoroutineScheduler
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment

/** Starting at tap time, one video at a time, and closing while the server prepares. */
@RunWith(RobolectricTestRunner::class)
class ServicesTest {
    private val h = SessionHarness()

    @After
    fun close() = h.close()

    @Test
    fun `prepare starts at once, and another playback ends the one before`() {
        val dispatcher = StandardTestDispatcher(h.scheduler)
        val services = SessionPlaybackServices({ h.environment() }, dispatcher)
        val first = services.prepare(PlayRequest(FixtureIds.SAMPLE_1080P))
        // The server is asked before any screen exists.
        h.settle()
        assertEquals(1, h.playbackRequests().size)
        val controller = services.controller(first)!!
        h.engine.ready()
        h.advance(2_000)

        val second = services.prepare(PlayRequest(FixtureIds.SAMPLE_HDR10))
        h.settle()
        assertNull(services.controller(first))
        assertNotNull(services.controller(second))
        assertEquals(listOf("start", "progress stopped", "delete", "start"), h.timeline())
        assertTrue(controller.state.value.phase !is PlayerPhase.Error)

        services.controller(second)!!.close()
        assertNull(services.controller(second))
        val saved = SavedPlaybackState(FixtureIds.SAMPLE_1080P, null, 5_000, null, null, "original", null)
        services.restore(saved)
        h.settle()
        assertEquals(5_000L, h.playbackRequests().last().startMs)
    }

    @Test
    fun `closing every playback ends its session, and the flush returns once its last reports went out`() {
        val dispatcher = StandardTestDispatcher(h.scheduler)
        val services = SessionPlaybackServices({ h.environment() }, dispatcher, reporter = h.reporter)
        val key = services.prepare(PlayRequest(FixtureIds.SAMPLE_1080P))
        h.settle()
        h.engine.ready()
        h.advance(2_000)

        services.closeAll()
        assertNull(services.controller(key))
        var sentBefore: List<String>? = null
        CoroutineScope(dispatcher).launch {
            services.flush()
            sentBefore = h.timeline()
        }
        h.settle()
        // A switch of profile after the flush sends nothing of this playback's.
        assertEquals(listOf("progress stopped", "delete"), sentBefore?.takeLast(2))
    }

    @Test
    fun `without a server signed in, the playback says why`() {
        val services = SessionPlaybackServices({ error("Sign in to play") }, StandardTestDispatcher(h.scheduler))
        val key = services.prepare(PlayRequest(1))
        assertEquals(PlayerPhase.Error("Sign in to play", canRetry = false), services.controller(key)!!.state.value.phase)
        services.controller(key)!!.close()
        assertNull(services.controller(key))
    }

    @Test
    fun `closing while the server prepares the session deletes it, and shows no error`() {
        val scheduler = TestCoroutineScheduler()
        val dispatcher = StandardTestDispatcher(scheduler)
        val api = FakePleyApi()
        val reporter = SessionReporter(CoroutineScope(dispatcher + SupervisorJob()))
        val env = PlaybackEnvironment(
            api = api,
            capabilities = FakeDeviceCapabilities(),
            policy = DefaultPlaybackCapsPolicy,
            details = ItemDetails.of(api),
            engines = { FakeEngineLink { scheduler.currentTime } },
            texts = ResourcePlayerTexts(RuntimeEnvironment.getApplication()),
            clientId = { "c" },
            reporter = reporter,
            subtitles = ApiSubtitleFetcher(api),
            now = { scheduler.currentTime },
        )
        val controller = SessionPlaybackController(PlayRequest(101), env, CoroutineScope(dispatcher + SupervisorJob()))
        api.hold()
        controller.start()
        scheduler.runCurrent()
        assertEquals(1, api.calls("item").size)
        // The details come; the server's answer is held.
        api.release()
        api.hold()
        scheduler.runCurrent()
        // POST /api/playback is out, and waits.
        assertEquals(1, api.calls("startPlayback").size)
        controller.close()
        api.release()
        scheduler.runCurrent()
        val deleted = api.calls("stopPlayback").single().args.single() as String
        assertTrue(deleted.isNotEmpty())
        // Nothing was played: no progress, no error.
        assertEquals(0, api.calls("reportProgress").size)
        assertFalse(controller.state.value.phase is PlayerPhase.Error)
    }
}
