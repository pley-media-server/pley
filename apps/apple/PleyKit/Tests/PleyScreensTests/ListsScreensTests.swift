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

/// A signed-in app on `server`, at Home.
@MainActor
private func signedIn(_ server: FixtureServer) async throws -> AppModel {
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
    await app.start()
    try #require(app.root == .signedIn)
    return app
}

/// A clock for the undo's time: removals wait until the test lets them go.
private actor Gate {
    private var waiting: [CheckedContinuation<Void, Never>] = []
    private var count = 0

    func wait() async {
        await withCheckedContinuation { waiting.append($0) }
    }

    /// Lets everything waiting go on.
    func open() {
        let all = waiting
        waiting = []
        all.forEach { $0.resume() }
    }

    var waiters: Int { waiting.count }
}

private func item(_ kind: ItemKind, _ title: String, year: Int? = nil, show: String? = nil, parentTitle: String? = nil,
                  season: Int? = nil, index: Int? = nil) -> ItemSummary {
    var item = ItemSummary(id: 1, libraryId: 1, kind: kind, title: title)
    item.year = year
    item.grandparentTitle = show
    item.parentTitle = parentTitle
    item.parentIndex = season
    item.index = index
    return item
}

/// Android's `SmartFilterTest` (the web's smartFilter.test.ts).
@Suite struct SmartFilterTests {
    @Test func theLibraryPagesFiltersInPlexsSyntax() {
        #expect(SmartFilter.of(LibraryParams(sort: .year, order: .descending, watched: false, genre: "Science Fiction"))
            == "genre=Science+Fiction&unwatched=1&sort=year%3Adesc")
        #expect(SmartFilter.of(LibraryParams(sort: .title, order: .ascending, watched: true, year: 1999))
            == "year=1999&unwatched=0&sort=titleSort")
    }

    @Test func filtersInWords() {
        #expect(SmartFilter.describe("type=1&genre=Comedy&unwatched=1&sort=year%3Adesc")
            == ["Genre: Comedy", "Unwatched", "Newest first"])
        // As python-plexapi writes them.
        #expect(SmartFilter.describe("year%3E%3E=2019&sort=movie.addedAt:desc&includeGuids=1&limit=20")
            == ["After 2019", "Recently added first", "At most 20"])
        #expect(SmartFilter.describe("resolution=4k") == ["resolution: 4k"])
    }

    @Test func whatItSavesReadsBackAsTheSameWords() {
        let filter = SmartFilter.of(LibraryParams(sort: .added, watched: false, genre: "Drama", year: 2001))
        #expect(SmartFilter.describe(filter) == ["Genre: Drama", "From 2001", "Unwatched", "Recently added first"])
    }

    @Test func aCollectionsOrderInWords() {
        #expect(SmartFilter.sortNote(.release) == "By release date")
        #expect(SmartFilter.sortNote(.custom) == "Custom order")
        #expect(SmartFilter.sortNote(CollectionSort(rawValue: "")) == nil)
    }
}

@Suite struct PlaylistWordsTests {
    @Test func fullTitlesAsAddToPlaylistSaysThem() {
        #expect(Words.fullTitle(item(.episode, "Office Olympics", show: "The Office", season: 2, index: 3))
            == "The Office — S2 · E3 — Office Olympics")
        #expect(Words.fullTitle(item(.season, "", parentTitle: "The Office", index: 2)) == "The Office — Season 2")
        #expect(Words.fullTitle(item(.track, "One More Time", show: "Daft Punk")) == "One More Time — Daft Punk")
        #expect(Words.fullTitle(item(.movie, "Heat", year: 1995)) == "Heat (1995)")
        #expect(Playlists.titleOf([item(.movie, "Heat"), item(.movie, "Ronin"), item(.movie, "Collateral")])
            == "3 items")
    }

    @Test func rowsCountsTitlesAndToasts() {
        let row = Playlists.rowTitles(item(.episode, "Glasshouse", show: "Severance", season: 2, index: 5))
        #expect(row.title == "Glasshouse" && row.subtitle == "Severance · S2 · E5")
        #expect(Words.playlistMeta(count: 12, durationMs: 11_520_000) == "12 items · 3h 12m")
        #expect(Words.playlistMeta(count: 1, durationMs: 0) == "1 item")
        #expect(Playlists.added(0, to: "Weekend") == "Nothing to add")
        #expect(Playlists.added(1, to: "Weekend") == "Added to Weekend")
        #expect(Playlists.added(3, to: "Weekend") == "Added 3 items to Weekend")
        #expect(Playlists.check("  ") != nil)
        #expect(Playlists.check(String(repeating: "a", count: 201)) != nil)
        #expect(Playlists.check(" Road trip ") == nil)
        #expect(Playlists.moved([1, 2, 3], from: 2, to: 0) == [3, 1, 2])
        #expect(Playlists.kind(of: .album) == .audio && Playlists.kind(of: .show) == .video)
    }

    @Test func theFixturesRead() throws {
        let collection = try PleyJSON.decoder.decode(CollectionDetail.self, from: Fixtures.data("collections/detail.json"))
        #expect(collection.collection.title == "Dune Collection" && collection.collection.automatic)
        #expect(collection.collection.sort == .release && collection.items.map(\.id) == [105, 104])
        let smart = try PleyJSON.decoder.decode(PlaylistDetail.self, from: Fixtures.data("playlists/smart_detail.json"))
        #expect(smart.playlist.smart && smart.playlist.libraryId == 1 && smart.entries.count == 2)
        let lists = try PleyJSON.decoder.decode(PlaylistList.self, from: Fixtures.data("playlists/list.json"))
        #expect(lists.playlists.map(\.kind) == [.audio, .video, .video])
        #expect(PlaylistCard(lists.playlists[2]).meta == "3 items · 3h 47m")
    }
}

@Suite("Collections")
@MainActor
struct CollectionScreensTests {
    @Test func aLibrarysCollectionsAndItsPage() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let library = LibraryModel(app: app, libraryId: 1, view: "tab=collections")
        await library.load()
        #expect(library.tabs == [.recommended, .library, .collections])
        #expect(library.params.tab == .collections)
        #expect(library.collections.cards.map(\.title) == ["Dune Collection", "Unwatched Sci-Fi", "Villeneuve Picks"])
        let dune = library.collections.cards[0]
        #expect(dune.count == "2" && dune.automatic && !dune.smart)
        #expect(dune.spokenLabel == "Dune Collection, collection, 2 movies")

        let page = CollectionModel(app: app, collectionId: 803)
        await page.load()
        #expect(page.state == .loaded && page.smart && page.movies)
        #expect(page.notes == ["Genre: Science Fiction", "Unwatched", "Newest first"])
        #expect(page.count == "2 movies" && page.canShuffle)
        // The poster falls back to an item's through the server's 307.
        let api = try #require(await app.session.api)
        let bytes = try await api.image(path: page.poster[0], width: 200)
        #expect(!bytes.isEmpty)

        let gone = CollectionModel(app: app, collectionId: 999)
        await gone.load()
        #expect(gone.state == .notFound)
    }

    @Test func moviesPlayAsAQueueAndShuffleStartsAnywhere() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let page = CollectionModel(app: app, collectionId: 802)
        await page.load()
        #expect(page.notes == ["Custom order"])
        let play = try #require(await page.play(shuffle: false))
        #expect(play.itemId == 101)
        #expect(play.queue == PlayQueue(kind: .collection, id: 802, itemIds: [101, 105, 104], position: 0))
        page.random = { _ in 2 }
        let shuffled = try #require(await page.play(shuffle: true))
        #expect(shuffled.itemId == 104 && shuffled.queue?.position == 2 && shuffled.queue?.itemIds == [101, 105, 104])
    }

    @Test func showsPlayTheirNextEpisode() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addCollection(id: 810, libraryId: 2, title: "Office Shows", itemIds: [201])
        let app = try await signedIn(server)
        let page = CollectionModel(app: app, collectionId: 810)
        await page.load()
        #expect(!page.movies && page.count == "1 show" && !page.canShuffle)
        let play = try #require(await page.play(shuffle: false))
        // Severance's on-deck episode, by itself.
        #expect(play.itemId == 204 && play.queue == nil)
        page.random = { _ in 0 }
        #expect(await page.play(shuffle: true)?.itemId == 204)
    }
}

@Suite("Playlists")
@MainActor
struct PlaylistScreensTests {
    @Test func theProfilesPlaylistsByTitle() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = PlaylistsModel(app: app)
        await model.load()
        #expect(model.cards.map(\.title) == ["Focus", "Unwatched Sci-Fi Night", "Weekend"])
        #expect(model.cards.map(\.smart) == [false, true, false])
        #expect(model.cards[0].video == false)
        model.new()
        #expect(app.shell.sheet == .newPlaylist([]))
    }

    @Test func aPlaylistsPageAndItsQueue() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = PlaylistModel(app: app, playlistId: 901)
        await model.load()
        #expect(model.title == "Weekend" && model.editable && model.video)
        #expect(model.meta == "3 items · 3h 47m")
        #expect(model.rows.map(\.number) == [1, 2, 3])
        #expect(model.rows[1].title == "Good News About Hell")
        #expect(model.rows[1].subtitle == "Severance · S1 · E1")
        #expect(model.rows[1].duration == "57m")
        #expect(model.rows[0].tile.shape == .landscape)
        let play = try #require(model.play())
        #expect(play.itemId == 101)
        #expect(play.queue == PlayQueue(kind: .playlist, id: 901, itemIds: [101, 203, 204], position: 0))
        model.random = { _ in 1 }
        #expect(model.play(shuffle: true)?.queue?.position == 1)
        #expect(model.play(from: 2)?.itemId == 204)
    }

    @Test func movesShowAtOnceAndOnlyTheLatestOrderFollowsTheOneOnItsWay() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = PlaylistModel(app: app, playlistId: 901)
        await model.load()
        model.move(from: 2, to: 0)
        #expect(model.rows.map(\.entryId) == [9103, 9101, 9102])
        model.move(9101, by: 1)
        model.move(fromOffsets: IndexSet(integer: 0), toOffset: 3)
        #expect(model.rows.map(\.entryId) == [9102, 9101, 9103])
        #expect(await eventually { model.order == nil && server.playlistOrders.count == 2 })
        // The first, then only the last of the two made while it was on its way.
        #expect(server.playlistOrders == [[9103, 9101, 9102], [9102, 9101, 9103]])
        #expect(server.playlistEntries(901)?.map(\.entry) == [9102, 9101, 9103])
        #expect(model.rows.map(\.entryId) == [9102, 9101, 9103])
    }

    @Test func aRefusedMoveGoesBackWithAToast() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.refusePlaylistOrder(status: 400)
        let app = try await signedIn(server)
        let model = PlaylistModel(app: app, playlistId: 901)
        await model.load()
        model.move(from: 0, to: 2)
        #expect(model.rows.map(\.entryId) == [9102, 9103, 9101])
        #expect(await eventually { model.order == nil })
        #expect(model.rows.map(\.entryId) == [9101, 9102, 9103])
        #expect(app.shell.toast?.text.hasPrefix("Couldn't move it") == true)
    }

    @Test func aRemovalWaitsForItsUndoBeforeItIsSent() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = PlaylistModel(app: app, playlistId: 901)
        await model.load()
        let gate = Gate()
        model.waitForUndo = { await gate.wait() }

        // Undone: it comes back, and nothing is sent even once the time is up.
        model.remove(9101)
        #expect(model.rows.map(\.entryId) == [9102, 9103])
        #expect(model.meta == "2 items · 1h 51m")
        #expect(app.shell.toast?.text == "Removed Arrival")
        #expect(app.shell.toast?.undo != nil)
        app.shell.undoToast()
        #expect(model.rows.map(\.entryId) == [9101, 9102, 9103])
        await gate.open()
        try await Task.sleep(for: .milliseconds(200))
        #expect(server.playlistRemovals.isEmpty)

        // Not undone: sent once its time has passed, not before.
        model.remove(9102)
        #expect(await eventually { model.undoable(9102) })
        try await Task.sleep(for: .milliseconds(200))
        #expect(server.playlistRemovals.isEmpty)
        #expect(model.rows.map(\.entryId) == [9101, 9103])
        while await gate.waiters == 0 { try await Task.sleep(for: .milliseconds(10)) }
        await gate.open()
        #expect(await eventually { server.playlistRemovals == [9102] })
        #expect(await eventually { model.hidden.isEmpty })
        #expect(model.rows.map(\.entryId) == [9101, 9103])
        #expect(model.meta == "2 items · 2h 50m")
        #expect(server.playlistEntries(901)?.map(\.entry) == [9101, 9103])
    }

    @Test func anEntryMovedWhileARemovalCanBeUndoneKeepsItsPlace() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = PlaylistModel(app: app, playlistId: 901)
        await model.load()
        let gate = Gate()
        model.waitForUndo = { await gate.wait() }

        // The second goes, waiting for its Undo; the last moves to the top meanwhile.
        model.remove(9102)
        #expect(model.rows.map(\.entryId) == [9101, 9103])
        model.move(from: 1, to: 0)
        #expect(model.rows.map(\.entryId) == [9103, 9101])
        // The server gets every entry: the one waiting stays after the one it followed.
        #expect(await eventually { server.playlistOrders.count == 1 })
        #expect(server.playlistOrders == [[9103, 9101, 9102]])
        #expect(await eventually { model.order == nil })
        // Undone: it shows where it was, after Arrival.
        app.shell.undoToast()
        #expect(model.rows.map(\.entryId) == [9103, 9101, 9102])
        #expect(server.playlistEntries(901)?.map(\.entry) == [9103, 9101, 9102])
        await gate.open()
        try await Task.sleep(for: .milliseconds(100))
        #expect(server.playlistRemovals.isEmpty)
    }

    @Test func aRefusedRemovalComesBack() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.refusePlaylistRemoval(status: 403)
        let app = try await signedIn(server)
        let model = PlaylistModel(app: app, playlistId: 901)
        await model.load()
        model.waitForUndo = {}
        model.remove(9103)
        #expect(await eventually { app.shell.toast?.text.hasPrefix("Couldn't remove it") == true })
        #expect(model.rows.map(\.entryId) == [9101, 9102, 9103])
    }

    @Test func smartPlaylistsAreReadOnlyAndMusicPlaysInNowPlaying() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addMusic()
        let app = try await signedIn(server)
        let smart = PlaylistModel(app: app, playlistId: 902)
        await smart.load()
        #expect(smart.smart && !smart.editable)
        #expect(smart.notes == ["Genre: Science Fiction", "Unwatched"])
        smart.move(from: 0, to: 1)
        smart.remove(104)
        #expect(smart.rows.map(\.entryId) == [104, 101])
        #expect(smart.order == nil && app.shell.toast == nil)
        #expect(await smart.rename("Other") == false)
        #expect(server.playlistOrders.isEmpty && server.playlistRemovals.isEmpty)
        // A smart playlist still plays, as a queue.
        #expect(smart.play()?.queue?.itemIds == [104, 101])

        let music = PlaylistModel(app: app, playlistId: 903)
        await music.load()
        #expect(!music.video)
        #expect(music.rows.first?.tile.shape == .square)
        // Music plays in Now Playing, not the player.
        #expect(music.play() == nil)
        #expect(await eventually { app.music.state.track?.id == 303 })
        #expect(app.player == nil && app.shell.toast == nil)
        // A track's Go to Album opens its album there.
        let track = try #require(music.rows.first?.item)
        #expect(Screen.page(of: track) == .album(302, track: 303))
    }

    @Test func renameAndDelete() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.shell.select(.playlists)
        app.shell.open(.playlist(901))
        let lists = PlaylistsModel(app: app)
        await lists.load()
        let model = PlaylistModel(app: app, playlistId: 901)
        await model.load()
        #expect(await model.rename("  ") == false)
        #expect(await model.rename(" Long Weekend "))
        #expect(model.title == "Long Weekend" && server.playlistTitle(901) == "Long Weekend")
        // The list follows.
        #expect(await eventually { lists.cards.contains { $0.title == "Long Weekend" } })
        #expect(await model.delete())
        #expect(app.shell.path(for: .playlists).isEmpty)
        #expect(server.playlistEntries(901) == nil)
        #expect(await eventually { !lists.cards.contains { $0.id == 901 } })
    }
}

@Suite("Adding to playlists")
@MainActor
struct AddToPlaylistTests {
    @Test func anEpisodeGoesIntoAVideoPlaylist() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = AddToPlaylistModel(app: app, itemIds: [203])
        await model.load()
        #expect(model.title == "Severance — S1 · E1 — Good News About Hell")
        #expect(model.kind == .video)
        // Smart playlists fill themselves, and music takes no video.
        #expect(model.playlists?.map(\.title) == ["Weekend"])
        let weekend = try #require(model.playlists?.first)
        #expect(await model.add(to: weekend))
        #expect(app.shell.toast?.text == "Added to Weekend")
        #expect(server.playlistEntries(901)?.map(\.item) == [101, 203, 204, 203])
    }

    @Test func aShowAddsItsEpisodesAndATrackGoesToMusic() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let show = AddToPlaylistModel(app: app, itemIds: [201])
        await show.load()
        let weekend = try #require(show.playlists?.first)
        #expect(await show.add(to: weekend))
        #expect(app.shell.toast?.text == "Added 3 items to Weekend")

        let track = AddToPlaylistModel(app: app, itemIds: [303])
        await track.load()
        #expect(track.kind == .audio)
        #expect(track.playlists?.map(\.title) == ["Focus"])

        let two = AddToPlaylistModel(app: app, itemIds: [101, 203])
        await two.load()
        #expect(two.title == "2 items")
        // A new playlist named on the spot holds them.
        #expect(await two.create("") == false)
        #expect(await two.create(" Road Trip "))
        #expect(app.shell.toast?.text == "Added 2 items to Road Trip")
        let made = try #require(server.playlistIds["Road Trip"])
        #expect(server.playlistEntries(made)?.map(\.item) == [101, 203])
    }

    @Test func newPlaylistsAndSaveAs() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let empty = NewPlaylistModel(app: app, mode: .items([]))
        #expect(empty.asksKind && empty.filters.isEmpty)
        let made = try #require(await empty.create("Evening Music", kind: .audio))
        #expect(server.playlistEntries(made)?.isEmpty == true)
        let lists = try PleyJSON.decoder.decode(PlaylistDetail.self, from: Data(
            #"{"id":1,"title":"x","kind":"audio","entries":[]}"#.utf8))
        #expect(lists.playlist.kind == .audio)

        // Save as… from a library page: its filters and order, in Plex's syntax.
        let library = LibraryModel(app: app, libraryId: 1, view: "sort=year&watched=false&genre=Comedy")
        await library.load()
        library.saveAs()
        let query = "genre=Comedy&unwatched=1&sort=year%3Adesc"
        #expect(app.shell.sheet == .saveSmartPlaylist(libraryId: 1, query: query))
        let smart = NewPlaylistModel(app: app, mode: .smart(libraryId: 1, query: query))
        #expect(!smart.asksKind)
        #expect(smart.filters == "Genre: Comedy · Unwatched · Newest first")
        #expect(await smart.create("") == nil)
        let id = try #require(await smart.create("Funny Ones"))
        #expect(server.playlistFilter(id)?.filter == query && server.playlistFilter(id)?.library == 1)
    }
}
