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
        for section in [ShellSection.home, .libraries, .search, .playlists, .me, .settings, .liveTv, .library(12)] {
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

    @Test func appleTVShowsSevenTabsAtMostWhateverTheLibrariesLiveTVAndMusic() {
        for (live, music) in [(false, false), (true, false), (false, true), (true, true)] {
            for count in 0...9 {
                let libraries = (1...max(count, 1)).prefix(count).map { Library(id: $0, name: "L\($0)", kind: .movies) }
                let tabs = ShellModel.sections(tv: true, libraries: libraries, liveTv: live, nowPlaying: music)
                #expect(tabs.count <= ShellModel.maxTabs)
                #expect(Array(tabs.prefix(2)) == [.search, .home])
                let end: [ShellSection] = music ? [.playlists, .nowPlaying, .settings] : [.playlists, .settings]
                #expect(Array(tabs.suffix(end.count)) == end)
                #expect(tabs.contains(.liveTv) == live)
                if live { #expect(tabs[2] == .liveTv) }
                let room = 3 - (live ? 1 : 0) - (music ? 1 : 0)
                let shown = tabs.compactMap { if case .library(let id) = $0 { id } else { nil } }
                if count <= room {
                    // Each library has its tab while they all fit.
                    #expect(shown == Array(1...max(count, 1)).prefix(count).map { $0 })
                    #expect(!tabs.contains(.libraries))
                } else {
                    // The first that fit keep theirs; Libraries takes the rest's place, last of them.
                    #expect(shown == Array((1...max(room - 1, 1)).prefix(room - 1)))
                    #expect(tabs[tabs.count - end.count - 1] == .libraries)
                    #expect(tabs.count == ShellModel.maxTabs)
                }
            }
        }
        // Music playing beside Live TV and four libraries: Libraries holds them all.
        #expect(ShellModel.sections(tv: true, libraries: (1...4).map { Library(id: $0, name: "L\($0)", kind: .movies) },
                                    liveTv: true, nowPlaying: true)
            == [.search, .home, .liveTv, .libraries, .playlists, .nowPlaying, .settings])
        // Four libraries and Live TV: one library keeps its tab.
        let four = (1...4).map { Library(id: $0, name: "L\($0)", kind: .movies) }
        #expect(ShellModel.sections(tv: true, libraries: four, liveTv: true)
            == [.search, .home, .liveTv, .library(1), .libraries, .playlists, .settings])
        // Named as people name them, the tabs must fit across the screen too.
        let named = ["Movies", "Shows", "Clips", "Music"].enumerated().map {
            Library(id: $0.offset + 1, name: $0.element, kind: .movies)
        }
        #expect(ShellModel.sections(tv: true, libraries: Array(named.prefix(2)))
            == [.search, .home, .library(1), .library(2), .playlists, .settings])
        #expect(ShellModel.sections(tv: true, libraries: named)
            == [.search, .home, .library(1), .library(2), .libraries, .playlists, .settings])
        #expect(ShellModel.sections(tv: true, libraries: named, liveTv: true)
            == [.search, .home, .liveTv, .library(1), .libraries, .playlists, .settings])
        // A long name gives its tab to Libraries.
        let long = [Library(id: 1, name: "Documentaries and Concerts", kind: .movies),
                    Library(id: 2, name: "Shows", kind: .shows)]
        #expect(ShellModel.sections(tv: true, libraries: long, liveTv: true)
            == [.search, .home, .liveTv, .libraries, .playlists, .settings])
        for tabs in [ShellModel.sections(tv: true, libraries: named, liveTv: true),
                     ShellModel.sections(tv: true, libraries: long, liveTv: true, nowPlaying: true)] {
            let width = tabs.reduce(0) { $0 + ShellModel.tabWidth($1, libraries: named + long) }
            #expect(width <= ShellModel.tabRoom)
        }
        // The phone's five tabs don't change: Live TV is a row of Libraries.
        #expect(ShellModel.sections(tv: false, libraries: four, liveTv: true) == [.home, .libraries, .search, .playlists, .me])
    }

    @Test func liveTVShowsOnceTheServerSaysItIsAvailable() {
        let settings = freshSettings()
        let shell = ShellModel(settings: settings, tv: false)
        #expect(!shell.showsLiveTv && !shell.shows(.liveTv))
        shell.setLiveTv(LiveSummary(available: false, allowed: true))
        #expect(!shell.sidebar.contains(.liveTv) && !shell.viewMenu.contains { $0.section == .liveTv })
        shell.setLiveTv(LiveSummary(available: true, allowed: true, channels: 3))
        #expect(shell.sidebar.prefix(2) == [.home, .liveTv])
        let menu = shell.viewMenu
        #expect(menu.map(\.title).prefix(3) == ["Home", "Live TV", "Playlists"])
        #expect(menu[1].key == "l" && menu[0].key == "1" && menu[2].key == "2")
        shell.select(.liveTv)
        #expect(shell.section == .liveTv && settings.lastSection == "liveTv")
        // Shown last: it comes back once Live TV is known to show.
        let again = ShellModel(settings: settings, tv: true)
        #expect(again.section == .home)
        again.setLibraries([Library(id: 1, name: "Movies", kind: .movies)])
        #expect(again.section == .home)
        again.setLiveTv(LiveSummary(available: true, allowed: true, channels: 3))
        #expect(again.section == .liveTv)
        #expect(again.sections == [.search, .home, .liveTv, .library(1), .playlists, .settings])
        // It went: back to Home.
        again.setLiveTv(LiveSummary(available: false, allowed: false))
        #expect(again.section == .home && !again.sections.contains(.liveTv))
        again.setLiveTv(LiveSummary(available: true, allowed: true, channels: 3))
        again.reset()
        #expect(again.liveTv == nil && !again.sections.contains(.liveTv))
    }

    @Test func aToastsUndoRunsOnce() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        var undone = 0
        shell.show("Removed Arrival") { undone += 1 }
        #expect(shell.toast?.text == "Removed Arrival")
        shell.runToastAction()
        shell.runToastAction()
        #expect(undone == 1)
        #expect(shell.toast == nil)
    }

    @Test func aToastCarriesItsOneButtonAndSaysAnError() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        var opened = 0
        shell.show("“Arrival” is ready to watch.", action: ToastAction("Open") { opened += 1 })
        #expect(shell.toast?.action?.label == "Open" && shell.toast?.action?.identifier == "toast.action")
        #expect(shell.toast?.style == .plain)
        shell.runToastAction()
        #expect(opened == 1 && shell.toast == nil)
        shell.show("TMDB isn't answering.", action: nil, error: true)
        #expect(shell.toast?.style == .error && shell.toast?.action == nil)
        // Undo keeps its own identifier.
        shell.show("Removed Arrival") {}
        #expect(shell.toast?.action?.identifier == "toast.undo")
    }

    @Test func theWayIntoRequestsCarriesItsNumberUntilAReset() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        #expect(shell.requestsEntry == .none)
        let entry = RequestsEntry(discover: true, requests: true, manage: true, badge: 3, failed: 1)
        shell.setRequestsEntry(entry)
        #expect(shell.requestsEntry == entry && shell.requestsEntry.alarm && shell.requestsEntry.badgeText == "3")
        shell.reset()
        #expect(shell.requestsEntry == .none)
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
