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

/// A signed-in app on `server`, at Home, with its event stream.
@MainActor
private func signedIn(_ server: FixtureServer) async throws -> AppModel {
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
    await app.start()
    try #require(app.root == .signedIn)
    return app
}

private func item(_ id: Int, _ kind: ItemKind, positionMs: Int = 0, durationMs: Int? = 6_000_000,
                  parent: Int? = nil, grandparent: Int? = nil, leaves: Int? = nil, viewed: Int? = nil,
                  watched: Bool = false) -> ItemSummary {
    var item = ItemSummary(id: id, libraryId: 1, kind: kind, title: "Item \(id)")
    item.userState.positionMs = positionMs
    item.userState.watched = watched
    item.durationMs = durationMs
    item.parentId = parent
    item.grandparentId = grandparent
    item.leafCount = leaves
    item.viewedLeafCount = viewed
    return item
}

@Suite struct LibraryParamsTests {
    @Test func paramsGoToTheWebsQueryAndBack() {
        #expect(LibraryParams().queryString == "")
        let params = LibraryParams(tab: .recommended, sort: .year, order: .ascending, watched: false,
                                   genre: "Science Fiction", year: 2021)
        #expect(params.queryString == "tab=recommended&sort=year&order=asc&watched=false&genre=Science+Fiction&year=2021")
        #expect(LibraryParams.parse(params.queryString) == params)
        // The sort's own order is left out, and comes back as `nil`.
        let added = LibraryParams.parse("sort=added&order=desc")
        #expect(added.sort == .added && added.effectiveOrder == .descending)
        #expect(added.queryString == "sort=added")
        #expect(added.query.order == "desc")
        // Unknown values fall back; a route's tab is used unless the query names one.
        let odd = LibraryParams.parse("sort=nonsense&order=up&watched=maybe&year=x", tab: .recommended)
        #expect(odd == LibraryParams(tab: .recommended))
        #expect(LibraryParams.parse("genre=Com%C3%A9die").genre == "Comédie")
    }

    @Test func sortsOrdersAndFilters() {
        var params = LibraryParams().sorted(by: .rating)
        #expect(params.effectiveOrder == .descending)
        params = params.flipped()
        #expect(params.effectiveOrder == .ascending && params.queryString == "sort=rating&order=asc")
        params = params.sorted(by: .title)
        #expect(params.order == nil && params.effectiveOrder == .ascending)
        params.watched = false
        params.genre = "Drama"
        #expect(params.filtered)
        #expect(params.query.watched == false && params.query.genre == "Drama")
        #expect(!params.cleared().filtered)
        #expect(params.cleared().sort == params.sort)
    }

    @Test func eachKindOfLibraryShowsWhatItCan() {
        let music = LibraryParams(tab: .collections, sort: .duration, order: .ascending, watched: true, genre: "Pop")
            .valid(for: .music)
        #expect(music == LibraryParams(tab: .library, sort: .title))
        // Shows have no duration sort.
        #expect(LibraryParams(sort: .duration).valid(for: .shows).sort == .title)
        #expect(LibraryParams(sort: .duration, watched: false).valid(for: .movies)
            == LibraryParams(sort: .duration, watched: false))
        #expect(LibraryParams.tabs(for: .music) == [.recommended, .library])
        #expect(LibraryParams.tabs(for: .movies) == [.recommended, .library, .collections])
        #expect(LibraryParams.sorts(for: .music) == [.title, .added, .userRating, .lastPlayed])
        #expect(LibraryParams.sorts(for: .shows).contains(.bitrate))
    }
}

@Suite struct AlphaIndexTests {
    @Test func titlesGoUnderTheirFirstLetterWithoutAccents() {
        #expect(AlphaIndex.letter(of: "Élan") == 5)
        #expect(AlphaIndex.letter(of: " arrival") == 1)
        #expect(AlphaIndex.letter(of: "2001: A Space Odyssey") == 0)
        #expect(AlphaIndex.letter(of: "Ωmega") == 0)
        #expect(AlphaIndex.letter(of: "") == 0)
        #expect(AlphaIndex.letters.count == 27 && AlphaIndex.letters[0] == "#" && AlphaIndex.letters[26] == "Z")
    }

    @Test func jumpsGoToTheLetterOrTheNextThatHasTitles() {
        let index = AlphaIndex.of(titles: ["12 Monkeys", "Arrival", "Alien", "Élan", "Dune", "Zodiac"])
        #expect(index.present == [0, 1, 4, 5, 26])
        #expect(index.has(1) && !index.has(2))
        #expect(index.position(of: 5) == 3)
        #expect(index.jumpTarget(1) == 1)
        // C has none: D's first.
        #expect(index.jumpTarget(3) == 4)
        // Nothing after Z's: the last before it.
        #expect(index.jumpTarget(26) == 5)
        #expect(index.letter(at: 2) == 1)
        #expect(index.letter(at: 4) == 4)
        #expect(index.letter(at: 5) == 26)
        // Sort titles count, else titles.
        var item = ItemSummary(id: 1, libraryId: 1, kind: .movie, title: "The Matrix")
        item.sortTitle = "Matrix"
        #expect(AlphaIndex.of([item]).has(13))
    }
}

@Suite struct ItemActionsTests {
    @Test func eachKindHasItsActions() {
        #expect(ItemActions.actions(for: item(1, .movie), inWatchlist: false)
            == [.play, .markWatched, .addToMyList, .addToPlaylist])
        #expect(ItemActions.actions(for: item(1, .movie, positionMs: 60_000), inWatchlist: true, continueWatching: true)
            == [.resume, .fromStart, .markWatched, .removeFromContinueWatching, .removeFromMyList, .addToPlaylist])
        #expect(ItemActions.actions(for: item(2, .episode, parent: 20, grandparent: 21, watched: true))
            == [.play, .markUnwatched, .addToPlaylist, .goToShow, .goToSeason])
        #expect(ItemActions.actions(for: item(3, .season, parent: 21, leaves: 4, viewed: 4), inWatchlist: false)
            == [.play, .markUnwatched, .addToPlaylist, .goToShow])
        #expect(ItemActions.actions(for: item(4, .show, leaves: 4, viewed: 1), inWatchlist: nil)
            == [.play, .markWatched, .addToPlaylist])
        #expect(ItemActions.actions(for: item(5, .album)) == [.open, .addToPlaylist])
        #expect(ItemActions.actions(for: item(6, .extra)) == [.play])
        #expect(ItemActions.actions(for: ItemSummary(id: 7, libraryId: 1, kind: ItemKind(rawValue: "hologram"),
                                                     title: "?")).isEmpty)
        // Nothing administrative, ever.
        #expect(ItemAction.allCases.count == 12)
    }

    @Test func actionsThatOpenAPage() {
        let episode = item(2, .episode, parent: 20, grandparent: 21)
        #expect(ItemActions.destination(.goToShow, episode) == .item(21))
        #expect(ItemActions.destination(.goToSeason, episode) == .item(20))
        #expect(ItemActions.destination(.goToShow, item(3, .season, parent: 21)) == .item(21))
        #expect(ItemActions.destination(.open, item(5, .album)) == .album(5))
        #expect(ItemActions.destination(.open, item(6, .artist)) == .artist(6))
        #expect(ItemActions.destination(.open, item(7, .track, parent: 5)) == .album(5, track: 7))
        #expect(ItemActions.destination(.markWatched, episode) == nil)
    }

    @MainActor
    @Test func aMarkShowsAtOnceAndGoesBackWhenRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let arrival = try #require(try await app.session.api?.item(101)).summary
        // My List is known as soon as it is read.
        #expect(await eventually { app.actions.listed != nil })
        #expect(app.actions.actions(for: arrival).contains(.addToMyList))

        await app.actions.perform(.markWatched, on: arrival)
        #expect(server.watched(101) == true)
        #expect(app.actions.shown(arrival).userState.watched)
        #expect(app.actions.actions(for: arrival).contains(.markUnwatched))

        server.answer(path: "/api/items/101/unwatched", status: 500, json: #"{"code":"oops","message":"disk full"}"#)
        let shown = app.actions.shown(arrival)
        await app.actions.perform(.markUnwatched, on: shown)
        // Refused: the mark is back as it was, and a toast says why.
        #expect(app.actions.shown(arrival).userState.watched)
        #expect(app.shell.toast?.text.hasPrefix("Couldn't mark it") == true)

        await app.actions.perform(.addToMyList, on: arrival)
        #expect(server.watchlistIds.contains(101))
        #expect(app.actions.actions(for: arrival).contains(.removeFromMyList))
        server.answer(path: "/api/watchlist/101", status: 403, json: #"{"code":"forbidden","message":"no"}"#)
        await app.actions.perform(.removeFromMyList, on: arrival)
        #expect(app.actions.actions(for: arrival).contains(.removeFromMyList))
    }

    @MainActor
    @Test func playStartsTheItemOrAShowsOnDeckEpisode() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        #expect(await app.actions.perform(.fromStart, on: item(101, .movie)) == .play(PlayRequest(itemId: 101,
                                                                                                  fromStart: true)))
        let onDeck = try #require(try await app.session.api?.onDeck(201))
        #expect(await app.actions.perform(.play, on: item(201, .show)) == .play(PlayRequest(itemId: onDeck.id)))
        #expect(await app.actions.perform(.goToShow, on: item(2, .episode, parent: 20, grandparent: 21))
            == .open(.item(21)))
    }
}

@MainActor
@Suite struct LibraryScreensTests {
    @Test func theGridReadsOnlyThePagesNearWhereItIs() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addLibrary(id: 9, name: "Big", count: 5_000)
        let app = try await signedIn(server)
        let model = LibraryModel(app: app, libraryId: 9)
        await model.load()
        let grid = try #require(model.grid)
        #expect(grid.total == 5_000)
        // More than 2,000 titles: no A to Z.
        #expect(model.alpha == nil)
        #expect(grid.tile(at: 0)?.title == "A 00001")
        #expect(grid.tile(at: 100) == nil)

        // Straight to the middle: that page only, and the next past 70 % of it.
        grid.showing(3_050)
        #expect(await eventually { grid.tile(at: 3_050) != nil })
        grid.showing(3_080)
        #expect(await eventually { grid.tile(at: 3_150) != nil })
        let offsets = Set(server.requests("/api/libraries/9/items").compactMap { $0.query["offset"] })
        #expect(offsets == ["0", "3000", "3100"])

        // Scrolling through it all never keeps more than 2,000.
        var most = 0
        for index in stride(from: 0, to: 5_000, by: 25) {
            grid.showing(index)
            _ = await eventually { grid.tile(at: index) != nil }
            most = max(most, grid.kept)
        }
        #expect(most <= 2_000)
        #expect(grid.pages.count <= LibraryGrid.maxPages)
        #expect(grid.tile(at: 4_999) != nil)
        #expect(grid.tile(at: 0) == nil)
    }

    @Test func aTitleSortOfFortyHasAnAToZ() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addLibrary(id: 8, name: "Clips", count: 40)
        let app = try await signedIn(server)
        let model = LibraryModel(app: app, libraryId: 8)
        await model.load()
        #expect(await eventually { model.alpha != nil })
        let alpha = try #require(model.alpha)
        // 40 titles over A to Z.
        #expect(alpha.present.count == 26)
        #expect(model.jumpTarget(3) == alpha.position(of: 3))
        // Another sort: none.
        model.sort(by: .added)
        #expect(model.alpha == nil)
        #expect(await eventually { model.grid?.total == 40 })
        try await Task.sleep(for: .milliseconds(100))
        #expect(model.alpha == nil)
    }

    @Test func theViewIsRememberedAndSeeAllWinsOnce() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = LibraryModel(app: app, libraryId: 1)
        await model.load()
        #expect(model.params == LibraryParams())
        #expect(model.tabs == [.recommended, .library, .collections])
        model.sort(by: .year)
        model.filter(watched: false)
        #expect(model.filterSummary == "Unwatched")
        // A page the first listing's A to Z asked for may reach the server
        // after the new listing's, so the new one is found by its filter.
        #expect(await eventually {
            server.requests("/api/libraries/1/items").contains { $0.query["watched"] == "false" }
        })
        let asked = try #require(server.requests("/api/libraries/1/items").last { $0.query["watched"] == "false" })
        #expect(asked.query["sort"] == "year" && asked.query["order"] == "desc")
        model.select(.recommended)
        #expect(await eventually { model.rows?.isEmpty == false })
        #expect(model.rows?.first?.continueWatching == true)

        // Opened again: as it was left.
        let again = LibraryModel(app: app, libraryId: 1)
        await again.load()
        #expect(again.params == LibraryParams(tab: .recommended, sort: .year, watched: false))
        // From See all: its view.
        let seeAll = LibraryModel(app: app, libraryId: 1, view: "sort=added&order=desc")
        await seeAll.load()
        #expect(seeAll.params == LibraryParams(sort: .added, order: .descending))
        seeAll.clearFilters()
        #expect(seeAll.params.sort == .added)

        // A library that isn't there.
        let gone = LibraryModel(app: app, libraryId: 77)
        await gone.load()
        #expect(gone.state == .notFound)
    }

    @Test func librariesMoveAtOnceAndSaveInOrder() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = LibrariesModel(app: app)
        await model.reload()
        #expect(model.entries.map(\.name) == ["Movies", "TV Shows", "Music"])
        #expect(model.entries.map(\.count) == ["412 movies", "37 shows", "128 artists"])
        #expect(model.reorderable)
        model.move(fromOffsets: [2], toOffset: 0)
        #expect(model.entries.map(\.id) == [3, 1, 2])
        model.move(3, by: 1)
        #expect(model.entries.map(\.id) == [1, 3, 2])
        #expect(app.shell.libraries.map(\.id) == [1, 3, 2])
        await model.saved()
        #expect(server.libraryOrder == [1, 3, 2])
        #expect(server.requests("/api/me/libraries/order").count == 2)

        // A library came or went meanwhile: the list is read again.
        server.refuseLibraryOrder(status: 409)
        model.move(fromOffsets: [0], toOffset: 3)
        #expect(model.entries.map(\.id) == [3, 2, 1])
        await model.saved()
        #expect(model.entries.map(\.id) == [1, 3, 2])

        server.refuseLibraryOrder(status: nil)
        await model.useServerOrder()
        #expect(model.entries.map(\.id) == [1, 2, 3])
    }

    @Test func scansShowOnTheirLibrary() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setScanning(2, true)
        let app = try await signedIn(server)
        let model = LibrariesModel(app: app)
        await model.reload()
        #expect(model.entries.map(\.scanning) == [false, true, false])
        #expect(model.entries[1].spoken == "TV Shows, 37 shows, scanning")
        #expect(await eventually { server.openEventStreams == 1 })
        server.pushEvent(#"{"type":"scan_started","library_id":1}"#)
        #expect(await eventually { model.entries.first?.scanning == true })
    }
}

@MainActor
@Suite struct FollowingTheServerTests {
    @Test func homeReadsItsRowsAgainWhenTheServerSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let home = HomeModel(app: app)
        await home.reload()
        #expect(home.rows.count == 4)
        #expect(home.rows.last?.shape == .square)
        #expect(await eventually { server.openEventStreams == 1 })
        let before = server.requests("/api/hubs/home").count
        server.pushEvent(#"{"type":"user_state_changed","item_ids":[202,203,201],"children":false}"#)
        server.pushEvent(#"{"type":"library_changed","library_id":2}"#)
        #expect(await eventually { server.requests("/api/hubs/home").count == before + 1 })
        try await Task.sleep(for: .milliseconds(500))
        // Gathered: one read for both.
        #expect(server.requests("/api/hubs/home").count == before + 1)
        // Events that change nothing Home shows read nothing.
        server.pushEvent(#"{"type":"sessions_changed"}"#)
        try await Task.sleep(for: .milliseconds(500))
        #expect(server.requests("/api/hubs/home").count == before + 1)
    }

    @Test func homeLeavesContinueWatchingAtOnceAndWarmsTheFirstThree() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let home = HomeModel(app: app)
        await home.reload()
        let row = try #require(home.rows.first)
        #expect(row.continueWatching && row.shape == .landscape)
        #expect(await eventually {
            row.tiles.prefix(3).allSatisfy { tile in !server.requests("/api/items/\(tile.id)").isEmpty }
        })
        let first = try #require(row.tiles.first)
        await home.removeFromContinueWatching(first.item)
        let left = home.rows.first.map { $0.tiles.map(\.id) } ?? []
        #expect(!left.contains(first.id))
        #expect(server.hiddenFromContinueWatching.contains(first.id))

        // Apple TV's info block: a warmed title's details at once; another's
        // what its tile says, then its details once the focus rests.
        let warmed = try #require(home.rows.first?.tiles.first)
        home.focus(warmed)
        #expect(home.info?.complete == true)
        let movie = try #require(home.rows.dropFirst().flatMap(\.tiles).first { $0.item.kind == .movie })
        home.focus(movie)
        #expect(home.info?.title == movie.item.title)
        #expect(home.info?.complete == false)
        #expect(await eventually { home.info?.complete == true })
    }

    @Test func homeSaysWhyItIsEmpty() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: "/api/hubs/home", status: 200, json: #"{"hubs":[]}"#)
        server.setScanning(1, true)
        let app = try await signedIn(server)
        let home = HomeModel(app: app)
        await home.reload()
        #expect(home.state == .empty(scanning: true))
        server.answer(path: "/api/libraries", status: 200, json: "[]")
        await home.reload()
        #expect(home.state == .noLibraries)
    }

    @Test func aPlaybackStoppedFromElsewhereCloses() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, positionMs: 0))
        let app = try await signedIn(server)
        #expect(await eventually { server.openEventStreams == 1 })
        let controller = try #require(await app.play(PlayRequest(itemId: 7)))
        #expect(await eventually { controller.session != nil })
        let id = try #require(controller.session?.id)
        // Another session's stop changes nothing.
        server.pushEvent(#"{"type":"session_stopped","session_id":"other","message":null,"by_admin":true}"#)
        try await Task.sleep(for: .milliseconds(200))
        #expect(app.player === controller)
        server.pushEvent(#"{"type":"session_stopped","session_id":"\#(id)","message":"Bedtime","by_admin":true}"#)
        #expect(await eventually { app.player == nil })
        #expect(app.shell.toast?.text == "Playback was stopped by the server's administrator: Bedtime")
        #expect(controller.state.stoppedByServer)
    }
}
