package app.pley.android.screens.admin.users

import app.pley.android.data.Session
import app.pley.android.model.Me
import app.pley.android.model.PleyJson
import app.pley.android.model.RequestAccess
import app.pley.android.model.admin.UserAccount
import app.pley.android.model.admin.UserApp
import app.pley.android.model.admin.UserAppPerson
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Users: the list, an account's page with each change it sends (and what
 * it doesn't offer on one's own), and adding an account.
 */
class UsersViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    private val session = MutableStateFlow<Session?>(Session("http://nas:7539", Me(id = 1, username = "ann", name = "Ann", accountAdmin = true), setOf("server")))

    private fun user(
        id: Long,
        name: String,
        admin: Boolean = false,
        managed: Boolean = false,
        disabled: Boolean = false,
        maxAge: Int? = null,
        access: String = "ask",
        all: Boolean = true,
        ids: List<Long> = emptyList(),
        pin: Boolean = false,
        twoStep: Boolean = false,
    ) = """{"id":$id,"username":"${name.lowercase()}","name":"$name","is_admin":$admin,"managed":$managed,"home":true,
        "has_pin":$pin,"avatar_url":${if (id == 1L) "\"/api/users/1/avatar?v=2\"" else "null"},"all_libraries":$all,
        "library_ids":${ids.joinToString(",", "[", "]")},"max_age":${maxAge ?: "null"},"disabled":$disabled,
        "has_password":${!managed},"two_step":$twoStep,"request_access":"${if (admin) "auto" else access}"}"""

    private fun server(vararg users: String): FakeServerAdmin = FakeServerAdmin().apply {
        answer("GET /api/users", users.joinToString(",", "[", "]"))
        answer("GET /api/libraries", """[{"id":2,"name":"Movies","kind":"movies"},{"id":3,"name":"Shows","kind":"shows"}]""")
        answer("GET /api/server", """{"name":"Home","version":"0.1.6","two_step_required":true}""")
        answer("GET /api/users/2/devices", """[{"id":"s1","name":"Firefox on Windows","ip":"10.0.0.2","last_seen_at":5}]""")
        answer("GET /api/users/2/apps", "[]")
        answer("GET /api/users/1/devices", "[]")
        answer("GET /api/users/1/apps", "[]")
    }

    /** PATCH /api/users/{id} answers the account with what was sent merged in. */
    private fun FakeServerAdmin.patches(id: Long, before: String) {
        var now = PleyJson.parseToJsonElement(before).jsonObject
        answer("PATCH /api/users/$id") { body ->
            now = JsonObject(now + (body as JsonObject).filterKeys { it != "password" && it != "pin" })
            now
        }
    }

    private fun body(server: FakeServerAdmin, route: String): JsonElement? = server.callsTo(route).last().body

    @Test
    fun theAgeLimitAndItsExceptionsAreSentAsTheWebsDialogSendsThem() = runTest {
        val bea = """{"id":2,"username":"bea","name":"Bea","max_age":11,"max_age_system":null,"birth_month":"2015-03",
            "show_labels":["Kids"],"hide_labels":[]}"""
        val server = server(user(1, "Ann", admin = true), bea).apply { answer("GET /api/labels", "[]") }
        // As the server does: a limit sent stops one following a birthday.
        var state = PleyJson.parseToJsonElement(bea).jsonObject
        server.answer("PATCH /api/users/2") { body ->
            val sent = body as JsonObject
            state = JsonObject(state + sent + if ("max_age" in sent) mapOf("birth_month" to JsonNull) else emptyMap())
            state
        }
        val page = UserViewModel(server, session, 2)
        val sent = { body(server, "PATCH /api/users/2").toString() }
        val now = { accessOf(page.page.state.value.valueOrNull!!.user) }

        // Another birthday: only it is sent, the server works the limit out.
        page.setAgeLimit(now().copy(birthMonth = "2016-05", maxAge = 10))
        assertEquals("""{"birth_month":"2016-05"}""", sent())
        // A limit of the same age stops it following the birthday, so it's sent all the same.
        page.setAgeLimit(now().copy(birthMonth = null))
        assertEquals("""{"max_age":11}""", sent())
        // Another country's ratings: the system goes with the limit; back to the region's is a null.
        page.setAgeLimit(now().copy(maxAge = 10, maxAgeSystem = "CA"))
        assertEquals("""{"max_age":10,"max_age_system":"CA"}""", sent())
        page.setAgeLimit(now().copy(maxAgeSystem = null))
        assertEquals("""{"max_age_system":null}""", sent())
        val calls = server.callsTo("PATCH /api/users/2").size
        page.setAgeLimit(now())
        assertEquals("nothing changed, nothing sent", calls, server.callsTo("PATCH /api/users/2").size)

        page.setLabels(listOf("Kids", "Anime"), emptyList())
        assertEquals("""{"show_labels":["Kids","Anime"]}""", sent())
        page.setLabels(listOf("Anime", "Kids"), listOf("Horror"))
        assertEquals("""{"hide_labels":["Horror"]}""", sent())
        // Read when the page opened, and again after each change, as a label a rule names is offered from then on.
        assertEquals(3, server.callsTo("GET /api/labels").size)
    }

    @Test
    fun theListReadsTheAccountsTheirLibrariesAndTheServersRules() = runTest {
        val server = server(user(1, "Ann", admin = true), user(2, "Bea", all = false, ids = listOf(2)))
        val page = UsersViewModel(server, session)
        val users = (page.users.state.value as AdminLoad.Ready).value
        assertEquals(listOf("Ann", "Bea"), users.map { it.name })
        assertEquals(1L, page.selfId)
        assertEquals("http://nas:7539/api/users/1/avatar?v=2", page.avatar(users[0]))
        assertNull(page.avatar(users[1]))
        assertEquals(listOf("Movies", "Shows"), page.libraries.state.value.valueOrNull?.map { it.name })
        assertTrue(page.server.state.value.valueOrNull?.twoStepRequired == true)
        // Bea has a password and no two-step sign-in, which the server requires: she's marked.
        assertTrue(mustSetUpTwoStep(users[1], page.server.state.value.valueOrNull))
        assertFalse(mustSetUpTwoStep(users[0].copy(twoStep = true), page.server.state.value.valueOrNull))

        page.shownAgain()
        assertEquals(2, server.callsTo("GET /api/users").size)
    }

    @Test
    fun anAccountsPageKnowsWhoIsTheLastAdministratorAndWhoIsSelf() = runTest {
        val server = server(user(1, "Ann", admin = true), user(2, "Bea"), user(3, "Cid", admin = true, disabled = true))
        val ann = UserViewModel(server, session, 1)
        val page = ann.page.state.value.valueOrNull!!
        assertTrue(ann.self.value)
        // One's own devices and apps stay in the web app: they aren't asked for.
        assertEquals(emptyList<Any>(), ann.devices.state.value.valueOrNull)
        assertEquals(emptyList<Any>(), ann.apps.state.value.valueOrNull)
        assertTrue(server.callsTo("GET /api/users/1/devices").isEmpty())
        assertTrue(server.callsTo("GET /api/users/1/apps").isEmpty())
        // Cid is an administrator, but disabled: Ann is the last one.
        assertEquals(0, page.otherAdmins)
        assertTrue(page.lastAdmin)
        val bea = UserViewModel(server, session, 2)
        assertFalse(bea.self.value)
        assertFalse(bea.page.state.value.valueOrNull!!.lastAdmin)
        assertEquals(1, bea.devices.state.value.valueOrNull?.size)
    }

    @Test
    fun anAccountGoneFromTheServerIsThePagesError() = runTest {
        val page = UserViewModel(server(user(1, "Ann", admin = true)), session, 9)
        val failed = page.page.state.value as AdminLoad.Failed
        assertEquals(404, (failed.error as ApiException).status)
    }

    @Test
    fun eachChangeIsSentAtOnceAndTheAnswerShown() = runTest {
        val bea = user(2, "Bea", maxAge = 13, pin = true)
        val server = server(user(1, "Ann", admin = true), bea).apply { patches(2, bea) }
        val page = UserViewModel(server, session, 2)
        val done = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }

        page.rename("Beatrice")
        assertEquals("""{"display_name":"Beatrice"}""", body(server, "PATCH /api/users/2").toString())
        assertEquals("Beatrice", page.page.state.value.valueOrNull?.user?.displayName)
        // The answer is shown as it is: the accounts aren't read again.
        assertEquals(1, server.callsTo("GET /api/users").size)

        // Nothing typed: no display name, the username shows; sent as an explicit null.
        page.rename("")
        assertEquals(JsonNull, body(server, "PATCH /api/users/2")!!.jsonObject["display_name"])

        page.setHome(false)
        assertEquals("""{"home":false}""", body(server, "PATCH /api/users/2").toString())
        page.setDisabled(true)
        assertEquals("""{"disabled":true}""", body(server, "PATCH /api/users/2").toString())
        page.setRequests(RequestAccess.Auto)
        assertEquals("""{"request_access":"auto"}""", body(server, "PATCH /api/users/2").toString())
        page.setAgeLimit(AccessChoice(maxAge = null))
        assertEquals(JsonNull, body(server, "PATCH /api/users/2")!!.jsonObject["max_age"])
        page.setAgeLimit(AccessChoice(maxAge = 17))
        assertEquals("""{"max_age":17}""", body(server, "PATCH /api/users/2").toString())
        page.setPin("4321")
        assertEquals("""{"pin":"4321"}""", body(server, "PATCH /api/users/2").toString())
        page.removePin()
        assertEquals("""{"pin":null}""", body(server, "PATCH /api/users/2").toString())

        assertTrue(done.isEmpty())
        assertEquals(1, server.callsTo("GET /api/users/2/devices").size)
        page.setPassword("a long new one")
        assertEquals("""{"password":"a long new one"}""", body(server, "PATCH /api/users/2").toString())
        assertEquals(listOf(AdminDone(UserViewModel.SAVED, listOf("Bea"))), done)
        // The server signed Bea out everywhere: her devices and apps are read again.
        assertEquals(2, server.callsTo("GET /api/users/2/devices").size)
        assertEquals(2, server.callsTo("GET /api/users/2/apps").size)
    }

    @Test
    fun oneNoLongerAnAdministratorGetsTheRequestsThePageShows() = runTest {
        val cid = user(3, "Cid", admin = true)
        val dee = user(4, "Dee", admin = true, maxAge = 13)
        val server = server(user(1, "Ann", admin = true), cid, dee).apply {
            patches(3, cid)
            patches(4, dee)
        }
        UserViewModel(server, session, 3).removeAdmin()
        assertEquals("""{"is_admin":false,"request_access":"ask"}""", body(server, "PATCH /api/users/3").toString())
        // With an age limit, as the server makes such an account: it may not ask.
        UserViewModel(server, session, 4).removeAdmin()
        assertEquals("""{"is_admin":false,"request_access":"off"}""", body(server, "PATCH /api/users/4").toString())
    }

    @Test
    fun anAccountShownAfterAFailedReadIsStillChanged() = runTest {
        val bea = user(2, "Bea")
        val server = server(user(1, "Ann", admin = true), bea).apply { patches(2, bea) }
        val page = UserViewModel(server, session, 2)
        server.fail("GET /api/users", ApiException(503, "unavailable", "the server is busy"))
        page.page.reload()
        assertTrue(page.page.state.value is AdminLoad.Failed)
        page.setHome(false)
        assertEquals("""{"home":false}""", body(server, "PATCH /api/users/2").toString())
        assertTrue("the answer is shown", page.page.state.value is AdminLoad.Ready)
    }

    @Test
    fun librariesSendOnlyWhatChanged() = runTest {
        val bea = user(2, "Bea", all = false, ids = listOf(2))
        val server = server(user(1, "Ann", admin = true), bea).apply { patches(2, bea) }
        val page = UserViewModel(server, session, 2)

        page.setLibraries(all = false, ids = listOf(2))
        assertTrue(server.callsTo("PATCH /api/users/2").isEmpty())

        page.setLibraries(all = false, ids = listOf(2, 3))
        assertEquals("""{"library_ids":[2,3]}""", body(server, "PATCH /api/users/2").toString())

        page.setLibraries(all = true, ids = listOf(2, 3))
        assertEquals("""{"all_libraries":true}""", body(server, "PATCH /api/users/2").toString())
    }

    @Test
    fun aRefusedChangeIsSaidAndTheAccountReadAgain() = runTest {
        val server = server(user(1, "Ann", admin = true), user(2, "Bea"))
        val refused = ApiException(400, "bad_request", "Pley needs at least one administrator")
        server.fail("PATCH /api/users/1", refused)
        val page = UserViewModel(server, session, 1)
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.errors.collect { errors += it } }
        page.removeAdmin()
        assertEquals(listOf<Throwable>(refused), errors)
        assertEquals(2, server.callsTo("GET /api/users").size)
        assertTrue(page.page.state.value.valueOrNull!!.user.isAdmin)
    }

    @Test
    fun twoStepSignInsAndDeletingAreSentAndSaid() = runTest {
        val server = server(user(1, "Ann", admin = true), user(2, "Bea", twoStep = true)).apply {
            answer("DELETE /api/users/2/two-step", null)
            answer("DELETE /api/users/2/devices", null)
            answer("DELETE /api/users/2/apps/7", null)
            answer("DELETE /api/users/2", null)
        }
        val page = UserViewModel(server, session, 2)
        val done = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { done += it } }

        // The server requires two-step sign-in: turning it off signs Bea out, which is what's said.
        page.turnOffTwoStep()
        assertEquals(1, server.callsTo("DELETE /api/users/2/two-step").size)
        assertEquals(AdminDone(UserViewModel.TWO_STEP_SIGNED_OUT, listOf("Bea")), done.last())

        page.signOutEverywhere()
        assertEquals(1, server.callsTo("DELETE /api/users/2/devices").size)
        assertEquals(AdminDone(UserViewModel.SIGNED_OUT_EVERYWHERE, listOf("Bea")), done.last())
        assertEquals(3, server.callsTo("GET /api/users/2/devices").size)

        val app = UserApp(7, "https://pley.app/android", "Pley for Android", true, "pley.app", profile = UserAppPerson(2, "Bea"), approvedBy = UserAppPerson(2, "Bea"))
        page.signOutApp(app).join()
        assertEquals(1, server.callsTo("DELETE /api/users/2/apps/7").size)
        assertEquals(AdminDone(UserViewModel.APP_SIGNED_OUT, listOf("Pley for Android")), done.last())

        assertNull(page.deleted.value)
        page.delete()
        assertEquals(1, server.callsTo("DELETE /api/users/2").size)
        assertEquals("Bea", page.deleted.value)
    }

    @Test
    fun aNewAccountIsCheckedAsTheWebChecksItThenSentTogether() = runTest {
        val server = FakeServerAdmin().apply {
            answer("GET /api/libraries", """[{"id":2,"name":"Movies","kind":"movies"}]""")
            answer("POST /api/users") { body ->
                val sent = body!!.jsonObject
                PleyJson.parseToJsonElement("""{"id":9,"username":${sent["username"]},"name":${sent["display_name"] ?: sent["username"]}}""")
            }
        }
        val page = NewUserViewModel(server)
        assertEquals(NewUserProblem.EnterUsername, page.create())
        page.edit { it.copy(username = "sam") }
        assertEquals(NewUserProblem.Password(PasswordProblem.TooShort), page.create())
        page.edit { it.copy(password = "SAM") }
        assertEquals(NewUserProblem.Password(PasswordProblem.TooShort), page.create())
        page.edit { it.copy(username = "longname1", password = "LongName1") }
        assertEquals(NewUserProblem.Password(PasswordProblem.SameAsUsername), page.create())
        page.edit { it.copy(password = "a good password", pin = "12") }
        assertEquals(NewUserProblem.PinDigits, page.create())
        page.edit { it.copy(pin = "", access = AccessChoice(allLibraries = false)) }
        assertEquals(NewUserProblem.PickLibrary, page.create())
        assertTrue(server.callsTo("POST /api/users").isEmpty())

        page.edit { it.copy(access = AccessChoice(allLibraries = false, libraryIds = listOf(2), maxAge = 13)) }
        assertNull(page.create())
        val sent = body(server, "POST /api/users")!!.jsonObject
        assertEquals("\"longname1\"", sent["username"].toString())
        assertEquals("\"a good password\"", sent["password"].toString())
        // An age limit: may not ask, as the server makes such an account.
        assertEquals("\"off\"", sent["request_access"].toString())
        assertEquals("[2]", sent["library_ids"].toString())
        assertEquals("longname1", page.created.value?.name)
    }

    @Test
    fun aNewAccountsLimitFollowsABirthdayAndItsLabelsMayNotClash() = runTest {
        val server = FakeServerAdmin().apply {
            answer("GET /api/libraries", "[]")
            answer("POST /api/users", """{"id":9,"username":"kid","name":"Kid","managed":true}""")
        }
        val page = NewUserViewModel(server)
        page.edit {
            it.copy(
                kind = AccountKind.Managed,
                username = "kid",
                access = AccessChoice(maxAge = 9, birthMonth = "2017-01", showLabels = listOf("Kids"), hideLabels = listOf("kids")),
            )
        }
        assertEquals(NewUserProblem.LabelsInBoth(listOf("Kids")), page.create())
        page.edit { it.copy(access = it.access.copy(hideLabels = listOf("Horror"), maxAgeSystem = "CA")) }
        assertNull(page.create())
        val sent = body(server, "POST /api/users")!!.jsonObject
        // A limit that follows a birthday is the server's to work out.
        assertNull(sent["max_age"])
        assertEquals("\"2017-01\"", sent["birth_month"].toString())
        assertEquals("\"CA\"", sent["max_age_system"].toString())
        assertEquals("""["Kids"]""", sent["show_labels"].toString())
        assertEquals("""["Horror"]""", sent["hide_labels"].toString())
    }

    @Test
    fun aManagedProfileHasNoPasswordAndItsKindForgetsTheRequestsChosen() = runTest {
        val server = FakeServerAdmin().apply {
            answer("GET /api/libraries", "[]")
            answer("POST /api/users", """{"id":9,"username":"kid","name":"Kiddo","managed":true}""")
        }
        val page = NewUserViewModel(server)
        page.edit { it.copy(username = " kid ", displayName = " Kiddo ", password = "left over", requestChoice = RequestAccess.Auto) }
        assertEquals(RequestAccess.Auto, page.form.value.requests)
        page.edit { it.copy(kind = AccountKind.Managed) }
        assertNull(page.form.value.requestChoice)
        assertEquals(RequestAccess.Off, page.form.value.requests)
        assertNull(page.create())
        val sent = body(server, "POST /api/users")!!.jsonObject
        assertEquals("\"kid\"", sent["username"].toString())
        assertEquals("\"Kiddo\"", sent["display_name"].toString())
        assertNull(sent["password"])
        assertNull("the web app makes administrators", sent["is_admin"])
        assertEquals("true", sent["managed"].toString())
        assertEquals("\"off\"", sent["request_access"].toString())
    }

    @Test
    fun accessHelpersFollowTheWeb() {
        assertEquals(listOf(2L, 3L), toggled(listOf(2L), 3L))
        assertEquals(listOf(3L), toggled(listOf(2L, 3L), 2L))
        assertTrue(AccessChoice(allLibraries = false).noLibrary)
        assertFalse(AccessChoice().noLibrary)
        assertTrue(pinOk("1234"))
        assertTrue(pinOk("12345678"))
        assertFalse(pinOk("123"))
        assertFalse(pinOk("123456789"))
        assertFalse(pinOk("12a4"))
        // Counted in characters as people see them, as the web does.
        assertNull(passwordProblem("😀😀😀😀😀😀😀😀"))
        val admin = PleyJson.decodeFromString(UserAccount.serializer(), """{"id":1,"username":"a","name":"a","is_admin":true,"request_access":"auto"}""")
        assertEquals(RequestAccess.Auto, requestsShown(admin))
        assertEquals(RequestAccess.Ask, requestsShown(admin.copy(isAdmin = false, requestAccess = RequestAccess.Ask)))
    }
}
