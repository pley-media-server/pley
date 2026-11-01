import UIKit
import XCTest

/// The screens on iPhone and iPad against a test server (check-screens.sh):
/// sign in with a code, "Who's watching?" with Kids by PIN, Home, an item,
/// Me, back to the administrator by PIN, and Sign out.
@MainActor
final class ScreensTests: XCTestCase {
    override func setUp() async throws {
        continueAfterFailure = false
        try XCTSkipUnless(Admin.ready, "Run by scripts/check-screens.sh, which starts the test server.")
    }

    func testSignInProfilesHomeMeAndSignOut() async throws {
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
        walk.openMe()
        walk.signOut()
        walk.shot("07-signed-out")
    }
}
