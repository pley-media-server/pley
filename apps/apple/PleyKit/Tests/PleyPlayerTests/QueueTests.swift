import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

/// Three 60 s movies, played through a collection or playlist: the second
/// is watched, with a resume point left over from before.
@MainActor
private func queueRig(position: Int = 0) async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    for (n, id) in [51, 52, 53].enumerated() {
        var movie = FixtureServer.Playable(itemId: id, fileId: id * 10, title: "Movie \(n + 1)", container: "mp4",
                                           durationMs: 60_000, positionMs: id == 52 ? 30_000 : 0)
        movie.watched = id == 52
        server.addPlayable(movie)
    }
    let queue = PlayQueue(kind: .playlist, id: 901, itemIds: [51, 52, 53], position: position)
    return try Rig(server: server, request: PlayRequest(itemId: queue.current ?? 51, queue: queue))
}

@MainActor
private func started(_ rig: Rig, loads: Int = 1) async throws {
    if loads == 1 { rig.controller.start() }
    try await until("the media loaded") { rig.engine.loads.count == loads }
    rig.engine.emit(.ready)
    try await until("it plays") { rig.controller.state.phase == .playing }
}

@Suite("Play queues")
struct PlayQueueTests {
    @Test func aQueueKnowsWhatComesNextAndWhereItEnds() {
        let queue = PlayQueue(kind: .collection, id: 801, itemIds: [105, 104], position: 0)
        #expect(queue.current == 105 && queue.nextId == 104)
        let second = try? #require(queue.advanced())
        #expect(second?.current == 104 && second?.position == 1)
        #expect(second?.nextId == nil && second?.advanced() == nil)
        // A position outside it is brought inside; a queue is 5,000 items at most.
        #expect(PlayQueue(kind: .playlist, id: 1, itemIds: [1, 2], position: 9).position == 1)
        #expect(PlayQueue(kind: .playlist, id: 1, itemIds: Array(1...6_000)).itemIds.count == PlayQueue.maxItems)
        // A movie has something next only through a queue that goes on.
        #expect(!NextUp.hasNext(.movie))
        #expect(NextUp.hasNext(.movie, queue: queue))
        #expect(!NextUp.hasNext(.episode, queue: second))
        #expect(NextUp.hasNext(.episode))
    }

    @Test func aRequestWithItsQueueSurvivesTheMacsWindow() throws {
        let request = PlayRequest(itemId: 104, queue: PlayQueue(kind: .collection, id: 801, itemIds: [105, 104],
                                                                position: 1))
        let data = try JSONEncoder().encode(request)
        #expect(try JSONDecoder().decode(PlayRequest.self, from: data) == request)
        // Requests from before queues still read.
        let old = try JSONDecoder().decode(PlayRequest.self, from: Data(#"{"itemId":5,"fromStart":false}"#.utf8))
        #expect(old.queue == nil && old.itemId == 5)
    }
}

@Suite("Play queues in the controller")
@MainActor
struct QueueControllerTests {
    @Test func theQueuesNextPlaysFromTheStartWhenWatchedThenTheQueueEnds() async throws {
        let rig = try await queueRig()
        defer { rig.cleanUp() }
        try await started(rig)
        rig.engine.playing(at: 59_000)
        rig.clock.advance(ms: 500)
        rig.controller.tick()
        rig.engine.emit(.ended)
        try await until("the card") { rig.controller.state.upNext != nil }
        let card = try #require(rig.controller.state.upNext)
        #expect(card.itemId == 52)
        #expect(card.title == "Movie 2")
        #expect(card.heading == "Up next")
        // Read fresh, by itself: no next episode is asked for.
        #expect(rig.server.requests("/api/items/52").count >= 1)
        #expect(rig.server.requests("/api/items/51/next").isEmpty)

        rig.controller.playNext()
        try await started(rig, loads: 2)
        #expect(rig.controller.state.itemId == 52)
        // Watched: from the start, not its old resume point.
        #expect(await rig.requests().last?.startMs == 0)
        #expect(rig.engine.loads.last?.startMs == 0)

        // Through the queue still: the third one follows.
        rig.engine.emit(.ended)
        try await until("the third's card") { rig.controller.state.upNext?.itemId == 53 }
        rig.controller.playNext()
        try await started(rig, loads: 3)
        #expect(rig.controller.state.itemId == 53)

        // The end of the queue: no card, and Play Next says nothing follows.
        rig.engine.emit(.ended)
        try await until("the end") { rig.controller.state.phase == .ended }
        #expect(rig.controller.state.upNext == nil)
        rig.controller.playNext()
        try await until("the notice") { rig.controller.state.notice != nil }
        #expect(rig.controller.state.notice == "Nothing plays next")
        #expect(rig.engine.loads.count == 3)
        await rig.controller.close()
    }

    @Test func shuffleStartsInsideTheQueueAndGoesOnFromThere() async throws {
        let rig = try await queueRig(position: 1)
        defer { rig.cleanUp() }
        try await started(rig)
        #expect(rig.controller.state.itemId == 52)
        rig.engine.emit(.ended)
        try await until("the card") { rig.controller.state.upNext != nil }
        #expect(rig.controller.state.upNext?.itemId == 53)
        await rig.controller.close()
    }
}
