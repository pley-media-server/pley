import UIKit
import XCTest

/// The screens on iPhone and iPad against a test server (check-screens.sh):
/// sign in with a code, "Who's watching?" with Kids by PIN, Home, an item,
/// Me, back to the administrator by PIN, See all, a title's actions, a
/// movie's, a show's, a season's, an episode's, an artist's and an album's
/// pages, a library's sort, filter and letters, Save as Smart Playlist, a
/// collection's page, a playlist made, changed and deleted, the libraries'
/// order, a search and its recent searches, the profile's subtitle default,
/// History, Settings' quality and subtitle size, Diagnostics, About, and
/// Sign out. Another walk looks at the screens at the largest text size.
/// A person's page is walked when a title has cast, which comes from a
/// metadata source; the test server has none, so it is skipped.
@MainActor
final class ScreensTests: XCTestCase {
    override func setUp() async throws {
        continueAfterFailure = false
        try XCTSkipUnless(Admin.ready, "Run by scripts/check-screens.sh, which starts the test server.")
    }

    func testSignInProfilesHomeBrowseMeAndSignOut() async throws {
        if UIDevice.current.userInterfaceIdiom == .pad {
            // The sidebar shows beside the page in landscape.
            XCUIDevice.shared.orientation = .landscapeLeft
        }
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        walk.startSignedOut()

        await walk.signInWithCode()
        walk.wait(walk.button("profiles.profile.Kids"), "Who's watching?", timeout: 30)
        walk.shot("03-profiles")
        walk.choose("Kids", pin: Admin.kidsPin)
        walk.seeHome("Kids")
        walk.shot("04-home")
        walk.openATile()

        walk.openMe()
        walk.meSays("Kids")
        walk.shot("06-me")
        walk.wait(walk.element("me.switchProfile"), "Switch profile").tap()
        walk.choose(Admin.adminName, pin: Admin.adminPin)
        walk.seeHome(Admin.adminName)
        walk.openMe()
        walk.meSays(Admin.adminName)

        walk.openHome()
        walk.seeHome(Admin.adminName)
        walk.seeAll()
        let libraries = await Admin.libraryIds()
        await walk.itemActions(movies: libraries["Movies"] ?? 0)
        await walk.moviePage(movies: libraries["Movies"] ?? 0)
        walk.showPages(shows: libraries["Shows"] ?? 0)
        walk.musicPages(music: libraries["Music"] ?? 0)
        walk.openLibrary(libraries["Movies"] ?? 0, "Movies")
        walk.sortAndFilter()
        await walk.saveAsSmartPlaylist(movies: libraries["Movies"] ?? 0)
        walk.collectionPage(movies: libraries["Movies"] ?? 0)
        await walk.playlistWalk(movies: libraries["Movies"] ?? 0)
        walk.openLibrary(libraries["Clips"] ?? 0, "Clips")
        walk.jumpByLetter()
        await walk.reorder()
        walk.search()
        await walk.trackDefaults()
        walk.history()
        walk.settings()
        walk.openMe()
        await walk.signOutEndingTheGrant()
        walk.shot("07-signed-out")
    }

    /// The screens at the largest accessibility text size, looked at and
    /// checked to be reachable; nothing on the server changes.
    func testLargeTextScreens() async throws {
        if UIDevice.current.userInterfaceIdiom == .pad {
            XCUIDevice.shared.orientation = .landscapeLeft
        }
        let app = XCUIApplication()
        app.launchArguments += Walk.largestText
        app.launch()
        let walk = Walk(app: app, test: self)
        await walk.startSignedIn()
        walk.largeTextPass(libraries: await Admin.libraryIds())
        // Signing out ends music's session, which the tests after this one
        // would otherwise find on the server.
        walk.signOut()
    }

    /// Live TV against live-source.sh's channels (one connection at a
    /// time): the categories, the guide and the cards, a channel played,
    /// channel up, a favourite, Recently watched, Watch here instead when
    /// another device holds the connection, leaving, and Search's channels.
    func testLiveTv() async throws {
        guard let one = await Admin.channelId("Test One"), let two = await Admin.channelId("Test Two") else {
            throw XCTSkip("The test server has no Live TV source: check-screens.sh adds one.")
        }
        await Admin.unfavorite(two)
        if UIDevice.current.userInterfaceIdiom == .pad {
            XCUIDevice.shared.orientation = .landscapeLeft
        }
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        await walk.startSignedIn()

        walk.openLiveTv()
        walk.chooseCategory("all")
        let programme = app.buttons.matching(NSPredicate(format: "label CONTAINS 'Test One at'")).firstMatch
        walk.wait(programme, "Test One's programme in the guide", timeout: 30)
        walk.shot("70-live-guide")
        walk.wait(app.buttons["Channels"], "the Channels view").tap()
        walk.wait(walk.element("live.channel.\(one)"), "Test One's card").tap()
        await walk.livePlayer(says: "playing")
        await walk.liveChannel(is: "Test One")
        walk.shot("71-live-player")

        walk.tapInPlayer("live.channelUp", "Channel up")
        await walk.liveChannel(is: "Test Two")
        await walk.livePlayer(says: "playing")
        walk.tapInPlayer("live.favorite", "the star")
        let starred = await walk.holds { await Admin.favoriteChannels().contains(two) }
        XCTAssertTrue(starred, "Test Two is a favourite at the server")
        walk.tapInPlayer("live.close", "Close")
        let left = await walk.holds(within: 4) { await Admin.liveViewers() == 0 }
        XCTAssertTrue(left, "leaving ends the session at the server")

        walk.chooseCategory("favorites")
        walk.wait(walk.element("live.channel.\(two)"), "Test Two in Favourites")
        walk.chooseCategory("recent")
        walk.wait(walk.element("live.channel.\(two)"), "Test Two in Recently watched")
        walk.wait(walk.element("live.channel.\(one)"), "Test One in Recently watched")

        // Another device holds the one connection: Watch here instead takes it.
        let other = await Admin.watch(two)
        XCTAssertNotNil(other, "the other device watches Test Two")
        walk.chooseCategory("all")
        walk.wait(walk.element("live.channel.\(one)"), "Test One's card").tap()
        walk.wait(walk.element("live.trouble"), "All connections are in use", timeout: 30)
        XCTAssertEqual(walk.element("live.trouble.title").label, "All connections are in use")
        walk.shot("72-live-busy")
        walk.tap("live.takeOver", "Watch here instead")
        await walk.livePlayer(says: "playing")
        if let other {
            let stopped = await walk.holds { await Admin.playlistStatus(other) == 410 }
            XCTAssertTrue(stopped, "the other device's playlist answers 410")
        }
        walk.tapInPlayer("live.close", "Close")
        let gone = await walk.holds(within: 4) { await Admin.liveViewers() == 0 }
        XCTAssertTrue(gone, "leaving ends the session at the server")

        walk.openSearch()
        let field = walk.wait(app.searchFields.firstMatch, "the search field")
        field.tap()
        field.typeText("Test")
        walk.wait(walk.element("search.group.channels"), "Live TV's channels found", timeout: 30)
        walk.shot("73-live-search")
        let cancel = app.buttons.matching(NSPredicate(format: "label IN {'Cancel', 'Close'}")).firstMatch
        if cancel.exists { cancel.tap() }
    }

    /// Music: an album played from its page (the mini player, the track
    /// marked, the session on the dashboard), Next (the first session gone),
    /// Now Playing with Shuffle and a 15-minute sleep timer, and a music
    /// playlist made of the artist's tracks played from its second entry,
    /// then paused; the playlist is deleted afterwards.
    func testMusic() async throws {
        if UIDevice.current.userInterfaceIdiom == .pad {
            XCUIDevice.shared.orientation = .landscapeLeft
        }
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        await walk.startSignedIn()
        await walk.musicWalk(music: await Admin.libraryIds()["Music"] ?? 0)
        walk.openMe()
        walk.signOut()
        let ended = await walk.holds { await Admin.sessions().isEmpty }
        XCTAssertTrue(ended, "signing out ends music's session")
    }

    /// The player's extras on Direct H264: Find Subtitles… says the server
    /// has no subtitle search set up, its SRT brings Subtitle style, whose
    /// size is marked once picked, Speed 1.5× is marked once picked, and
    /// Picture in Picture shows the page behind with the playback going on.
    func testPlayerExtras() async throws {
        if UIDevice.current.userInterfaceIdiom == .pad {
            XCUIDevice.shared.orientation = .landscapeLeft
        }
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        await walk.startSignedIn()
        await walk.playerExtras(movies: await Admin.libraryIds()["Movies"] ?? 0)
    }

    /// Downloads: the original of Direct H264 played from the device, the
    /// show's season converted and downloaded, and Delete all downloads.
    func testDownloads() async throws {
        if UIDevice.current.userInterfaceIdiom == .pad {
            XCUIDevice.shared.orientation = .landscapeLeft
        }
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        await walk.startSignedIn()
        let libraries = await Admin.libraryIds()
        await walk.downloads(movies: libraries["Movies"] ?? 0, shows: libraries["Shows"] ?? 0)
    }

    func testPersonPage() async throws {
        let libraries = await Admin.libraryIds()
        var found: (library: String, title: String)?
        for name in ["Movies", "Shows"] {
            if let title = await Admin.titleWithCast(library: libraries[name] ?? 0) {
                found = (name, title)
                break
            }
        }
        guard let found else {
            throw XCTSkip("No title has cast: people come from a metadata source, and the test server has none.")
        }
        if UIDevice.current.userInterfaceIdiom == .pad {
            XCUIDevice.shared.orientation = .landscapeLeft
        }
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        await walk.startSignedIn()
        walk.personPage(library: libraries[found.library] ?? 0, name: found.library, title: found.title)
    }
}
