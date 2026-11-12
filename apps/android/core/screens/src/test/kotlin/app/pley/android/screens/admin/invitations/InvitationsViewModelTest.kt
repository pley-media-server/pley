package app.pley.android.screens.admin.invitations

import app.pley.android.data.Session
import app.pley.android.model.Me
import app.pley.android.model.admin.Invitation
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.users.AccessChoice
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Invitations: the list and its states, deleting one, and making one with its link. */
class InvitationsViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    private val session = MutableStateFlow<Session?>(Session("http://nas:7539/", Me(id = 1, username = "ann", name = "Ann", accountAdmin = true), setOf("server")))

    private val list = """
        [{"code":"active1","created_at":900,"expires_at":2000,"uses_left":2,"all_libraries":true,"library_ids":[],"max_age":null,"note":"for a friend"},
         {"code":"spent","created_at":900,"expires_at":null,"uses_left":0,"all_libraries":true,"library_ids":[],"max_age":null,"note":null},
         {"code":"late","created_at":500,"expires_at":999,"uses_left":3,"all_libraries":false,"library_ids":[2],"max_age":13,"note":null}]
    """

    private fun server() = FakeServerAdmin().apply {
        answer("GET /api/invites", list)
        answer("GET /api/libraries", """[{"id":2,"name":"Movies","kind":"movies"}]""")
    }

    @Test
    fun theListShowsEachLinksStateAsOfWhenThePageOpened() = runTest {
        val page = InvitationsViewModel(server(), session, clock = { 1000 })
        val invitations = (page.invitations.state.value as AdminLoad.Ready).value
        assertEquals(listOf(InvitationState.Active, InvitationState.Used, InvitationState.Expired), invitations.map { stateOf(it, page.now.value) })
        assertEquals("http://nas:7539/invite/active1", page.link(invitations[0]))
        // Spent counts first, as on the web: a used link that also expired is Used.
        assertEquals(InvitationState.Used, stateOf(Invitation("x", expiresAt = 1, usesLeft = 0), 1000))
        // Exactly at its time it has expired.
        assertEquals(InvitationState.Expired, stateOf(Invitation("x", expiresAt = 1000, usesLeft = 1), 1000))
    }

    @Test
    fun aLinkNeedsTheServersAddress() = runTest {
        session.value = null
        val page = InvitationsViewModel(server(), session)
        assertNull(page.link(Invitation("abc")))
    }

    @Test
    fun deletingSendsItSaysSoAndReadsTheListAgain() = runTest {
        val server = server().apply { answer("DELETE /api/invites/late", null) }
        val page = InvitationsViewModel(server, session)
        val done = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }
        page.delete(Invitation("late")).join()
        assertEquals(1, server.callsTo("DELETE /api/invites/late").size)
        assertEquals(listOf(AdminDone(InvitationsViewModel.DELETED)), done)
        assertEquals(2, server.callsTo("GET /api/invites").size)
    }

    @Test
    fun aRefusedDeleteIsSaid() = runTest {
        val server = server()
        val refused = ApiException(403, "forbidden", "administrators only")
        server.fail("DELETE /api/invites/late", refused)
        val page = InvitationsViewModel(server, session)
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        page.delete(Invitation("late")).join()
        assertEquals(listOf<Throwable>(refused), errors)
    }

    @Test
    fun aNewLinkIsCheckedAsTheWebChecksItThenSentAndItsLinkShown() = runTest {
        val server = server().apply {
            answer("POST /api/invites", """{"code":"Nw7Code","created_at":1000,"expires_at":3592000,"uses_left":5,"all_libraries":false,"library_ids":[2],"max_age":7,"note":"for Kai"}""")
        }
        val page = NewInvitationViewModel(server, session)
        assertEquals(NewInvitationForm(days = 7, uses = 1), page.form.value)

        page.edit { it.copy(access = AccessChoice(allLibraries = false)) }
        assertEquals(NewInvitationProblem.PickLibrary, page.create())
        page.edit { it.copy(access = AccessChoice(allLibraries = false, libraryIds = listOf(2), maxAge = 7, showLabels = listOf("a"), hideLabels = listOf("A"))) }
        assertEquals(NewInvitationProblem.LabelsInBoth, page.create())
        page.edit {
            it.copy(
                access = AccessChoice(allLibraries = false, libraryIds = listOf(2), maxAge = 7, maxAgeSystem = "GB", showLabels = listOf("Kids")),
                uses = 0,
            )
        }
        assertEquals(NewInvitationProblem.AtLeastOne, page.create())
        assertTrue(server.callsTo("POST /api/invites").isEmpty())
        assertNull(page.link())

        page.edit { it.copy(uses = 5, days = 30, note = "  for Kai ") }
        assertNull(page.create())
        val sent = server.callsTo("POST /api/invites").single().body!!.jsonObject
        assertEquals(
            """{"expires_in_days":30,"uses":5,"all_libraries":false,"library_ids":[2],"max_age":7,"max_age_system":"GB","show_labels":["Kids"],"note":"for Kai"}""",
            sent.toString(),
        )
        assertEquals("Nw7Code", page.created.value?.code)
        assertEquals("http://nas:7539/invite/Nw7Code", page.link())
    }

    @Test
    fun aLinkThatNeverExpiresSendsNoNoteWhenNoneIsTyped() = runTest {
        val server = server().apply { answer("POST /api/invites", """{"code":"c","uses_left":1}""") }
        val page = NewInvitationViewModel(server, session)
        page.edit { it.copy(days = 0, note = "   ") }
        page.create()
        assertEquals(
            """{"expires_in_days":0,"uses":1,"all_libraries":true,"library_ids":[]}""",
            server.callsTo("POST /api/invites").single().body.toString(),
        )
    }
}
