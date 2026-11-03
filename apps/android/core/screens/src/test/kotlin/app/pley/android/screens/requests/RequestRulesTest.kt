package app.pley.android.screens.requests

import app.pley.android.data.RequestsState
import app.pley.android.model.ApiErrorBody
import app.pley.android.model.DeclinedTitle
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverPage
import app.pley.android.model.DiscoverSeason
import app.pley.android.model.DownloadState
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestAccess
import app.pley.android.model.RequestAction
import app.pley.android.model.RequestDownload
import app.pley.android.model.RequestList
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestStage
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.SeasonState
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.screens.PleyKey
import app.pley.android.testing.Fixtures
import java.net.ConnectException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Requests' rules as every client keeps them: badges, the main action, the picker, pills and actions. */
class RequestRulesTest {
    private val summary: RequestSummary = Fixtures.decode("requests/summary.json")
    private val open = RequestsState.Open(summary)
    private val trending: DiscoverPage = Fixtures.decode("discover/trending.json")
    private val movie: DiscoverDetail = Fixtures.decode("discover/movie.json")
    private val show: DiscoverDetail = Fixtures.decode("discover/show.json")
    private val list: RequestList = Fixtures.decode("requests/list.json")

    private fun title(tmdbId: Long) = trending.results.first { it.tmdbId == tmdbId }

    private fun request(id: Long) = list.requests.first { it.id == id }

    @Test
    fun badgesSayWhatATitleIsForThisAccount() {
        assertNull(badgeOf(title(1184918)))
        assertEquals(TitleBadge.PartlyInLibrary, badgeOf(title(95396)))
        assertEquals(TitleBadge.InLibrary, badgeOf(title(329865)))
        assertEquals(TitleBadge.Requested, badgeOf(title(933260)))
        // Partial without a library title: what's there is only asked for.
        assertEquals(TitleBadge.Requested, badgeOf(title(1396)))
        assertEquals(TitleBadge.Declined, badgeOf(title(945961)))
        assertNull(badgeOf(TitleStatus.Restricted, null))
    }

    @Test
    fun rowsFollowARequestsStatus() {
        val robot = title(1184918)
        val asked = request(44).copy(tmdbId = robot.tmdbId)
        assertEquals(TitleStatus.Requested, patched(robot, asked).status)
        assertEquals(44L, patched(robot, asked).requestId)
        assertEquals(TitleStatus.Declined, patched(robot, asked.copy(status = RequestStatus.Declined, stage = RequestStage.Declined)).status)
        val ready = patched(robot, asked.copy(status = RequestStatus.Available, stage = RequestStage.Available, itemId = 300))
        assertEquals(TitleStatus.Library, ready.status)
        assertEquals(300L, ready.itemId)
        assertEquals(TitleBadge.InLibrary, badgeOf(ready))
        // Gone: no badge.
        assertNull(badgeOf(patched(title(933260), asked.copy(status = RequestStatus.Available, stage = RequestStage.Gone))))
        // A 404 says nothing of the title: others may still ask for it.
        val unread = patched(title(933260).copy(requestId = 44), null)
        assertEquals(TitleStatus.Requested, unread.status)
        assertNull(unread.requestId)
    }

    @Test
    fun theMainActionIsTheFirstThatApplies() {
        assertEquals(MainAction.Request(Ask.Yes), mainAction(movie, open))
        assertEquals(MainAction.SignInAgain, mainAction(movie, RequestsState.SignInAgain(summary.copy(signInAgain = RequestScope.Requests))))
        assertEquals(
            MainAction.Restricted("Rated R: above this account's age limit"),
            mainAction(movie.copy(status = TitleStatus.Restricted, restriction = "Rated R: above this account's age limit"), open),
        )
        assertEquals(MainAction.Open(101), mainAction(movie.copy(status = TitleStatus.Library, itemId = 101), open))
        assertEquals(
            MainAction.Declined("Not for us", 1762000000),
            mainAction(movie.copy(status = TitleStatus.Declined, declined = DeclinedTitle("Not for us", 1761000000, 1762000000)), open),
        )
        val pending = request(44)
        val requested = movie.copy(status = TitleStatus.Requested, requestId = 44, requests = listOf(pending))
        assertEquals(MainAction.Mine(pending), mainAction(requested, open))
        val followed = pending.copy(mine = false, watching = true)
        assertEquals(MainAction.Following(followed), mainAction(requested.copy(watching = true, requests = listOf(followed)), open))
        assertEquals(MainAction.TellMeToo(Ask.Yes), mainAction(requested.copy(requestId = null, requests = emptyList()), open))
        // Breaking Bad: seasons 3 and 5 can still be asked for.
        assertEquals(MainAction.Partial(null, Ask.Yes), mainAction(show, open))
    }

    @Test
    fun withoutAccessOrAllowanceRequestWaits() {
        val off = RequestsState.Open(summary.copy(access = RequestAccess.Off, own = 3))
        assertEquals(MainAction.Request(Ask.NoAccess), mainAction(movie, off))
        // Following costs nothing: Tell me too works at the limit.
        val requested = movie.copy(status = TitleStatus.Requested, moviesLeft = 0, moviesNextAt = 1761700000)
        assertEquals(MainAction.TellMeToo(Ask.Yes), mainAction(requested, open))
        assertEquals(MainAction.Request(Ask.Limit), mainAction(movie.copy(moviesLeft = 0), open))
        assertEquals(MainAction.Partial(null, Ask.Limit), mainAction(show.copy(seasonsLeft = 0), open))
        // No more seasons to ask for: only Open.
        val whole = show.copy(seasons = show.seasons!!.map { if (it.number in 3..5) it.copy(state = SeasonState.Library) else it }, itemId = 7)
        assertEquals(MainAction.Partial(7, null), mainAction(whole, open))
    }

    @Test
    fun thePickerTicksWhatCanBeAskedForLowestFirst() {
        val picker = SeasonPicker.of(show, left = null)
        // Specials last.
        assertEquals(listOf(1, 2, 3, 4, 5, 0), picker.seasons.map { it.season.number })
        // 1 and 2 are requested, 4 declined: 3 and 5 are ticked; the specials are offered unticked.
        assertEquals(setOf(3, 5), picker.picked)
        assertEquals(listOf(false, false, true, false, true, true), picker.seasons.map { it.askable })
        assertTrue(picker.ready)
        // What can't be asked for can't be ticked.
        assertEquals(picker, picker.toggle(1))
        assertEquals(setOf(3, 5, 0), picker.toggle(0).picked)
        assertEquals(listOf(0, 3, 5), picker.toggle(0).asked())
    }

    @Test
    fun thePickerKeepsToTheAllowance() {
        val one = SeasonPicker.of(show, left = 1)
        assertEquals(setOf(3), one.picked)
        val two = one.toggle(5)
        assertTrue(two.overLimit)
        assertFalse(two.ready)
        assertFalse(SeasonPicker.of(show, left = 0).ready)
        assertFalse(one.toggle(3).ready)
        // Unaired seasons are offered, unticked.
        val coming = show.copy(seasons = show.seasons!! + DiscoverSeason(6, "Season 6", episodes = 8, state = SeasonState.Unaired))
        val withSix = SeasonPicker.of(coming, left = null)
        assertTrue(withSix.seasons.first { it.season.number == 6 }.askable)
        assertFalse(6 in withSix.picked)
    }

    @Test
    fun moreSeasonsShowOnlyForMissingOrPartSeasonsButTheSpecials() {
        assertTrue(moreSeasons(show))
        val onlySpecials = show.copy(seasons = show.seasons!!.map { if (it.number in 3..5) it.copy(state = SeasonState.Requested) else it })
        assertFalse(moreSeasons(onlySpecials))
        assertFalse(moreSeasons(show.copy(status = TitleStatus.Restricted)))
    }

    @Test
    fun seasonNumbersReadAsRanges() {
        assertEquals("1–3, 5", seasonRanges(listOf(5, 1, 3, 2)))
        assertEquals("2", seasonRanges(listOf(2)))
        assertEquals("1–2, 4–6, 9", seasonRanges(listOf(1, 2, 4, 5, 6, 9, 9)))
        assertEquals("", seasonRanges(listOf(0)))
    }

    @Test
    fun tellMeTooAsksForTheSeasonsOthersAskedFor() {
        assertEquals(NewRequest(TitleKind.Show, 1396, listOf(1, 2)), tellMeToo(show))
        assertEquals(NewRequest(TitleKind.Movie, 1184918), tellMeToo(movie))
        assertEquals(NewRequest(TitleKind.Show, 1396, listOf(1, 2)), askAgain(show.requests.single()))
    }

    @Test
    fun pillsSayWhereARequestIs() {
        val downloading = show.requests.single()
        assertEquals(StagePill.Downloading(0.42f, 1761601200, DownloadState.Downloading), stagePill(downloading, manage = false))
        assertEquals(PillTone.Blue, stagePill(downloading, manage = false).tone)
        assertEquals(StagePill.Pending, stagePill(request(44), manage = false))
        assertEquals(PillTone.Amber, StagePill.Pending.tone)
        val failed = request(48)
        assertEquals(StagePill.NotSentYet(failed.retryAt), stagePill(failed, manage = false))
        assertEquals(StagePill.CouldNotSend(failed.reason, failed.retryAt), stagePill(failed, manage = true))
        assertEquals(PillTone.Red, stagePill(failed, manage = true).tone)
        assertEquals(StagePill.Waiting(show = true, at = 1762000000), stagePill(downloading.copy(stage = RequestStage.Waiting, stageAt = 1762000000), false))
        val partial = downloading.copy(stage = RequestStage.Partial, availableSeasons = listOf(1))
        assertEquals(StagePill.Partial(listOf(1)), stagePill(partial, false))
        // The bar shows while there's a download, a partial request's too.
        assertEquals(0.42f, downloadProgress(partial))
        assertEquals(PillTone.Gray, stagePill(downloading.copy(stage = RequestStage.Gone), false).tone)
        assertTrue(downloading.moving)
        assertFalse(request(44).moving)
    }

    @Test
    fun cardsOfferExactlyTheServersActions() {
        val pending = request(44).copy(actions = listOf(RequestAction.Approve, RequestAction.Decline, RequestAction.Handled, RequestAction.Delete, RequestAction.Unknown))
        assertEquals(
            listOf(CardAction.Approve, CardAction.ApproveWith, CardAction.Decline, CardAction.Handled, CardAction.Delete),
            cardActions(pending, dialogs = true),
        )
        // TVs: plain Approve, and Decline with a reason… as a second step.
        assertEquals(
            listOf(CardAction.Approve, CardAction.Decline, CardAction.DeclineWithReason, CardAction.Handled, CardAction.Delete),
            cardActions(pending, dialogs = false),
        )
        assertEquals(listOf(CardAction.Withdraw), cardActions(request(44), dialogs = true))
        // A declined one is removed, not withdrawn.
        val declined = request(46).copy(actions = listOf(RequestAction.Withdraw, RequestAction.AskAgain))
        assertEquals(listOf(CardAction.Remove, CardAction.AskAgain), cardActions(declined, dialogs = true))
        assertEquals(listOf(CardAction.StopFollowing), cardActions(request(47), dialogs = true))
        assertEquals(listOf(CardAction.TryAgain), cardActions(request(48).copy(actions = listOf(RequestAction.Retry)), dialogs = true))
        assertTrue(CardAction.Withdraw.confirms && CardAction.Remove.confirms && CardAction.Delete.confirms)
        assertFalse(CardAction.Approve.confirms)
    }

    @Test
    fun cardsOpenTheLibraryOnceThere() {
        val ready = request(45)
        assertEquals(PleyKey.Item(101), opens(ready, mayAsk = true))
        assertEquals(PleyKey.DiscoverTitle(request(44).kind, request(44).tmdbId), opens(request(44), mayAsk = true))
        assertNull(opens(request(44), mayAsk = false))
        assertTrue(readyElsewhere(ready.copy(itemId = null)))
        assertFalse(readyElsewhere(ready))
    }

    @Test
    fun refusalsSayWhatTheyMean() {
        assertEquals(RequestProblem.Off, RequestProblem.of(ApiException(503, "unavailable", "off", details = ApiErrorBody("unavailable", "off", setup = false))))
        assertEquals(RequestProblem.TmdbKey, RequestProblem.of(ApiException(503, "tmdb_key", "refused")))
        assertEquals(RequestProblem.TmdbDown, RequestProblem.of(ApiException(503, "tmdb_down", "away")))
        assertTrue(RequestProblem.TmdbDown.retry)
        assertFalse(RequestProblem.TmdbKey.retry)
        assertEquals(RequestProblem.NoAccess, RequestProblem.of(ApiException(403, "no_access", "no")))
        assertEquals(
            RequestProblem.SignInAgain(RequestScope.Approvals),
            RequestProblem.of(ApiException(403, "insufficient_scope", "scope", bearerError = "insufficient_scope", scope = "approvals")),
        )
        assertEquals(RequestProblem.Restricted("Rated R"), RequestProblem.of(ApiException(403, "restricted", "Rated R")))
        assertEquals(RequestProblem.Declined(1762000000), RequestProblem.of(ApiException(409, "declined", "d", details = ApiErrorBody("declined", "d", retryAt = 1762000000))))
        assertEquals(RequestProblem.Limit(null), RequestProblem.of(ApiException(429, "too_many_requests", "limit")))
        val current = request(44)
        assertEquals(
            RequestProblem.Conflict("Already approved", current),
            RequestProblem.of(ApiException(409, "conflict", "Already approved", details = ApiErrorBody("conflict", "Already approved", request = current))),
        )
        assertEquals(RequestProblem.NotFound, RequestProblem.of(ApiException(404, "not_found", "gone")))
        // Only an unreachable server is offline: never a 503 with pley's JSON.
        assertTrue(RequestProblem.of(OfflineException(ConnectException("refused"))) is RequestProblem.Offline)
    }

    @Test
    fun theAllowanceSaysWhatIsLeft() {
        val allowance = Allowance.of(summary)
        assertTrue(allowance.limited)
        assertTrue(allowance.allows(TitleKind.Movie))
        assertFalse(allowance.copy(moviesLeft = 0).allows(TitleKind.Movie))
        assertTrue(allowance.copy(moviesLeft = 0).allows(TitleKind.Show))
        assertFalse(Allowance.of(summary.copy(moviesLeft = null, seasonsLeft = null)).limited)
    }

    @Test
    fun theApproveDialogSendsOnlyWhatChanged() {
        val options: app.pley.android.model.RequestOptions = Fixtures.decode("requests/options.json")
        val arcane = Fixtures.decode<RequestList>("requests/attention.json").requests.first { it.id == 49L }
        val dialog = ApproveDialog(arcane, options.of(arcane.kind), loading = false)
        // Anime: anime's profile and folder to start with.
        assertEquals(1L, dialog.profile)
        assertEquals("/anime", dialog.folder)
        assertEquals(listOf(1, 2, 3, 5), dialog.seasons)
        assertEquals(app.pley.android.model.Approval(), dialog.approval(1, "/anime", setOf(1, 2, 3, 5)))
        assertEquals(app.pley.android.model.Approval(qualityProfile = 6, seasons = listOf(1, 2)), dialog.approval(6, "/anime", setOf(2, 1)))
        assertFalse(ApproveDialog(arcane, null, loading = false).choices)
        assertTrue(dialog.choices)
    }

    @Test
    fun aDeclineReasonIsCutToTheServersLimitWithoutSplittingAPair() {
        assertEquals(3, declineReasonEnd("abc"))
        assertEquals(DECLINE_REASON_MAX, declineReasonEnd("a".repeat(DECLINE_REASON_MAX)))
        assertEquals(DECLINE_REASON_MAX, declineReasonEnd("a".repeat(DECLINE_REASON_MAX + 5)))
        // An emoji (two UTF-16 units) across the limit goes whole rather than leaving its first half.
        val across = "a".repeat(DECLINE_REASON_MAX - 1) + "😀" + "b"
        assertEquals(DECLINE_REASON_MAX - 1, declineReasonEnd(across))
        // One that ends at the limit stays.
        val within = "a".repeat(DECLINE_REASON_MAX - 2) + "😀" + "b"
        assertEquals(DECLINE_REASON_MAX, declineReasonEnd(within))
    }

    @Test
    fun downloadsSayTheirState() {
        val queued = show.requests.single().copy(download = RequestDownload(0.0, DownloadState.Queued, null, 1))
        assertEquals(StagePill.Downloading(0f, null, DownloadState.Queued), stagePill(queued, false))
        val unknown: MediaRequest = queued.copy(stage = RequestStage.Unknown, download = null)
        assertEquals(StagePill.Unknown, stagePill(unknown, false))
    }
}
