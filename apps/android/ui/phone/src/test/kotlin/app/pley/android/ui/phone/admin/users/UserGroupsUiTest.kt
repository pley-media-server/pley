package app.pley.android.ui.phone.admin.users

import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import app.pley.android.model.admin.UserAccount
import app.pley.android.screens.admin.users.UserPage
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * An account's page asks before what takes something from the account at
 * once (its administrator's role, disabling it, its PIN), and only sends
 * it once that is confirmed; what gives back (enabling it) is sent as it
 * is chosen.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class UserGroupsUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val cid = UserAccount(id = 3, username = "cid", name = "Cid", isAdmin = true, hasPin = true)

    private val sent = mutableListOf<String>()

    private fun roles(user: UserAccount = cid) {
        compose.setContent {
            PhoneTheme {
                RolesGroup(
                    UserPage(user, otherAdmins = 1),
                    self = false,
                    idle = true,
                    onRemoveAdmin = { sent += "admin off" },
                    onHome = { sent += "home $it" },
                    onDisabled = { sent += "disabled $it" },
                )
            }
        }
    }

    @Test
    fun takingTheRoleAwayAsksFirst() {
        roles()
        compose.onNodeWithTag(USER_ADMIN).performClick()
        compose.onNodeWithText("Take away Cid's administrator role?").assertExists()
        assertEquals(emptyList<String>(), sent)
        compose.onNodeWithText("Take it away").performClick()
        assertEquals(listOf("admin off"), sent)
        compose.onNodeWithText("Take away Cid's administrator role?").assertDoesNotExist()
    }

    @Test
    fun disablingAsksFirstAndCancelSendsNothing() {
        roles()
        compose.onNodeWithTag(USER_DISABLED).performClick()
        compose.onNodeWithText("Cid is signed out everywhere, and can't sign in or play anything until the account is enabled again.").assertExists()
        compose.onNodeWithText("Cancel").performClick()
        assertEquals(emptyList<String>(), sent)
        compose.onNodeWithTag(USER_DISABLED).performClick()
        compose.onNodeWithText("Disable").performClick()
        assertEquals(listOf("disabled true"), sent)
    }

    @Test
    fun enablingAgainIsSentAtOnce() {
        roles(cid.copy(disabled = true))
        compose.onNodeWithTag(USER_DISABLED).performClick()
        assertEquals(listOf("disabled false"), sent)
        compose.onNodeWithText("Disable Cid?").assertDoesNotExist()
    }

    @Test
    fun removingThePinAsksFirst() {
        compose.setContent {
            PhoneTheme { PinGroup(cid.copy(isAdmin = false), idle = true, onPin = {}, onRemovePin = { sent += "pin off" }) }
        }
        compose.onNodeWithTag(USER_REMOVE_PIN).performClick()
        compose.onNodeWithText("Remove the PIN for Cid?").assertExists()
        compose.onNodeWithText("Anyone in the household can then switch to Cid without a PIN, until a new one is set.").assertExists()
        assertEquals(emptyList<String>(), sent)
        compose.onNode(hasText("Remove PIN") and hasAnyAncestor(isDialog())).performClick()
        assertEquals(listOf("pin off"), sent)
    }

    @Test
    fun aQuestionStaysOpenWhenThePhoneTurns() {
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            PhoneTheme {
                RolesGroup(UserPage(cid, otherAdmins = 1), self = false, idle = true, onRemoveAdmin = { sent += "admin off" }, onHome = {}, onDisabled = {})
            }
        }
        compose.onNodeWithTag(USER_ADMIN).performClick()
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Take away Cid's administrator role?").assertExists()
        compose.onNodeWithText("Take it away").performClick()
        assertEquals(listOf("admin off"), sent)
    }
}
