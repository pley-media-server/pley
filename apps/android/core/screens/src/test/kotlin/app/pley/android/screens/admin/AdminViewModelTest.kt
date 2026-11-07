package app.pley.android.screens.admin

import app.pley.android.data.AdminAccess
import app.pley.android.data.ServerAdmin
import app.pley.android.data.Session
import app.pley.android.model.Me
import app.pley.android.network.ApiException
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * What every page of managing the server does: it reads when it opens,
 * keeps what it showed when reading again fails, sends a change at once,
 * says when the server refused it, and reads again after it either way.
 */
class AdminViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    /** A page of names, renamed one at a time. */
    private class Names(admin: ServerAdmin) : AdminViewModel(admin) {
        val names = resource { get(ListSerializer(String.serializer()), listOf("api", "names")) }

        fun rename(to: String) = change(said = AdminDone("renamed", listOf(to)), after = names::reload) {
            call("PUT", listOf("api", "names"), JsonPrimitive(to))
        }
    }

    @Test
    fun aPageReadsWhenItOpensAndKeepsWhatItShowedWhenReadingAgainFails() = runTest {
        val server = FakeServerAdmin()
        server.answer("GET /api/names", """["Ann"]""")
        val page = Names(server)
        assertEquals(AdminLoad.Ready(listOf("Ann")), page.names.state.value)

        val down = ApiException(503, "unavailable", "the server is busy")
        server.fail("GET /api/names", down)
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        page.names.reload()
        assertEquals(AdminLoad.Failed(down, listOf("Ann")), page.names.state.value)
        assertEquals(listOf("Ann"), page.names.state.value.valueOrNull)
        assertEquals(listOf<Throwable>(down), errors)
    }

    @Test
    fun aFirstReadThatFailsIsThePagesErrorNotAToast() = runTest {
        val server = FakeServerAdmin()
        val down = ApiException(500, "internal", "it broke")
        server.fail("GET /api/names", down)
        val page = Names(server)
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        assertEquals(AdminLoad.Failed<List<String>>(down, null), page.names.state.value)
        assertTrue(errors.isEmpty())
    }

    @Test
    fun aChangeIsSentConfirmedAndReadAgain() = runTest {
        val server = FakeServerAdmin()
        var names = listOf("Ann")
        server.answer("GET /api/names") { app.pley.android.model.PleyJson.parseToJsonElement(names.joinToString(",", "[", "]") { "\"$it\"" }) }
        server.answer("PUT /api/names") { body ->
            names = listOf((body as JsonPrimitive).content)
            null
        }
        val page = Names(server)
        val done = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }
        page.rename("Bea").join()
        assertEquals(AdminLoad.Ready(listOf("Bea")), page.names.state.value)
        assertEquals(listOf(AdminDone("renamed", listOf("Bea"))), done)
        assertEquals(0, page.busy.value)
    }

    @Test
    fun aRefusedChangeIsSaidAndWhatTheServerHasIsShown() = runTest {
        val server = FakeServerAdmin()
        server.answer("GET /api/names", """["Ann"]""")
        val refused = ApiException(400, "bad_request", "that name is taken")
        server.fail("PUT /api/names", refused)
        val page = Names(server)
        val errors = mutableListOf<Throwable>()
        val done = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }
        page.rename("Bea").join()
        assertEquals(listOf<Throwable>(refused), errors)
        assertTrue(done.isEmpty())
        assertEquals(2, server.callsTo("GET /api/names").size)
        assertEquals(AdminLoad.Ready(listOf("Ann")), page.names.state.value)
    }

    @Test
    fun whoManagesTheServerIsTheAccountsRoleAndTheSignInsScope() = runTest {
        fun session(admin: Boolean?, scopes: Set<String>) =
            Session("http://nas:7539", Me(id = 1, username = "ann", name = "Ann", accountAdmin = admin), scopes)
        assertEquals(AdminAccess.None, ServerAdmin.accessOf(null))
        assertEquals(AdminAccess.None, ServerAdmin.accessOf(session(null, setOf("library", "server"))))
        assertEquals(AdminAccess.None, ServerAdmin.accessOf(session(false, setOf("library", "server"))))
        assertEquals(AdminAccess.SignInAgain, ServerAdmin.accessOf(session(true, setOf("library", "play"))))
        assertEquals(AdminAccess.Manages, ServerAdmin.accessOf(session(true, setOf("library", "server"))))
        assertEquals(AdminAccess.None, ManageServerViewModel(ServerAdmin.None).access.value)
        val manages = ManageServerViewModel(FakeServerAdmin(AdminAccess.Manages))
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { manages.access.collect {} }
        assertEquals(AdminAccess.Manages, manages.access.first { it != AdminAccess.None })
    }
}
