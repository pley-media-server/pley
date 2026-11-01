import XCTest

/// Apple TV's screens against a test server (check-screens.sh): sign in
/// with a code, "Who's watching?" with Kids by PIN, Home, an item,
/// Settings, back to the administrator by PIN, See all, a title's actions,
/// a movie's, a show's, a season's, an episode's, an artist's and an
/// album's pages, a library tab's sort, filter and letters, and Sign out.
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
        await walk.moviePage(movies: libraries["Movies"] ?? 0)
        walk.showPages()
        walk.musicPages()
        walk.openLibraryTab("Movies")
        walk.sortAndFilter()
        walk.openLibraryTab("Clips")
        walk.jumpByLetter()
        walk.openSettings()
        walk.signOut()
        walk.shot("07-signed-out")
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
