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

@MainActor
@Suite struct ShellModelTests {
    private func item(_ id: Int, _ kind: ItemKind, parent: Int? = nil) -> ItemSummary {
        var item = ItemSummary(id: id, libraryId: 1, kind: kind, title: "Item \(id)")
        item.parentId = parent
        return item
    }

    @Test func pagesOpenOnTheSectionShowingAndBackTakesOneOff() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        #expect(shell.section == .home)
        shell.open(.item(7))
        shell.open(item: item(8, .movie))
        #expect(shell.path(for: .home) == [.item(7), .item(8)])
        shell.select(.me)
        shell.open(.history)
        #expect(shell.path(for: .me) == [.history])
        #expect(shell.path(for: .home) == [.item(7), .item(8)])
        shell.select(.home)
        shell.back()
        #expect(shell.path(for: .home) == [.item(7)])
        // The navigation's own Back sets the path.
        shell.setPath([], for: .home)
        #expect(shell.path(for: .home).isEmpty)
    }

    @Test func choosingTheSectionShowingGoesBackToItsStart() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        shell.open(.item(1))
        shell.open(.item(2))
        shell.select(.me)
        #expect(shell.path(for: .home).count == 2)
        shell.select(.home)
        #expect(shell.path(for: .home).count == 2)
        shell.select(.home)
        #expect(shell.path(for: .home).isEmpty)
        // A section this device doesn't show isn't chosen.
        shell.select(.settings)
        #expect(shell.section == .home)
        shell.select(.playlists)
        #expect(shell.section == .playlists)
    }

    @Test func musicHasItsOwnPages() {
        #expect(Screen.page(of: item(1, .artist)) == .artist(1))
        #expect(Screen.page(of: item(2, .album)) == .album(2))
        #expect(Screen.page(of: item(3, .track, parent: 2)) == .album(2, track: 3))
        #expect(Screen.page(of: item(4, .track)) == .item(4))
        #expect(Screen.page(of: item(5, .episode)) == .item(5))
    }

    @Test func seeAllListsTheLibraryAsTheRowDoes() {
        #expect(ShellModel.seeAll(Hub(key: "recently_added:3", title: "Recently Added in Movies", libraryId: 3))
            == .library(3, view: "sort=added&order=desc"))
        #expect(ShellModel.seeAll(Hub(key: "top_rated:3", title: "Top Rated", libraryId: 3))
            == .library(3, view: "sort=rating&order=desc"))
        #expect(ShellModel.seeAll(Hub(key: "continue_watching", title: "Continue Watching")) == nil)
        #expect(ShellModel.seeAll(Hub(key: "my_list", title: "My List")) == nil)
    }

    @Test func theSectionShownLastIsKeptAndAResetGoesHome() {
        let settings = freshSettings()
        let shell = ShellModel(settings: settings, tv: false)
        shell.select(.me)
        shell.open(.history)
        #expect(ShellModel(settings: settings, tv: false).section == .me)
        // A section another device has isn't restored here.
        #expect(ShellModel(settings: settings, tv: true).section == .home)
        shell.sheet = .addToPlaylist([1])
        shell.show("Added to Favourites")
        shell.reset()
        #expect(shell.section == .home)
        #expect(shell.paths.isEmpty)
        #expect(shell.sheet == nil)
        #expect(shell.toast == nil)
        #expect(settings.lastSection == "home")
    }

    @Test func aLibraryShownLastComesBackOnceTheLibrariesAreRead() {
        let settings = freshSettings()
        let libraries = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows)]
        let first = ShellModel(settings: settings, tv: true)
        first.setLibraries(libraries)
        first.select(.library(2))
        // Home until they are read: no tab is a library's yet.
        let shell = ShellModel(settings: settings, tv: true)
        #expect(shell.section == .home)
        shell.setLibraries(libraries)
        #expect(shell.section == .library(2))
        // One that went stays on Home.
        let again = ShellModel(settings: settings, tv: true)
        again.setLibraries([libraries[0]])
        #expect(again.section == .home)
        // A tab chosen meanwhile wins.
        settings.lastSection = ShellSection.library(1).key
        let chosen = ShellModel(settings: settings, tv: true)
        chosen.select(.search)
        chosen.setLibraries(libraries)
        #expect(chosen.section == .search)
    }

    @Test func sectionsKeepTheirNames() {
        for section in [ShellSection.home, .libraries, .search, .playlists, .me, .settings, .library(12)] {
            #expect(ShellSection(key: section.key) == section)
        }
        #expect(ShellSection(key: "library:x") == nil)
        #expect(ShellSection(key: "live") == nil)
    }

    @Test func searchAndPlaylistsHaveATabASidebarRowAndAnAppleTVTab() {
        // The Android phone's five tabs.
        #expect(ShellModel.sections(tv: false, libraries: []) == [.home, .libraries, .search, .playlists, .me])
        let libraries = [Library(id: 1, name: "Movies", kind: .movies), Library(id: 2, name: "Shows", kind: .shows)]
        // Search first on Apple TV, where tvOS puts it.
        #expect(ShellModel.sections(tv: true, libraries: libraries)
            == [.search, .home, .library(1), .library(2), .playlists, .settings])
        let shell = ShellModel(settings: freshSettings(), tv: false)
        shell.setLibraries(libraries)
        #expect(shell.sidebar == [.home, .search, .playlists, .library(1), .library(2)])
    }

    @Test func aToastsUndoRunsOnce() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        var undone = 0
        shell.show("Removed Arrival") { undone += 1 }
        #expect(shell.toast?.text == "Removed Arrival")
        shell.undoToast()
        shell.undoToast()
        #expect(undone == 1)
        #expect(shell.toast == nil)
    }
}

@MainActor
@Suite struct ShellNavigationTests {
    /// Me, or Apple TV's Settings, which holds the same.
    private let meSection = ShellModel.sections(tv: ShellModel.isTV, libraries: []).last!

    private func signedIn(_ server: FixtureServer) async throws -> AppModel {
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        try #require(app.root == .signedIn)
        return app
    }

    @Test func appleTVReadsItsLibraryTabsWhateverTabShows() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        settings.lastSection = ShellSection.search.key
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      settings: settings, tv: true)
        await app.start()
        #expect(app.shell.section == .search)
        #expect(await eventually { app.shell.sections.contains(.library(1)) })
        #expect(app.shell.section == .search)
    }

    @Test func aProfileChangeStartsEverySectionAgainAtHome() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.shell.open(.item(7))
        app.shell.select(meSection)
        app.shell.open(.history)
        let me = MeModel(app: app)
        await me.load()
        #expect(me.canSwitch)
        me.switchProfile()
        #expect(app.root == .profiles)
        #expect(app.profilesFromShell)
        let profiles = ProfilesModel(app: app)
        await profiles.load()
        await profiles.choose(try #require(profiles.profiles.first { $0.name == "Kids" }))
        #expect(app.user?.name == "Kids")
        #expect(app.root == .signedIn)
        #expect(app.shell.section == .home)
        #expect(app.shell.paths.isEmpty)
    }

    @Test func choosingTheProfileInUseGoesBackAsItWas() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.shell.select(meSection)
        app.showProfiles()
        let profiles = ProfilesModel(app: app)
        await profiles.load()
        await profiles.choose(try #require(profiles.profiles.first(where: \.current)))
        #expect(app.root == .signedIn)
        #expect(app.shell.section == meSection)
        #expect(!app.profilesFromShell)
    }

    @Test func signingOutStartsAgainAtHomeNextTime() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.shell.select(meSection)
        app.shell.open(.history)
        await MeModel(app: app).signOut()
        #expect(app.root == .servers)
        #expect(app.shell.section == .home)
        #expect(app.shell.paths.isEmpty)
        #expect(app.user == nil)
    }

    @Test func meSaysWhoAndWhere() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let me = MeModel(app: app)
        #expect(me.name == "Alex")
        #expect(me.kind == nil)
        #expect(me.serverName == "Living Room")
        #expect(!me.serverAddress.isEmpty)
        me.showServers()
        #expect(app.root == .servers)
    }

    @Test func oneQueueOfLateReportsPerServerAndProfile() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let first = app.lateReports(origin: server.origin, profileId: 2)
        #expect(app.lateReports(origin: server.origin, profileId: 2) === first)
        #expect(app.lateReports(origin: server.origin, profileId: 3) !== first)
    }
}

@Suite struct TileTests {
    @Test func rowsAreShapedByWhatTheyHold() {
        var episode = ItemSummary(id: 1, libraryId: 1, kind: .episode, title: "Glasshouse")
        episode.grandparentTitle = "The Office"
        episode.grandparentId = 9
        episode.parentIndex = 2
        episode.index = 5
        episode.durationMs = 3_480_000
        episode.userState.positionMs = 1_920_000
        let movie = ItemSummary(id: 2, libraryId: 1, kind: .movie, title: "Arrival")
        let album = ItemSummary(id: 3, libraryId: 4, kind: .album, title: "Discovery")
        #expect(RowShape.of(Hub(key: "continue_watching", title: "", items: [movie])) == .landscape)
        #expect(RowShape.of(Hub(key: "recently_added:5", title: "", items: [episode])) == .landscape)
        #expect(RowShape.of(Hub(key: "recently_added:4", title: "", items: [album])) == .square)
        #expect(RowShape.of(Hub(key: "recently_added:1", title: "", items: [movie])) == .poster)

        let still = Tile(episode, shape: .landscape)
        #expect(still.title == "The Office")
        #expect(still.subtitle == "S2 · E5 · 26 min left")
        #expect(still.pictures == ["/api/items/9/images/backdrop"])
        #expect(still.progress > 0.5)
        let poster = Tile(episode, shape: .poster)
        #expect(poster.title == "Glasshouse")
        #expect(poster.subtitle == "26 min left")
        #expect(poster.pictures == ["/api/items/9/images/poster"])
    }

    @Test func homeRowsCarryTheirSeeAll() {
        let row = HomeRow(Hub(key: "recently_added:3", title: "Recently Added in Movies", libraryId: 3,
                              items: [ItemSummary(id: 2, libraryId: 3, kind: .movie, title: "Arrival")]))
        #expect(row.seeAll == .library(3, view: "sort=added&order=desc"))
        #expect(row.tiles.map(\.id) == [2])
        #expect(row.shape == .poster)
    }
}
