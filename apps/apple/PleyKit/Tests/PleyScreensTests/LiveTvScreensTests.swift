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

/// Live TV on the fixture server, signed in.
@MainActor
private func liveRepository(_ server: FixtureServer) async throws -> LiveTvRepository {
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let session = SessionManager(transport: HTTPTransport(userAgent: "pleyApple/9.9.9 (iOS 18.1; iPhone)"), tokenStore: store,
                                 servers: ServerRepository(store: MemoryServerListStore()), client: .ios)
    _ = try await session.open(origin: server.origin)
    return LiveTvRepository(api: { @Sendable in
        guard let api = await session.api else { throw PleyError.signedOut }
        return api
    })
}

private func started() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}

private func group(_ name: String, _ count: Int = 1) -> LiveGroup {
    LiveGroup(name: name, region: LiveTvRules.splitRegion(name)?.region,
              label: LiveTvRules.splitRegion(name)?.rest ?? name, count: count)
}

@MainActor
@Suite struct LiveTvRulesTests {
    @Test func regionsOfTwoGroupsOrMoreGatherTheirs() {
        let groups = [group("UK| Sport", 3), group("FR| Info"), group("Radio"), group("UK| News", 2), group("FR| Sport"),
                      group("DE| Film")]
        let sections = LiveTvRules.groupSections(groups)
        #expect(sections.regions.map(\.region) == ["FR", "UK"])
        #expect(sections.regions[1].groups.map(\.name) == ["UK| Sport", "UK| News"])
        #expect(sections.regions[1].count == 5)
        // A region of one group, and no region, stay in the provider's order.
        #expect(sections.others.map(\.name) == ["Radio", "DE| Film"])
        // Fewer than two regions: one list.
        let one = LiveTvRules.groupSections([group("UK| Sport"), group("UK| News"), group("Radio")])
        #expect(one.regions.isEmpty && one.others.count == 3)
    }

    @Test func regionsAreSplitAsProvidersWriteThem() {
        #expect(LiveTvRules.splitRegion("UK| Sport")! == ("UK", "Sport"))
        #expect(LiveTvRules.splitRegion("[FR] Info")! == ("FR", "Info"))
        #expect(LiveTvRules.splitRegion("DE: Film")! == ("DE", "Film"))
        #expect(LiveTvRules.splitRegion("|UK| News")! == ("UK", "News"))
        #expect(LiveTvRules.splitRegion("Radio") == nil)
        #expect(LiveTvRules.splitRegion("Sport: live") == nil)
        let channel = LiveChannel(id: 1, name: "UK| Sky Sports 1", group: "UK| Sport")
        #expect(LiveTvRules.channelName(channel, in: .group("UK| Sport")) == "Sky Sports 1")
        #expect(LiveTvRules.channelName(channel, in: .all) == "UK| Sky Sports 1")
        #expect(LiveTvRules.categoryName(.group("UK| Sport"), groups: [group("UK| Sport")]) == "UK · Sport")
        #expect(LiveTvRules.categoryName(.recent, groups: []) == "Recently watched")
    }

    @Test func theGuideIsFetchedInChunksAroundWhatShows() {
        let chunk = LiveTvRepository.guideChunk
        let now: Int64 = 1_750_000_000
        let start = LiveTvRules.guideStart(now)
        #expect(start % LiveTvRules.slot == 0 && start <= now - LiveTvRules.slot && start > now - 2 * LiveTvRules.slot)
        let chunks = LiveTvRules.chunks(start: start, from: start, to: start + 2 * 3600)
        // What shows, and the one after; none before the guide starts.
        #expect(chunks.first == start / chunk * chunk)
        #expect(chunks.allSatisfy { $0 % chunk == 0 })
        #expect(chunks.contains((start + 2 * 3600) / chunk * chunk + chunk))
        // None past its span.
        let late = LiveTvRules.chunks(start: start, from: start + LiveTvRules.guideSpan - 3600,
                                      to: start + LiveTvRules.guideSpan + 3600)
        #expect(late.allSatisfy { $0 < start + LiveTvRules.guideSpan })
    }

    @Test func guidesMergeOneProgrammePerStart() {
        let a = LiveProgram(channelId: 1, start: 0, stop: 1800, title: "A")
        let b = LiveProgram(channelId: 1, start: 1800, stop: 3600, title: "B")
        let first = LiveGuide(from: 0, to: 3600, programs: [1: [b, a]])
        let second = LiveGuide(from: 0, to: 3600, programs: [1: [a], 2: [b]])
        let merged = LiveTvRules.merge([first, second])
        #expect(merged[1]?.map(\.title) == ["A", "B"])
        #expect(merged[2]?.count == 1)
        #expect(LiveTvRules.between(merged[1], from: 1800, to: 2000).map(\.title) == ["B"])
    }

    @Test func theLayoutPlacesProgrammesByTime() {
        let layout = GuideLayout(start: 1000, pointsPerMinute: 6)
        #expect(layout.x(1000 + 1800) == 180)
        #expect(layout.time(atX: 180) == 2800)
        let program = LiveProgram(channelId: 1, start: 400, stop: 1000 + 600, title: "Started before")
        let frame = layout.frame(program)
        #expect(frame.x == 0 && frame.width == 60)
        #expect(GuideLayout.pointsPerMinute(textScale: 4) == 15)
        #expect(GuideLayout.pointsPerMinute(textScale: 1, tv: true) == 12)
        #expect(layout.slots.count == Int(LiveTvRules.guideSpan / LiveTvRules.slot))
    }
}

@MainActor
@Suite struct LiveTvScreensTests {
    @Test func pagesComeAsTheListScrollsAndTenAtMostAreKept() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setLiveLineup(count: 2_500)
        let live = try await liveRepository(server)
        let paging = ChannelPaging(live: live, maxPages: 10)
        paging.show(.all)
        #expect(await eventually { paging.pages.total == 2_500 && paging.pages.pages[0] != nil })
        #expect(paging.pages[0]?.name == "Channel 0001")
        #expect(paging.pages[150] == nil)
        paging.showingRows(first: 150, last: 160)
        #expect(await eventually { paging.pages[150]?.name == "Channel 0151" })
        // Scrolled a long way: the pages furthest from the rows in view go.
        for first in stride(from: 200, through: 2_400, by: 100) {
            paging.showingRows(first: first, last: first + 10)
            #expect(await eventually { paging.pages.pages[first / 100] != nil })
        }
        #expect(paging.pages.pages.count <= 10)
        #expect(paging.pages.pages[24] != nil && paging.pages.pages[1] == nil)
        // Another category starts again.
        paging.show(.group("UK| Sport"))
        #expect(await eventually { paging.pages.total == 833 })
    }

    @Test func aFavouriteShowsInTheListAtOnceAndGoesBackWhenRefused() async throws {
        let server = try await started()
        defer { server.stop() }
        let live = try await liveRepository(server)
        let paging = ChannelPaging(live: live)
        paging.show(.all)
        #expect(await eventually { paging.pages[0] != nil })
        #expect(paging.pages[0]?.favorite == false)
        server.refuseLiveFavorite(status: 500)
        let saving = Task { try? await live.setFavorite(1, true) }
        #expect(await eventually { paging.pages[0]?.favorite == true })
        await saving.value
        #expect(await eventually { paging.pages[0]?.favorite == false })
    }

    @Test func theListOpensOnTheChannelWatchedLastInTheCategoryKept() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setLiveLineup(count: 600)
        let live = try await liveRepository(server)
        let settings = freshSettings()
        settings.keepLiveView(origin: server.origin, profileId: 1, category: "group:UK| Sport", view: "channels",
                              channel: 1001 + 3 * 120 + 1)
        let model = LiveTvModel(live: live, settings: settings, origin: server.origin, profileId: 1)
        #expect(model.category == .group("UK| Sport") && model.view == .channels)
        model.start()
        #expect(await eventually { model.status == .ready && model.landing != nil })
        #expect(model.landing == 120)
        model.landed()
        #expect(model.landing == nil)
        // Kept per server and profile.
        model.show(.favorites)
        model.show(.guide)
        let kept = settings.liveView(origin: server.origin, profileId: 1)
        #expect(kept.category == "favorites" && kept.view == "guide")
        #expect(settings.liveView(origin: server.origin, profileId: 2).category == nil)
        // A group gone opens All channels.
        settings.keepLiveView(origin: server.origin, profileId: 3, category: "group:Gone")
        let gone = LiveTvModel(live: live, settings: settings, origin: server.origin, profileId: 3)
        gone.start()
        #expect(await eventually { gone.status == .ready })
        #expect(gone.category == .all)
    }

    @Test func aProfileWithoutLiveTVIsToldSo() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setLiveAllowed(false)
        let live = try await liveRepository(server)
        let model = LiveTvModel(live: live, settings: freshSettings(), origin: server.origin, profileId: 1)
        model.start()
        #expect(await eventually { model.status == .notAllowed })
    }

    @Test func theGuideOfTheChannelsLoadedComesInChunks() async throws {
        let server = try await started()
        defer { server.stop() }
        let live = try await liveRepository(server)
        let model = LiveTvModel(live: live, settings: freshSettings(), origin: server.origin, profileId: 1)
        model.start()
        #expect(await eventually { !model.programs.isEmpty })
        let news = try #require(model.paging.pages[0])
        let shown = model.programs(of: news, from: model.guideStart, to: model.guideStart + 7200)
        #expect(!shown.isEmpty && shown.allSatisfy { $0.channelId == news.id })
        // Apple TV's window stays in the guide.
        model.moveWindow(by: -3)
        #expect(model.window == model.guideStart)
        model.moveWindow(by: 2)
        #expect(model.window == model.guideStart + 3600)
        model.moveWindow(by: 1_000)
        #expect(model.window == model.guideStart + LiveTvRules.guideSpan - LiveTvModel.windowSpan)
    }

    @Test func liveTVShowsInTheShellOnceTheServerSaysSo() async throws {
        let server = try await started()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Fixture", lastUsed: Date())], tv: true)
        await app.start()
        #expect(await eventually { app.shell.showsLiveTv })
        #expect(app.shell.sections.contains(.liveTv))
        // The lineup changed and Live TV went for this profile.
        server.setLiveAllowed(false)
        app.serverEvents.heard(.liveChannelsChanged)
        #expect(await eventually { !app.shell.sections.contains(.liveTv) })
    }
}

@MainActor
@Suite struct LivePlayerTests {
    private func player(_ server: FixtureServer, category: LiveCategory = .all, channel: Int = 1001,
                        wait: Duration = .milliseconds(100)) async throws -> (LivePlayerModel, LiveTvRepository) {
        let live = try await liveRepository(server)
        let playback = LivePlayback(live: live, clientId: "apple-test", engines: { QuietLiveEngine() })
        let model = LivePlayerModel(request: LiveRequest(channelId: channel, category: category), live: live,
                                    playback: playback, numberWait: wait)
        return (model, live)
    }

    @Test func channelUpAndDownCrossPagesAndGoRoundTheEnds() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setLiveLineup(count: 250)
        let (model, _) = try await player(server, channel: 1001 + 99)
        defer { model.close() }
        // The last of the first page: up is the first of the second.
        #expect(try await model.neighbour(of: 1100, by: 1)?.id == 1101)
        #expect(try await model.neighbour(of: 1101, by: -1)?.id == 1100)
        // Round the ends.
        #expect(try await model.neighbour(of: 1250, by: 1)?.id == 1001)
        #expect(try await model.neighbour(of: 1001, by: -1)?.id == 1250)
        model.zap(by: 1)
        #expect(await eventually { model.channelId == 1101 })
        // In a group: its channels only.
        model.show(.group("FR| Info"))
        #expect(try await model.neighbour(of: 1003, by: 1)?.id == 1006)
    }

    @Test func recentlyWatchedKeepsTheOrderItOpenedWith() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setLiveLineup(count: 5)
        server.setLiveRecent([1003, 1001, 1005])
        let (model, _) = try await player(server, category: .recent, channel: 1003)
        defer { model.close() }
        #expect(try await model.neighbour(of: 1003, by: 1)?.id == 1001)
        // Watching reorders the server's list; the player's stays.
        #expect(await eventually { server.liveSessions.values.contains(1003) })
        server.setLiveRecent([1001, 1003, 1005])
        #expect(try await model.neighbour(of: 1001, by: 1)?.id == 1005)
        #expect(try await model.neighbour(of: 1005, by: 1)?.id == 1003)
    }

    @Test func aNumberTunesAWhileAfterItsLastDigitOrSaysThereIsNone() async throws {
        let server = try await started()
        defer { server.stop() }
        server.setLiveLineup(count: 150)
        let (model, _) = try await player(server, category: .group("UK| News"))
        defer { model.close() }
        model.type(1)
        model.type(2)
        #expect(model.digits == "12")
        model.deleteDigit()
        model.type(0)
        model.type(4)
        #expect(model.digits == "104")
        #expect(await eventually { model.channelId == 1001 + 103 })
        #expect(model.digits.isEmpty)
        // Not in UK| News: All channels.
        #expect(model.category == .all)
        for _ in 0..<6 { model.type(9) }
        #expect(model.digits == "99999")
        model.enterNumber()
        #expect(await eventually { model.notice == "No channel 99999" })
        #expect(model.channelId == 1104)
    }

    @Test func everyConnectionInUseOffersToWatchHereInstead() async throws {
        let server = try await started()
        defer { server.stop() }
        server.holdLiveConnection(2)
        let (model, _) = try await player(server, channel: 1)
        defer { model.close() }
        #expect(await eventually {
            if case .failed(let trouble) = model.state.phase { return trouble.kind == .busy }
            return false
        })
        guard case .failed(let trouble) = model.state.phase else { return }
        let busy = try #require(trouble.connections.first { $0.canTakeOver })
        model.takeOver(busy.channelId)
        #expect(await eventually { server.liveSessions.values.sorted() == [1] })
    }

    @Test func theBackgroundLetsTheChannelGoAndTheForegroundTunesItAgain() async throws {
        let server = try await started()
        defer { server.stop() }
        let (model, _) = try await player(server, channel: 1)
        #expect(await eventually { server.liveSessions.values.sorted() == [1] })
        model.sceneActive(false)
        #expect(await eventually { server.liveSessions.isEmpty })
        model.sceneActive(true)
        #expect(await eventually { server.liveSessions.values.sorted() == [1] })
        model.close()
        #expect(await eventually { server.liveSessions.isEmpty })
    }

    @Test func liveAndVideoAreNeverOpenTogether() async throws {
        let server = try await started()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Fixture", lastUsed: Date())])
        await app.start()
        let live = await app.watchLive(LiveRequest(channelId: 1, category: .all))
        #expect(live != nil && app.livePlayer != nil)
        #expect(await eventually { server.liveSessions.values.sorted() == [1] })
        await app.play(PlayRequest(itemId: 101))
        #expect(app.livePlayer == nil)
        #expect(await eventually { server.liveSessions.isEmpty })
        await app.closePlayer()
    }
}

/// A live engine that plays nothing and says nothing.
@MainActor
final class QuietLiveEngine: LiveEngine {
    let events: AsyncStream<LiveEngineEvent>
    private let continuation: AsyncStream<LiveEngineEvent>.Continuation

    init() {
        (events, continuation) = AsyncStream.makeStream(of: LiveEngineEvent.self)
    }

    func load(url: URL) { continuation.yield(.playing) }
    func play() {}
    func hasVideo() -> Bool { true }
    func close() { continuation.finish() }
}
