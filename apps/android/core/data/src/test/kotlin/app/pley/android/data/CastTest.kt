package app.pley.android.data

import app.pley.android.model.CastAction
import app.pley.android.model.CastControlRequest
import app.pley.android.model.CastPlatform
import app.pley.android.model.CastPlayRequest
import app.pley.android.model.CastPlayState
import app.pley.android.model.CastReport
import app.pley.android.model.CastState
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.ServerEvent
import app.pley.android.network.ApiException
import app.pley.android.network.LiveEvent
import app.pley.android.testing.data.TestData
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class CastTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val tv = "8f14e45fceea167a5a36dedd4bea2543"
    private val movie = ItemSummary(id = 101, libraryId = 1, kind = ItemKind.Movie, title = "A movie", durationMs = 6_000_000)

    private fun TestScope.time(ms: Long) {
        testScheduler.advanceTimeBy(ms)
        testScheduler.runCurrent()
    }

    /** A TV signed in, its stream open: listed to be cast to as "Living room". */
    private suspend fun TestScope.signedInTv(): TestData {
        val data = TestData(this, folder.newFolder(), profile = TestData.FIRE_TV_T0, castName = { "Living room" })
        data.signIn()
        data.useServer()
        data.graph.session
        testScheduler.runCurrent()
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()
        return data
    }

    @Test
    fun `a phone lists the devices when its stream connects, and again when they change`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.liveUpdates.start()
        data.graph.cast.start()
        testScheduler.runCurrent()
        val first = data.api.calls("castDevices").size
        assertEquals(listOf("Living room TV", "Bedroom"), data.graph.cast.devices.value.map { it.name })
        assertEquals(CastPlatform.AndroidTv, data.graph.cast.devices.value[0].platform)
        // The phone isn't listed itself: it only sends.
        assertNull(data.streams.single())

        data.events.send(LiveEvent.Event(ServerEvent.CastDevicesChanged))
        testScheduler.runCurrent()
        assertEquals(first + 1, data.api.calls("castDevices").size)
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        assertEquals(first + 2, data.api.calls("castDevices").size)
    }

    @Test
    fun `what a device plays is heard, and its position moves on from when it said so`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.liveUpdates.start()
        data.graph.cast.start()
        testScheduler.runCurrent()
        // The fixture said so 1.5 s before the answer: 600 s then, 601.5 s now.
        val listed = data.graph.cast.devices.value[0].playing!!
        assertEquals(601_500, listed.positionAt(data.graph.cast.now()))

        val heard = CastState(movie, positionMs = 10_000, durationMs = 6_000_000, state = CastPlayState.Playing, ageMs = 0)
        data.events.send(LiveEvent.Event(ServerEvent.CastStateChanged(data.graph.cast.devices.value[0].id, heard)))
        testScheduler.runCurrent()
        val playing = data.graph.cast.devices.value[0].playing!!
        assertEquals(movie, playing.item)
        time(5_000)
        assertEquals(15_000, playing.positionAt(data.graph.cast.now()))
        // Paused, it stays; at the end, it stops there.
        assertEquals(10_000, playing.copy(state = CastPlayState.Paused).positionAt(data.graph.cast.now()))
        assertEquals(6_000_000, playing.positionAt(data.graph.cast.now() + 10_000_000))

        data.events.send(LiveEvent.Event(ServerEvent.CastStateChanged(data.graph.cast.devices.value[0].id, null)))
        testScheduler.runCurrent()
        assertNull(data.graph.cast.devices.value[0].playing)
    }

    @Test
    fun `sending a title remembers it, its remote shows, and an answer naming it is heard`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.liveUpdates.start()
        data.graph.cast.start()
        testScheduler.runCurrent()
        val remotes = collect(data.graph.cast.remotes)
        val bedroom = data.graph.cast.devices.value[1].id

        data.graph.cast.play(bedroom, 101, fileId = 1001, positionMs = 754_000)
        assertEquals(listOf(bedroom, CastPlayRequest(101, 1001, 754_000)), data.api.calls("castPlay").single().args)
        assertEquals(CastSent(bedroom, 101, data.graph.cast.now()), data.graph.cast.sent.value)
        assertEquals(listOf(bedroom), remotes)

        data.events.send(LiveEvent.Event(ServerEvent.CastStateChanged(bedroom, CastState(movie, 754_000, state = CastPlayState.Buffering))))
        testScheduler.runCurrent()
        assertTrue(data.graph.cast.sent.value!!.answered)

        // From a title page: no position.
        data.graph.cast.play(bedroom, 203)
        assertEquals(CastPlayRequest(203), data.api.calls("castPlay").last().args[1])
        assertFalse(data.graph.cast.sent.value!!.answered)

        data.graph.cast.control(bedroom, CastAction.Seek, 90_000)
        assertEquals(listOf(bedroom, CastControlRequest(CastAction.Seek, 90_000)), data.api.calls("castControl").single().args)
    }

    @Test
    fun `an older server without casting lists nothing`() = runTest {
        val data = signedIn(folder.newFolder())
        data.api.fail("castDevices", ApiException(404, "not_found", "no such route"))
        data.graph.liveUpdates.start()
        data.graph.cast.start()
        testScheduler.runCurrent()
        assertTrue(data.graph.cast.devices.value.isEmpty())
    }

    @Test
    fun `a TV is listed as it opens its stream, under one id it keeps`() = runTest {
        val data = signedInTv()
        val listed = data.streams.filterNotNull().single()
        assertEquals("Living room", listed.name)
        assertEquals(CastPlatform.AndroidTv, listed.platform)
        assertTrue(Settings.isCastDeviceId(listed.id))
        assertEquals(listed.id, data.graph.settings.settings.first().castDeviceId)
        assertEquals(listed.id, data.graph.settings.castDeviceId())
    }

    @Test
    fun `a TV hears the commands that name it, and reports what it plays`() = runTest {
        val data = signedInTv()
        val id = data.streams.filterNotNull().single().id
        val heard = collect(data.graph.cast.commands)
        testScheduler.runCurrent()

        data.events.send(LiveEvent.Event(ServerEvent.CastPlay(id, 101)))
        data.events.send(LiveEvent.Event(ServerEvent.CastPlay(tv, 203)))
        data.events.send(LiveEvent.Event(ServerEvent.CastControl(id, CastAction.Pause)))
        testScheduler.runCurrent()
        assertEquals(listOf(ServerEvent.CastPlay(id, 101), ServerEvent.CastControl(id, CastAction.Pause)), heard)

        data.graph.cast.report(CastReport(101, 0, 6_000_000, CastPlayState.Buffering))
        testScheduler.runCurrent()
        assertEquals(listOf(id, CastReport(101, 0, 6_000_000, CastPlayState.Buffering)), data.api.calls("reportCastState").single().args)
        // A failure is ignored; the next report goes all the same.
        data.api.fail("reportCastState", ApiException(404, "not_found", "that device isn't connected"))
        data.graph.cast.report(CastReport())
        testScheduler.runCurrent()
        assertEquals(2, data.api.calls("reportCastState").size)
    }

    @Test
    fun `a phone reports nothing, as nothing is cast to it`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.cast.report(CastReport(101))
        testScheduler.runCurrent()
        assertTrue(data.api.calls("reportCastState").isEmpty())
    }
}
