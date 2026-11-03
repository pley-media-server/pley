import Foundation
import os
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting
import Testing

private func api(_ server: FixtureServer) -> PleyAPI {
    let http = transport()
    return PleyAPI(client: APIClient(origin: server.origin, tokens: FakeTokens(server: server, transport: http),
                                     transport: http))
}

/// Events read, kept for the test.
private final class Heard: @unchecked Sendable {
    // Unchecked Sendable: behind the lock.
    private let lock = OSAllocatedUnfairLock(initialState: [ServerEvent]())
    func add(_ event: ServerEvent) { lock.withLock { $0.append(event) } }
    var all: [ServerEvent] { lock.withLock { $0 } }
}

/// Waits until `condition` holds, for at most 5 seconds.
private func eventually(_ condition: () -> Bool) async -> Bool {
    for _ in 0..<500 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return condition()
}

@Suite struct BrowseAPITests {
    @Test func aListingIsAskedAgainWithItsTag() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addLibrary(id: 9, name: "Big", count: 250)
        let api = api(server)
        let query = LibraryQuery(sort: "title", order: "asc")
        let first = try #require(try await api.libraryItems(9, query, offset: 100, limit: 100))
        #expect(first.page.total == 250)
        #expect(first.page.items.count == 100)
        #expect(first.page.items.first?.title == "K 00101")
        let tag = try #require(first.tag)
        // Unchanged: nothing comes back.
        #expect(try await api.libraryItems(9, query, offset: 100, limit: 100, tag: tag) == nil)
        let asked = try #require(server.requests("/api/libraries/9/items").last)
        #expect(asked.query["offset"] == "100" && asked.query["limit"] == "100" && asked.query["sort"] == "title")
        // A mark changes it.
        try await api.setWatched(900_100, true)
        #expect(try await api.libraryItems(9, query, offset: 100, limit: 100, tag: tag) != nil)
    }

    @Test func theProfilesOrderIsSavedAndForgotten() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = api(server)
        #expect(try await api.libraries().map(\.id) == [1, 2, 3])
        #expect(try await api.setLibraryOrder([3, 1, 2]).map(\.id) == [3, 1, 2])
        #expect(try await api.libraries().map(\.id) == [3, 1, 2])
        await #expect(throws: PleyError.self) { try await api.setLibraryOrder([3, 1]) }
        #expect(try await api.resetLibraryOrder().map(\.id) == [1, 2, 3])
    }

    @Test func marksAndMyList() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let api = api(server)
        try await api.setWatched(101, true)
        #expect(server.watched(101) == true)
        try await api.setWatched(101, false)
        #expect(server.watched(101) == false)
        try await api.setInWatchlist(101, true)
        #expect(try await api.watchlist().map(\.id).contains(101))
        try await api.setInWatchlist(101, false)
        #expect(!server.watchlistIds.contains(101))
        try await api.removeFromContinueWatching(202)
        #expect(server.hiddenFromContinueWatching == [202])
        #expect(!(try await api.home().hubs.first?.items.map(\.id) ?? []).contains(202))
    }
}

@Suite struct EventParserTests {
    @Test func eventsArriveInPiecesAndEndAtABlankLine() {
        let heard = Heard()
        let parser = EventParser { heard.add($0) }
        #expect(parser.feed(Data(": keep-alive\n\ndata: {\"type\":\"libra".utf8)))
        #expect(heard.all.isEmpty)
        #expect(parser.feed(Data("ries_changed\"}\r\n\r\nevent: x\ndata: {\"type\":\"library_changed\",".utf8)))
        #expect(heard.all == [.librariesChanged])
        // Several data lines are one event, joined by a newline (JSON allows it).
        #expect(parser.feed(Data("\ndata: \"library_id\": 4}\n\n".utf8)))
        #expect(heard.all == [.librariesChanged, .libraryChanged(libraryId: 4)])
        // Unknown types and unreadable data are skipped.
        #expect(parser.feed(Data("data: {\"type\":\"new_thing\"}\n\ndata: nonsense\n\n".utf8)))
        #expect(heard.all.count == 2)
    }

    @Test func aLineTooLongStopsTheReading() {
        let heard = Heard()
        let parser = EventParser(maxLine: 64) { heard.add($0) }
        #expect(parser.feed(Data("data: {\"type\":\"watchlist_changed\"}\n\n".utf8)))
        #expect(!parser.feed(Data(("data: " + String(repeating: "x", count: 70)).utf8)))
        #expect(parser.overflowed)
        #expect(!parser.feed(Data("\n\ndata: {\"type\":\"watchlist_changed\"}\n\n".utf8)))
        #expect(heard.all == [.watchlistChanged])
        // The bound is 64 KiB.
        #expect(EventStream.maxLine == 65_536)
    }
}

@Suite struct EventStreamTests {
    /// The waits asked for, in seconds, each shortened to 10 ms.
    private final class Waits: @unchecked Sendable {
        private let lock = OSAllocatedUnfairLock(initialState: [Double]())
        func add(_ s: Double) { lock.withLock { $0.append(s) } }
        var all: [Double] { lock.withLock { $0 } }
    }

    @Test func eventsComeAndTheStreamConnectsAgain() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let waits = Waits()
        let stream = EventStream(api: api(server)) { seconds in
            waits.add(seconds)
            try await Task.sleep(for: .milliseconds(10))
        }
        let heard = Heard()
        let connected = OSAllocatedUnfairLock(initialState: 0)
        let reader = Task {
            for await signal in stream.signals() {
                switch signal {
                case .connected: connected.withLock { $0 += 1 }
                case .event(let event): heard.add(event)
                }
            }
        }
        defer { reader.cancel() }
        #expect(await eventually { server.openEventStreams == 1 && connected.withLock { $0 } == 1 })
        server.pushEvent(#"{"type":"user_state_changed","item_ids":[202,203,201],"children":false}"#)
        #expect(await eventually { heard.all == [.userStateChanged(itemIds: [202, 203, 201], children: false)] })
        let asked = try #require(server.requests("/api/events").first)
        #expect(asked.headers["authorization"]?.hasPrefix("Bearer #") == true)
        #expect(asked.headers["accept"] == "text/event-stream")

        // The server ends it (a token that expired): it connects again after a second.
        server.endEvents()
        #expect(await eventually { server.eventConnects == 2 && connected.withLock { $0 } == 2 })
        #expect(waits.all == [1])

        // A line too long drops the connection, and it connects again.
        server.pushEventText("data: " + String(repeating: "x", count: EventStream.maxLine + 10))
        #expect(await eventually { server.eventConnects == 3 })
        server.pushEvent(#"{"type":"watchlist_changed"}"#)
        #expect(await eventually { heard.all.last == .watchlistChanged })
    }

    @Test func failuresWaitLongerAndTooManyStreamsWaitHalfAMinute() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.refuseEvents(status: 503)
        let waits = Waits()
        let stream = EventStream(api: api(server)) { seconds in
            waits.add(seconds)
            try await Task.sleep(for: .milliseconds(5))
        }
        let reader = Task { for await _ in stream.signals() {} }
        defer { reader.cancel() }
        #expect(await eventually { waits.all.count >= 5 })
        #expect(Array(waits.all.prefix(5)) == [1, 2, 5, 15, 15])
        server.refuseEvents(status: 429)
        let before = waits.all.count
        #expect(await eventually { waits.all.count > before + 1 })
        #expect(waits.all.suffix(1) == [30])
    }

    @Test func aRefusedAppStopsForGood() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.refuseEveryToken()
        let stream = EventStream(api: api(server)) { _ in try await Task.sleep(for: .milliseconds(5)) }
        var signals = 0
        for await _ in stream.signals() { signals += 1 }
        #expect(signals == 0)
    }
}
