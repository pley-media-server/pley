import Foundation
import PleyModel
import PleyTesting
import Testing

private func decode<T: Decodable>(_ type: T.Type, fixture path: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Fixtures.data(path))
}

@Suite("Libraries and events decode as the Android app reads them")
struct LibrariesDecodingTests {
    @Test func theLibrariesList() throws {
        let libraries = try decode([Library].self, fixture: "libraries/list.json")
        #expect(libraries.map(\.name) == ["Movies", "TV Shows", "Music"])
        #expect(libraries.map(\.kind) == [.movies, .shows, .music])
        #expect(libraries[0].itemCount == 412)
        #expect(libraries[0].scannedAt == 1_761_300_000)
        #expect(!libraries[0].scanning)
        #expect(Words.libraryCount(libraries[0]) == "412 movies")
        #expect(Words.libraryCount(libraries[1]) == "37 shows")
        #expect(Words.libraryCount(libraries[2]) == "128 artists")
    }

    @Test func aLibraryBeingScanned() throws {
        let library = try decode(Library.self, fixture: "libraries/library.json")
        #expect(library.id == 2)
        #expect(library.scanning)
        #expect(library.scannedAt == nil)
        #expect(Words.libraryCount(library) == "0 shows")
    }

    @Test func aPageOfItemsAndTheFilters() throws {
        let page = try decode(LibraryItems.self, fixture: "libraries/items.json")
        #expect(page.total == 412)
        #expect(page.items.map(\.title) == ["Arrival", "Dune", "Dune: Part Two"])
        let bitrate = try decode(LibraryItems.self, fixture: "libraries/items_by_bitrate.json")
        #expect(bitrate.items[0].media?.bitrate == 6_617_000)
        #expect(Words.fileLine(bitrate.items[0].media) == "1080p · 6.6 Mbps · 50.4 GB")
        #expect(Words.fileLine(nil) == "")
        let filters = try decode(LibraryFilters.self, fixture: "libraries/filters.json")
        #expect(filters.genres.contains("Science Fiction"))
        #expect(filters.years.first == 2024)
        let hubs = try decode(HomeHubs.self, fixture: "libraries/hubs.json")
        #expect(hubs.hubs.first?.key == "continue_watching:1")
    }

    @Test func eachEventReadsByItsType() throws {
        func event(_ path: String) throws -> ServerEvent? { ServerEvent.parse(try Fixtures.data(path)) }
        #expect(try event("events/libraries_changed.json") == .librariesChanged)
        #expect(try event("events/library_changed.json") == .libraryChanged(libraryId: 2))
        #expect(try event("events/scan_started.json") == .scanStarted(libraryId: 1))
        #expect(try event("events/scan_progress.json")
            == .scanProgress(libraryId: 1, phase: "probing", processed: 118, total: 412))
        #expect(try event("events/scan_finished.json") == .scanFinished(libraryId: 1))
        #expect(try event("events/sessions_changed.json") == .sessionsChanged)
        // A newer server's type still reads, as unknown.
        #expect(try event("events/unknown_type.json") == .unknown("transcoder_restarted"))
    }

    @Test func theProfilesOwnEvents() {
        func event(_ json: String) -> ServerEvent? { ServerEvent.parse(Data(json.utf8)) }
        #expect(event(#"{"type":"user_state_changed","item_ids":[202,203,201],"children":false}"#)
            == .userStateChanged(itemIds: [202, 203, 201], children: false))
        #expect(event(#"{"type":"watchlist_changed"}"#) == .watchlistChanged)
        #expect(event(#"{"type":"session_stopped","session_id":"abc","message":null,"by_admin":true}"#)
            == .sessionStopped(sessionId: "abc", message: nil, byAdmin: true))
        #expect(event(#"{"type":"live_taken_over","session_id":"abc","channel_id":7,"message":"Someone else needed the connection."}"#)
            == .liveTakenOver(sessionId: "abc", message: "Someone else needed the connection."))
        // Missing what the type needs, or not an event at all.
        #expect(event(#"{"type":"library_changed"}"#) == nil)
        #expect(event("not json") == nil)
    }
}
