package app.pley.android.ui.tv.onboarding

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import app.pley.android.auth.Needs
import app.pley.android.common.FoundServer
import app.pley.android.screens.onboarding.LinkCode
import app.pley.android.screens.onboarding.LinkStatus
import app.pley.android.screens.onboarding.LinkUiState
import app.pley.android.screens.onboarding.ProfileTile
import app.pley.android.screens.onboarding.ProfilesStep
import app.pley.android.screens.onboarding.ServerRow
import app.pley.android.screens.onboarding.ServersProblem
import app.pley.android.screens.onboarding.ServersUiState
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Getting in on TV with the remote (D-pad rule 11): the PIN pad (focus on
 * 5, the remote's digits, Delete, OK, a locked PIN), the address pad, the
 * link screen's focus, and the servers list's.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-land-television-xhdpi")
class TvOnboardingUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val sam = ProfileTile(2, "Sam", null, "#3d9970", Needs.PIN, current = false)

    // ---------------------------------------------------------------- the PIN pad

    private var pin = ""
    private var submitted = mutableListOf<String>()
    private var cancelled = 0

    private fun pinPad(lockedOut: Boolean = false) {
        var step by mutableStateOf(ProfilesStep.Pin(sam, lockedOut = lockedOut))
        compose.setContent {
            TvTheme(reducedMotion = true) {
                TvPinPad(
                    step = step,
                    onDigit = { d ->
                        if (pin.length < 8) pin += d
                        step = step.copy(digits = pin.length)
                    },
                    onDelete = {
                        pin = pin.dropLast(1)
                        step = step.copy(digits = pin.length)
                    },
                    onSubmit = {
                        if (pin.length >= 4) submitted += pin
                        pin = ""
                        step = step.copy(digits = 0)
                    },
                    onAnother = { cancelled++ },
                )
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun thePinPadStartsOnFive() {
        pinPad()
        compose.onNodeWithTag("pley.pad.5").assertIsFocused()
        compose.onNodeWithText("Enter Sam's PIN").assertExists()
    }

    @Test
    fun theRemotesDigitsTypeAndTheDpadPressesKeys() {
        pinPad()
        compose.press(Key.One)
        compose.press(Key.Two)
        compose.press(Key.NumPad3)
        // Select on 5, then Delete removes it.
        compose.press(Key.DirectionCenter)
        assertEquals("1235", pin)
        compose.press(Key.Backspace)
        assertEquals("123", pin)
        compose.press(Key.Four)
        // Down twice and right to OK.
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionRight)
        compose.onNodeWithTag("pley.pad.ok").assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(listOf("1234"), submitted)
    }

    @Test
    fun aPinTypedWithTheRemotesDigitsIsSentWithOk() {
        pinPad()
        compose.press(Key.One)
        compose.press(Key.Two)
        compose.press(Key.Three)
        compose.onNodeWithTag("pley.pad.5").assertIsFocused()
        compose.press(Key.Four)
        // Four digits could be sent: OK has focus, so OK on the remote sends them rather than typing a 5.
        compose.onNodeWithTag("pley.pad.ok").assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(listOf("1234"), submitted)
    }

    @Test
    fun aLockedPinCantBeTypedAndOffersAnotherProfile() {
        pinPad(lockedOut = true)
        compose.onNodeWithTag("pley.pad.5").assertIsNotEnabled()
        compose.onNodeWithText("Use another profile").assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(1, cancelled)
    }

    // ---------------------------------------------------------------- the address pad

    @Test
    fun theAddressPadStartsWithTheNetworkAndConnects() {
        val connected = mutableListOf<String>()
        compose.setContent {
            TvTheme(reducedMotion = true) {
                TvAddressPad(ServersUiState(subnet = "192.168.1."), onConnect = { connected += it }, onEdited = {})
            }
        }
        compose.waitForIdle()
        compose.onNodeWithTag("pley.pad.5").assertIsFocused()
        compose.press(Key.Two)
        compose.press(Key.Zero)
        // Down to "0", left to ".", right twice to ":".
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionRight)
        compose.onNodeWithTag("pley.pad.:").assertIsFocused()
        compose.press(Key.DirectionCenter)
        compose.press(Key.Eight)
        compose.press(Key.Zero)
        compose.press(Key.Backspace)
        compose.press(Key.One)
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(CONNECT_TAG).assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(listOf("192.168.1.20:81"), connected)
    }

    @Test
    fun aKeyboardTypesAServersNameIntoThePad() {
        val connected = mutableListOf<String>()
        compose.setContent {
            TvTheme(reducedMotion = true) {
                TvAddressPad(ServersUiState(), onConnect = { connected += it }, onEdited = {})
            }
        }
        compose.waitForIdle()
        compose.press(Key.N)
        compose.press(Key.A)
        compose.press(Key.S)
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionRight)
        compose.press(Key.DirectionCenter)
        compose.press(Key.Eight)
        compose.press(Key.One)
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        assertEquals(listOf("nas:81"), connected)
    }

    @Test
    fun theAddressPadSaysWhatWentWrong() {
        compose.setContent {
            TvTheme(reducedMotion = true) {
                TvAddressPad(ServersUiState(addressProblem = ServersProblem.NotPley("router:7539")), onConnect = {}, onEdited = {})
            }
        }
        compose.onNodeWithText("Something answers at router:7539, but it isn't Pley.").assertExists()
    }

    // ---------------------------------------------------------------- the link screen

    private val code = LinkCode("BCDF-GHJK", "192.168.1.20:7539/link", "http://192.168.1.20:7539/link?code=BCDFGHJK", Long.MAX_VALUE)

    private fun link(status: LinkStatus, twoStepRequired: Boolean = false): () -> Int {
        var state by mutableStateOf(LinkUiState("http://192.168.1.20:7539", code, LinkStatus.Waiting, twoStepRequired = twoStepRequired))
        var newCodes = 0
        compose.setContent {
            TvTheme(reducedMotion = true) {
                TvLinkContent(state, onNewCode = { newCodes++ }, backLabel = "Change server", onBack = {})
            }
        }
        compose.waitForIdle()
        state = state.copy(status = status)
        compose.waitForIdle()
        return { newCodes }
    }

    @Test
    fun theLinkScreenShowsTheCodeAndAddressAndFocusesChangeServer() {
        link(LinkStatus.Waiting)
        compose.onNodeWithTag(BACK_TAG).assertIsFocused()
        compose.onNodeWithText("Waiting for approval…").assertExists()
        compose.onNodeWithText("BCDF", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("GHJK", useUnmergedTree = true).assertExists()
        // Read letter by letter, as one node.
        compose.onNodeWithTag(CODE_TAG).assertContentDescriptionEquals("The code: B C D F, G H J K")
        compose.onNodeWithText("two-step", substring = true).assertDoesNotExist()
    }

    @Test
    fun theLinkScreenSaysWhenTheServerRequiresTwoStepSignIn() {
        link(LinkStatus.Waiting, twoStepRequired = true)
        compose.onNodeWithText("This server requires two-step sign-in: your phone sets it up if you haven't yet.").assertExists()
        // The rest of the screen is as it was.
        compose.onNodeWithText("BCDF", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("Waiting for approval…").assertExists()
        compose.onNodeWithTag(BACK_TAG).assertIsFocused()
    }

    @Test
    fun aDeclineFocusesGetANewCode() {
        val newCodes = link(LinkStatus.Declined)
        compose.onNodeWithText("Request declined").assertExists()
        compose.onNodeWithTag(NEW_CODE_TAG).assertIsFocused()
        compose.press(Key.DirectionCenter)
        assertEquals(1, newCodes())
    }

    @Test
    fun anExpiredCodeWaitsForOk() {
        val newCodes = link(LinkStatus.Expired)
        compose.onNodeWithText("Code expired: press OK for a new one").assertExists()
        compose.press(Key.DirectionCenter)
        assertEquals(1, newCodes())
    }

    @Test
    fun signedInSaysWho() {
        link(LinkStatus.SignedIn("Anna"))
        compose.onNodeWithText("Signed in as Anna").assertExists()
        compose.onNodeWithTag(QR_TAG).assertDoesNotExist()
        compose.onNodeWithTag(BACK_TAG).assertDoesNotExist()
    }

    // ---------------------------------------------------------------- the servers

    private fun servers(state: ServersUiState, chosen: MutableList<String> = mutableListOf()) {
        compose.setContent {
            TvTheme(reducedMotion = true) {
                TvServersContent(state, onChoose = { chosen += it }, onForget = {}, onEnterAddress = {}, onSearchNetwork = {}, onDemo = {})
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun theFirstServerHasFocusAndOkOpensIt() {
        val nas = FoundServer("http://192.168.1.20:7539", "Living room NAS", null, "1.0")
        val chosen = mutableListOf<String>()
        servers(
            ServersUiState(
                rows = listOf(ServerRow(nas.url, nas.name, "192.168.1.20:7539", saved = true, found = true), ServerRow("http://192.168.1.42:7539", "Office", "192.168.1.42:7539", saved = false, found = true)),
                searched = true,
            ),
            chosen,
        )
        compose.onNodeWithTag(SERVER_ROW_TAG + nas.url).assertIsFocused()
        compose.onNodeWithText("192.168.1.20:7539").assertExists()
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        assertEquals(listOf("http://192.168.1.42:7539"), chosen)
    }

    @Test
    fun withNoServerEnterAnAddressHasFocus() {
        servers(ServersUiState(searched = true))
        compose.onNodeWithTag(ENTER_ADDRESS_TAG).assertIsFocused()
        compose.onNodeWithText("No servers found. Make sure this TV and the server are on the same network.").assertExists()
    }

    @Test
    fun aSavedServerThatDoesntAnswerAndALookalikeAreTagged() {
        servers(
            ServersUiState(
                rows = listOf(
                    ServerRow("http://192.168.1.20:7539", "NAS", "192.168.1.20:7539", saved = true, found = false, unreachable = true),
                    ServerRow("http://192.168.1.66:7539", "NAS", "192.168.1.66:7539", saved = false, found = true, lookalike = true),
                ),
                searched = true,
            ),
        )
        compose.onNodeWithText("Can't reach it").assertExists()
        compose.onNodeWithText("Not your saved server").assertExists()
        compose.onNodeWithText("Hold OK on a saved server to forget it.").assertExists()
    }
}
