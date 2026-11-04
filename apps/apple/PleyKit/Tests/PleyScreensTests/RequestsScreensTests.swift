import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
@testable import PleyScreens
import PleyTesting
import Testing

/// A signed-in app on `server`, at Home, with the sign-in's `scope`.
@MainActor
private func signedIn(_ server: FixtureServer, scope: String = FixtureServer.allScopes) async throws -> AppModel {
    let store = MemoryTokenStore()
    let tokens = server.issueTokens(scope: scope)
    try await store.save(StoredTokens(
        issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
        revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
        accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken, scope: scope))
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
    await app.start()
    try #require(app.root == .signedIn)
    return app
}

/// Signed in, with the requests' summary read.
@MainActor
private func ready(_ server: FixtureServer, scope: String = FixtureServer.allScopes) async throws -> AppModel {
    let app = try await signedIn(server, scope: scope)
    #expect(await eventually { app.requests.summary != nil })
    return app
}

/// Signed in, with the news caught up on at sign-in told (the fixture's
/// person has one: Arrival is ready) and its toast gone.
@MainActor
private func caughtUp(_ server: FixtureServer) async throws -> AppModel {
    server.setRequestsUnseen(1)
    let app = try await ready(server)
    #expect(await eventually { app.shell.toast?.text == "“Arrival” is ready to watch." })
    app.shell.dismissToast()
    #expect(await eventually { server.openEventStreams == 1 })
    return app
}

private func object(_ data: Data?) -> [String: Any]? {
    data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
}

/// The ways in, the gate, and what the server tells.
@MainActor
@Suite struct RequestsGateTests {
    @Test func theSummaryOpensTheWaysIn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setRequestsUnseen(1)
        let app = try await ready(server)
        #expect(app.requests.gate.mayAsk)
        #expect(app.shell.requestsEntry == RequestsEntry(discover: true, requests: true, badge: 1))
        #expect(!server.requests("/api/requests/summary").isEmpty)
    }

    @Test func aSignInFromBeforeRequestsIsAskedToSignInAgain() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server, scope: "library play playlists profiles")
        guard case .signInAgain = app.requests.gate else {
            Issue.record("the gate is \(app.requests.gate)")
            return
        }
        #expect(app.shell.requestsEntry == RequestsEntry(discover: true))
        let discover = DiscoverModel(app: app)
        await discover.load()
        #expect(discover.problem?.signInAgain == true)
        // Nothing of Discover is read.
        #expect(server.requests("/api/discover/trending").isEmpty)
        // Sign in again: the server's sign-in shows over the servers.
        discover.signInAgain()
        #expect(app.root == .servers && app.signingIn.map(\.origin) == [server.origin] && app.notice != nil)
        app.cancelSignInAgain()
        #expect(app.root == .signedIn && app.signingIn.isEmpty && app.notice == nil)
    }

    @Test func requestsOffOrAnOlderServerShowNothing() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setRequestsOff(true)
        let app = try await ready(server)
        #expect(app.requests.gate == .hidden && !app.shell.requestsEntry.shows)
        let older = try await fixture()
        defer { older.stop() }
        older.setRequestsMissing(true)
        let other = try await ready(older)
        #expect(other.requests.gate == .hidden)
    }

    @Test func aManagerSignedInWithoutApprovalsSeesTheBanner() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.useManagerSummary()
        let app = try await ready(server, scope: "library play playlists profiles requests")
        #expect(app.requests.summary?.signInAgain == "approvals")
        #expect(app.requests.summary?.manage == false && !app.shell.requestsEntry.manage)
        // Not a manager: Your requests, on all of them, with the banner.
        let list = RequestsListModel(app: app)
        await list.load()
        #expect(list.approvalsBanner && !list.manage && list.title == "Your requests")
        #expect(list.filter?.view == .all)
        // The list still reads: approving is what needs the sign-in again.
        #expect(await eventually { !list.cards.isEmpty })
    }

    #if !os(tvOS)
    @Test func newsIsToldWithOpen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await caughtUp(server)
        server.pushFixtureEvent("events/request_available.json")
        #expect(await eventually { app.shell.toast?.text == "Seasons 1–2 of “Breaking Bad” are ready to watch." })
        #expect(app.shell.toast?.action?.label == "Open")
        app.shell.runToastAction()
        #expect(app.shell.toast == nil)
        #expect(await eventually { app.shell.path(for: app.shell.section).last == .item(230) })
    }

    @Test func newsWaitsForThePlayerToClose() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await caughtUp(server)
        _ = try #require(await app.play(PlayRequest(itemId: 7)))
        for _ in 0..<5 { server.pushFixtureEvent("events/request_decided.json") }
        #expect(await eventually { app.newsHeld.count == AppModel.heldNewsMax })
        #expect(app.shell.toast == nil)
        await app.closePlayer()
        #expect(await eventually { app.shell.toast?.text == "“Alien: Romulus” was declined: “We have enough aliens for now”." })
        #expect(app.newsHeld.count == AppModel.heldNewsMax - 1)
    }

    @Test func heldNewsIsToldOneAtATimeWhenPlayersOpenAndClose() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await caughtUp(server)
        app.heldNewsGap = 1
        _ = try #require(await app.play(PlayRequest(itemId: 7)))
        for _ in 0..<3 { server.pushFixtureEvent("events/request_decided.json") }
        #expect(await eventually { app.newsHeld.count == 3 })
        await app.closePlayer()
        #expect(app.newsHeld.count == 2)
        // Another video, closed half a gap later: the news goes on a gap
        // after that, not when the first close's would have.
        _ = try #require(await app.play(PlayRequest(itemId: 7)))
        try await Task.sleep(for: .milliseconds(500))
        await app.closePlayer()
        #expect(app.newsHeld.count == 1)
        try await Task.sleep(for: .milliseconds(750))
        #expect(app.newsHeld.count == 1)
        #expect(await eventually { app.newsHeld.isEmpty })
    }
    #endif

    @Test func aChangeReachesTheScreens() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        #expect(await eventually { server.openEventStreams == 1 })
        let page = DiscoverTitleModel(app: app, kind: .show, tmdbId: 1396)
        page.debounce = 0.05
        await page.load()
        let before = server.requests("/api/discover/show/1396").count
        server.pushFixtureEvent("events/requests_changed.json")
        #expect(await eventually { server.requests("/api/discover/show/1396").count == before + 1 })
    }
}

@MainActor
@Suite struct DiscoverScreensTests {
    @Test func discoverReadsTrendingFirstWithItsHero() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = DiscoverModel(app: app)
        await model.load()
        let trending = try #require(model.rows.first)
        #expect(trending.list == .trending && trending.cards.count == 6)
        // Short of a screenful, its next pages are read: the same titles, then none.
        #expect(server.requests("/api/discover/trending").compactMap { $0.query["page"] } == ["1", "2", "3"])
        #expect(trending.pages.settled)
        let hero = try #require(model.hero)
        #expect(hero.card.title == "The Wild Robot" && hero.primary == .request(enabled: true))
        // What's left to ask for shows on the title's page, unless nothing is.
        #expect(hero.lines.isEmpty)
        #expect(model.problem == nil && !model.nothing)
        // Your requests.
        #expect(await eventually { model.yourRequests?.isEmpty == false })
        // Other rows are read as they show.
        #expect(server.requests("/api/discover/movies").isEmpty)
        await model.rowAppeared(.movies)
        #expect(model.rows.first { $0.list == .movies }?.cards.count == 6)
    }

    @Test func theHeroAsksForAMovie() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = DiscoverModel(app: app)
        await model.load()
        #expect(await eventually { model.hero != nil })
        model.heroPrimary()
        model.heroPrimary()
        #expect(await eventually { app.shell.toast?.text == "Asked for “The Wild Robot”: an administrator decides." })
        // Pressed twice while on its way: asked once.
        #expect(server.askedBodies.count == 1)
        #expect(object(server.askedBodies.first)?["tmdb_id"] as? Int == 1_184_918)
        // Its page is read again: no button now, only the request's stage.
        #expect(await eventually { if case .mine(_?) = model.hero?.action { true } else { false } })
        #expect(model.hero?.primary == nil && model.hero?.pill != nil)
        #expect(model.rows.first?.cards.first?.badge == "Requested")
    }

    @Test func aHeroWithNothingLeftToAskForSaysWhen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setRequestSummary("""
        {"enabled": true, "access": "ask", "movies_left": 0, "seasons_left": 5, "movies_limit": 3,
         "seasons_limit": 10, "manage": false, "pending": 0, "failed": 0, "unseen": 0, "own": 4}
        """)
        // The title's page isn't read: the hero goes by the summary.
        server.failRequests(prefix: "/api/discover/movie/", status: 500)
        let app = try await ready(server)
        let model = DiscoverModel(app: app)
        await model.load()
        let hero = try #require(model.hero)
        #expect(hero.action == .request(.limit) && hero.primary == .request(enabled: false))
        #expect(hero.lines == [RequestWords.allForNow])
    }

    @Test func discoverSaysWhyItCantShow() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        server.failRequests(prefix: "/api/discover/", status: 503)
        let model = DiscoverModel(app: app)
        await model.load()
        #expect(model.problem?.title == RequestWords.discoverFailed || model.problem?.title == RequestWords.tmdbDown)
        #expect(model.problem?.retry == true && model.hero == nil)
        server.failRequests(prefix: "/api/discover/")
        model.retry()
        #expect(await eventually { model.rows.first?.cards.count == 6 })
        #expect(await eventually { model.problem == nil })
    }

    @Test func aTitlesPageAsksForPickedSeasons() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = DiscoverTitleModel(app: app, kind: .show, tmdbId: 1396)
        await model.load()
        #expect(model.state == .ready)
        #expect(model.action == .partial(itemId: nil, more: .yes))
        #expect(model.page?.title == "Breaking Bad")
        #expect(model.seasons.map(\.number) == [1, 2, 3, 4, 5, 0])
        #expect(model.seasons[0].pill != nil && model.seasons[2].pill == nil)
        #expect(model.requests.map(\.id) == [42])
        #expect(model.picker == nil)
        model.openPicker()
        #expect(model.picker?.picked == [3, 5])
        model.toggle(5)
        model.debounce = 0.05
        let reads = server.requests("/api/discover/show/1396").count
        model.askPicked()
        #expect(await eventually { model.picker == nil && !model.busy })
        #expect(object(server.askedBodies.last)?["seasons"] as? [Int] == [3])
        #expect(await eventually { app.shell.toast?.text == "Asked for “Breaking Bad”: an administrator decides." })
        // Read again once: the change the ask sends, heard here too, doesn't read it twice.
        try await Task.sleep(for: .milliseconds(300))
        #expect(server.requests("/api/discover/show/1396").count == reads + 1)
    }

    @Test func requestMoreSeasonsOpensThePicker() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = DiscoverTitleModel(app: app, kind: .show, tmdbId: 1396, pick: true)
        await model.load()
        #expect(model.picker?.picked == [3, 5])
        // Only once: read again, it stays as the person left it.
        model.toggle(3)
        await model.load()
        #expect(model.picker?.picked == [5])
    }

    @Test func aMoviesRefusalIsTold() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = DiscoverTitleModel(app: app, kind: .movie, tmdbId: 945_961)
        await model.load()
        #expect(model.action == .request(.yes) && model.lines == ["You can ask for 2 more movies.", "Limit: 3 movies per 7 days."])
        server.answerAsk(status: 409, fixture: "errors/declined.json")
        model.request()
        #expect(await eventually { app.shell.toast?.style == .error && !model.busy })
        #expect(app.shell.toast?.text.hasPrefix("You can ask again") == true)
    }

    @Test func aTitleNotThereSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = DiscoverTitleModel(app: app, kind: .movie, tmdbId: 0)
        await model.load()
        #expect(model.state == .failed(RequestProblem.notFound.words(manage: false, summary: nil)))
        #expect(model.open() == nil)
    }

    @Test func searchResultsReadOnTillAScreenful() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = DiscoverResultsModel(app: app, query: "the")
        await model.load()
        #expect(model.state == .ready && model.cards.map(\.title) == ["The Wild Robot", "The Substance"])
        #expect(model.heading.contains("the"))
        #expect(await eventually { server.requests("/api/discover/search").count == 3 })
        let none = DiscoverResultsModel(app: app, query: "nothing at all")
        await none.load()
        #expect(none.state == .empty && none.emptyTitle.contains("nothing at all"))
    }

    @Test func searchShowsWhatTheLibrariesDontHave() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        #expect(app.shell.requestsEntry.discover)
        let model = SearchModel(app: app, debounce: .milliseconds(10))
        #expect(model.discoverEntry)
        model.query = "a"
        #expect(await eventually { if case .ready = model.discover { true } else { false } })
        guard case .ready(let query, let cards) = model.discover else { return }
        // Missing only: Arrival, in a library, is left out.
        #expect(query == "a" && cards.map(\.title) == ["Severance", "The Substance", "Breaking Bad", "Alien: Romulus"])
        #expect(model.discoverAll == .discoverResults(query: "a"))
        #expect(server.requests("/api/discover/search").allSatisfy { $0.query["missing"] == "1" })
    }

    @Test func searchWithNothingAnywhereSaysNoResults() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = SearchModel(app: app, debounce: .milliseconds(10))
        model.query = "nothing at all"
        #expect(await eventually { !server.requests("/api/discover/search").isEmpty && model.state == .empty("nothing at all") })
        // TMDB has none either (the row was loading once it was asked): no
        // row, and not "Nothing in your libraries matches".
        #expect(await eventually { model.discover == .hidden })
        #expect(!model.nothingInLibraries)
    }

    @Test func theDiscoverRowFollowsWhetherAskingWorks() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = SearchModel(app: app, debounce: .milliseconds(10))
        model.query = "the"
        #expect(await eventually { if case .ready = model.discover { true } else { false } })
        server.setRequestsOff(true)
        await app.requests.refreshNow()
        #expect(await eventually { model.discover == .hidden })
        server.setRequestsOff(false)
        await app.requests.refreshNow()
        #expect(await eventually { if case .ready(let query, _) = model.discover { query == "the" } else { false } })
    }

    @Test func aPageOpenedBeforeTheGateShowsTheBlockWhenItOpens() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setRequestsOff(true)
        let app = try await signedIn(server)
        #expect(await eventually { app.requests.summary != nil })
        let model = ItemModel(app: app, itemId: 201)
        await model.load()
        try await Task.sleep(for: .milliseconds(200))
        #expect(model.requestBlock == nil)
        server.setRequestsOff(false)
        await app.requests.refreshNow()
        #expect(await eventually { model.requestBlock?.more == true })
    }

    @Test func aLibraryShowOffersMoreSeasons() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = ItemModel(app: app, itemId: 201)
        await model.load()
        #expect(await eventually { model.requestBlock != nil })
        #expect(model.requestBlock?.more == true)
        #expect(model.requestBlock?.picker == .discoverTitle(.show, tmdbId: 95396, pick: true))
        // A movie Discover doesn't know: nothing, and it isn't asked again.
        let movie = ItemModel(app: app, itemId: 101)
        movie.requestRetry = 0.02
        await movie.load()
        try await Task.sleep(for: .milliseconds(300))
        #expect(movie.requestBlock == nil)
        #expect(server.requests("/api/items/101/request").count == 1)
    }
}

@MainActor
@Suite struct RequestsListTests {
    @Test func yourRequestsShowAndTheirNewsIsSeen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setRequestsUnseen(1)
        let app = try await ready(server)
        let model = RequestsListModel(app: app)
        await model.load()
        #expect(model.title == "Your requests" && !model.manage)
        #expect(model.filter == RequestFilter(view: .all))
        #expect(model.state == .ready && model.cards.map(\.id) == [48, 47, 46, 45, 44, 42])
        #expect(await eventually { server.requestsSeenCalls == 1 })
        #expect(await eventually { app.requests.summary?.unseen == 0 && app.shell.requestsEntry.badge == 0 })
        // Movies only.
        model.select(kind: .movie)
        #expect(await eventually { model.cards.map(\.id) == [48, 46, 45, 44] })
        #expect(model.kindChips.first { $0.id == "movie" }?.selected == true)
        // Seen once only.
        await model.refresh()
        #expect(server.requestsSeenCalls == 1)
    }

    @Test func withdrawingAsksFirst() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = RequestsListModel(app: app)
        await model.load()
        let card = try #require(model.cards.first { $0.id == 44 })
        #expect(card.buttons == [.withdraw])
        model.runner.tap(.withdraw, on: card)
        #expect(model.runner.confirming?.action == .withdraw)
        #expect(server.requestActions.isEmpty)
        model.runner.confirm()
        #expect(await eventually { !model.cards.contains { $0.id == 44 } })
        #expect(server.requestActions == ["DELETE /api/requests/44"])
    }

    @Test func aRequestGoneElsewhereLeavesTheList() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        #expect(await eventually { server.openEventStreams == 1 })
        let model = RequestsListModel(app: app)
        await model.load()
        #expect(model.cards.contains { $0.id == 45 })
        // Deleted on another device: it reads as not found, and its card goes.
        try await app.requestsAPI().deleteRequest(45)
        server.pushEvent(#"{ "type": "requests_changed", "request_id": 45 }"#)
        #expect(await eventually { !model.cards.contains { $0.id == 45 } })
        #expect(model.cards.map(\.id) == [48, 47, 46, 44, 42])
    }

    @Test func theBadgeStaysClearWhileTheServerHearsItWasSeen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setRequestsUnseen(1)
        let app = try await ready(server)
        app.shell.dismissToast()
        server.delay(path: "/api/requests/seen", ms: 1500, method: "POST")
        let model = RequestsListModel(app: app)
        await model.load()
        #expect(app.requests.summary?.unseen == 0)
        // A summary read before the server has heard still counts the news.
        let reads = server.requests("/api/requests/summary").count
        try await Task.sleep(for: .milliseconds(1200))
        #expect(app.requests.summary?.unseen == 0)
        #expect(server.requests("/api/requests/summary").count == reads)
        #expect(await eventually { server.requestsSeenCalls == 1 })
        #expect(await eventually { server.requests("/api/requests/summary").count == reads + 1 })
        #expect(app.requests.summary?.unseen == 0)
    }

    @Test func managersOpenOnWhatNeedsThem() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.useManagerSummary()
        let app = try await ready(server)
        let model = RequestsListModel(app: app)
        await model.load()
        #expect(model.title == "Requests" && model.filter?.view == .attention)
        #expect(model.cards.map(\.id) == [44, 49, 50])
        #expect(model.views.first?.id == "attention" && model.views.first?.selected == true)
        #expect(server.requests("/api/requests").last?.query["order"] == "oldest")
        // Approved, it no longer needs anyone.
        let arcane = try #require(model.cards.first { $0.id == 49 })
        #expect(arcane.buttons == [.approve, .decline] && arcane.menu == [.approveWith, .handled, .delete])
        #expect(model.views.map(\.title).contains("Pending · 2"))
        model.runner.tap(.approve, on: arcane)
        #expect(await eventually { model.cards.map(\.id) == [44, 50] })
        // The chips follow: it left Pending for Approved.
        #expect(model.views.map(\.title).contains("Pending · 1"))
        #expect(model.views.map(\.title).contains("Approved · 5"))
        // Declined, with a reason.
        let failed = try #require(model.cards.first { $0.id == 50 })
        model.runner.tap(.decline, on: failed)
        #expect(model.runner.declining?.id == 50)
        model.runner.setReason("  Not for us  ")
        model.runner.decline()
        #expect(await eventually { model.cards.map(\.id) == [44] })
        #expect(object(server.requestActionBodies.last)?["reason"] as? String == "Not for us")
        #expect(server.requestActions == ["POST /api/requests/49/approve", "POST /api/requests/50/decline"])
    }

    @Test func approvingWithChoicesSendsOnlyWhatChanged() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.useManagerSummary()
        let app = try await ready(server)
        let model = RequestsListModel(app: app)
        await model.load()
        let arcane = try #require(model.cards.first { $0.id == 49 })
        model.runner.tap(.approveWith, on: arcane)
        let dialog = try #require(model.runner.approving)
        #expect(await eventually { !dialog.loading })
        #expect(dialog.choices && dialog.profile == 1 && dialog.folder == "/anime")
        dialog.toggle(season: 5)
        dialog.approve()
        #expect(await eventually { server.requestActions == ["POST /api/requests/49/approve"] })
        let sent = try #require(object(server.requestActionBodies.first))
        #expect(sent["seasons"] as? [Int] == [1, 2, 3] && sent["quality_profile"] == nil)
        #expect(model.runner.approving == nil)
    }

    @Test func aFilterWithNothingSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await ready(server)
        let model = RequestsListModel(app: app)
        model.typingPause = 0.01
        await model.load()
        model.setQuery("zzz")
        #expect(await eventually { if case .empty(let words) = model.state { !words.discover } else { false } })
    }
}
