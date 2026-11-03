import Foundation
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private func client(_ server: FixtureServer, scope: String = FixtureServer.allScopes) -> PleyAPI {
    let http = transport()
    return PleyAPI(client: APIClient(origin: server.origin,
                                     tokens: FakeTokens(server: server, transport: http, scope: scope),
                                     transport: http))
}

private func object(_ data: Data?) -> [String: Any]? {
    data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
}

@Suite struct RequestsAPITests {
    @Test func theSummaryAndDiscover() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = client(server)
        let summary = try await api.requestSummary()
        #expect(summary.enabled && summary.mayAsk && summary.unseen == 0)
        let trending = try await api.discover(.trending)
        #expect(trending.results.count == 6 && trending.totalPages == 3)
        #expect(try await api.discover(.trending, page: 3).results.isEmpty)
        #expect(server.requests("/api/discover/trending").last?.query["page"] == "3")
        let missing = try await api.discoverSearch("  wild  ", missing: true)
        #expect(!missing.results.contains { $0.status == .library })
        let asked = try #require(server.requests("/api/discover/search").last)
        #expect(asked.query["q"] == "wild" && asked.query["missing"] == "1" && asked.query["page"] == "1")
        #expect(try await api.discoverDetail(.show, tmdbId: 1396).seasons?.count == 6)
        #expect(server.requests("/api/discover/show/1396").count == 1)
    }

    @Test func aTitleOrRequestThatIsntThereIsNil() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = client(server)
        #expect(try await api.itemRequest(201)?.tmdbId == 95396)
        #expect(try await api.itemRequest(999) == nil)
        #expect(try await api.request(44)?.title == "The Substance")
        #expect(try await api.request(999) == nil)
        // Any other refusal still throws.
        await #expect(throws: PleyError.self) { try await api.discoverDetail(.movie, tmdbId: 0) }
    }

    @Test func askingSaysWhetherTheRequestIsNew() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = client(server)
        let created = try await api.ask(NewRequest(kind: .show, tmdbId: 1396, seasons: [3, 5]))
        #expect(created.created && created.request.id == 44 && created.request.status == .pending)
        let body = try #require(object(server.askedBodies.last))
        #expect(body["kind"] as? String == "show" && body["tmdb_id"] as? Int == 1396 && body["seasons"] as? [Int] == [3, 5])
        // 200: a request this account follows now.
        server.answerAsk(status: 200)
        let followed = try await api.ask(NewRequest(kind: .movie, tmdbId: 933260))
        #expect(!followed.created && followed.request.id == 44)
        #expect(server.askedBodies.count == 2)
    }

    @Test func refusalsSayWhenAndWhatIsNow() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = client(server)
        server.answerAsk(status: 409, fixture: "errors/declined.json")
        do {
            _ = try await api.ask(NewRequest(kind: .movie, tmdbId: 945961))
            Issue.record("asking went through")
        } catch PleyError.refused(409, "declined", _, let retryAt, nil) {
            #expect(retryAt == 1_763_992_000)
        }
        server.answerAsk(status: 409, fixture: "errors/conflict_request.json")
        do {
            _ = try await api.ask(NewRequest(kind: .movie, tmdbId: 933260))
            Issue.record("asking went through")
        } catch PleyError.refused(409, "conflict", _, nil, let request) {
            #expect(request?.id == 44 && request?.status == .approved)
        }
        server.failRequests(prefix: "/api/discover/", status: 503)
        do {
            _ = try await api.discover(.trending)
            Issue.record("Discover answered")
        } catch PleyError.notAvailable(let code, _, let setup) {
            #expect(code == "tmdb_down" && setup)
        }
        server.failRequests(prefix: "/api/discover/")
        server.setRequestsOff(true)
        do {
            _ = try await api.requests(RequestQuery())
            Issue.record("the list answered")
        } catch PleyError.notAvailable(let code, _, let setup) {
            #expect(code == "unavailable" && !setup)
        }
        // The summary still answers, and says requests are off.
        #expect(try await api.requestSummary().enabled == false)
    }

    @Test func managersNeedTheApprovalsScope() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = client(server, scope: "library play playlists profiles requests")
        server.useManagerSummary()
        #expect(try await api.requestSummary().signInAgain == "approvals")
        do {
            _ = try await api.approveRequest(44)
            Issue.record("approved without the scope")
        } catch PleyError.insufficientScope(let scope) {
            #expect(scope == "approvals")
        }
        #expect(server.requestActions.isEmpty)
        // Asking needs only `requests`.
        #expect(try await api.requests(RequestQuery()).requests.count == 6)
        let old = client(server, scope: "library play playlists profiles")
        #expect(try await old.requestSummary().signInAgain == "requests")
    }

    @Test func decisionsSendWhatTheyChose() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = client(server)
        let approved = try await api.approveRequest(49, Approval(qualityProfile: 4, rootFolder: "/tv", seasons: [1]))
        #expect(approved.status == .approved && approved.stage == .searching)
        let declined = try await api.declineRequest(50, reason: "  Not for us ")
        #expect(declined.status == .declined && declined.reason == "Not for us")
        _ = try await api.declineRequest(44, reason: "   ")
        _ = try await api.markHandled(48)
        try await api.deleteRequest(46)
        try await api.unwatchRequest(47)
        try await api.markRequestsSeen()
        #expect(server.requestActions == ["POST /api/requests/49/approve", "POST /api/requests/50/decline",
                                          "POST /api/requests/44/decline", "POST /api/requests/48/handled",
                                          "DELETE /api/requests/46", "DELETE /api/requests/47/watch"])
        let bodies = server.requestActionBodies
        let approval = try #require(object(bodies[0]))
        #expect(approval["quality_profile"] as? Int == 4 && approval["root_folder"] as? String == "/tv"
            && approval["seasons"] as? [Int] == [1])
        #expect(object(bodies[1])?["reason"] as? String == "Not for us")
        // An empty reason is left out.
        #expect(object(bodies[2])?.keys.contains("reason") == false)
        #expect(server.requestsSeenCalls == 1)
        #expect(try await api.request(46) == nil)
        #expect(try await api.request(50)?.status == .declined)
        let options = try await api.requestOptions()
        #expect(options.radarr != nil || options.sonarr != nil)
    }

    @Test func aQuerysFiltersReachTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = client(server)
        var query = RequestQuery()
        query.mine = true
        query.kind = .movie
        let mine = try await api.requests(query)
        #expect(mine.requests.allSatisfy { $0.kind == .movie } && mine.requests.map(\.id) == [48, 46, 45, 44])
        query = RequestQuery()
        query.attention = true
        #expect(try await api.requests(query).requests.map(\.id) == [44, 49, 50])
        let asked = try #require(server.requests("/api/requests").last)
        #expect(asked.query["attention"] == "1")
    }

    @Test func aSearchIsTrimmedAndCut() {
        #expect(PleyAPI.searchText("  dune  ") == "dune")
        let long = String(repeating: "a", count: 199) + "é" + "bcd"
        #expect(PleyAPI.searchText(long).unicodeScalars.count == 200)
        // A scalar is never cut in half: a flag is two scalars, cut between them.
        let flags = String(repeating: "b", count: 199) + "🇬🇧"
        #expect(PleyAPI.searchText(flags).unicodeScalars.count == 200)
        #expect(PleyAPI.searchText(String(repeating: "x", count: 300)).count == PleyAPI.discoverSearchMax)
    }
}
