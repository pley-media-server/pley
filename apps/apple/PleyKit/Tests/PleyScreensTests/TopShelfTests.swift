import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyPlayer
@testable import PleyScreens
import PleyShelf
import PleyTesting
import Testing

/// The pictures the shelf asked for, and how many times it was told of a change.
private actor Asked {
    var images: [(path: String, width: Int)] = []
    var changes = 0
    func image(_ path: String, _ width: Int) { images.append((path, width)) }
    func changed() { changes += 1 }
}

private let jpeg = Data([0xFF, 0xD8, 0xFF, 0xE0]) + Data(repeating: 7, count: 100)

private func shelfFolder() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("pley-shelf-\(UUID().uuidString)", isDirectory: true)
}

@MainActor
private func writer(_ asked: Asked, folder: URL = shelfFolder()) -> TopShelfWriter {
    TopShelfWriter(store: ShelfStore(directory: folder), images: { path, width in
        await asked.image(path, width)
        return jpeg
    }, changed: { Task { await asked.changed() } })
}

private func item(_ id: Int, kind: ItemKind = .movie, position: Int = 0) -> ItemSummary {
    var item = ItemSummary(id: id, libraryId: 1, kind: kind, title: "Title \(id)")
    item.durationMs = 100_000
    item.userState.positionMs = position
    item.backdropUrl = "/api/items/\(id)/images/backdrop?v=1"
    item.posterUrl = "/api/items/\(id)/images/poster?v=1"
    return item
}

@MainActor
struct TopShelfWriterTests {
    @Test func entriesSayWhatEachTitleIs() {
        var episode = item(204, kind: .episode, position: 25_000)
        episode.title = "Half Loop"
        episode.grandparentTitle = "Severance"
        episode.parentIndex = 1
        episode.index = 2
        episode.thumbUrl = "/api/items/204/images/thumb?v=1"
        var poster = item(9)
        poster.backdropUrl = nil
        let made = TopShelfWriter.entries([episode, item(101), poster])
        #expect(made[0].entry == ShelfEntry(itemId: 204, title: "Severance", subtitle: "S1 · E2", progress: 0.25,
                                            kind: .continue))
        #expect(made[0].path == "/api/items/204/images/thumb?v=1")
        // Not started: next up, with its backdrop.
        #expect(made[1].entry == ShelfEntry(itemId: 101, title: "Title 101", kind: .next))
        #expect(made[1].path == "/api/items/101/images/backdrop?v=1")
        #expect(made[2].entry.shape == .poster)
        #expect(made[2].path == "/api/items/9/images/poster?v=1")
    }

    @Test func tenTitlesAtMostWithTheirPicturesAt800() async throws {
        let asked = Asked()
        let shelf = writer(asked)
        let items = (1...14).map { item($0, position: 1_000) }
        shelf.show([Hub(key: "my_list", title: "My List", items: [item(99)]),
                    Hub(key: "continue_watching", title: "Continue Watching", items: items)], owner: "a-1")
        await shelf.idle()
        let saved = try #require(shelf.store.read())
        #expect(saved.owner == "a-1")
        #expect(saved.entries.map(\.itemId) == Array(1...10))
        #expect(saved.entries.allSatisfy { $0.image.map(shelf.store.hasImage) == true })
        #expect(await asked.images.count == 10)
        #expect(await asked.images.allSatisfy { $0.width == 800 })

        // The same row again: nothing read again, nothing written.
        shelf.show([Hub(key: "continue_watching", title: "Continue Watching", items: items)], owner: "a-1")
        await shelf.idle()
        #expect(await asked.images.count == 10)
    }

    @Test func anotherOwnerEmptiesTheShelfAndSigningOutToo() async throws {
        let asked = Asked()
        let folder = shelfFolder()
        let shelf = writer(asked, folder: folder)
        shelf.show([Hub(key: "continue_watching", title: "", items: [item(1, position: 5)])], owner: "a-1")
        await shelf.idle()
        #expect(shelf.shown?.owner == "a-1")

        shelf.owner(is: "a-1")
        #expect(shelf.shown != nil)
        shelf.owner(is: "a-2")
        await shelf.idle()
        #expect(shelf.shown == nil)
        #expect(ShelfStore(directory: folder).read() == nil)
        #expect(try FileManager.default.contentsOfDirectory(atPath: folder.path) == ["link.key"])

        shelf.show([Hub(key: "continue_watching", title: "", items: [item(2, position: 5)])], owner: "a-2")
        await shelf.idle()
        shelf.clear()
        await shelf.idle()
        #expect(ShelfStore(directory: folder).read() == nil)
        // The system was told of each change.
        #expect(await asked.changes >= 3)
    }

    @Test func aRowWrittenThenClearedStaysCleared() async throws {
        let folder = shelfFolder()
        let shelf = writer(Asked(), folder: folder)
        shelf.show([Hub(key: "continue_watching", title: "", items: [item(1, position: 5)])], owner: "a-1")
        shelf.clear()
        await shelf.idle()
        #expect(shelf.shown == nil)
        #expect(ShelfStore(directory: folder).read() == nil)
    }

    @Test func forgettingTheServerEmptiesItsRowOnly() async throws {
        let shelf = writer(Asked())
        let owner = ShelfSnapshot.owner(origin: "http://pley.local:7539", profileId: 4)
        shelf.show([Hub(key: "continue_watching", title: "", items: [item(1, position: 5)])], owner: owner)
        await shelf.idle()
        shelf.forget(origin: "http://other.local:7539")
        #expect(shelf.shown != nil)
        shelf.forget(origin: "http://pley.local:7539")
        await shelf.idle()
        #expect(shelf.shown == nil)
    }

    @Test func theKeyStaysTheSameAcrossLaunches() {
        let folder = shelfFolder()
        let first = writer(Asked(), folder: folder)
        let second = writer(Asked(), folder: folder)
        let link = ShelfLink(action: .open, itemId: 3, owner: "o")
        #expect(first.url(link) == second.url(link))
    }
}

@MainActor
struct ShelfLinkScreensTests {
    /// A TV app on `server` whose shelf is in `folder`.
    private func tvApp(_ server: FixtureServer, folder: URL, settings: AppSettings = freshSettings(),
                       store: (any TokenStore)? = nil) async throws -> AppModel {
        let tokens: any TokenStore
        if let store {
            tokens = store
        } else {
            tokens = MemoryTokenStore()
            try await keepSignIn(server, in: tokens)
        }
        let asked = Asked()
        return app(tokens, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                   settings: settings, tv: true, shelf: { _ in writer(asked, folder: folder) })
    }

    @Test func homeWritesContinueWatchingAndSigningOutEmptiesIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let folder = shelfFolder()
        let app = try await tvApp(server, folder: folder)
        await app.start()
        #expect(app.root == .signedIn)
        await HomeModel(app: app).reload()
        let shelf = try #require(app.shelf)
        await shelf.idle()
        let saved = try #require(ShelfStore(directory: folder).read())
        #expect(saved.owner == ShelfSnapshot.owner(origin: server.origin, profileId: 2))
        #expect(saved.entries.map(\.itemId) == [204, 101])
        #expect(saved.entries.first?.subtitle == "S1 · E2")

        await app.signOut()
        await shelf.idle()
        #expect(ShelfStore(directory: folder).read() == nil)
    }

    @Test func aSignedLinkOpensThePageOrPlays() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await tvApp(server, folder: shelfFolder())
        await app.start()
        let shelf = try #require(app.shelf)
        let owner = try #require(app.shelfOwner)

        // Unsigned, signed with another key, or another owner's: nothing.
        app.openShelfLink(try #require(URL(string: "app.pley.apple:/shelf/open/204?owner=\(owner)")))
        app.openShelfLink(ShelfLink(action: .open, itemId: 204, owner: owner)
            .url(key: .init(size: .bits256)))
        app.openShelfLink(try #require(shelf.url(ShelfLink(action: .open, itemId: 204, owner: owner + "9"))))
        try await Task.sleep(for: .milliseconds(100))
        #expect(app.shell.path(for: .home).isEmpty)

        app.shell.select(.search)
        app.openShelfLink(try #require(shelf.url(ShelfLink(action: .open, itemId: 204, owner: owner))))
        #expect(await eventually { app.shell.path(for: .home) == [.item(204)] })
        #expect(app.shell.section == .home)

        app.openShelfLink(try #require(shelf.url(ShelfLink(action: .play, itemId: 101, owner: owner))))
        #expect(await eventually { app.player?.state.itemId == 101 })
        await app.closePlayer()
    }

    @Test func aLinkWaitsThroughWhosWatchingForTheSameProfile() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let folder = shelfFolder()
        // A first start learns the household, so the next asks "Who's watching?".
        let first = try await tvApp(server, folder: folder, settings: settings)
        await first.start()
        await ProfilesModel(app: first).load()
        let owner = try #require(first.shelfOwner)
        let link = try #require(first.shelf?.url(ShelfLink(action: .open, itemId: 204, owner: owner)))

        let app = try await tvApp(server, folder: folder, settings: settings)
        // The link comes with the launch, before the app has opened the server.
        app.openShelfLink(link)
        await app.start()
        #expect(app.root == .profiles)
        try await Task.sleep(for: .milliseconds(100))
        #expect(app.shell.path(for: .home).isEmpty)
        let model = ProfilesModel(app: app)
        await model.load()
        await model.choose(try #require(model.profiles.first { $0.current }))
        #expect(app.root == .signedIn)
        #expect(await eventually { app.shell.path(for: .home) == [.item(204)] })
    }

    @Test func anotherProfileChosenDropsTheLink() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let folder = shelfFolder()
        let first = try await tvApp(server, folder: folder, settings: settings)
        await first.start()
        await ProfilesModel(app: first).load()
        let owner = try #require(first.shelfOwner)
        let link = try #require(first.shelf?.url(ShelfLink(action: .open, itemId: 204, owner: owner)))

        let app = try await tvApp(server, folder: folder, settings: settings)
        await app.start()
        #expect(app.root == .profiles)
        app.openShelfLink(link)
        let model = ProfilesModel(app: app)
        await model.load()
        await model.choose(try #require(model.profiles.first { $0.name == "Kids" }))
        #expect(app.root == .signedIn)
        #expect(app.user?.id == 4)
        try await Task.sleep(for: .milliseconds(200))
        #expect(app.shell.path(for: .home).isEmpty)
        // Choosing the profile again later doesn't bring the link back.
        app.showProfiles()
        await model.choose(try #require(model.profiles.first { $0.id == 2 }))
        try await Task.sleep(for: .milliseconds(200))
        #expect(app.shell.path(for: .home).isEmpty)
    }
}
