import XCTest

/// Apple TV's screens against a test server (check-screens.sh): sign in
/// with a code, "Who's watching?" with Kids by PIN, Home, an item,
/// Settings, back to the administrator by PIN, See all, a title's actions,
/// a library tab's sort, filter and letters, and Sign out.
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
        walk.openLibraryTab("Movies")
        walk.sortAndFilter()
        walk.openLibraryTab("Clips")
        walk.jumpByLetter()
        walk.openSettings()
        walk.signOut()
        walk.shot("07-signed-out")
    }
}
