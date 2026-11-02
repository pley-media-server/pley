import XCTest

/// Apple TV's screens against a test server (check-screens.sh): sign in
/// with a code, "Who's watching?" with Kids by PIN, Home, an item,
/// Settings, back to the administrator by PIN, See all, a title's actions,
/// a row keeping its focus, Play/Pause on a tile, a movie's, a show's, a
/// season's, an episode's, an artist's and an album's pages, a library
/// tab's sort, filter and letters, Save as Smart Playlist, a collection's
/// page, a playlist made, changed and deleted, a search and its recent
/// searches, the profile's subtitle default,
/// History, quality and subtitle size, Diagnostics, About, Always start as
/// across a relaunch, and Sign out.
/// A person's page is walked when a title has cast, which comes from a
/// metadata source; the test server has none, so it is skipped.
@MainActor
final class ScreensTests: XCTestCase {
    override func setUp() async throws {
        continueAfterFailure = false
        try XCTSkipUnless(Admin.ready, "Run by scripts/check-screens.sh, which starts the test server.")
    }

    func testSignInProfilesHomeBrowseSettingsAndSignOut() async throws {
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

        walk.openSettings()
        walk.meSays("Kids")
        walk.shot("06-settings")
        walk.select(walk.element("me.switchProfile"), "Switch profile", first: [.up, .down])
        walk.choose(Admin.adminName, pin: Admin.adminPin)
        walk.seeHome(Admin.adminName)
        walk.openSettings()
        walk.meSays(Admin.adminName)

        walk.openHome()
        walk.seeHome(Admin.adminName)
        walk.seeAll()
        let libraries = await Admin.libraryIds()
        await walk.itemActions(movies: libraries["Movies"] ?? 0)
        walk.rowKeepsItsFocus(movies: libraries["Movies"] ?? 0)
        await walk.playPauseOnATile(movies: libraries["Movies"] ?? 0)
        await walk.moviePage(movies: libraries["Movies"] ?? 0)
        walk.showPages()
        walk.musicPages()
        walk.openLibraryTab("Movies")
        walk.sortAndFilter()
        await walk.saveAsSmartPlaylist()
        walk.collectionPage()
        await walk.playlistWalk(movies: libraries["Movies"] ?? 0)
        walk.openLibraryTab("Clips")
        walk.jumpByLetter()
        walk.search()
        await walk.trackDefaults()
        walk.history()
        walk.settings()
        walk.alwaysStartAs()
        walk.openSettings()
        await walk.signOutEndingTheGrant()
        walk.shot("07-signed-out")
    }

    /// Live TV against live-source.sh's channels (one connection at a
    /// time): the tab after Home, the categories, the guide, a channel
    /// played, channel up, a number on the number pad, a favourite,
    /// Recently watched, Watch here instead, leaving, and Search's channels.
    func testLiveTv() async throws {
        guard let one = await Admin.channelId("Test One"), let two = await Admin.channelId("Test Two"),
              let three = await Admin.channelId("Test Three") else {
            throw XCTSkip("The test server has no Live TV source: check-screens.sh adds one.")
        }
        await Admin.unfavorite(three)
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        walk.startSignedOut()
        await walk.signInWithCode()
        walk.chooseSignedIn()
        walk.seeHome(Admin.adminName)

        walk.openLiveTv()
        walk.shot("70-live-tabs")
        walk.chooseCategory("all")
        let programme = app.buttons.matching(NSPredicate(format: "label CONTAINS 'Test One at'")).firstMatch
        walk.wait(programme, "Test One's programme in the guide", timeout: 30)
        walk.shot("71-live-guide")
        walk.select(walk.element("live.guide.channel.\(one)"), "Test One", first: [.right, .down])
        await walk.livePlayer(says: "playing")
        await walk.liveChannel(is: "Test One")
        walk.shot("72-live-player")

        walk.remote.press(.up)
        await walk.liveChannel(is: "Test Two")
        await walk.livePlayer(says: "playing")

        walk.selectInOverlay("live.numberPad", "Channel number")
        for digit in [1, 0, 3] {
            walk.select(walk.element("live.pad.\(digit)"), "\(digit) on the number pad")
        }
        walk.shot("73-live-number")
        walk.select(walk.element("live.pad.ok"), "OK on the number pad")
        await walk.liveChannel(is: "Test Three")
        await walk.livePlayer(says: "playing")

        walk.selectInOverlay("live.favorite", "Add to favourites")
        let starred = await walk.holds { await Admin.favoriteChannels().contains(three) }
        XCTAssertTrue(starred, "Test Three is a favourite at the server")
        // Menu hides the overlay, then leaves.
        walk.remote.press(.menu)
        walk.remote.press(.menu)
        let left = await walk.holds(within: 4) { await Admin.liveViewers() == 0 }
        XCTAssertTrue(left, "leaving ends the session at the server")

        walk.chooseCategory("favorites")
        walk.wait(walk.element("live.guide.channel.\(three)"), "Test Three in Favourites")
        walk.chooseCategory("recent")
        walk.wait(walk.element("live.guide.channel.\(two)"), "Test Two in Recently watched")

        // Another device holds the one connection: Watch here instead takes it.
        let other = await Admin.watch(two)
        XCTAssertNotNil(other, "the other device watches Test Two")
        walk.chooseCategory("all")
        walk.select(walk.element("live.guide.channel.\(one)"), "Test One", first: [.right, .down])
        walk.wait(walk.element("live.trouble"), "All connections are in use", timeout: 30)
        XCTAssertEqual(walk.element("live.trouble.title").label, "All connections are in use")
        walk.shot("74-live-busy")
        walk.select(walk.button("live.takeOver"), "Watch here instead", first: [.down, .right])
        await walk.livePlayer(says: "playing")
        if let other {
            let stopped = await walk.holds { await Admin.playlistStatus(other) == 410 }
            XCTAssertTrue(stopped, "the other device's playlist answers 410")
        }
        walk.remote.press(.menu)
        let gone = await walk.holds(within: 4) { await Admin.liveViewers() == 0 }
        XCTAssertTrue(gone, "leaving ends the session at the server")

        walk.openSearch()
        walk.focus(app.keyboards.firstMatch, "the keyboard", first: [.down])
        app.typeText("Test")
        // The channels come last: the focus moves down the groups to them.
        let found = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'search.group.'")).firstMatch
        walk.wait(found, "something found", timeout: 30)
        let channels = walk.element("search.group.channels")
        for _ in 0..<8 where !channels.exists {
            walk.remote.press(.down)
        }
        walk.wait(channels, "Live TV's channels found")
        walk.shot("75-live-search")
        walk.openSettings()
        walk.signOut()
    }

    /// Music: an album played from its page opens the Now Playing tab (the
    /// session on the dashboard), Next (the first session gone), Shuffle and
    /// a 15-minute sleep timer, and a music playlist played from its second
    /// entry, then paused with the remote's Play/Pause; the playlist is
    /// deleted afterwards.
    func testMusic() async throws {
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        walk.startSignedOut()
        await walk.signInWithCode()
        walk.chooseSignedIn()
        walk.seeHome(Admin.adminName)
        await walk.musicWalk(music: await Admin.libraryIds()["Music"] ?? 0)
        walk.openSettings()
        walk.signOut()
        await walk.eventually("signing out ends music's session") { await Admin.sessions().isEmpty }
    }

    /// The player's extras on Direct H264: Find Subtitles… in the
    /// transport bar says the server has no subtitle search set up, Speed
    /// 1.5× is marked once picked, and Picture in Picture shows the page
    /// behind with the playback going on.
    func testPlayerExtras() async throws {
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        walk.startSignedOut()
        await walk.signInWithCode()
        walk.chooseSignedIn()
        walk.seeHome(Admin.adminName)
        await walk.playerExtras(movies: await Admin.libraryIds()["Movies"] ?? 0)
        walk.openSettings()
        walk.signOut()
    }

    /// Apple TV's Top Shelf: Home's Continue Watching is saved for the
    /// shelf, an unsigned link does nothing, the signed links open Two Audio's
    /// page and play it, and signing out empties the shelf.
    func testTopShelfLink() async throws {
        guard let id = await Admin.titles(library: await Admin.libraryIds()["Movies"] ?? 0)["Two Audio"] else {
            return XCTFail("no Two Audio in Movies")
        }
        let started = await Admin.startWatching(id)
        XCTAssertTrue(started, "Two Audio is part way through")
        addTeardownBlock { await Admin.stopWatching(id) }
        let app = XCUIApplication()
        app.launchArguments += ["-PleyShelfLinks", "YES"]
        app.launch()
        let walk = Walk(app: app, test: self)
        walk.startSignedOut()
        await walk.signInWithCode()
        walk.chooseSignedIn()
        walk.seeHome(Admin.adminName)
        await walk.shelfLinks(id)
        walk.openSettings()
        walk.signOut()
        let empty = NSPredicate(format: "label == '0'")
        await fulfillment(of: [XCTNSPredicateExpectation(predicate: empty, object: walk.element("test.shelf.count"))],
                          timeout: 10)
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
        let app = XCUIApplication()
        app.launch()
        let walk = Walk(app: app, test: self)
        walk.startSignedOut()
        await walk.signInWithCode()
        walk.choose(Admin.adminName, pin: Admin.adminPin)
        walk.seeHome(Admin.adminName)
        walk.personPage(library: found.library, title: found.title)
        walk.openSettings()
        walk.signOut()
    }
}
