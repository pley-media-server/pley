package app.pley.android.screens.admin.libraries

import app.pley.android.model.PleyJson
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * The list of Settings → Libraries: what it reads, scanning, moving the
 * libraries in the order everyone sees, and following a scan.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class LibrariesAdminViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    private fun library(id: Long, name: String, scanning: Boolean = false, count: Int = 0) =
        """{"id":$id,"name":"$name","kind":"movies","paths":["/m/$name"],"item_count":$count,"created_at":1,"scanned_at":null,"scanning":$scanning}"""

    private fun server(vararg libraries: String, scans: String = "[]") = FakeServerAdmin().apply {
        answer("GET /api/libraries", libraries.joinToString(",", "[", "]"))
        answer("GET /api/activity", """{"scans":$scans,"sessions":[]}""")
    }

    @Test
    fun itReadsTheLibrariesAndTheirScans() = runTest {
        val server = server(library(1, "Films"), library(2, "Shows", scanning = true), scans = """[{"library_id":2,"phase":"walking"}]""")
        val page = LibrariesAdminViewModel(server)
        assertEquals(listOf("Films", "Shows"), page.libraries.state.value.valueOrNull?.map { it.name })
        assertEquals(listOf(2L), page.scans.state.value.valueOrNull?.map { it.libraryId })
        assertEquals(setOf(2L), LibraryAdmin.scanning(page.libraries.state.value.valueOrNull!!, page.scans.state.value.valueOrNull!!))
    }

    @Test
    fun scanningOneSaysSoAndReadsAgain() = runTest {
        val server = server(library(1, "Films"))
        server.answer("POST /api/libraries/1/scan", null)
        server.answer("POST /api/libraries/scan", null)
        val page = LibrariesAdminViewModel(server)
        val done = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }
        val films = page.libraries.state.value.valueOrNull!!.single()

        page.scan(films, force = true).join()
        assertEquals("""{"force":true}""", server.callsTo("POST /api/libraries/1/scan").single().body.toString())
        page.scanAll().join()
        assertNull(server.callsTo("POST /api/libraries/scan").single().body)
        assertEquals(
            listOf(AdminDone(LibraryAdmin.RESCAN_STARTED, listOf("Films")), AdminDone(LibraryAdmin.SCANNING_ALL)),
            done,
        )
        assertEquals(3, server.callsTo("GET /api/activity").size)
        assertEquals(3, server.callsTo("GET /api/libraries").size)
    }

    @Test
    fun aMoveShowsAtOnceAndSavesTheWholeOrder() = runTest {
        val server = server(library(1, "Films"), library(2, "Shows"), library(3, "Music"))
        server.answer("PUT /api/libraries/order") { body ->
            val ids = body!!.jsonObject["ids"]!!.jsonArray.map { it.jsonPrimitive.long }
            val names = mapOf(1L to "Films", 2L to "Shows", 3L to "Music")
            PleyJson.parseToJsonElement(ids.joinToString(",", "[", "]") { library(it, names.getValue(it)) })
        }
        val page = LibrariesAdminViewModel(server)
        page.move(2, 0)
        assertEquals(listOf(3L, 1L, 2L), page.libraries.state.value.valueOrNull?.map { it.id })
        assertEquals("""{"ids":[3,1,2]}""", server.callsTo("PUT /api/libraries/order").single().body.toString())
        page.moveBy(3, 1)
        assertEquals(listOf(1L, 3L, 2L), page.libraries.state.value.valueOrNull?.map { it.id })
        assertEquals("""{"ids":[1,3,2]}""", server.callsTo("PUT /api/libraries/order").last().body.toString())
        // Nothing moves past the ends.
        page.moveBy(1, -1)
        assertEquals(2, server.callsTo("PUT /api/libraries/order").size)
    }

    @Test
    fun aRefusedMoveSaysWhyAndShowsTheServersOrder() = runTest {
        val server = server(library(1, "Films"), library(2, "Shows"))
        val changed = ApiException(409, "conflict", "the libraries have changed: the order must list each of them once")
        server.fail("PUT /api/libraries/order", changed)
        val page = LibrariesAdminViewModel(server)
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        page.move(0, 1)
        assertEquals(listOf<Throwable>(changed), errors)
        assertEquals(listOf(1L, 2L), page.libraries.state.value.valueOrNull?.map { it.id })
        assertEquals(2, server.callsTo("GET /api/libraries").size)
    }

    @Test
    fun aScanIsFollowedAndTheLibrariesReadAgainWhenItEnds() = runTest {
        var scans = """[{"library_id":1,"phase":"probing","processed":3,"total":10}]"""
        var scanning = true
        val server = FakeServerAdmin()
        server.answer("GET /api/libraries") { PleyJson.parseToJsonElement("[" + library(1, "Films", scanning, count = if (scanning) 0 else 5) + "]") }
        server.answer("GET /api/activity") { PleyJson.parseToJsonElement("""{"scans":$scans}""") }
        val page = LibrariesAdminViewModel(server)

        scans = """[{"library_id":1,"phase":"metadata","processed":9,"total":10}]"""
        page.poll()
        assertEquals(9L, page.scans.state.value.valueOrNull?.single()?.processed)
        assertEquals(1, server.callsTo("GET /api/libraries").size)

        scans = """[{"library_id":1,"phase":"done","processed":10,"total":10}]"""
        scanning = false
        page.poll()
        assertEquals(2, server.callsTo("GET /api/libraries").size)
        assertEquals(5, page.libraries.state.value.valueOrNull?.single()?.itemCount)
        assertTrue(LibraryAdmin.scanning(page.libraries.state.value.valueOrNull!!, page.scans.state.value.valueOrNull!!).isEmpty())
    }

    @Test
    fun theListReadsAgainEachTimeItShowsAfterTheFirst() = runTest {
        var libraries = "[" + library(1, "Films") + "]"
        val server = FakeServerAdmin()
        server.answer("GET /api/libraries") { PleyJson.parseToJsonElement(libraries) }
        server.answer("GET /api/activity", """{"scans":[]}""")
        val page = LibrariesAdminViewModel(server)
        // Shown the first time: what it read as it opened is enough.
        val first = backgroundScope.launch { page.watchScans() }
        testScheduler.runCurrent()
        first.cancel()
        assertEquals(1, server.callsTo("GET /api/libraries").size)
        // Back from a library's page, where one was added.
        libraries = "[" + library(1, "Films") + "," + library(2, "Shows") + "]"
        val again = backgroundScope.launch { page.watchScans() }
        testScheduler.runCurrent()
        again.cancel()
        assertEquals(listOf("Films", "Shows"), page.libraries.state.value.valueOrNull?.map { it.name })
        assertEquals(2, server.callsTo("GET /api/activity").size)
    }

    @Test
    fun aPollThatFailsKeepsWhatWasShown() = runTest {
        val server = server(library(1, "Films", scanning = true), scans = """[{"library_id":1,"phase":"walking"}]""")
        val page = LibrariesAdminViewModel(server)
        server.fail("GET /api/activity", ApiException(503, "unavailable", "busy"))
        page.poll()
        assertTrue(page.scans.state.value is AdminLoad.Ready)
        assertEquals(1, page.scans.state.value.valueOrNull?.size)
    }
}
