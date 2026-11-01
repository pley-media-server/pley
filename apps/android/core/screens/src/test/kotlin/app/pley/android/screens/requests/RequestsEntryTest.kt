package app.pley.android.screens.requests

import app.pley.android.data.RequestsState
import app.pley.android.model.RequestAccess
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestSummary
import app.pley.android.testing.Fixtures
import org.junit.Assert.assertEquals
import org.junit.Test

/** The ways into requests: which show, and the one number they carry. */
class RequestsEntryTest {
    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")
    private val manager: RequestSummary = Fixtures.decode("requests/summary_manager.json")

    private fun entry(summary: RequestSummary?) = RequestsEntry.of(RequestsState.of(summary))

    @Test
    fun anyoneWhoMayAskSeesDiscoverAndTheirRequestsWithWhatIsNew() {
        assertEquals(RequestsEntry(discover = true, requests = true, manage = false, badge = 1), entry(summary))
    }

    @Test
    fun managersCountWhatWaitsForThemInTheDangerColourWhileSomethingFailed() {
        assertEquals(RequestsEntry(discover = true, requests = true, manage = true, badge = 3, failed = 1), entry(manager))
        assertEquals(false, entry(manager.copy(failed = 0)).alarm)
        assertEquals(2, entry(manager.copy(failed = 0)).badge)
    }

    @Test
    fun aSignInFromBeforeRequestsShowsOnlyDiscoverToSayToSignInAgain() {
        val before = summary.copy(signInAgain = RequestScope.Requests, own = 0, unseen = 0)
        assertEquals(RequestsEntry(discover = true), entry(before))
    }

    @Test
    fun withoutAccessOnlyRequestsAlreadyMadeShow() {
        assertEquals(RequestsEntry(requests = true, badge = 1), entry(summary.copy(access = RequestAccess.Off)))
        assertEquals(RequestsEntry.None, entry(summary.copy(access = RequestAccess.Off, own = 0, unseen = 0)))
    }

    @Test
    fun nothingShowsWhileRequestsAreOffOrUnknown() {
        assertEquals(RequestsEntry.None, entry(summary.copy(enabled = false)))
        assertEquals(RequestsEntry.None, entry(null))
        assertEquals(RequestsEntry.None, RequestsEntry.of(RequestsState.Unknown))
    }
}
