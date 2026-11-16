package app.pley.android.ui.phone.admin.users

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import app.pley.android.data.Session
import app.pley.android.model.Me
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.testing.data.FakeServerAdmin
import app.pley.android.ui.phone.account.AccountTestApplication
import app.pley.android.ui.phone.account.AccountTestGraph
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Another administrator's page is only shown, with a line saying their
 * role, sign-in, devices and account change in the web app: none of what
 * the server would refuse is offered. An account that isn't an
 * administrator's offers it all. The screen is tall, so every group is
 * drawn and what isn't offered is truly left out.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h3000dp-xxhdpi", application = AccountTestApplication::class)
class UserPageUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val graph = AccountTestGraph()
    private val server = FakeServerAdmin().apply {
        answer(
            "GET /api/users",
            """[{"id":1,"username":"ann","name":"Ann","is_admin":true,"home":true,"has_password":true},
                {"id":2,"username":"bea","name":"Bea","home":true,"has_password":true,"has_pin":true,"two_step":true,"request_access":"ask"},
                {"id":3,"username":"cid","name":"Cid","is_admin":true,"home":true,"has_password":true,"two_step":true,"request_access":"auto"}]""",
        )
        answer("GET /api/libraries", "[]")
        answer("GET /api/server", """{"name":"Home","version":"0.1.6"}""")
        answer("GET /api/ratings/systems", """{"region":"US","systems":[]}""")
        answer("GET /api/labels", "[]")
        answer("GET /api/system", "{}")
        for ((id, name) in listOf(2 to "Bea", 3 to "Cid")) {
            answer("GET /api/users/$id/devices", """[{"id":"s1","name":"Firefox on Windows","ip":"10.0.0.2","last_seen_at":5}]""")
            answer(
                "GET /api/users/$id/apps",
                """[{"id":7,"client_id":"https://pley.app/android","name":"Pley for Android","first_party":true,"host":"pley.app",
                    "scopes":["library"],"profile":{"id":$id,"name":"$name"},"approved_by":{"id":$id,"name":"$name"}}]""",
            )
        }
    }

    private val navigator = object : Navigator {
        override fun open(key: PleyKey) = Unit
        override fun back() = Unit
        override fun restart(key: PleyKey) = Unit
    }

    @Before
    fun graph() {
        graph.admin = server
        graph.sessionFlow.value = Session(AccountTestGraph.SERVER, Me(id = 1, username = "ann", name = "Ann", accountAdmin = true), setOf("server"))
        AccountTestApplication.graph = graph
    }

    private fun show(id: Long) {
        compose.setContent {
            PhoneTheme {
                val navigation = rememberPhoneNavigation(PleyKey.Home) {}
                val resources = LocalResources.current
                val shell = remember { PhoneShellState(navigation, resources, AccountTestGraph.SERVER) }
                CompositionLocalProvider(
                    LocalPhoneNavigator provides navigator,
                    LocalPhoneShell provides shell,
                    LocalOffline provides shell.offlineState,
                ) { UserAccountPage(id) }
            }
        }
        compose.waitUntil(5_000) { compose.onAllNodesWithText("Firefox on Windows").fetchSemanticsNodes().isNotEmpty() }
    }

    @Test
    fun anotherAdministratorsAccountIsOnlyShown() {
        show(3)
        compose.onNodeWithTag(USER_OTHER_ADMIN).assertExists()
        compose.onNodeWithText("Another administrator's role, sign-in, devices and account change in Pley's web app.").assertExists()
        compose.onNodeWithTag(USER_NAME).assertIsNotEnabled()
        // Their devices and apps are listed, as the web's Users page lists them.
        compose.onNodeWithText("Pley for Android").assertExists()
        for (tag in listOf(USER_ADMIN, USER_DISABLED, USER_PASSWORD, USER_REMOVE_PIN, USER_SIGN_OUT_EVERYWHERE, USER_DELETE)) {
            compose.onNodeWithTag(tag).assertDoesNotExist()
        }
        for (text in listOf("Profile PIN", "Two-step sign-in", "Sign out", "Member of this home")) {
            assertEquals(text, 0, compose.onAllNodesWithText(text).fetchSemanticsNodes().size)
        }
        assertEquals(emptyList<String>(), server.calls.map { it.route }.filterNot { it.startsWith("GET ") })
    }

    @Test
    fun anAccountThatIsntAnAdministratorsOffersEveryChange() {
        show(2)
        compose.onNodeWithTag(USER_OTHER_ADMIN).assertDoesNotExist()
        compose.onNodeWithTag(USER_NAME).assertIsEnabled()
        // The role is only shown: the web app makes administrators.
        compose.onNodeWithTag(USER_ADMIN).assertIsNotEnabled()
        for (tag in listOf(USER_DISABLED, USER_PASSWORD, USER_REMOVE_PIN, USER_SIGN_OUT_EVERYWHERE, USER_DELETE)) {
            compose.onNodeWithTag(tag).assertIsEnabled()
        }
        for (text in listOf("Profile PIN", "Two-step sign-in", "Sign out")) {
            assertEquals(text, 1, compose.onAllNodesWithText(text).fetchSemanticsNodes().size)
        }
    }
}
