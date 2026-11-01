package app.pley.android.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RequestsTest {
    private val on = RequestSummary(enabled = true, access = RequestAccess.Ask)

    @Test
    fun discoverShowsWhileRequestsAreOnAndTheAccountMayAsk() {
        assertTrue(on.discover)
        assertTrue(on.mayAsk)
        assertFalse(on.copy(enabled = false).discover)
        assertFalse(on.copy(access = RequestAccess.Off).discover)
        // An app signed in before requests: Discover shows, only to sign in again.
        val before = on.copy(signInAgain = RequestScope.Requests)
        assertTrue(before.discover)
        assertFalse(before.mayAsk)
        assertFalse(before.yourRequests)
    }

    @Test
    fun yourRequestsShowWhileThereAreSomeOrTheAccountMayAsk() {
        assertTrue(on.yourRequests)
        assertFalse(on.copy(access = RequestAccess.Off).yourRequests)
        assertTrue(on.copy(access = RequestAccess.Off, own = 2).yourRequests)
        assertFalse(on.copy(enabled = false, own = 2).yourRequests)
        // Approvals missing: everything else works as for anyone.
        assertTrue(on.copy(signInAgain = RequestScope.Approvals).yourRequests)
        assertTrue(on.copy(access = RequestAccess.Off, manage = true).requestsEntry)
        assertFalse(on.copy(access = RequestAccess.Off).requestsEntry)
    }

    @Test
    fun theBadgeCountsWhatWaitsForThePerson() {
        val someone = on.copy(unseen = 2, pending = 0)
        assertEquals(2, someone.badge)
        assertFalse(someone.badgeAlarm)
        val manager = on.copy(manage = true, pending = 3, failed = 0, unseen = 1)
        assertEquals(4, manager.badge)
        assertFalse(manager.badgeAlarm)
        assertEquals(5, manager.copy(failed = 1).badge)
        assertTrue(manager.copy(failed = 1).badgeAlarm)
    }

    @Test
    fun newNamesFromANewerServerReadAsUnknown() {
        val title = PleyJson.decodeFromString(
            DiscoverTitle.serializer(),
            """{"kind":"episode","tmdb_id":1,"title":"T","status":"wishlisted","later":true}""",
        )
        assertEquals(TitleKind.Unknown, title.kind)
        assertEquals(TitleStatus.Unknown, title.status)
        val request = PleyJson.decodeFromString(
            MediaRequest.serializer(),
            """{"id":1,"kind":"movie","tmdb_id":2,"title":"T","status":"queued","stage":"seeding","actions":["withdraw","boost"],"download":{"progress":0.5,"state":"stalled"}}""",
        )
        assertEquals(RequestStatus.Unknown, request.status)
        assertEquals(RequestStage.Unknown, request.stage)
        assertEquals(listOf(RequestAction.Withdraw, RequestAction.Unknown), request.actions)
        assertEquals(DownloadState.Unknown, request.download?.state)
        val summary = PleyJson.decodeFromString(RequestSummary.serializer(), """{"enabled":true,"access":"limited","sign_in_again":"admin"}""")
        assertEquals(RequestAccess.Unknown, summary.access)
        assertEquals(RequestScope.Unknown, summary.signInAgain)
        assertTrue(summary.mayAsk)
    }

    @Test
    fun bodiesCarryOnlyWhatIsChosen() {
        assertEquals("""{"kind":"movie","tmdb_id":5}""", PleyJson.encodeToString(NewRequest.serializer(), NewRequest(TitleKind.Movie, 5)))
        assertEquals("{}", PleyJson.encodeToString(Approval.serializer(), Approval()))
        assertEquals("{}", PleyJson.encodeToString(Decline.serializer(), Decline()))
        assertEquals("""{"reason":"No"}""", PleyJson.encodeToString(Decline.serializer(), Decline("No")))
    }

    @Test
    fun aTitlePageIsTheTitleListsShow() {
        val detail = DiscoverDetail(TitleKind.Show, 1396, "Breaking Bad", year = 2008, status = TitleStatus.Partial, requestId = 42)
        assertEquals(DiscoverTitle(TitleKind.Show, 1396, "Breaking Bad", year = 2008, status = TitleStatus.Partial, requestId = 42), detail.toTitle())
        assertEquals(TitleKind.Show, TitleKind.of(ItemKind.Show))
        assertNull(TitleKind.of(ItemKind.Season))
        val list = RequestList(counts = mapOf("pending" to 3))
        assertEquals(3, list.count(RequestStatus.Pending))
        assertEquals(0, list.count(RequestStatus.Failed))
    }
}
