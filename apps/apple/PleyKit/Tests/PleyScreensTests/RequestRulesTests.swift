import Foundation
import PleyData
import PleyModel
import PleyNetwork
@testable import PleyScreens
import PleyTesting
import Testing

private func decode<T: Decodable>(_ type: T.Type, _ path: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Fixtures.data(path))
}

/// Requests' rules as every client keeps them, as Android's
/// `RequestRulesTest` and `RequestsEntryTest` pin them.
@Suite struct RequestRulesTests {
    let summary: RequestSummary
    let manager: RequestSummary
    let trending: DiscoverPage
    let movie: DiscoverDetail
    let show: DiscoverDetail
    let list: RequestList
    let attention: RequestList
    var open: RequestsGate { .open(summary) }

    init() throws {
        summary = try decode(RequestSummary.self, "requests/summary.json")
        manager = try decode(RequestSummary.self, "requests/summary_manager.json")
        trending = try decode(DiscoverPage.self, "discover/trending.json")
        movie = try decode(DiscoverDetail.self, "discover/movie.json")
        show = try decode(DiscoverDetail.self, "discover/show.json")
        list = try decode(RequestList.self, "requests/list.json")
        attention = try decode(RequestList.self, "requests/attention.json")
    }

    func title(_ tmdbId: Int) -> DiscoverTitle { trending.results.first { $0.tmdbId == tmdbId }! }
    func request(_ id: Int) -> MediaRequest { list.requests.first { $0.id == id }! }

    // MARK: Ways in

    @Test func anyoneWhoMayAskSeesDiscoverAndTheirRequestsWithWhatIsNew() {
        let entry = RequestsEntry.of(RequestsGate.of(summary))
        #expect(entry == RequestsEntry(discover: true, requests: true, manage: false, badge: 1))
        #expect(entry.badgeText == "1" && entry.requestsTitle == "Your requests" && entry.spoken != nil)
    }

    @Test func managersCountWhatWaitsForThemInTheDangerColourWhileSomethingFailed() {
        let entry = RequestsEntry.of(RequestsGate.of(manager))
        #expect(entry == RequestsEntry(discover: true, requests: true, manage: true, badge: 3, failed: 1))
        #expect(entry.alarm && entry.requestsTitle == "Requests")
        var calm = manager
        calm.failed = 0
        #expect(!RequestsEntry.of(.of(calm)).alarm && RequestsEntry.of(.of(calm)).badge == 2)
    }

    @Test func aSignInFromBeforeRequestsShowsOnlyDiscoverToSayToSignInAgain() {
        var before = summary
        before.signInAgain = "requests"
        before.own = 0
        before.unseen = 0
        #expect(RequestsGate.of(before) == .signInAgain(before))
        #expect(RequestsEntry.of(.of(before)) == RequestsEntry(discover: true))
        #expect(RequestProblem.of(RequestsGate.of(before)) == .signInAgain(.requests))
    }

    @Test func withoutAccessOnlyRequestsAlreadyMadeShow() {
        var off = summary
        off.access = .off
        #expect(RequestsEntry.of(.of(off)) == RequestsEntry(requests: true, badge: 1))
        off.own = 0
        off.unseen = 0
        #expect(RequestsEntry.of(.of(off)) == .none)
        #expect(RequestsGate.of(off) == .noAccess)
        #expect(RequestProblem.of(RequestsGate.of(off)) == .noAccess)
    }

    @Test func nothingShowsWhileRequestsAreOffOrUnknown() {
        var disabled = summary
        disabled.enabled = false
        #expect(RequestsGate.of(disabled) == .hidden && RequestsEntry.of(.of(disabled)) == .none)
        #expect(RequestsGate.of(nil) == .unknown && RequestsEntry.of(.unknown) == .none)
        #expect(!RequestsEntry.none.shows)
        #expect(RequestProblem.of(RequestsGate.unknown) == nil && RequestProblem.of(RequestsGate.of(summary)) == nil)
        #expect(RequestProblem.of(RequestsGate.hidden) == .off)
    }

    // MARK: Titles

    @Test func badgesSayWhatATitleIsForThisAccount() {
        #expect(RequestRules.badge(title(1_184_918)) == nil)
        #expect(RequestRules.badge(title(95396)) == "Partly in library")
        #expect(RequestRules.badge(title(329_865)) == "In library")
        #expect(RequestRules.badge(title(933_260)) == "Requested")
        // Partial without a library title: what's there is only asked for.
        #expect(RequestRules.badge(title(1396)) == "Requested")
        #expect(RequestRules.badge(title(945_961)) == "Declined")
        let card = DiscoverCard(title(933_260))
        #expect(card.key == "movie:933260" && card.subtitle == "2024 · Movie" && card.badge == "Requested")
        #expect(card.spoken == "The Substance, 2024, Movie, Requested")
        #expect(card.screen == .discoverTitle(.movie, tmdbId: 933_260))
    }

    @Test func aTitleALibraryHasWholeOpensItsLibraryPage() {
        // Arrival: in a library this account can open, whole.
        #expect(RequestRules.opens(title(329_865)) == .item(101))
        #expect(DiscoverCard(title(329_865)).screen == .item(101))
        // Severance: partly there, with more seasons to ask for on its Discover page.
        #expect(RequestRules.opens(title(95396)) == .discoverTitle(.show, tmdbId: 95396))
        #expect(RequestRules.opens(title(1_184_918)) == .discoverTitle(.movie, tmdbId: 1_184_918))
        var nowhere = title(329_865)
        nowhere.itemId = nil
        #expect(RequestRules.opens(nowhere) == .discoverTitle(.movie, tmdbId: 329_865))
    }

    @Test func rowsFollowARequestsStatus() {
        let robot = title(1_184_918)
        var asked = request(44)
        asked.tmdbId = robot.tmdbId
        #expect(RequestRules.patched(robot, with: asked).status == .requested)
        #expect(RequestRules.patched(robot, with: asked).requestId == 44)
        var declined = asked
        declined.status = .declined
        declined.stage = .declined
        #expect(RequestRules.patched(robot, with: declined).status == .declined)
        var available = asked
        available.status = .available
        available.stage = .available
        available.itemId = 300
        let ready = RequestRules.patched(robot, with: available)
        #expect(ready.status == .library && ready.itemId == 300 && RequestRules.badge(ready) == "In library")
        // Gone: no badge.
        available.stage = .gone
        #expect(RequestRules.badge(RequestRules.patched(title(933_260), with: available)) == nil)
        // A 404 says nothing of the title: others may still ask for it.
        var substance = title(933_260)
        substance.requestId = 44
        let unread = RequestRules.patched(substance, with: nil)
        #expect(unread.status == .requested && unread.requestId == nil)
    }

    @Test func aChangeTouchesItsTitlesAndRequests() {
        var substance = title(933_260)
        substance.requestId = 44
        #expect(RequestRules.touches(substance, .everything))
        #expect(RequestRules.touches(substance, RequestChange(id: 44, kind: nil, tmdbId: nil)))
        #expect(RequestRules.touches(substance, RequestChange(id: nil, kind: .movie, tmdbId: 933_260)))
        #expect(!RequestRules.touches(substance, RequestChange(id: 45, kind: .show, tmdbId: 933_260)))
    }

    @Test func theMainActionIsTheFirstThatApplies() {
        #expect(RequestRules.mainAction(movie, gate: open) == .request(.yes))
        var before = summary
        before.signInAgain = "requests"
        #expect(RequestRules.mainAction(movie, gate: .signInAgain(before)) == .signInAgain)
        var restricted = movie
        restricted.status = .restricted
        restricted.restriction = "Rated R: above this account's age limit"
        #expect(RequestRules.mainAction(restricted, gate: open) == .restricted("Rated R: above this account's age limit"))
        var library = movie
        library.status = .library
        library.itemId = 101
        #expect(RequestRules.mainAction(library, gate: open) == .open(itemId: 101))
        var declined = movie
        declined.status = .declined
        declined.declined = DeclinedNote(reason: "Not for us", at: 1_761_000_000, againAt: 1_762_000_000)
        #expect(RequestRules.mainAction(declined, gate: open) == .declined(reason: "Not for us", againAt: 1_762_000_000))
        let pending = request(44)
        var requested = movie
        requested.status = .requested
        requested.requestId = 44
        requested.requests = [pending]
        #expect(RequestRules.mainAction(requested, gate: open) == .mine(pending))
        var followed = pending
        followed.mine = false
        followed.watching = true
        var following = requested
        following.watching = true
        following.requests = [followed]
        #expect(RequestRules.mainAction(following, gate: open) == .following(followed))
        var others = requested
        others.requestId = nil
        others.requests = []
        #expect(RequestRules.mainAction(others, gate: open) == .tellMeToo(.yes))
        // Breaking Bad: seasons 3 and 5 can still be asked for.
        #expect(RequestRules.mainAction(show, gate: open) == .partial(itemId: nil, more: .yes))
    }

    @Test func withoutAccessOrAllowanceRequestWaits() {
        var offSummary = summary
        offSummary.access = .off
        offSummary.own = 3
        #expect(RequestRules.mainAction(movie, gate: .of(offSummary)) == .request(.noAccess))
        // Following costs nothing: Tell me too works at the limit.
        var requested = movie
        requested.status = .requested
        requested.moviesLeft = 0
        requested.moviesNextAt = 1_761_700_000
        #expect(RequestRules.mainAction(requested, gate: open) == .tellMeToo(.yes))
        var spent = movie
        spent.moviesLeft = 0
        #expect(RequestRules.mainAction(spent, gate: open) == .request(.limit))
        var noSeasons = show
        noSeasons.seasonsLeft = 0
        #expect(RequestRules.mainAction(noSeasons, gate: open) == .partial(itemId: nil, more: .limit))
        // No more seasons to ask for: only Open.
        var whole = show
        whole.seasons = show.seasons?.map { season in
            var season = season
            if (3...5).contains(season.number) { season.state = .library }
            return season
        }
        whole.itemId = 7
        #expect(RequestRules.mainAction(whole, gate: open) == .partial(itemId: 7, more: nil))
        // The hero's, from what a row knows.
        #expect(RequestRules.titleAction(title(329_865), gate: open, allowance: nil) == .open(itemId: 101))
        #expect(RequestRules.titleAction(title(933_260), gate: open, allowance: nil) == .mine(nil))
        #expect(RequestRules.titleAction(title(1_184_918), gate: open,
                                         allowance: Allowance(moviesLeft: 0, seasonsLeft: 2)) == .request(.limit))
    }

    // MARK: The picker

    @Test func thePickerTicksWhatCanBeAskedForLowestFirst() {
        let picker = SeasonPicker.of(show, left: nil)
        // Specials last.
        #expect(picker.rows.map(\.season.number) == [1, 2, 3, 4, 5, 0])
        // 1 and 2 are requested, 4 declined: 3 and 5 are ticked; the specials are offered unticked.
        #expect(picker.picked == [3, 5])
        #expect(picker.rows.map(\.askable) == [false, false, true, false, true, true])
        #expect(picker.ready && picker.button == "Request 2 seasons")
        // What can't be asked for can't be ticked.
        var same = picker
        same.toggle(1)
        #expect(same == picker)
        var specials = picker
        specials.toggle(0)
        #expect(specials.picked == [0, 3, 5] && specials.asked == [0, 3, 5])
    }

    @Test func thePickerKeepsToTheAllowance() {
        let one = SeasonPicker.of(show, left: 1)
        #expect(one.picked == [3])
        var two = one
        two.toggle(5)
        #expect(two.overLimit && !two.ready)
        #expect(!SeasonPicker.of(show, left: 0).ready)
        var none = one
        none.toggle(3)
        #expect(!none.ready && none.button == "Pick a season.")
        // Unaired seasons are offered, unticked.
        var coming = show
        coming.seasons = (show.seasons ?? []) + [DiscoverSeason(number: 6, name: "Season 6", episodes: 8, state: .unaired)]
        let withSix = SeasonPicker.of(coming, left: nil)
        #expect(withSix.rows.first { $0.season.number == 6 }?.askable == true)
        #expect(!withSix.picked.contains(6))
        // Read again: the picks still askable are kept.
        var picked = withSix
        picked.toggle(6)
        picked.toggle(3)
        var later = coming
        later.seasons = coming.seasons?.map { season in
            var season = season
            if season.number == 5 { season.state = .requested }
            return season
        }
        #expect(picked.rebuilt(from: later, left: nil).picked == [6])
    }

    @Test func moreSeasonsShowOnlyForMissingOrPartSeasonsButTheSpecials() {
        #expect(RequestRules.moreSeasons(show))
        var onlySpecials = show
        onlySpecials.seasons = show.seasons?.map { season in
            var season = season
            if (3...5).contains(season.number) { season.state = .requested }
            return season
        }
        #expect(!RequestRules.moreSeasons(onlySpecials))
        var restricted = show
        restricted.status = .restricted
        #expect(!RequestRules.moreSeasons(restricted))
    }

    @Test func tellMeTooAsksForTheSeasonsOthersAskedFor() {
        #expect(RequestRules.tellMeToo(show) == NewRequest(kind: .show, tmdbId: 1396, seasons: [1, 2]))
        #expect(RequestRules.tellMeToo(movie) == NewRequest(kind: .movie, tmdbId: movie.tmdbId))
        #expect(RequestRules.askAgain(show.requests[0]) == NewRequest(kind: .show, tmdbId: 1396, seasons: [1, 2]))
        #expect(RequestRules.activeRequest(show)?.id == 42)
        #expect(RequestRules.requestOf(show.seasons![1], in: show)?.id == 42)
        #expect(RequestRules.requestOf(show.seasons![3], in: show) == nil)
    }

    // MARK: Requests

    @Test func pillsSayWhereARequestIs() {
        let downloading = show.requests[0]
        #expect(StagePill.of(downloading, manage: false)
            == .downloading(progress: 0.42, eta: 1_761_601_200, state: .downloading))
        #expect(StagePill.of(downloading, manage: false).tone == .blue)
        #expect(StagePill.of(request(44), manage: false) == .pending && StagePill.pending.tone == .amber)
        let failed = request(48)
        #expect(StagePill.of(failed, manage: false) == .notSentYet(retryAt: failed.retryAt))
        #expect(StagePill.of(failed, manage: true) == .couldNotSend(reason: failed.reason, retryAt: failed.retryAt))
        #expect(StagePill.of(failed, manage: true).tone == .red)
        var waiting = downloading
        waiting.stage = .waiting
        waiting.stageAt = 1_762_000_000
        #expect(StagePill.of(waiting, manage: false) == .waiting(show: true, at: 1_762_000_000))
        var partial = downloading
        partial.stage = .partial
        partial.availableSeasons = [1]
        #expect(StagePill.of(partial, manage: false) == .partial(seasons: [1]))
        // The bar shows while there's a download, a partial request's too.
        #expect(partial.downloadProgress == 0.42)
        var gone = downloading
        gone.stage = .gone
        #expect(StagePill.of(gone, manage: false).tone == .gray)
        #expect(downloading.moving && !request(44).moving)
    }

    @Test func downloadsSayTheirState() {
        var queued = show.requests[0]
        queued.download = RequestDownload(progress: 0, state: .queued, eta: nil, episodes: 1)
        #expect(StagePill.of(queued, manage: false) == .downloading(progress: 0, eta: nil, state: .queued))
        var unknown = queued
        unknown.stage = RequestStage(rawValue: "teleporting")
        unknown.download = nil
        #expect(StagePill.of(unknown, manage: false) == .unknown)
    }

    @Test func cardsOfferExactlyTheServersActions() {
        var pending = request(44)
        pending.actions = [.approve, .decline, .handled, .delete, RequestAction(rawValue: "snooze")]
        let manage = CardAction.of(pending)
        #expect(manage.buttons == [.approve, .decline] && manage.menu == [.approveWith, .handled, .delete])
        #expect(CardAction.of(request(44)) == ([.withdraw], []))
        // A declined one is removed, not withdrawn.
        var declined = request(46)
        declined.actions = [.withdraw, .askAgain]
        #expect(CardAction.of(declined).buttons == [.remove, .askAgain])
        #expect(CardAction.of(request(47)).buttons == [.stopFollowing])
        var retry = request(48)
        retry.actions = [.retry]
        #expect(CardAction.of(retry).buttons == [.tryAgain])
        // Ready: Watch first.
        #expect(CardAction.of(request(45)).buttons == [.watch])
        #expect(CardAction.withdraw.confirms && CardAction.remove.confirms && CardAction.delete.confirms)
        #expect(!CardAction.approve.confirms)
        #expect(CardAction.withdraw.confirmation("Arrival") != nil && CardAction.approve.confirmation("Arrival") == nil)
        #expect(CardAction.allCases.allSatisfy { !$0.title.isEmpty && !$0.symbol.isEmpty })
    }

    @Test func appleTVsPanelOffersTheServersActionsInItsOrder() {
        var pending = request(44)
        pending.actions = [.approve, .decline, .handled, .delete, RequestAction(rawValue: "snooze"), .decline]
        // Approve plain (the settings' choices), Decline with Decline with a reason… after it; once each.
        #expect(CardAction.panel(pending) == [.approve, .decline, .declineWithReason, .handled, .delete])
        pending.actions = [.delete, .decline, .approve]
        #expect(CardAction.panel(pending) == [.delete, .decline, .declineWithReason, .approve])
        #expect(CardAction.panel(request(44)) == [.withdraw])
        var declined = request(46)
        declined.actions = [.withdraw, .askAgain]
        #expect(CardAction.panel(declined) == [.remove, .askAgain])
        #expect(CardAction.panel(request(47)) == [.stopFollowing])
        var retry = request(48)
        retry.actions = [.retry]
        #expect(CardAction.panel(retry) == [.tryAgain])
        // Open and Details are the panel's own: no Watch, no Approve….
        let all = list.requests.flatMap(CardAction.panel) + attention.requests.flatMap(CardAction.panel)
        #expect(!all.contains(.watch) && !all.contains(.approveWith))
        #expect(CardAction.declineWithReason.title == "Decline with a reason…" && !CardAction.declineWithReason.confirms)
    }

    @Test func cardsOpenTheLibraryOnceThere() {
        let ready = request(45)
        #expect(RequestRules.opens(ready, mayAsk: true) == .item(101))
        #expect(RequestRules.opens(request(44), mayAsk: true) == .discoverTitle(.movie, tmdbId: request(44).tmdbId))
        #expect(RequestRules.opens(request(44), mayAsk: false) == nil)
        var elsewhere = ready
        elsewhere.itemId = nil
        #expect(elsewhere.readyElsewhere && !ready.readyElsewhere)
        let card = RequestCard.of(ready, manage: false, mayAsk: true, now: 1_761_825_600)
        #expect(card.watch == .item(101) && card.opens == .item(101) && card.elsewhere == nil)
        #expect(card.pill.text == "Ready to watch" && card.spoken == "\(ready.title), Ready to watch")
        let away = RequestCard.of(elsewhere, manage: false, mayAsk: true, now: 1_761_825_600)
        #expect(away.watch == nil && away.elsewhere != nil)
    }

    @Test func cardsSayWhoAskedOnlyToManagers() {
        let now = 1_761_825_600
        let arcane = attention.requests.first { $0.id == 49 }!
        let card = RequestCard.of(arcane, manage: true, mayAsk: true, now: now)
        #expect(card.asked.hasPrefix("Kit · "))
        #expect(card.line.contains("Seasons 1–3, 5"))
        let declined = RequestCard.of(request(46), manage: false, mayAsk: true, now: now)
        #expect(declined.reason == request(46).reason.map { "\u{201C}\($0)\u{201D}" })
        #expect(declined.note == nil && declined.sentTo == nil)
    }

    @Test func aLibraryTitleShowsItsRequest() throws {
        let answer = try decode(ItemRequest.self, "requests/item_request.json")
        let block = try #require(ItemRequestBlock.of(answer, mayAsk: true, manage: false, now: 1_761_825_600))
        #expect(block.picker == .discoverTitle(answer.kind, tmdbId: answer.tmdbId, pick: true))
        #expect(block.more == !(answer.askable ?? []).isEmpty)
        #expect(block.requestId == answer.request?.id)
        var bare = answer
        bare.request = nil
        bare.askable = []
        #expect(ItemRequestBlock.of(bare, mayAsk: true, manage: false, now: 0) == nil)
        bare.askable = [3]
        #expect(ItemRequestBlock.of(bare, mayAsk: false, manage: false, now: 0) == nil)
        #expect(ItemRequestBlock.of(bare, mayAsk: true, manage: false, now: 0)?.more == true)
    }

    // MARK: Refusals

    @Test func refusalsSayWhatTheyMean() {
        #expect(RequestProblem.of(PleyError.notAvailable(code: "unavailable", message: "off", setup: false)) == .off)
        #expect(RequestProblem.of(PleyError.notAvailable(code: "tmdb_key", message: "refused", setup: true)) == .tmdbKey)
        #expect(RequestProblem.of(PleyError.notAvailable(code: "tmdb_down", message: "away", setup: true)) == .tmdbDown)
        #expect(RequestProblem.tmdbDown.retry && !RequestProblem.tmdbKey.retry)
        #expect(RequestProblem.of(PleyError.api(status: 403, code: "no_access", message: "no")) == .noAccess)
        #expect(RequestProblem.of(PleyError.insufficientScope(scope: "approvals")) == .signInAgain(.approvals))
        #expect(RequestProblem.of(PleyError.insufficientScope(scope: "requests")) == .signInAgain(.requests))
        #expect(RequestProblem.of(PleyError.api(status: 403, code: "restricted", message: "Rated R")) == .restricted("Rated R"))
        #expect(RequestProblem.of(PleyError.refused(status: 409, code: "declined", message: "d", retryAt: 1_762_000_000,
                                                    request: nil)) == .declined(retryAt: 1_762_000_000))
        #expect(RequestProblem.of(PleyError.api(status: 429, code: "too_many_requests", message: "limit"))
            == .limit(retryAt: nil))
        let current = request(44)
        #expect(RequestProblem.of(PleyError.refused(status: 409, code: "conflict", message: "Already approved",
                                                    retryAt: nil, request: current))
            == .conflict("Already approved", current))
        #expect(RequestProblem.of(PleyError.api(status: 404, code: "not_found", message: "gone")) == .notFound)
        // Only an unreachable server is offline: never a 503 with Pley's JSON.
        guard case .offline = RequestProblem.of(PleyError.unreachable("refused")) else {
            Issue.record("not offline")
            return
        }
        guard case .other = RequestProblem.of(PleyError.api(status: 503, code: "busy", message: "busy")) else {
            Issue.record("a 503 is offline")
            return
        }
    }

    @Test func problemsSayWhatToDo() {
        let now = 1_761_825_600
        let signIn = RequestProblem.signInAgain(.requests).words(manage: false, summary: summary)
        #expect(signIn.signInAgain && signIn.body == nil && !signIn.retry)
        #expect(RequestProblem.signInAgain(.approvals).words(manage: true, summary: manager).signInAgain)
        #expect(RequestProblem.tmdbKey.words(manage: true, summary: manager).body
            != RequestProblem.tmdbKey.words(manage: false, summary: summary).body)
        #expect(RequestProblem.declined(retryAt: now + 12 * 86_400).words(manage: false, summary: summary, now: now).title
            == RequestWords.askAgainAt(now + 12 * 86_400, now: now))
        #expect(RequestProblem.limit(retryAt: nil).words(manage: false, summary: summary).title
            == "You can ask for at most 10 seasons per 7 days: pick fewer.")
        let offline = RequestProblem.offline("No network.").words(manage: false, summary: nil, failure: "Couldn't load Discover")
        #expect(offline.title == "Couldn't load Discover" && offline.body == "No network." && offline.retry)
        #expect(RequestProblem.offline("No network.").words(manage: false, summary: nil).title == "No network.")
        #expect(RequestProblem.conflict("Already approved", nil).toast(manage: false, summary: nil) == "Already approved")
        #expect(RequestProblem.off.toast(manage: false, summary: nil)
            == "\(RequestWords.offTitle) \(RequestWords.offBody)")
    }

    // MARK: Approving, declining

    @Test func theApproveDialogSendsOnlyWhatChanged() throws {
        let options = try decode(RequestOptions.self, "requests/options.json")
        let arcane = attention.requests.first { $0.id == 49 }!
        let sonarr = options.of(arcane.kind)
        // Anime: anime's profile and folder to start with.
        #expect(Approving.profile(sonarr, for: arcane) == 1)
        #expect(Approving.folder(sonarr, for: arcane) == "/anime")
        #expect(Approving.seasons(arcane) == [1, 2, 3, 5])
        #expect(Approving.approval(options: sonarr, request: arcane, profile: 1, folder: "/anime", seasons: [1, 2, 3, 5])
            == Approval())
        #expect(Approving.approval(options: sonarr, request: arcane, profile: 6, folder: "/anime", seasons: [2, 1])
            == Approval(qualityProfile: 6, seasons: [1, 2]))
        #expect(Approving.profile(nil, for: arcane) == nil)
    }

    @Test func aDeclineReasonIsCutToTheServersLimitWithoutSplittingAPair() {
        let max = RequestRules.declineReasonMax
        #expect(RequestRules.cutReason("abc") == "abc")
        #expect(RequestRules.cutReason(String(repeating: "a", count: max)).utf16.count == max)
        #expect(RequestRules.cutReason(String(repeating: "a", count: max + 5)).utf16.count == max)
        // An emoji (two UTF-16 units) across the limit goes whole rather than leaving its first half.
        let across = String(repeating: "a", count: max - 1) + "😀" + "b"
        #expect(RequestRules.cutReason(across).utf16.count == max - 1)
        // One that ends at the limit stays.
        let within = String(repeating: "a", count: max - 2) + "😀" + "b"
        #expect(RequestRules.cutReason(within).utf16.count == max)
        #expect(RequestRules.declineReason("   ") == nil && RequestRules.declineReason(" Not for us ") == "Not for us")
    }

    // MARK: Lists

    @Test func aFilterAsksForWhatItKeeps() {
        let attentionQuery = RequestFilter(view: .attention).query(offset: 0, limit: 30)
        #expect(attentionQuery.attention && attentionQuery.oldest && attentionQuery.statuses.isEmpty)
        let filter = RequestFilter(view: .pending, kind: .movie, q: "  sub ", mine: true)
        let query = filter.query(offset: 30, limit: 30)
        #expect(query.statuses == [.pending] && query.kind == .movie && query.q == "sub" && query.mine && query.offset == 30)
        #expect(filter.narrowed && filter.filtered && !RequestFilter().filtered)
        #expect(filter.matches(request(44)))
        #expect(!filter.matches(request(48)))
        var approved = request(44)
        approved.status = .approved
        #expect(!filter.matches(approved))
        #expect(RequestFilter(view: .attention).matches(attention.requests.first { $0.id == 50 }!) == true)
        #expect(RequestView.statuses.map(\.key) == ["pending", "approved", "available", "declined", "failed"])
    }

    @Test func pagesOfTitlesAreReadOnceEach() {
        var pages = TitlePages()
        #expect(pages.more && !pages.settled && pages.short(20))
        pages = pages.with(trending)
        #expect(pages.titles.count == 6 && pages.page == 1 && pages.totalPages == 3)
        var second = trending
        second.page = 2
        pages = pages.with(second)
        // Page 2 brought the same titles: none twice.
        #expect(pages.titles.count == 6 && pages.page == 2 && pages.more)
        var last = trending
        last.page = 3
        last.results = []
        pages = pages.with(last)
        #expect(!pages.more && pages.settled && !pages.short(20))
        // The first page again starts over.
        #expect(pages.with(trending).page == 1)
    }
}
