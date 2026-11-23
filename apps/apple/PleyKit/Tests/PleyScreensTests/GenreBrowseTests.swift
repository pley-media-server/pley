import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
@testable import PleyScreens
import PleyTesting
import Testing

/// Search's "Browse by genre": a movie or show library's genres, each
/// opening that library filtered by it.
@MainActor
@Suite struct GenreBrowseTests {
    private func signedIn(_ server: FixtureServer) async throws -> AppModel {
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        try #require(app.root == .signedIn)
        await LibrariesModel(app: app).reload()
        return app
    }

    @Test func theFirstVideoLibrarysGenresShowAndOpenItFiltered() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = GenreBrowseModel(app: app)
        await model.load()
        // Music has no genres to offer.
        #expect(model.libraries.map(\.name) == ["Movies", "TV Shows"])
        #expect(model.libraryName == "Movies")
        #expect(model.genres == ["Action", "Adventure", "Drama", "Science Fiction", "Thriller"])
        #expect(model.screen(for: "Science Fiction") == .library(1, view: "genre=Science+Fiction"))
        #expect(LibraryParams.parse("genre=Science+Fiction").genre == "Science Fiction")
        // Another library picked stays picked when Search loads again.
        await model.choose(2)
        await model.load()
        #expect(model.libraryName == "TV Shows")
        #expect(model.screen(for: "Drama") == .library(2, view: "genre=Drama"))
        // Not one of the profile's video libraries: nothing changes.
        await model.choose(3)
        #expect(model.libraryId == 2)
    }

    @Test func aGenresColorIsItsOwnWhereverItShows() {
        #expect(GenreBrowseModel.tint("Drama") == GenreBrowseModel.tint("drama"))
        #expect((0..<GenreBrowseModel.tints).contains(GenreBrowseModel.tint("Science Fiction")))
        let tints = Set(["Action", "Adventure", "Comedy", "Drama", "Horror", "Romance", "Thriller", "Western"]
            .map(GenreBrowseModel.tint))
        #expect(tints.count > 3)
    }
}
