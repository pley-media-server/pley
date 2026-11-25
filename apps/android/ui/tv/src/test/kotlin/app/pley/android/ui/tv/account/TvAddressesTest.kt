package app.pley.android.ui.tv.account

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.SoftwareKeyboardController
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performTextInput
import app.pley.android.auth.AddressStatus
import app.pley.android.common.Tier
import app.pley.android.screens.account.AddressProblem
import app.pley.android.screens.account.AddressRow
import app.pley.android.screens.account.AddressesUiState
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Addresses with the remote: focus starts on the first address and each
 * says in words what Pley knows of it; Select on one added on this TV asks
 * first, with focus on Cancel, and focus goes on to its neighbor once it
 * is removed; Add an address types one on the TV's keyboard and comes back
 * to the list, on Add an address, once added or on Back.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvAddressesTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    /** The TV's keyboard, as the field asks it to show. */
    private class Keyboard : SoftwareKeyboardController {
        var shown = 0

        override fun show() {
            shown++
        }

        override fun hide() = Unit
    }

    private val keyboard = Keyboard()

    private val home = AddressRow(SERVER, "192.168.1.20", AddressStatus.IN_USE, removable = false)
    private val away = AddressRow(AWAY, "pley.example.com", AddressStatus.READY, removable = true)

    private var state by mutableStateOf(AddressesUiState(server = SERVER, inUse = "192.168.1.20", atHome = true, home = listOf(home), away = listOf(away)))
    private val added = mutableListOf<String>()
    private val removed = mutableListOf<String>()

    /** The page over [state]; adding and removing change it as the server's addresses would. */
    private fun show() {
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalSoftwareKeyboardController provides keyboard) {
                    TvAddressesContent(
                        state,
                        onAdd = { typed, done ->
                            added += typed
                            state = state.copy(away = listOf(away))
                            done()
                        },
                        onEdited = { state = state.copy(problem = null) },
                        onRemove = { url ->
                            removed += url
                            state = state.copy(away = state.away.filterNot { it.url == url })
                        },
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    @Test
    fun focusStartsOnTheFirstAddressAndEachSaysWhatPleyKnowsOfIt() {
        show()
        compose.onNodeWithText("Pley picks the best one by itself.").assertExists()
        compose.onNodeWithText("At home").assertExists()
        compose.onNodeWithText("Away").assertExists()
        compose.focused().assert(hasText("192.168.1.20")).assert(hasText("In use")).assert(!hasText("Remove"))
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("pley.example.com")).assert(hasText("Ready")).assert(hasText("Remove"))
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(TV_ADDRESSES_ADD).assertIsFocused()
        // Two addresses already: no nudge to add another.
        compose.onNodeWithText("Add another to play at home and away.").assertDoesNotExist()
    }

    @Test
    fun aHomeAddressSaysWhatItNeedsAndOneNotTriedSaysSoOnlyWhileChecking() {
        state = state.copy(
            home = listOf(home.copy(status = AddressStatus.NEEDS_HOME_NETWORK)),
            away = listOf(away.copy(status = AddressStatus.UNTRIED)),
            checking = true,
        )
        show()
        compose.onNodeWithText("Needs Wi-Fi or Ethernet").assertExists()
        compose.onNodeWithText("Checking…").assertExists()
        state = state.copy(checking = false)
        compose.waitForIdle()
        compose.onNodeWithText("Checking…").assertDoesNotExist()
    }

    @Test
    fun selectOnAnAddressAddedHereAsksFirstAndFocusGoesOnOnceItIsRemoved() {
        show()
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        compose.onNodeWithText("Remove pley.example.com?").assertExists()
        compose.focused().assert(hasText("Cancel"))
        compose.press(Key.DirectionRight)
        compose.focused().assert(hasText("Remove"))
        compose.press(Key.DirectionCenter)
        assertEquals(listOf(AWAY), removed)
        compose.onNodeWithText("Remove pley.example.com?").assertDoesNotExist()
        compose.onNodeWithTag(TV_ADDRESS_ROW + AWAY).assertDoesNotExist()
        // The last address went: focus is on the one before it, not nowhere.
        compose.focused().assert(hasText("192.168.1.20"))
    }

    @Test
    fun cancelKeepsTheAddressAndGivesItFocusBack() {
        show()
        compose.press(Key.DirectionDown)
        compose.press(Key.DirectionCenter)
        compose.focused().assert(hasText("Cancel"))
        compose.press(Key.DirectionCenter)
        compose.onNodeWithText("Remove pley.example.com?").assertDoesNotExist()
        assertEquals(emptyList<String>(), removed)
        compose.onNodeWithTag(TV_ADDRESS_ROW + AWAY).assertIsFocused()
    }

    @Test
    fun theServersOwnAddressIsntRemovedHere() {
        show()
        compose.onNodeWithTag(TV_ADDRESS_ROW + SERVER).assertIsFocused()
        compose.press(Key.DirectionCenter)
        compose.onNodeWithText("Remove 192.168.1.20?").assertDoesNotExist()
        compose.onNodeWithTag(TV_ADDRESS_ROW + SERVER).assertIsFocused()
    }

    @Test
    fun addAnAddressTypesItOnTheTvsKeyboardAndComesBackToTheList() {
        state = state.copy(away = emptyList())
        show()
        compose.onNodeWithText("Add another to play at home and away.").assertExists()
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(TV_ADDRESSES_ADD).assertIsFocused()
        compose.press(Key.DirectionCenter)
        // The TV's keyboard is up, typing into an empty field, under what it is for.
        compose.onNodeWithText("Add an address").assertExists()
        assertTrue(keyboard.shown > 0)
        val typing = compose.onNode(hasSetTextAction())
        typing.assertIsFocused()
        typing.performTextInput("pley.example.com")
        typing.performImeAction()
        assertEquals(listOf("pley.example.com"), added)
        compose.waitForIdle()
        compose.onNodeWithTag(TV_ADDRESS_ROW + AWAY).assertExists()
        compose.onNodeWithTag(TV_ADDRESSES_ADD).assertIsFocused()
    }

    @Test
    fun backLeavesTheAddressUnadded() {
        show()
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionCenter)
        compose.onNode(hasSetTextAction()).performTextInput("pley.example.com")
        // The first Back closes the keyboard; the next leaves.
        back()
        back()
        assertEquals(emptyList<String>(), added)
        compose.onNodeWithTag(TV_ADDRESSES_PAGE).assertExists()
        compose.onNodeWithTag(TV_ADDRESSES_ADD).assertIsFocused()
    }

    @Test
    fun whatWasWrongWithAnAddressSaysWhy() {
        show()
        compose.press(Key.DirectionDown, times = 2)
        compose.press(Key.DirectionCenter)
        compose.onNode(hasSetTextAction()).performTextInput("pley.example.com")
        compose.waitForIdle()
        state = state.copy(problem = AddressProblem.INTERNET)
        compose.waitForIdle()
        compose.onNodeWithText("Internet addresses are added on the server.").assertExists()
        state = state.copy(problem = AddressProblem.KNOWN)
        compose.waitForIdle()
        compose.onNodeWithText("Already in the list.").assertExists()
    }

    @Test
    fun aServerFromBeforeProofsKeepsToOneAddress() {
        state = state.copy(away = emptyList(), canMove = false)
        show()
        compose.onNodeWithText("Update Pley on your server to use more than one address.").assertExists()
        compose.onNodeWithText("Add another to play at home and away.").assertDoesNotExist()
        compose.onNodeWithTag(TV_ADDRESSES_ADD).assertIsNotEnabled()
        compose.onNodeWithTag(TV_ADDRESS_ROW + SERVER).assertIsFocused()
    }

    private companion object {
        const val SERVER = "http://192.168.1.20:7539"
        const val AWAY = "https://pley.example.com"
    }
}
