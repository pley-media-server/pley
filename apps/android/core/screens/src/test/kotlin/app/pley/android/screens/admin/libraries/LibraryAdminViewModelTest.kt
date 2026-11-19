package app.pley.android.screens.admin.libraries

import app.pley.android.model.LibraryKind
import app.pley.android.model.ScanPhase
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** A library's page and a new library's: the form, what it sends, scanning and deleting. */
@OptIn(ExperimentalCoroutinesApi::class)
class LibraryAdminViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    private val films = """{"id":3,"name":"Films","kind":"movies","paths":["/m/a"],"item_count":2,"created_at":1,"scanned_at":5,"scanning":false}"""

    private fun existing() = FakeServerAdmin().apply {
        answer("GET /api/libraries/3", films)
        answer("GET /api/activity", """{"scans":[{"library_id":3,"phase":"done"},{"library_id":4,"phase":"walking"}]}""")
    }

    @Test
    fun aNewLibraryReadsNothingAndStartsEmpty() = runTest {
        val server = FakeServerAdmin()
        val page = LibraryAdminViewModel(server, null)
        assertTrue(page.isNew)
        assertEquals(LibraryPage(null, null), page.page.state.value.valueOrNull)
        assertEquals(LibraryForm(), page.form.value)
        assertTrue(server.calls.isEmpty())
    }

    @Test
    fun withoutAFolderNothingIsSentAndTheFormSaysSo() = runTest {
        val server = FakeServerAdmin()
        val page = LibraryAdminViewModel(server, null)
        page.save("Movies")
        assertTrue(page.form.value!!.noFolder)
        assertTrue(server.calls.isEmpty())
        page.addPath("  /m/films ")
        assertFalse(page.form.value!!.noFolder)
    }

    @Test
    fun aNewLibraryIsSentWithItsKindAndFoldersAndTheKindsNameWhenNoneIsTyped() = runTest {
        val server = FakeServerAdmin()
        server.answer("POST /api/libraries", films)
        val page = LibraryAdminViewModel(server, null)
        val done = mutableListOf<AdminDone>()
        var finished = 0
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.finished.collect { finished++ } }
        page.setKind(LibraryKind.Shows)
        page.addPath("/m/shows")
        page.addPath("/m/shows")
        page.addPath("/m/more")
        page.removePath("/m/more")
        page.setName("   ")
        page.save("TV Shows")
        assertEquals(
            """{"name":"TV Shows","kind":"shows","paths":["/m/shows"],"tmdb_collections":true}""",
            server.callsTo("POST /api/libraries").single().body.toString(),
        )
        assertEquals(listOf(AdminDone(LibraryAdmin.ADDED, listOf("TV Shows"))), done)
        assertEquals(1, finished)
    }

    @Test
    fun aNewFilmLibraryIsSentWithoutCollectionsFromTmdbWhenTurnedOffAndAnotherKindKeepsTheDefault() = runTest {
        val server = FakeServerAdmin()
        server.answer("POST /api/libraries", films)
        val page = LibraryAdminViewModel(server, null)
        assertTrue(page.form.value!!.tmdbCollections)
        page.setTmdbCollections(false)
        page.addPath("/m/films")
        page.save("Movies")
        assertEquals(
            """{"name":"Movies","kind":"movies","paths":["/m/films"],"tmdb_collections":false}""",
            server.callsTo("POST /api/libraries").last().body.toString(),
        )

        // The switch is a film library's only: turned off, then Music picked, the server's default goes.
        val music = LibraryAdminViewModel(server, null)
        music.setTmdbCollections(false)
        music.setKind(LibraryKind.Music)
        music.addPath("/m/music")
        music.save("Music")
        assertEquals(
            """{"name":"Music","kind":"music","paths":["/m/music"],"tmdb_collections":true}""",
            server.callsTo("POST /api/libraries").last().body.toString(),
        )
    }

    @Test
    fun aRefusedLibraryStaysOpenWithTheServersWords() = runTest {
        val server = FakeServerAdmin()
        val refused = ApiException(400, "bad_request", "folder '/nope' does not exist or is not accessible")
        server.fail("POST /api/libraries", refused)
        val page = LibraryAdminViewModel(server, null)
        val errors = mutableListOf<Throwable>()
        var finished = 0
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.finished.collect { finished++ } }
        page.addPath("/nope")
        page.save("Movies")
        assertEquals(listOf<Throwable>(refused), errors)
        assertEquals(0, finished)
        assertEquals(listOf("/nope"), page.form.value?.paths)
    }

    @Test
    fun aLibrarysFormStartsFromItAndSavesItsNameAndFolders() = runTest {
        val server = existing()
        server.answer("PATCH /api/libraries/3", films)
        val page = LibraryAdminViewModel(server, 3)
        val shown = page.page.state.value.valueOrNull!!
        assertEquals("Films", shown.library?.name)
        assertEquals(ScanPhase.Done, shown.scan?.phase)
        assertFalse(shown.scanning)
        val form = page.form.value!!
        assertEquals(LibraryForm("Films", LibraryKind.Movies, listOf("/m/a")), form)
        assertFalse(page.changed(form, shown.library))
        // A library's kind stays what it was made as.
        page.setKind(LibraryKind.Music)
        assertEquals(LibraryKind.Movies, page.form.value?.kind)

        page.setName("Cinema")
        page.addPath("/m/b")
        assertTrue(page.changed(page.form.value!!, shown.library))
        page.save("Movies")
        assertEquals("""{"name":"Cinema","paths":["/m/a","/m/b"]}""", server.callsTo("PATCH /api/libraries/3").single().body.toString())
    }

    @Test
    fun aLibrarysCollectionsFromTmdbStartFromItAndAreSentWhenTheyChange() = runTest {
        val server = FakeServerAdmin()
        val off = films.replace("\"scanning\":false", "\"scanning\":false,\"tmdb_collections\":false")
        server.answer("GET /api/libraries/3", off)
        server.answer("GET /api/activity", """{"scans":[]}""")
        server.answer("PATCH /api/libraries/3", films)
        val page = LibraryAdminViewModel(server, 3)
        val library = page.page.state.value.valueOrNull!!.library
        assertFalse(page.form.value!!.tmdbCollections)
        assertFalse(page.changed(page.form.value!!, library))

        page.setTmdbCollections(true)
        assertTrue(page.changed(page.form.value!!, library))
        page.setTmdbCollections(false)
        assertFalse(page.changed(page.form.value!!, library))

        page.setTmdbCollections(true)
        page.save("Movies")
        assertEquals(
            """{"name":"Films","paths":["/m/a"],"tmdb_collections":true}""",
            server.callsTo("PATCH /api/libraries/3").single().body.toString(),
        )
    }

    @Test
    fun turningALibrarysCollectionsFromTmdbOffAsksFirstAndTurningThemOnDoesNot() = runTest {
        val server = FakeServerAdmin()
        server.answer("GET /api/libraries/3", films)
        server.answer("GET /api/activity", """{"scans":[]}""")
        val page = LibraryAdminViewModel(server, 3)
        val library = page.page.state.value.valueOrNull!!.library!!
        assertFalse(page.asksFirst(page.form.value!!, library))
        page.setTmdbCollections(false)
        assertTrue(page.asksFirst(page.form.value!!, library))
        assertFalse(page.asksFirst(page.form.value!!.copy(tmdbCollections = true), library.copy(tmdbCollections = false)))
        // A new library has nothing to remove.
        assertFalse(page.asksFirst(LibraryForm(tmdbCollections = false), null))
    }

    @Test
    fun aLibraryIsScannedAndDeleted() = runTest {
        val server = existing()
        server.answer("POST /api/libraries/3/scan", null)
        server.answer("DELETE /api/libraries/3", null)
        val page = LibraryAdminViewModel(server, 3)
        val done = mutableListOf<AdminDone>()
        var finished = 0
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.finished.collect { finished++ } }
        page.scan(force = false)
        assertEquals("""{"force":false}""", server.callsTo("POST /api/libraries/3/scan").single().body.toString())
        assertEquals(2, server.callsTo("GET /api/libraries/3").size)
        page.delete()
        assertNull(server.callsTo("DELETE /api/libraries/3").single().body)
        assertEquals(
            listOf(AdminDone(LibraryAdmin.SCAN_STARTED, listOf("Films")), AdminDone(LibraryAdmin.DELETED, listOf("Films"))),
            done,
        )
        assertEquals(1, finished)
    }
}
