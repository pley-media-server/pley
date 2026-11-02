import Combine
import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
@testable import PleyScreens
import PleyTesting
import Testing

/// Downloads as the screens see them, the test saying where each is.
@MainActor
final class FakeDownloads: DownloadsService {
    let available = true
    var entries: [DownloadEntry] = [] { didSet { subject.send() } }
    var storage = DownloadStorage(usedBytes: 1_288_490_189, freeBytes: 21_474_836_480)
    private let subject = PassthroughSubject<Void, Never>()
    var changed: AnyPublisher<Void, Never> { subject.eraseToAnyPublisher() }
    private(set) var scope: DownloadScope?
    private(set) var asked: [[Int]] = []
    private(set) var removed: [Int] = []
    private(set) var forgotten: [String] = []
    private(set) var resumed = 0
    var refusal: (any Error)?
    func signedIn(_ scope: DownloadScope?) { self.scope = scope }
    var local: (any LocalMedia)? { nil }
    func download(_ itemIds: [Int]) async throws {
        if let refusal { throw refusal }
        asked.append(itemIds)
    }
    func remove(_ itemId: Int) { removed.append(itemId) }
    func retry(_ itemId: Int) {}
    func removeAll() { entries = [] }
    func forgetServer(_ origin: String) { forgotten.append(origin) }
    func resume() { resumed += 1 }
}

private func snapshotFolder() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("pley-screens-snapshots-\(UUID().uuidString)")
}

@MainActor
@Suite(.serialized) struct OfflineScreensTests {
    @Test func theAppStartsFromWhatItSavedWhenTheServerDoesntAnswer() async throws {
        let server = try await fixture()
        let folder = snapshotFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let saved = [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())]
        let settings = freshSettings()
        let snapshots = SnapshotStore(root: folder)
        let first = app(store, servers: saved, settings: settings, snapshots: snapshots)
        await first.start()
        try #require(first.root == .signedIn && !first.offline)
        let home = HomeModel(app: first)
        await home.reload()
        let rows = home.rows.map(\.key)
        #expect(!rows.isEmpty)
        // Saved off the main actor: there once written.
        for _ in 0..<200 {
            let home = await snapshots.read(origin: server.origin, profileId: 2, key: SnapshotKeys.home)
            let me = await snapshots.read(origin: server.origin, profileId: 2, key: SnapshotKeys.me)
            if home != nil && me != nil { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        server.stop()

        let downloads = FakeDownloads()
        let again = app(store, servers: saved, settings: settings, downloads: downloads, snapshots: snapshots)
        await again.start()
        #expect(again.root == .signedIn && again.offline)
        #expect(again.user?.name == "Alex" && again.notice == nil)
        #expect(downloads.scope?.profileId == 2 && downloads.scope?.origin == server.origin)
        let offlineHome = HomeModel(app: again)
        await offlineHome.reload()
        #expect(offlineHome.rows.map(\.key) == rows)
        // What wasn't saved says the server can't be reached.
        let person = PersonModel(app: again, personId: 9_999)
        await person.load()
        #expect(person.page == nil)
        await again.signOut()
    }

    @Test func withoutASavedProfileTheServersShowAsBefore() async throws {
        let server = try await fixture()
        let origin = server.origin
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        server.stop()
        let folder = snapshotFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let app = app(store, servers: [SavedServer(origin: origin, name: "Gone", lastUsed: Date())],
                      snapshots: SnapshotStore(root: folder))
        await app.start()
        #expect(app.root == .servers && !app.offline)
        #expect(app.notice?.hasPrefix("Gone: ") == true)
    }

    @Test func aScreenShowsWhatWasSavedWhenTheServerGoesAndTheAppReconnects() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let folder = snapshotFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let downloads = FakeDownloads()
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      downloads: downloads, snapshots: SnapshotStore(root: folder))
        await app.start()
        let playlists = PlaylistsModel(app: app)
        await playlists.load()
        let shown = playlists.cards.map(\.id)
        try await Task.sleep(for: .milliseconds(200))
        // The server is taken for gone: what was saved shows, and nothing is asked of it.
        app.wentOffline()
        #expect(app.offline)
        let before = server.requests("/api/playlists").count
        let again = PlaylistsModel(app: app)
        await again.load()
        #expect(again.cards.map(\.id) == shown)
        #expect(server.requests("/api/playlists").count == before)
        // It answers: online again, and the screens read it afresh.
        #expect(await app.reconnect())
        #expect(!app.offline && downloads.resumed == 1)
        #expect(await eventually { app.change?.everything == true })
    }

    @Test func signingOutAndOpeningAnotherServerIsOnline() async throws {
        let first = try await fixture()
        defer { first.stop() }
        let second = try await fixture()
        defer { second.stop() }
        let folder = snapshotFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = MemoryTokenStore()
        try await keepSignIn(first, in: store)
        try await keepSignIn(second, in: store)
        let app = app(store, servers: [SavedServer(origin: first.origin, name: "Living Room", lastUsed: Date())],
                      snapshots: SnapshotStore(root: folder))
        await app.start()
        try #require(app.root == .signedIn)
        app.wentOffline()
        #expect(app.offline)
        await app.signOut()
        #expect(!app.offline)
        try await app.open(ServerEntry(origin: second.origin, name: "Bedroom", id: nil, saved: false, found: true))
        #expect(app.root == .signedIn && !app.offline)
        // Its screens ask it, with no snapshot of its own to fall back on.
        let before = second.requests("/api/playlists").count
        let playlists = PlaylistsModel(app: app)
        await playlists.load()
        #expect(second.requests("/api/playlists").count == before + 1)
        #expect(!playlists.cards.isEmpty)
        await app.signOut()
    }

    @Test func forgettingAServerDeletesItsDownloadsAndSavedScreens() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let folder = snapshotFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let snapshots = SnapshotStore(root: folder)
        let downloads = FakeDownloads()
        let entry = ServerEntry(origin: server.origin, name: "Living Room", id: nil, saved: true, found: false)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      downloads: downloads, snapshots: snapshots)
        await app.start()
        await snapshots.write(origin: server.origin, profileId: 2, key: "home", data: Data("[]".utf8))
        await app.signOut()
        // Signing out keeps them.
        #expect(downloads.forgotten.isEmpty && downloads.scope == nil)
        try await app.forget(entry)
        #expect(downloads.forgotten == [server.origin])
        #expect(await snapshots.read(origin: server.origin, profileId: 2, key: "home") == nil)
    }
}

@MainActor
@Suite struct DownloadScreensTests {
    private func signedIn(_ server: FixtureServer, downloads: FakeDownloads) async throws -> AppModel {
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      downloads: downloads)
        await app.start()
        try #require(app.root == .signedIn)
        return app
    }

    @Test func aMoviesPageDownloadsItAndFollowsItsDownload() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let downloads = FakeDownloads()
        let app = try await signedIn(server, downloads: downloads)
        let model = ItemModel(app: app, itemId: 101)
        await model.load()
        #expect(model.downloadButton == .download && !model.canDownloadSeason)
        await model.download()
        #expect(downloads.asked == [[101]])
        #expect(app.shell.toast?.text == DownloadWords.started)
        downloads.entries = [DownloadEntry(itemId: 101, fileId: 1, kind: .movie, title: "Arrival",
                                           sizeBytes: 1_000, downloadedBytes: 400, state: .downloading)]
        #expect(model.downloadButton == .progress("Downloading · 40% of 1000 B", 0.4))
        downloads.entries[0].state = .done
        #expect(model.downloadButton == .downloaded)
        downloads.entries[0].state = .failed
        downloads.entries[0].error = "Not enough space on this device"
        #expect(model.downloadButton == .failed("Didn't download: Not enough space on this device"))
        model.removeDownload()
        #expect(downloads.removed == [101])
    }

    @Test func aShowsPageDownloadsTheSeasonShown() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let downloads = FakeDownloads()
        let app = try await signedIn(server, downloads: downloads)
        let model = ItemModel(app: app, itemId: 201)
        await model.load()
        #expect(model.downloadButton == nil && model.canDownloadSeason)
        await model.downloadSeason()
        let episodes = try #require(model.page?.seasons?.episodes).map(\.id)
        #expect(downloads.asked == [episodes])
    }

    @Test func wokenInTheBackgroundTheDownloadsGoOnAsTheProfileUsedLast() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let settings = freshSettings()
        settings.setLastProfile(2, origin: server.origin)
        let downloads = FakeDownloads()
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      settings: settings, downloads: downloads)
        let asked = server.journal.count
        // No window: nothing shows, and the server isn't asked.
        await app.wakeForDownloads()
        #expect(downloads.scope?.origin == server.origin && downloads.scope?.profileId == 2)
        #expect(app.root == .starting && app.user == nil)
        #expect(server.journal.count == asked)
        // A window opens later: the app starts as ever.
        await app.start()
        #expect(app.root == .signedIn && app.user?.id == 2)
        #expect(await eventually { downloads.scope?.profileId == 2 && downloads.scope?.origin == server.origin })
    }

    @Test func wokenWithNoSignInKeptTheDownloadsWait() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        settings.setLastProfile(2, origin: server.origin)
        let downloads = FakeDownloads()
        let app = app(servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      settings: settings, downloads: downloads)
        await app.wakeForDownloads()
        #expect(downloads.scope == nil && app.root == .starting)
    }

    @Test func appleTVHasNoDownloads() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        let model = ItemModel(app: app, itemId: 101)
        await model.load()
        #expect(model.downloadButton == nil && !model.canDownloadSeason)
        #expect(!DownloadsModel(app: app).available)
    }

    @Test func meDownloadsListsThemWithTheirWords() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let downloads = FakeDownloads()
        let app = try await signedIn(server, downloads: downloads)
        downloads.entries = [
            DownloadEntry(itemId: 204, fileId: 2, kind: .episode, title: "Glasshouse", showTitle: "Pley Test Show",
                          seasonIndex: 1, episodeIndex: 2, sizeBytes: 0, state: .queued, converting: 0.25),
            DownloadEntry(itemId: 205, fileId: 3, kind: .episode, title: "Next", showTitle: "Pley Test Show",
                          seasonIndex: 1, episodeIndex: 3, state: .waiting),
            DownloadEntry(itemId: 206, fileId: 4, kind: .episode, title: "Later", showTitle: "Pley Test Show",
                          seasonIndex: 1, episodeIndex: 4, state: .waiting, serverAway: true),
            DownloadEntry(itemId: 101, fileId: 1, kind: .movie, title: "Arrival", year: 2016,
                          sizeBytes: 1_288_490_189, state: .done),
        ]
        let model = DownloadsModel(app: app)
        #expect(await eventually { model.rows.count == 4 })
        #expect(model.rows.map(\.status) == ["Preparing on the server · 25%", "Waiting for Wi-Fi",
                                             "Waiting for your server", "1.2 GB"])
        #expect(model.rows[0].title == "Pley Test Show" && model.rows[0].subtitle == "S1 · E2 · Glasshouse")
        #expect(model.rows[3].subtitle == "2016" && model.rows[3].canPlay && !model.rows[0].canPlay)
        #expect(model.rows[3].accessibilityLabel == "Arrival, 2016, 1.2 GB")
        #expect(model.storage == "1.2 GB on this device · 20.0 GB free")
        model.mobileData = true
        #expect(app.settings.downloadOnMobileData && downloads.resumed >= 1)
        model.quality = .hd720
        #expect(app.settings.downloadQuality == .hd720)
        model.removeAll()
        #expect(await eventually { model.rows.isEmpty })
    }
}
