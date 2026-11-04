import Foundation
import PleyModel
import PleyTesting
import Testing

private func decode<T: Decodable>(_ type: T.Type, fixture path: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Fixtures.data(path))
}

private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Data(json.utf8))
}

@Suite("Requests' answers decode as the Android app reads them")
struct RequestsDecodingTests {
    @Test func theSummarySaysWhatShows() throws {
        let summary = try decode(RequestSummary.self, fixture: "requests/summary.json")
        #expect(summary.enabled && summary.access == .ask && !summary.manage)
        #expect(summary.moviesLeft == 2 && summary.seasonsLeft == 5 && summary.moviesLimit == 3 && summary.seasonsLimit == 10)
        #expect(summary.unseen == 1 && summary.own == 4 && summary.changedAt == 1_761_600_000)
        #expect(summary.signInAgain == nil)
        #expect(summary.discover && summary.mayAsk && summary.yourRequests && summary.requestsEntry)
        // Not a manager: the number is the news.
        #expect(summary.badge == 1 && !summary.badgeAlarm)

        let manager = try decode(RequestSummary.self, fixture: "requests/summary_manager.json")
        #expect(manager.manage && manager.access == .auto)
        #expect(manager.moviesLeft == nil && manager.seasonsLeft == nil)
        // A manager's number: pending, failed and news; failed ones in the danger colour.
        #expect(manager.badge == 3 && manager.badgeAlarm)
    }

    @Test func aSummaryWithoutRequestsOrFromBeforeThemDecides() throws {
        let off = try decode(RequestSummary.self, #"{"enabled": false}"#)
        #expect(!off.enabled && off.access == .off && !off.discover && !off.requestsEntry)
        #expect(off == RequestSummary.off)
        let before = try decode(RequestSummary.self, #"{"enabled": true, "access": "ask", "sign_in_again": "requests"}"#)
        #expect(before.discover && !before.mayAsk && !before.yourRequests)
        let approvals = try decode(RequestSummary.self,
                                   #"{"enabled": true, "access": "auto", "manage": true, "sign_in_again": "approvals"}"#)
        #expect(approvals.mayAsk && approvals.requestsEntry)
        // No access, but requests made before: Your requests still shows.
        let kept = try decode(RequestSummary.self, #"{"enabled": true, "access": "off", "own": 2}"#)
        #expect(!kept.discover && kept.yourRequests)
        // A value this app doesn't know is kept as it came.
        #expect(try decode(RequestSummary.self, #"{"enabled": true, "access": "vip"}"#).access.rawValue == "vip")
    }

    @Test func trendingTitlesHaveTheirStatusAndKey() throws {
        let page = try decode(DiscoverPage.self, fixture: "discover/trending.json")
        #expect(page.page == 1 && page.totalPages == 3)
        #expect(page.results.map(\.title) == ["The Wild Robot", "Severance", "Arrival", "The Substance", "Breaking Bad",
                                              "Alien: Romulus"])
        #expect(page.results.map(\.status) == [.none, .partial, .library, .requested, .partial, .declined])
        #expect(page.results[0].key == "movie:1184918" && page.results[0].id == "movie:1184918")
        #expect(page.results[1].kind == .show && page.results[1].itemId == 201)
        #expect(page.results[3].requestId == 44)
        // A movie and a show may share a TMDB id: the key tells them apart.
        let show = DiscoverTitle(kind: .show, tmdbId: 1_184_918, title: "x")
        #expect(show.key != page.results[0].key)
    }

    @Test func aMoviesPage() throws {
        let movie = try decode(DiscoverDetail.self, fixture: "discover/movie.json")
        #expect(movie.kind == .movie && movie.title == "The Wild Robot" && movie.runtimeMinutes == 102)
        #expect(movie.certification == "PG" && movie.genres == ["Animation", "Science Fiction", "Family"])
        #expect(movie.cast.map(\.name) == ["Lupita Nyong'o", "Pedro Pascal", "Kit Connor"])
        #expect(movie.cast[0].character == "Roz (voice)" && movie.cast[2].character == nil)
        #expect(movie.seasons == nil && movie.requests.isEmpty && movie.moviesLeft == 2)
        #expect(movie.asTitle.key == "movie:1184918" && movie.asTitle.status == .none)
    }

    @Test func aShowsPageHasItsSeasonsAndRequests() throws {
        let show = try decode(DiscoverDetail.self, fixture: "discover/show.json")
        #expect(show.kind == .show && show.status == .partial && show.requestId == 42)
        let seasons = try #require(show.seasons)
        #expect(seasons.map(\.number) == [0, 1, 2, 3, 4, 5])
        #expect(seasons[1].state == .requested && seasons[1].requestId == 42)
        #expect(seasons[4].declined && seasons[4].state == .missing)
        #expect(seasons[0].name == "Specials" && seasons[0].episodes == 9)
        let request = try #require(show.requests.first)
        #expect(request.id == 42 && request.mine && request.stage == .downloading)
        #expect(request.download?.state == .downloading && request.downloadProgress == 0.42)
        #expect(request.moving && request.active && !request.readyToWatch)
    }

    @Test func aLibraryTitleAsDiscoverSeesIt() throws {
        let item = try decode(ItemRequest.self, fixture: "requests/item_request.json")
        #expect(item.kind == .show && item.tmdbId == 95396 && item.status == .partial)
        #expect(item.request == nil && item.askable == [2])
        // Not known yet (`null`) differs from nothing to ask for (`[]`).
        let unknown = try decode(ItemRequest.self, #"{"kind": "movie", "tmdb_id": 1, "status": "library", "askable": null}"#)
        #expect(unknown.askable == nil)
        let none = try decode(ItemRequest.self, #"{"kind": "movie", "tmdb_id": 1, "status": "library", "askable": []}"#)
        #expect(none.askable == [])
    }

    @Test func listsOfRequests() throws {
        let list = try decode(RequestList.self, fixture: "requests/list.json")
        #expect(list.total == 6 && list.requests.map(\.id) == [48, 47, 46, 45, 44, 42])
        #expect(list.counts[.pending] == 1 && list.counts[.approved] == 2 && list.counts[.available] == 1)
        #expect(list.counts[RequestStatus(rawValue: "other")] == 0)
        #expect(list.people.isEmpty)
        let failed = list.requests[0]
        #expect(failed.status == .failed && failed.retryAt == 1_761_599_400 && failed.actions == [.withdraw])
        let followed = list.requests[1]
        #expect(!followed.mine && followed.watching && followed.actions == [.unwatch] && followed.seasons == [2])
        let declined = list.requests[2]
        #expect(declined.reason == "We have enough aliens for now" && declined.actions.isEmpty)
        let ready = list.requests[3]
        #expect(ready.unseen && ready.itemId == 101 && ready.readyToWatch)

        let attention = try decode(RequestList.self, fixture: "requests/attention.json")
        #expect(attention.requests.map(\.id) == [44, 49, 50])
        #expect(attention.people.map(\.name) == ["Kit", "Sam"])
        #expect(attention.requests[0].actions == [.approve, .decline, .handled, .delete])
        #expect(attention.requests[2].actions == [.retry, .decline, .handled, .delete])
        #expect(attention.requests[1].requestedBy?.name == "Kit")
    }

    @Test func anUnknownActionStageOrStatusIsKeptAsItCame() throws {
        let request = try decode(MediaRequest.self, """
        {"id": 1, "kind": "movie", "tmdb_id": 2, "title": "X", "status": "archived", "stage": "teleporting",
         "actions": ["withdraw", "frobnicate"], "created_at": 1, "updated_at": 2}
        """)
        #expect(request.status.rawValue == "archived" && request.stage.rawValue == "teleporting")
        #expect(request.actions == [.withdraw, RequestAction(rawValue: "frobnicate")])
        #expect(!request.active)
        #expect(StagePill.of(request, manage: false) == .unknown)
    }

    @Test func approvingsChoices() throws {
        let options = try decode(RequestOptions.self, fixture: "requests/options.json")
        let radarr = try #require(options.radarr)
        #expect(radarr.qualityProfiles.map(\.name) == ["HD-1080p", "Ultra-HD"] && radarr.qualityProfile == 4)
        #expect(radarr.rootFolders[0].freeBytes == 1_832_000_000_000 && radarr.rootFolders[1].freeBytes == nil)
        #expect(options.of(.show)?.animeRootFolder == "/anime" && options.of(.show)?.animeQualityProfile == 1)
        #expect(options.of(.movie) == radarr)
    }

    @Test func errorBodiesSayWhenAndWhatIsNow() throws {
        let declined = try decode(APIErrorBody.self, fixture: "errors/declined.json")
        #expect(declined.code == "declined" && declined.retryAt == 1_763_992_000 && declined.refusal)
        let conflict = try decode(APIErrorBody.self, fixture: "errors/conflict_request.json")
        #expect(conflict.request?.id == 44 && conflict.request?.status == .approved && conflict.refusal)
        let down = try decode(APIErrorBody.self, fixture: "errors/tmdb_down.json")
        #expect(down.code == "tmdb_down" && down.setup == true && !down.refusal)
        // A body whose request doesn't read keeps the rest.
        let odd = try decode(APIErrorBody.self, #"{"code": "conflict", "message": "m", "request": {"id": "x"}}"#)
        #expect(odd.code == "conflict" && odd.request == nil)
    }

    @Test func requestEventsAreRead() throws {
        func event(_ name: String) throws -> ServerEvent {
            try #require(ServerEvent.parse(try Fixtures.data("events/\(name).json")))
        }
        #expect(try event("requests_changed") == .requestsChanged(requestId: 42, kind: .show, tmdbId: 1396))
        #expect(try event("request_available") == .requestAvailable(
            requestId: 42, title: "Breaking Bad", kind: .show, tmdbId: 1396, itemId: 230,
            posterUrl: "/api/discover/images/poster/ztkUQFLlC19CCMYHW9o1zWhJRNq.jpg", seasons: [1, 2], complete: false))
        #expect(try event("request_decided") == .requestDecided(
            requestId: 46, title: "Alien: Romulus", kind: .movie, tmdbId: 945961, status: .declined,
            reason: "We have enough aliens for now"))
        guard case .requestCreated(49, "Arcane", .show, 94605, [1, 2, 3, 5], let by, .pending) = try event("request_created")
        else { Issue.record("request_created"); return }
        #expect(by?.name == "Kit")
        #expect(try event("request_failed") == .requestFailed(requestId: 50, title: "Inside Out 2", kind: .movie,
                                                               reason: "Radarr refused the key"))
        #expect(try event("requests_settings_changed") == .requestsSettingsChanged)
        // Many changed: nothing named.
        #expect(ServerEvent.parse(Data(#"{"type": "requests_changed"}"#.utf8))
            == .requestsChanged(requestId: nil, kind: nil, tmdbId: nil))
        // An event missing what it must say is one this app doesn't know.
        #expect(ServerEvent.parse(Data(#"{"type": "request_available", "title": "X"}"#.utf8))
            == .unknown("request_available"))
    }

    @Test func aQueryAsksForWhatItSays() {
        func items(_ query: RequestQuery) -> [String] { query.items.map { "\($0.name)=\($0.value ?? "")" } }
        #expect(items(RequestQuery()) == ["limit=30"])
        #expect(items(RequestQuery(statuses: [.pending, .failed], attention: true, kind: .show, userId: 3, q: "dune",
                                   mine: true, unseen: true, changedSince: 5, oldest: true, offset: 30, limit: 500))
            == ["status=pending,failed", "attention=1", "kind=show", "user_id=3", "q=dune", "mine=1", "unseen=1",
                "changed_since=5", "order=oldest", "offset=30", "limit=200"])
    }

    @Test func aStagePillForEachStage() {
        func pill(_ stage: RequestStage, manage: Bool = false, _ change: (inout MediaRequest) -> Void = { _ in })
            -> StagePill {
            var request = MediaRequest(id: 1, kind: .show, tmdbId: 2, title: "X", availableSeasons: [1], stage: stage,
                                       stageAt: 100, reason: "why", retryAt: 300)
            change(&request)
            return StagePill.of(request, manage: manage)
        }
        #expect(pill(.pending) == .pending && pill(.pending).tone == .amber)
        #expect(pill(.waiting) == .waiting(show: true, at: 100) && pill(.waiting).tone == .blue)
        #expect(pill(.downloading) { $0.download = RequestDownload(progress: 1.5, eta: 9) }
            == .downloading(progress: 1, eta: 9, state: .downloading))
        #expect(pill(.partial) == .partial(seasons: [1]) && pill(.partial).tone == .green)
        #expect(pill(.gone).tone == .gray)
        #expect(pill(.failed, manage: true) == .couldNotSend(reason: "why", retryAt: 300))
        #expect(pill(.failed) == .notSentYet(retryAt: 300) && pill(.failed).tone == .red)
    }

    @Test func theAllowanceOfASummaryAndOfATitlePage() {
        let summary = RequestSummary(enabled: true, access: .ask, moviesLeft: 0, seasonsLeft: 5, moviesLimit: 3,
                                     seasonsLimit: 10, moviesNextAt: 99)
        let allowance = Allowance(summary: summary)
        #expect(allowance.limited && !allowance.allows(.movie) && allowance.allows(.show))
        #expect(allowance.left(.show) == 5 && allowance.left(.movie) == 0)
        let page = DiscoverDetail(kind: .movie, tmdbId: 1, title: "X", moviesLeft: 1, seasonsLeft: 2)
        let fromPage = Allowance(detail: page, summary: summary)
        #expect(fromPage.moviesLeft == 1 && fromPage.moviesLimit == 3 && fromPage.moviesNextAt == nil)
        #expect(!Allowance().limited && Allowance().allows(.movie))
    }
}
