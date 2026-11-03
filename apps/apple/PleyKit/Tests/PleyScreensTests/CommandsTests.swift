import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyPlayer
@testable import PleyScreens
import PleyTesting
import Testing

/// What the Mac's menus (and the iPad's keyboard) act on: the View menu's
/// sections, Search's field, Reload, Switch Profile, Add Server, Play and
/// the grid's size.
@MainActor
@Suite struct CommandsTests {
    private func signedIn(_ server: FixtureServer, scope: String = FixtureServer.allScopes) async throws -> AppModel {
        let store = MemoryTokenStore()
        let tokens = server.issueTokens(scope: scope)
        try await store.save(StoredTokens(
            issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
            revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
            accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken, scope: scope))
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        try #require(app.root == .signedIn)
        return app
    }

    @Test func theViewMenuNumbersHomePlaylistsAndTheFirstSevenLibraries() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        #expect(shell.viewMenu.map(\.key) == ["1", "2"])
        shell.setLibraries((1...9).map { Library(id: $0, name: "Library \($0)", kind: .movies) })
        let menu = shell.viewMenu
        #expect(menu.map(\.title).prefix(3) == ["Home", "Playlists", "Library 1"])
        #expect(menu.map(\.section).prefix(3) == [.home, .playlists, .library(1)])
        #expect(menu.compactMap(\.key) == ["1", "2", "3", "4", "5", "6", "7", "8", "9"])
        // The eighth and ninth libraries are listed without a key.
        #expect(menu.suffix(2).map(\.key) == [nil, nil])
        #expect(menu.suffix(2).map(\.section) == [.library(8), .library(9)])
    }

    @Test func findShowsSearchAndAsksForItsField() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        shell.select(.search)
        shell.open(.item(3))
        shell.select(.home)
        shell.findInSearch()
        #expect(shell.section == .search)
        #expect(shell.wantsSearchField)
        // Search's own page stays; only its field is asked for.
        #expect(shell.path(for: .search) == [.item(3)])
        shell.searchFieldShown()
        #expect(!shell.wantsSearchField)
        shell.findInSearch()
        #expect(shell.wantsSearchField)
        #expect(shell.section == .search)
    }

    @Test func reloadReadsThePageThatShowedLast() async {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        #expect(!shell.canReload)
        var read: [String] = []
        let home = UUID(), item = UUID()
        shell.showing(home) { read.append("home") }
        // A page pushed on top shows before the one under it goes.
        shell.showing(item) { read.append("item") }
        shell.leaving(home)
        #expect(shell.canReload)
        await shell.reloadShowing()
        #expect(read == ["item"])
        // Back: Home shows again, then the item page goes.
        shell.showing(home) { read.append("home") }
        shell.leaving(item)
        await shell.reloadShowing()
        #expect(read == ["item", "home"])
        shell.leaving(home)
        #expect(!shell.canReload)
        await shell.reloadShowing()
        #expect(read == ["item", "home"])
    }

    @Test func switchProfileNeedsTheScopeAndAHousehold() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        // The fixture's household is the administrator and Kids.
        #expect(app.canSwitchProfile)
        app.noteHousehold([HouseholdProfile(id: 1, name: "Only one", current: true)])
        #expect(!app.canSwitchProfile)

        let limited = try await signedIn(server, scope: "library play playlists")
        #expect(!limited.canSwitchProfile)
    }

    @Test func addServerShowsTheServersReadyForAnAddress() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        app.showServers(adding: true)
        #expect(app.root == .servers)
        #expect(app.addingServer)
        app.addingServerShown()
        #expect(!app.addingServer)
        app.showServers()
        #expect(!app.addingServer)
    }

    @Test func playPlaysVideoNotMusic() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let movie = ItemSummary(id: 42, libraryId: 1, kind: .movie, title: "Arrival")
        let target = try #require(PlayTarget.item(movie, actions: app.actions))
        #expect(target.menuTitle == "Play “Arrival”")
        let request = await target.request()
        #expect(request?.itemId == 42)
        for kind: ItemKind in [.artist, .album, .track] {
            #expect(PlayTarget.item(ItemSummary(id: 1, libraryId: 1, kind: kind, title: "Song"),
                                    actions: app.actions) == nil)
        }
        #expect(PlayTarget.plays(.show) && PlayTarget.plays(.season) && PlayTarget.plays(.episode))
    }

    @Test func tilesGrowAndShrinkOneSizeAtATime() {
        #expect(GridDensity.medium.stepped(by: 1) == .large)
        #expect(GridDensity.medium.stepped(by: -1) == .small)
        #expect(GridDensity.large.stepped(by: 1) == nil)
        #expect(GridDensity.small.stepped(by: -1) == nil)
    }
}
