package app.pley.android.ui.phone.onboarding

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.pressKey
import app.pley.android.auth.Needs
import app.pley.android.auth.TOO_OLD
import app.pley.android.common.UiText
import app.pley.android.screens.onboarding.BrowserSignIn
import app.pley.android.screens.onboarding.LinkCode
import app.pley.android.screens.onboarding.LinkStatus
import app.pley.android.screens.onboarding.LinkUiState
import app.pley.android.screens.onboarding.ProfileTile
import app.pley.android.screens.onboarding.ProfilesStep
import app.pley.android.screens.onboarding.ServerRow
import app.pley.android.screens.onboarding.ServersProblem
import app.pley.android.screens.onboarding.ServersUiState
import app.pley.android.screens.onboarding.SignInProblem
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import app.pley.android.common.R as CommonR

/**
 * Getting in on a phone: the sign-in page (cards, the typed address, the
 * browser sign-in's states and errors), the code page, and the PIN pad by
 * touch and by a keyboard's digits, Backspace, Enter and Escape.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class PhoneOnboardingUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val nas = ServerRow("http://192.168.1.20:7539", "Living room NAS", "192.168.1.20:7539", saved = false, found = true)
    private val office = ServerRow("http://192.168.1.42:7539", "Office", "192.168.1.42:7539", saved = true, found = true)

    private val events = mutableListOf<String>()

    private fun signIn(state: ServersUiState, signIn: BrowserSignIn = BrowserSignIn.Idle, withoutBrowser: Boolean = false) {
        compose.setContent {
            PhoneTheme {
                PhoneSignInContent(
                    state = state,
                    signIn = signIn,
                    withoutBrowser = withoutBrowser,
                    onSelect = { events += "select $it" },
                    onForget = { events += "forget $it" },
                    onConnect = { events += "connect $it" },
                    onAddressEdited = {},
                    onSearchAgain = { events += "search" },
                    onDemo = { events += "demo" },
                    onOpenSettings = { events += "settings" },
                    onContinue = { events += "continue" },
                    onUseCode = { events += "code" },
                    onRetry = { events += "retry" },
                    onOpenInBrowser = { events += "browser $it" },
                )
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun foundServersAreCardsAndContinueSignsInToTheChosenOne() {
        signIn(ServersUiState(rows = listOf(nas, office), selected = nas.url, searched = true))
        compose.onNodeWithText("Choose your server. Pley found these on your network.").assertExists()
        compose.onNodeWithTag(SERVER_CARD_TAG + nas.url).assertIsSelected()
        compose.onNodeWithText("192.168.1.42:7539").assertExists()
        compose.onNodeWithTag(SERVER_CARD_TAG + office.url).performClick()
        compose.onNodeWithTag(CONTINUE_TAG).assertIsEnabled().performClick()
        // With a browser, there's no code to choose.
        compose.onNodeWithText("Use a code instead").assertDoesNotExist()
        compose.onNodeWithText("two-step", substring = true).assertDoesNotExist()
        assertEquals(listOf("select ${office.url}", "continue"), events)
    }

    @Test
    fun aChosenServerThatRequiresTwoStepSignInSaysSo() {
        signIn(ServersUiState(rows = listOf(nas.copy(twoStepRequired = true), office), selected = nas.url, searched = true))
        val twoStep = "This server requires two-step sign-in: you'll set it up as you sign in, if you haven't yet."
        compose.onNodeWithText(twoStep).assertExists()
        compose.onNodeWithTag(CONTINUE_TAG).assertIsEnabled()
        // Continue means a typed address now, not that server.
        compose.onNodeWithTag(ENTER_ADDRESS_TAG).performClick()
        compose.onNodeWithTag(ADDRESS_FIELD_TAG).performTextInput("pley.example.com")
        compose.onNodeWithText(twoStep).assertDoesNotExist()
    }

    @Test
    fun anAddressIsTypedAndConnected() {
        signIn(ServersUiState(searched = true))
        compose.onNodeWithText("No server answered. Check that Pley is running on it, or enter its address.").assertExists()
        compose.onNodeWithTag(CONTINUE_TAG).assertIsNotEnabled()
        compose.onNodeWithTag(ENTER_ADDRESS_TAG).performClick()
        compose.onNodeWithTag(ADDRESS_FIELD_TAG).performTextInput("pley.example.com")
        compose.onNodeWithTag(CONNECT_TAG).performClick()
        assertEquals(listOf("connect pley.example.com"), events)
    }

    @Test
    fun theUnencryptedWarningAndTheBrowsersStatesShow() {
        signIn(
            ServersUiState(rows = listOf(nas), selected = nas.url, problem = ServersProblem.Unreachable("192.168.1.20:7539", UiText.res(CommonR.string.error_nothing_answers))),
            BrowserSignIn.Waiting,
        )
        compose.onNodeWithText("Couldn't reach 192.168.1.20:7539: nothing answers there").assertExists()
        compose.onNodeWithText("Finish signing in in your browser.").assertExists()
        compose.onNodeWithTag(CONTINUE_TAG).assertIsNotEnabled()
    }

    @Test
    fun anOlderServerOffersItsPageInTheBrowser() {
        signIn(
            ServersUiState(rows = listOf(nas), selected = nas.url),
            BrowserSignIn.Failed(nas.url, SignInProblem.TooOld, TOO_OLD),
        )
        compose.onNodeWithText("Update Pley on Living room NAS to sign in from the app").assertExists()
        compose.onNodeWithText("Open in browser").performClick()
        compose.onNodeWithText("Retry").performClick()
        assertEquals(listOf("browser ${nas.url}", "retry"), events)
    }

    @Test
    fun withoutABrowserTheButtonSignsInWithACode() {
        signIn(ServersUiState(rows = listOf(nas), selected = nas.url), withoutBrowser = true)
        compose.onNodeWithText("Sign in with a code from another device").performClick()
        compose.onNodeWithText("Use a code instead").assertDoesNotExist()
        assertEquals(listOf("code"), events)
    }

    @Test
    fun theCodePageShowsTheCodeAndAddress() {
        compose.setContent {
            PhoneTheme {
                PhoneLinkContent(
                    LinkUiState("http://192.168.1.20:7539", LinkCode("BCDF-GHJK", "192.168.1.20:7539/link", "q", Long.MAX_VALUE), LinkStatus.Declined),
                    onNewCode = { events += "new" },
                    onChangeServer = {},
                )
            }
        }
        compose.onNodeWithText("192.168.1.20:7539/link").assertExists()
        // Read out letter by letter, as one node.
        compose.onNodeWithTag(CODE_TAG).assertContentDescriptionEquals("The code: B C D F, G H J K")
        compose.onNodeWithText("Request declined").assertExists()
        compose.onNodeWithText("two-step", substring = true).assertDoesNotExist()
        compose.onNodeWithTag(NEW_CODE_TAG).performClick()
        assertEquals(listOf("new"), events)
    }

    @Test
    fun theCodePageSaysWhenTheServerRequiresTwoStepSignIn() {
        compose.setContent {
            PhoneTheme {
                PhoneLinkContent(
                    LinkUiState(
                        "http://192.168.1.20:7539",
                        LinkCode("BCDF-GHJK", "192.168.1.20:7539/link", "q", Long.MAX_VALUE),
                        LinkStatus.Waiting,
                        twoStepRequired = true,
                    ),
                    onNewCode = {},
                    onChangeServer = {},
                )
            }
        }
        compose.onNodeWithText(
            "This server requires two-step sign-in: the device that approves the code sets it up if you haven't yet.",
        ).assertExists()
    }

    @Test
    fun thePinPadTakesTouchAndAKeyboard() {
        val sam = ProfileTile(2, "Sam", null, null, Needs.PIN, current = false)
        var pin = ""
        var step by mutableStateOf(ProfilesStep.Pin(sam))
        compose.setContent {
            PhoneTheme {
                PhonePinPad(
                    step = step,
                    onDigit = { d ->
                        pin += d
                        step = step.copy(digits = pin.length)
                    },
                    onDelete = {
                        pin = pin.dropLast(1)
                        step = step.copy(digits = pin.length)
                    },
                    onSubmit = { events += "ok $pin" },
                    onCancel = { events += "cancel" },
                )
            }
        }
        compose.waitForIdle()
        compose.onNodeWithText("Enter Sam's PIN").assertExists()
        compose.onNodeWithTag("pley.pad.1").performClick()
        compose.onNodeWithTag("pley.pad.ok").assertIsNotEnabled()
        compose.onRoot().performKeyInput {
            pressKey(Key.Two)
            pressKey(Key.Three)
            pressKey(Key.Nine)
            pressKey(Key.Backspace)
            pressKey(Key.Four)
            pressKey(Key.Enter)
            pressKey(Key.Escape)
        }
        compose.waitForIdle()
        assertEquals("1234", pin)
        assertEquals(listOf("ok 1234", "cancel"), events)
    }
}
