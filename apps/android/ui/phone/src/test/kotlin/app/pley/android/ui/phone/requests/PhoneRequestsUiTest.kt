package app.pley.android.ui.phone.requests

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextReplacement
import app.pley.android.data.RequestNotice
import app.pley.android.model.Decline
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestList
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.network.ApiException
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.ItemRequestState
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeRequests
import app.pley.android.ui.phone.kit.LocalOffline
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.rememberPhoneNavigation
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The phone's request screens over fake requests: Discover's hero asking
 * for a movie and saying what comes next, TMDB's results for words (See
 * all on Discover), a show's season picker, the managers' Needs attention
 * with Decline's reason and Approve…, the person's own requests with Watch
 * and Withdraw asking first, Discover when requests are off, Me's ways in,
 * a library title's request, and the news told in a toast.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi", application = RequestsTestApplication::class)
class PhoneRequestsUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val opened = mutableListOf<PleyKey>()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit

        override fun restart(key: PleyKey) {
            opened += key
        }
    }

    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")
    private lateinit var requests: FakeRequests
    private lateinit var shell: PhoneShellState

    private fun show(requests: FakeRequests = FakeRequests(), content: @Composable () -> Unit) {
        this.requests = requests
        RequestsTestApplication.graph = RequestsTestGraph(requests)
        compose.setContent {
            PhoneTheme {
                val navigation = rememberPhoneNavigation(PleyKey.Home) {}
                val resources = LocalResources.current
                val state = remember { PhoneShellState(navigation, resources, RequestsTestGraph.SERVER) }
                shell = state
                CompositionLocalProvider(
                    LocalPhoneNavigator provides navigator,
                    LocalPhoneShell provides state,
                    LocalOffline provides state.offlineState,
                ) { content() }
            }
        }
        compose.waitForIdle()
    }

    /** Work done off the main thread shows [text] in a moment. */
    private fun waitFor(text: String) {
        compose.waitUntil(5_000) { compose.onAllNodesWithText(text, substring = true).fetchSemanticsNodes().isNotEmpty() }
    }

    private fun waitForCall(method: String) {
        compose.waitUntil(5_000) { requests.api.calls(method).isNotEmpty() }
    }

    private fun card(id: Long) = hasAnyAncestor(hasTestTag(REQUEST_CARD + id))

    @Test
    fun discoverAsksForTheHerosMovieAndSaysWhatComesNext() {
        show { PhoneDiscoverScreen() }
        waitFor("Trending this week")
        // Nothing to type in: TMDB is searched from Search.
        compose.onAllNodes(hasSetTextAction()).assertCountEquals(0)
        val hero = hasAnyAncestor(hasTestTag(DISCOVER_HERO))
        compose.onNode(hasText("The Wild Robot") and hero).assertExists()
        compose.onNode(hasText("Request") and hasClickAction() and hero).performClick()
        waitForCall("ask")
        assertEquals(NewRequest(TitleKind.Movie, 1184918), requests.api.calls("ask").single().args.single())
        compose.waitUntil(5_000) { shell.toasts.current.isNotEmpty() }
        assertEquals("Asked for “The Wild Robot”: an administrator decides.", shell.toasts.current.single().message)
        // TMDB's credit at the foot.
        compose.onNodeWithTag(DISCOVER_ROWS).performScrollToNode(hasText("Details and pictures from TMDB."))
    }

    @Test
    fun withRequestsOffDiscoverSaysSoAndOnlyThat() {
        show(FakeRequests(summary.copy(enabled = false))) { PhoneDiscoverScreen() }
        waitFor("Requests are off")
        compose.onNodeWithText("This server doesn't take requests for movies and shows.").assertExists()
        compose.onNodeWithTag(DISCOVER_ROWS).assertDoesNotExist()
        assertTrue(requests.api.calls("discover").isEmpty())
    }

    @Test
    fun seeAllOnDiscoverShowsEveryTitleTmdbFindsForTheWords() {
        show { PhoneDiscoverResultsScreen(PleyKey.DiscoverResults("robot")) }
        waitFor("Results for “robot”")
        // A results view: no field to type in.
        compose.onAllNodes(hasSetTextAction()).assertCountEquals(0)
        assertEquals(listOf<Any?>("robot", 1, false), requests.api.calls("discoverSearch").first().args)
        // Those in a library too, with their badges.
        compose.onNodeWithTag(DISCOVER_RESULTS).performScrollToNode(hasContentDescription("In library", substring = true))
        compose.onNodeWithText("The Wild Robot", useUnmergedTree = true).performClick()
        assertEquals(listOf<PleyKey>(PleyKey.DiscoverTitle(TitleKind.Movie, 1184918)), opened)
    }

    @Test
    fun whenTmdbCantBeSearchedSeeAllOnDiscoverSaysWhyWithRetry() {
        val away = FakeRequests()
        var down = true
        away.searches = { q, page, missing -> if (down) throw ApiException(503, "tmdb_down", "TMDB isn't answering") else away.api.discoverSearch(q, page, missing) }
        show(away) { PhoneDiscoverResultsScreen(PleyKey.DiscoverResults("robot")) }
        waitFor("TMDB isn't answering")
        down = false
        compose.onNodeWithText("Retry").performClick()
        waitFor("Results for “robot”")
    }

    @Test
    fun aShowAsksForTheSeasonsPicked() {
        show { PhoneDiscoverTitleScreen(PleyKey.DiscoverTitle(TitleKind.Show, 1396)) }
        waitFor("Change the equation.")
        compose.onNodeWithText("Request more seasons").performClick()
        compose.onNodeWithText("Which seasons?").assertExists()
        // The missing seasons start ticked, 3 and 5; 4 was declined, 1 and 2 asked for already.
        compose.onNodeWithText("Request 2 seasons").assertExists()
        compose.onNodeWithTag(TITLE_PICKER_SEASON + 5).performClick()
        compose.onNodeWithTag(TITLE_PICKER_REQUEST).performClick()
        waitForCall("ask")
        assertEquals(NewRequest(TitleKind.Show, 1396, listOf(3)), requests.api.calls("ask").single().args.single())
        compose.waitUntil(5_000) { shell.toasts.current.isNotEmpty() }
        assertEquals("Asked for “Breaking Bad”: an administrator decides.", shell.toasts.current.single().message)
    }

    @Test
    fun aTitlePageShowsEachSeasonsStateAndItsCast() {
        show { PhoneDiscoverTitleScreen(PleyKey.DiscoverTitle(TitleKind.Show, 1396)) }
        waitFor("Change the equation.")
        val page = compose.onNodeWithTag(TITLE_PAGE)
        // The request for it, downloading.
        page.performScrollToNode(hasTestTag(REQUEST_CARD + 42))
        page.performScrollToNode(hasTestTag(TITLE_SEASON + 4))
        compose.onNodeWithTag(TITLE_SEASON + 4).assert(hasText("Declined"))
        // The specials come last.
        page.performScrollToNode(hasTestTag(TITLE_SEASON + 0))
        page.performScrollToNode(hasTestTag(TITLE_CAST))
        compose.onAllNodesWithText("Bryan Cranston", substring = true, useUnmergedTree = true).onFirst().assertExists()
    }

    @Test
    fun managersOpenOnNeedsAttentionAndDeclineWithAReason() {
        show(FakeRequests(Fixtures.decode("requests/summary_manager.json"))) { PhoneRequestsScreen() }
        waitFor("The Substance")
        compose.onAllNodesWithText("Requests").onFirst().assertExists()
        compose.onNodeWithText("Needs attention · 3").assertExists()
        compose.onNode(hasText("Decline") and hasClickAction() and card(44)).performClick()
        compose.onNodeWithText("Decline “The Substance”?").assertExists()
        compose.onNodeWithText("The person who asked sees it.", useUnmergedTree = true).assertExists()
        compose.onNodeWithTag(DECLINE_REASON).performTextReplacement("  Not this one ")
        compose.onNode(hasText("Decline") and hasClickAction() and hasAnyAncestor(isDialog())).performClick()
        waitForCall("declineRequest")
        assertEquals(listOf(44L, Decline("Not this one")), requests.api.calls("declineRequest").single().args)
        // Declined, it needs no one now: its card leaves.
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(REQUEST_CARD + 44).fetchSemanticsNodes().isEmpty() }
    }

    @Test
    fun approveWithAsksForTheProfileAndFolder() {
        show(FakeRequests(Fixtures.decode("requests/summary_manager.json"))) { PhoneRequestsScreen() }
        waitFor("The Substance")
        compose.onNode(hasContentDescription("More actions") and card(44)).performClick()
        compose.onNodeWithText("Approve…").performClick()
        waitFor("Approve “The Substance”")
        compose.onNodeWithTag(APPROVE_DIALOG).assertExists()
        compose.onNode(hasText("Approve") and hasClickAction() and hasAnyAncestor(isDialog())).performClick()
        waitForCall("approveRequest")
        assertEquals(44L, requests.api.calls("approveRequest").single().args.first())
    }

    @Test
    fun yourRequestsWithdrawAfterAskingAndWatchWhatIsReady() {
        show { PhoneRequestsScreen() }
        waitFor("Inside Out 2")
        compose.onNodeWithText("Your requests").assertExists()
        // Needs attention is the managers'.
        compose.onNodeWithTag(REQUESTS_VIEW + "Attention").assertDoesNotExist()
        compose.onNode(hasText("Withdraw") and hasClickAction() and card(48)).performClick()
        compose.onNodeWithText("Withdraw your request for “Inside Out 2”?").assertExists()
        compose.onNode(hasText("Withdraw") and hasClickAction() and hasAnyAncestor(isDialog())).performClick()
        waitForCall("deleteRequest")
        assertEquals(listOf<Any>(48L), requests.api.calls("deleteRequest").single().args)
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(REQUEST_CARD + 48).fetchSemanticsNodes().isEmpty() }
        // A declined one says why; one that's here plays from its card.
        val list = compose.onNodeWithTag(REQUESTS_LIST)
        list.performScrollToNode(hasText("“We have enough aliens for now”", substring = true))
        list.performScrollToNode(hasTestTag(REQUEST_CARD + 45))
        compose.onNode(hasText("Watch") and hasClickAction() and card(45)).performClick()
        assertEquals(listOf<PleyKey>(PleyKey.Item(101)), opened)
    }

    @Test
    fun meLeadsToDiscoverAndToRequestsWithTheirNumber() {
        show { Column { RequestsRows(RequestsEntry(discover = true, requests = true, manage = true, badge = 3, failed = 1)) } }
        compose.onNodeWithText("Approve, decline and follow what was asked for").assertExists()
        // Its number in red says, in words, what waits.
        compose.onNodeWithTag(ME_REQUESTS).assert(hasContentDescription("3 requests need you, 1 couldn't be sent"))
        compose.onNodeWithTag(ME_REQUESTS).performClick()
        compose.onNodeWithTag(ME_DISCOVER).performClick()
        assertEquals(listOf(PleyKey.Requests, PleyKey.Discover), opened)
    }

    @Test
    fun mesTabSaysItsNumberInWords() {
        show { RequestsTabIcon(RequestsEntry(requests = true, badge = 2)) { Text("Me") } }
        compose.onNodeWithContentDescription("2 of your requests have news").assertExists()
    }

    @Test
    fun withoutRequestsMeHasNoWayIn() {
        show { Column { RequestsRows(RequestsEntry.None) } }
        compose.onNodeWithTag(ME_DISCOVER).assertDoesNotExist()
        compose.onNodeWithTag(ME_REQUESTS).assertDoesNotExist()
    }

    @Test
    fun aLibraryShowAsksForMoreSeasonsOnItsDiscoverPage() {
        val request = Fixtures.decode<RequestList>("requests/list.json").requests.first { it.id == 47L }
        val state = ItemRequestState(TitleKind.Show, 95396, TitleStatus.Partial, request, more = true, manage = false)
        show { ItemRequestBlock(state) }
        compose.onNodeWithTag(ITEM_REQUEST).assertExists()
        compose.onNodeWithText("Request more seasons").performClick()
        assertEquals(listOf<PleyKey>(PleyKey.DiscoverTitle(TitleKind.Show, 95396, pick = true)), opened)
    }

    @Test
    fun newsIsToldInAToastWithOpen() {
        show { }
        compose.runOnIdle {
            shell.tell(RequestNotice.Ready(key = 1, requestId = 45, title = "Arrival", kind = TitleKind.Movie, itemId = 101, seasons = null, complete = true))
            shell.tell(RequestNotice.Failed(key = 2, requestId = 50, title = "Inside Out 2", kind = TitleKind.Movie, reason = "Radarr refused the key"))
        }
        val (ready, failed) = shell.toasts.current
        assertEquals("“Arrival” is ready to watch.", ready.message)
        assertEquals("Open", ready.actionLabel)
        assertFalse(ready.error)
        assertEquals("“Inside Out 2” couldn't be sent to Radarr: Radarr refused the key", failed.message)
        assertTrue(failed.error)
    }
}
