import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
@testable import PleyScreens
import PleyTesting
import Testing

/// The iPhone's Listen tab: Recently Played from the history, the
/// playlists, the first music library's artists and the music libraries.
@MainActor
@Suite struct ListenTests {
    private func track(_ id: Int, album: Int, artist: Int = 50, at time: Int) -> HistoryEntry {
        var item = ItemSummary(id: id, libraryId: 3, kind: .track, title: "Track \(id)")
        item.parentId = album
        item.parentTitle = "Album \(album)"
        item.grandparentId = artist
        item.grandparentTitle = "Artist \(artist)"
        return HistoryEntry(id: id, viewedAt: time, user: HistoryUser(id: 1, name: "Sam"), item: item)
    }

    @Test func recentlyPlayedIsTheAlbumsOfTheLatestTracksEachOnce() {
        var movie = ItemSummary(id: 9, libraryId: 1, kind: .movie, title: "A film")
        movie.parentId = 99
        let entries = [track(1, album: 10, at: 100), track(2, album: 11, at: 300), track(3, album: 10, at: 200),
                       HistoryEntry(id: 9, viewedAt: 400, user: HistoryUser(id: 1, name: "Sam"), item: movie)]
        let albums = ListenModel.recentAlbums(entries)
        // Newest play first, an album once, films left out.
        #expect(albums.map(\.id) == [11, 10])
        #expect(albums[0].kind == .album)
        #expect(albums[0].title == "Album 11")
        #expect(albums[0].posterUrl == "/api/items/11/images/poster")
        #expect(Tile(albums[0], shape: .square).subtitle == "Artist 50")
        let many = (1...30).map { track($0, album: 100 + $0, at: $0) }
        #expect(ListenModel.recentAlbums(many).count == ListenModel.rowLimit)
    }

    @Test func listenHoldsTheMusicLibrariesAndThePlaylists() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        try #require(app.root == .signedIn)
        await LibrariesModel(app: app).reload()
        let model = ListenModel(app: app)
        await model.load()
        #expect(model.state == .loaded)
        #expect(model.libraries.map(\.name) == ["Music"])
        #expect(!model.playlists.cards.isEmpty)
        #expect(!model.isEmpty)
    }

    @Test func musicLeavesTheLibraryPickerForListen() {
        let libraries = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 3, name: "Music", kind: .music)]
        #expect(LibraryPicker.entries(libraries: libraries, liveTv: nil).map(\.section) == [.library(1)])
        #expect(LibraryPicker.direct(LibraryPicker.entries(libraries: libraries, liveTv: nil)) == .library(1))
    }
}
