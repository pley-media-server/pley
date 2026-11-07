package app.pley.android.screens.admin.dashboard

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.StreamAction
import app.pley.android.model.admin.DashboardResources
import app.pley.android.model.admin.DashboardTranscode
import app.pley.android.model.admin.ResourceSample
import app.pley.android.model.format.FormatWords
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import java.util.Locale
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * The dashboard: what it reads when it opens, what it reads again while
 * it shows and stops reading when it doesn't, and what an administrator
 * does from it (stopping a playback, removing a play, cleaning up,
 * backing up), with the bodies sent and the server's refusals.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class DashboardViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    private fun server() = FakeServerAdmin().apply {
        answer("GET /api/system", """{ "name": "Home", "version": "0.1.6", "os": "linux", "arch": "x86_64", "uptime_secs": 60, "data_dir": "/data", "database": { "ok": true, "checked_at": 1 } }""")
        answer("GET /api/system/storage", """{ "blobs": 1, "blob_bytes": 10, "media_folders": 0, "media_bytes": 0, "cache_bytes": 0 }""")
        answer("GET /api/history", history(total = 3, 1, 2))
        answer("GET /api/activity", activity("s1", "s2"))
        answer("GET /api/transcoder", """{ "available": true, "max_jobs": 4, "running_processes": 0, "hw_processes": 0, "max_hw_jobs": 0, "work_dir_bytes": 0, "counters": {}, "jobs": [] }""")
        answer("GET /api/system/resources", resources(1_000L))
    }

    private fun model(server: FakeServerAdmin) =
        DashboardViewModel(server, flowOf("http://nas:7539"), nowSeconds = { 1_700_000_000 }, compute = main.dispatcher)

    @Test
    fun itReadsTheServerItsStorageAndEveryonesPlaysWhenItOpens() = runTest {
        val server = server()
        val page = model(server)
        assertEquals("Home", page.system.state.value.valueOrNull?.name)
        assertEquals(10L, page.storage.state.value.valueOrNull?.blobBytes)
        assertEquals(true, page.system.state.value.valueOrNull?.database?.ok)
        assertEquals(listOf(1L, 2L), page.history.state.value.valueOrNull?.entries?.map { it.id })
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.plays.collect {} }
        val plays = page.plays.value!!
        assertEquals(listOf("Ann", "Ann"), plays.plays.map { it.user.name })
        assertEquals("A film", plays.plays.first().row.title)
        assertTrue(plays.canLoadMore)
        // What changes isn't read until the page shows.
        assertTrue(server.callsTo("GET /api/activity").isEmpty())
        assertEquals(AdminLoad.Loading, page.activity.value)
    }

    @Test
    fun whatChangesIsReadOnTheWebsTimersWhileThePageShowsAndNotAfter() = runTest {
        val server = server()
        val page = model(server)
        val showing = backgroundScope.launch { page.keepFresh() }
        runCurrent()
        assertEquals(1, server.callsTo("GET /api/activity").size)
        assertEquals(1, server.callsTo("GET /api/transcoder").size)
        assertEquals(1, server.callsTo("GET /api/system/resources").size)
        assertEquals(listOf("s1", "s2"), page.activity.value.valueOrNull?.sessions?.map { it.sessionId })

        advanceTimeBy(5_001)
        assertEquals(2, server.callsTo("GET /api/activity").size)
        assertEquals(3, server.callsTo("GET /api/transcoder").size) // at 0, 2.5 s and 5 s
        assertEquals(3, server.callsTo("GET /api/system/resources").size) // at 0, 2 s and 4 s

        showing.cancel()
        advanceTimeBy(60_000)
        assertEquals(2, server.callsTo("GET /api/activity").size)
        assertEquals(3, server.callsTo("GET /api/transcoder").size)
    }

    @Test
    fun resourcesKeepTheirSamplesAndAddOnlyTheNewOnes() = runTest {
        val server = server()
        var next = 1_000L
        server.answer("GET /api/system/resources") { app.pley.android.model.PleyJson.parseToJsonElement(resources(next)) }
        val page = model(server)
        page.readResources()
        next = 3_000L
        page.readResources()
        assertEquals(listOf(1_000L, 3_000L), page.resources.value.valueOrNull?.samples?.map { it.at })

        val held = DashboardResources(available = true, samples = (1..150L).map { ResourceSample(at = it) })
        val fresh = DashboardResources(available = true, cpus = 8, samples = listOf(ResourceSample(at = 151)))
        val merged = withSamplesOf(held, fresh)
        assertEquals(150, merged.samples.size)
        assertEquals(2L, merged.samples.first().at)
        assertEquals(8, merged.cpus)
        assertEquals(fresh, withSamplesOf(null, fresh))
    }

    @Test
    fun aReadThatFailsKeepsWhatWasShown() = runTest {
        val server = server()
        val page = model(server)
        page.readActivity()
        val down = ApiException(503, "unavailable", "busy")
        server.fail("GET /api/activity", down)
        page.readActivity()
        assertEquals(AdminLoad.Failed(down, page.activity.value.valueOrNull), page.activity.value)
        assertEquals(2, page.activity.value.valueOrNull?.sessions?.size)
    }

    @Test
    fun stoppingSendsTheMessageAndTheSessionLeavesTheList() = runTest {
        val server = server()
        server.answer("DELETE /api/sessions/s1", null)
        val page = model(server)
        page.readActivity()
        val done = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }
        // The list is read again afterwards: the server no longer has it.
        server.answer("GET /api/activity", activity("s2"))
        val session = page.activity.value.valueOrNull!!.sessions.first()
        page.stop(session, "  Bedtime  ").join()
        val sent = server.callsTo("DELETE /api/sessions/s1").single().body
        assertEquals(JsonPrimitive("Bedtime"), sent?.jsonObject?.get("message"))
        assertEquals(listOf("s2"), page.activity.value.valueOrNull?.sessions?.map { it.sessionId })
        assertEquals(listOf(AdminDone(DashboardViewModel.STOPPED)), done)
    }

    @Test
    fun stoppingWithoutAMessageSendsNoBodyAndARefusalIsSaid() = runTest {
        val server = server()
        val refused = ApiException(403, "forbidden", "not your session")
        server.fail("DELETE /api/sessions/s2", refused)
        server.answer("DELETE /api/sessions/s1", null)
        val page = model(server)
        page.readActivity()
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        val (first, second) = page.activity.value.valueOrNull!!.sessions
        page.stop(first, "   ").join()
        assertNull(server.callsTo("DELETE /api/sessions/s1").single().body)
        page.stop(second, "").join()
        assertEquals(listOf<Throwable>(refused), errors)
        assertTrue(page.activity.value.valueOrNull!!.sessions.any { it.sessionId == "s2" })
    }

    @Test
    fun showMoreAddsTheNextPlaysAndRemovingOneTakesItOut() = runTest {
        val server = server()
        val page = model(server)
        server.answer("GET /api/history", history(total = 3, 3))
        page.loadMore()
        assertEquals(listOf(1L, 2L, 3L), page.history.state.value.valueOrNull?.entries?.map { it.id })
        assertFalse(page.loadingMore.value)
        // Everyone's, ten at a time, after those shown.
        assertEquals(
            listOf(listOf("all" to "1", "offset" to "0", "limit" to "10"), listOf("all" to "1", "offset" to "2", "limit" to "10")),
            server.callsTo("GET /api/history").map { it.query },
        )

        server.answer("DELETE /api/history/2", null)
        page.removePlay(2)
        assertEquals(listOf(1L, 3L), page.history.state.value.valueOrNull?.entries?.map { it.id })
        assertEquals(2, page.history.state.value.valueOrNull?.total)
        assertTrue(page.removing.value.isEmpty())

        val refused = ApiException(404, "not_found", "no such play")
        server.fail("DELETE /api/history/3", refused)
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        page.removePlay(3)
        assertEquals(listOf(1L, 3L), page.history.state.value.valueOrNull?.entries?.map { it.id })
        assertEquals(listOf<Throwable>(refused), errors)
    }

    @Test
    fun cleaningUpSaysWhatItFreedAndReadsTheStorageAgain() = runTest {
        val server = server()
        server.answer("POST /api/system/storage/clean", """{ "blobs_removed": 2, "media_folders_removed": 1, "bytes_freed": 4096 }""")
        val page = model(server)
        val results = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.results.collect { results += it } }
        page.cleanUp().join()
        assertEquals(listOf(AdminDone(DashboardViewModel.CLEANED, listOf<Any>(3, 4096L))), results)
        assertEquals(2, server.callsTo("GET /api/system/storage").size)
        assertNull(server.callsTo("POST /api/system/storage/clean").single().body)

        server.answer("POST /api/system/storage/clean", """{ "blobs_removed": 0, "media_folders_removed": 0, "bytes_freed": 0 }""")
        page.cleanUp().join()
        assertEquals(AdminDone(DashboardViewModel.NOTHING_TO_CLEAN), results.last())
    }

    @Test
    fun showingAgainReadsTheServerItsStorageAndThePlaysAgain() = runTest {
        val server = server()
        val page = model(server)
        val first = backgroundScope.launch { page.keepFresh() }
        runCurrent()
        first.cancel()
        assertEquals(1, server.callsTo("GET /api/system").size)
        val again = backgroundScope.launch { page.keepFresh() }
        runCurrent()
        again.cancel()
        assertEquals(2, server.callsTo("GET /api/system").size)
        assertEquals(2, server.callsTo("GET /api/system/storage").size)
        assertEquals(2, server.callsTo("GET /api/history").size)
    }

    @Test
    fun theResourcesAskOnlyForTheSamplesSinceTheLast() = runTest {
        val server = server()
        val page = model(server)
        page.readResources()
        page.readResources()
        val asked = server.callsTo("GET /api/system/resources").map { it.query }
        assertEquals(emptyList<Pair<String, String>>(), asked[0])
        assertEquals(1, asked[1].size)
        assertEquals("since", asked[1].single().first)
    }

    @Test
    fun aTranscodeIsSummedUpAsTheWebDoes() {
        val words = FormatWords.English
        val size = { n: Int, total: Int -> "size $n of $total" }
        val gpu = DashboardTranscode(
            video = StreamAction.Transcode, audio = StreamAction.Copy, videoIn = "HEVC", videoOut = "H.264", audioIn = "EAC3 5.1",
            encoder = "h264_nvenc", decoder = "cuda", pipeline = "gpu", tonemap = "libplacebo", speed = 2.46, rendition = 1, variants = 2,
        )
        assertEquals("HEVC → H.264 · EAC3 5.1 (copy) · h264_nvenc · cuda decode · libplacebo · size 2 of 3 · 2.5×", dashboardTranscodeSummary(gpu, words, size))
        // The CPU decoding is said only where the GPU does the rest.
        val cpu = gpu.copy(decoder = "cpu", pipeline = "cpu", tonemap = null, variants = 0, speed = null)
        assertEquals("HEVC → H.264 · EAC3 5.1 (copy) · h264_nvenc", dashboardTranscodeSummary(cpu, words, size))
        assertEquals(
            "HEVC → H.264 · EAC3 5.1 (copy) · h264_nvenc · cpu decode",
            dashboardTranscodeSummary(cpu.copy(pipeline = "hybrid"), words, size),
        )
        val copy = DashboardTranscode(video = StreamAction.Copy, audio = StreamAction.Transcode, audioIn = "TrueHD", audioOut = "AAC", subtitleBurn = true)
        assertEquals("Video (copy) · TrueHD → AAC · burn-in subs", dashboardTranscodeSummary(copy, words, size))
    }

    @Test
    fun theDashboardWritesAsTheWeb() {
        assertEquals("0.1.6 (abc1234)", versionLabel("0.1.6", "abc1234"))
        assertEquals("0.1.6", versionLabel("0.1.6", "unknown"))
        assertEquals("4.5%", percentText(4.5, Locale.US))
        assertEquals("12%", percentText(12.4, Locale.US))
        assertEquals("0%", percentText(0.0, Locale.US))

        fun item(kind: ItemKind, backdrop: String? = null, thumb: String? = null, show: Long? = null) =
            ItemSummary(id = 1, libraryId = 1, kind = kind, title = "t", backdropUrl = backdrop, thumbUrl = thumb, grandparentId = show)
        assertEquals(listOf("/b", "/t"), sessionArt(item(ItemKind.Movie, backdrop = "/b", thumb = "/t")))
        assertEquals(listOf("/t", "/api/items/7/images/backdrop"), sessionArt(item(ItemKind.Episode, backdrop = "/b", thumb = "/t", show = 7)))
        assertEquals(listOf("/t", "/b"), sessionArt(item(ItemKind.Track, backdrop = "/b", thumb = "/t")))
    }

    private companion object {
        fun activity(vararg sessions: String) = sessions.joinToString(",", """{ "scans": [], "sessions": [""", "] }") { id ->
            """{ "session_id": "$id", "user_id": 2, "username": "bea", "device": null, "method": "direct", "position_ms": 0, "state": "playing",
                 "item": { "id": 9, "library_id": 1, "kind": "movie", "title": "A film" } }"""
        }

        fun history(total: Int, vararg ids: Long) = ids.joinToString(",", """{ "total": $total, "entries": [""", "] }") { id ->
            """{ "id": $id, "viewed_at": 1699990000, "device": "Chrome on Windows", "user": { "id": 1, "name": "Ann" },
                 "item": { "id": ${100 + id}, "library_id": 1, "kind": "movie", "title": "A film", "year": 2001 } }"""
        }

        fun resources(at: Long) = """{ "available": true, "interval_ms": 2000, "cpus": 4, "memory_total": 1000,
            "samples": [{ "at": $at, "cpu": 10, "pley_cpu": 2, "memory": 500, "pley_memory": 100 }] }"""
    }
}
