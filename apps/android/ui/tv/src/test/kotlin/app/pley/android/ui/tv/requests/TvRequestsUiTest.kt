package app.pley.android.ui.tv.requests

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasImeAction
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.text.input.ImeAction
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.Tier
import app.pley.android.model.Decline
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.RequestsDeps
import app.pley.android.screens.requests.RequestsViewModel
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.RailState
import app.pley.android.ui.tv.shell.TvNavigator
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The requests with the remote, as the Samsung and LG apps have them
 * (packages/tv's requests.test.tsx): the cards with focus on the first,
 * OK on one opens what may be done with it, what can't be undone asks
 * first in the panel and Back from the question goes back to the actions;
 * managers open on Needs attention with its number, filter by kind, and
 * decline with a reason typed on the TV's keyboard.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvRequestsUiTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val stack = mutableStateListOf<NavKey>(PleyKey.Home, PleyKey.Requests)
    private val navigator = TvNavigator(stack, RailState()) {}

    private fun show(requests: FakeRequests) {
        val vm = RequestsViewModel(RequestsDeps(requests, MutableStateFlow(SERVER), MutableStateFlow(true)))
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalTvNavigator provides navigator) { TvRequestsScreen(vm) }
            }
        }
        compose.waitForIdle()
    }

    private fun waitFor(text: String) {
        val shown = hasText(text, substring = true) or hasContentDescription(text, substring = true)
        compose.waitUntil(WAIT_MS) { compose.onAllNodes(shown).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    @Test
    fun okOnACardOpensWhatMayBeDoneAndWithdrawingAsksFirst() {
        val requests = FakeRequests()
        show(requests)
        waitFor("Inside Out 2")
        compose.onNodeWithText("Your requests").assertExists()
        compose.onNodeWithTag(TV_REQUESTS_CARD + 48).assertIsFocused()
        compose.press(Key.Enter)
        compose.onNodeWithTag(TV_REQUEST_PANEL).assertExists()
        compose.onNodeWithTag(TV_REQUEST_ROW + "Withdraw").assertIsFocused()
        compose.press(Key.Enter)
        waitFor("Withdraw your request for “Inside Out 2”?")
        // Cancel first: OK pressed twice undoes nothing. Back from the question goes back to the actions.
        compose.focused().assert(hasText("Cancel"))
        back()
        compose.onNodeWithTag(TV_REQUEST_ROW + "Withdraw").assertIsFocused()
        compose.press(Key.Enter)
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(TV_REQUEST_ROW + "confirm").assertIsFocused()
        compose.press(Key.Enter)
        compose.waitUntil(WAIT_MS) { requests.api.calls("deleteRequest").isNotEmpty() }
        assertEquals(listOf<Any?>(48L), requests.api.calls("deleteRequest").map { it.args.single() })
        // The card went: focus is on the one now in its place.
        compose.waitUntil(WAIT_MS) { compose.onAllNodes(hasContentDescription("Inside Out 2", substring = true)).fetchSemanticsNodes().isEmpty() }
        compose.waitForIdle()
        compose.onNodeWithTag(TV_REQUESTS_CARD + 47).assertIsFocused()
    }

    @Test
    fun detailsOpensTheTitlesPage() {
        show(FakeRequests())
        waitFor("Inside Out 2")
        compose.press(Key.Enter)
        compose.press(Key.DirectionDown)
        compose.onNodeWithTag(TV_REQUEST_ROW + "details").assertIsFocused()
        compose.press(Key.Enter)
        assertEquals(PleyKey.DiscoverTitle(TitleKind.Movie, 1022789), stack.last())
    }

    @Test
    fun managersOpenOnNeedsAttentionWithItsNumberAndFilterByKind() {
        val requests = FakeRequests(Fixtures.decode<RequestSummary>("requests/summary_manager.json"))
        show(requests)
        waitFor("The Substance")
        compose.onNodeWithText("Requests").assertExists()
        compose.onNodeWithText("Needs attention · 3").assertExists()
        assertEquals(true, (requests.api.calls("requests").first().args.single() as RequestQuery).attention)
        // Up from the cards: the filters; the kind chip goes movies and shows, movies, shows.
        compose.press(Key.DirectionUp)
        compose.onNodeWithTag(TV_REQUESTS_KIND).assertIsFocused()
        compose.press(Key.Enter)
        compose.waitUntil(WAIT_MS) { (requests.api.calls("requests").last().args.single() as RequestQuery).kind == TitleKind.Movie }
        compose.onNodeWithText("Movies").assertExists()
    }

    @Test
    fun aManagerDeclinesWithAReasonTypedOnTheKeyboard() {
        val requests = FakeRequests(Fixtures.decode<RequestSummary>("requests/summary_manager.json"))
        show(requests)
        waitFor("The Substance")
        compose.press(Key.Enter)
        // Approve, Decline, then Decline with a reason….
        compose.onNodeWithTag(TV_REQUEST_ROW + "Approve").assertIsFocused()
        compose.press(Key.DirectionDown, 2)
        compose.onNodeWithTag(TV_REQUEST_ROW + "DeclineWithReason").assertIsFocused()
        compose.press(Key.Enter)
        waitFor("Decline “The Substance”?")
        val reason = hasSetTextAction() and hasImeAction(ImeAction.Done)
        compose.onNode(reason).performTextInput("Not for us")
        compose.onNode(reason).performImeAction()
        compose.waitForIdle()
        compose.onNodeWithTag(TV_REQUEST_ROW + "decline").assertIsFocused()
        compose.press(Key.Enter)
        compose.waitUntil(WAIT_MS) { requests.api.calls("declineRequest").isNotEmpty() }
        assertEquals(listOf<Any?>(44L, Decline("Not for us")), requests.api.calls("declineRequest").single().args)
    }

    private companion object {
        const val SERVER = "http://127.0.0.1:1"
        const val WAIT_MS = 5_000L
    }
}
