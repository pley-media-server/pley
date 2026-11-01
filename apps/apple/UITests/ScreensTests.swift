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
        walk.signOut()
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
        walk.startSignedOut()
        await walk.signInWithCode()
        // The profile signed in goes on without its PIN.
        walk.wait(walk.button("profiles.profile.\(Admin.adminName)"), "Who's watching?", timeout: 30).tap()
        if walk.element("profiles.pin").waitForExistence(timeout: 3) {
            walk.element("profiles.pin").tap()
            walk.element("profiles.pin").typeText(Admin.adminPin)
            walk.tap("profiles.pin.continue", "Continue")
        }
        walk.largeTextPass(libraries: await Admin.libraryIds())
        walk.signOut()
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
        walk.startSignedOut()
        await walk.signInWithCode()
        walk.choose(Admin.adminName, pin: Admin.adminPin)
        walk.seeHome(Admin.adminName)
        walk.personPage(library: libraries[found.library] ?? 0, name: found.library, title: found.title)
        walk.openMe()
        walk.signOut()
    }
}
