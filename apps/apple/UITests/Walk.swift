import UIKit
import XCTest

/// Steps through the iPhone's and iPad's screens as a person does, by the
/// screens' accessibility identifiers (`<screen>.<part>[.<id>]`).
@MainActor
struct Walk {
    let app: XCUIApplication
    let test: XCTestCase

    /// The device, for the screenshots' names.
    var device: String { UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone" }

    func element(_ id: String) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: id).firstMatch
    }

    func button(_ id: String) -> XCUIElement {
        app.buttons.matching(identifier: id).firstMatch
    }

    /// The first tile on the screen.
    var tile: XCUIElement {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tile.item.'")).firstMatch
    }

    @discardableResult
    func wait(_ element: XCUIElement, _ what: String, timeout: Double = 20, file: StaticString = #filePath,
              line: UInt = #line) -> XCUIElement {
        XCTAssertTrue(element.waitForExistence(timeout: timeout), "\(what) didn't show", file: file, line: line)
        return element
    }

    func tap(_ id: String, _ what: String, file: StaticString = #filePath, line: UInt = #line) {
        wait(button(id), what, file: file, line: line).tap()
    }

    /// A screenshot, kept with the test's results and, when `PLEY_SHOTS` is
    /// set, written there as `<device>-<name>.png`.
    func shot(_ name: String) {
        let screenshot = XCUIScreen.main.screenshot()
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = "\(device)-\(name)"
        attachment.lifetime = .keepAlways
        test.add(attachment)
        if let folder = Admin.shots {
            try? screenshot.pngRepresentation.write(to: URL(fileURLWithPath: folder)
                .appendingPathComponent("\(device)-\(name).png"))
        }
    }

    // MARK: Steps

    /// Signed out of an earlier run, if it left the app signed in.
    func startSignedOut() {
        let address = element("servers.address")
        if address.waitForExistence(timeout: 8) { return }
        if tile.exists || element("shell.me").exists || app.tabBars.buttons["Me"].exists {
            openMe()
            signOut()
        } else if element("profiles.profile.\(Admin.adminName)").exists {
            choose(Admin.adminName, pin: Admin.adminPin)
            openMe()
            signOut()
        }
        wait(address, "the servers")
    }

    /// Adds the server by its address and signs in with a code, which the
    /// administrator allows.
    func signInWithCode() async {
        let address = wait(element("servers.address"), "the address field")
        address.tap()
        address.typeText(Admin.origin + "\n")
        shot("01-sign-in")
        tap("signIn.code", "Sign in with a code")
        let code = wait(element("signIn.userCode"), "the code").label
        XCTAssertFalse(code.isEmpty, "a code shows")
        shot("02-code")
        let allowed = await Admin.allow(code: code)
        XCTAssertTrue(allowed, "the administrator allowed the code")
    }

    /// Chooses `name` in "Who's watching?" and types its PIN.
    func choose(_ name: String, pin: String) {
        let profile = wait(button("profiles.profile.\(name)"), "\(name) in Who's watching?", timeout: 30)
        profile.tap()
        let field = wait(element("profiles.pin"), "\(name)'s PIN")
        field.tap()
        field.typeText(pin)
        tap("profiles.pin.continue", "Continue")
    }

    /// Home, with its rows of tiles.
    func seeHome(_ who: String) {
        wait(tile, "\(who)'s Home", timeout: 30)
    }

    /// Opens the first tile and goes back.
    func openATile() {
        tile.tap()
        wait(button("item.play"), "the item's Play")
        shot("05-item")
        // Back is named after the page under it.
        wait(app.navigationBars.buttons["Home"], "Back").tap()
        seeHome("the same")
    }

    /// Me: the tab on iPhone, the sidebar's foot on iPad and Mac.
    func openMe() {
        let tab = app.tabBars.buttons["Me"]
        if tab.exists {
            tab.tap()
        } else {
            wait(element("shell.me"), "Me in the sidebar").tap()
        }
        wait(element("me.name"), "Me")
    }

    /// Home again: the tab on iPhone, the sidebar's row on iPad and Mac.
    func openHome() {
        let tab = app.tabBars.buttons["Home"]
        if tab.exists {
            tab.tap()
        } else {
            wait(element("shell.sidebar.home"), "Home in the sidebar").tap()
        }
    }

    func meSays(_ name: String, file: StaticString = #filePath, line: UInt = #line) {
        let label = wait(element("me.name"), "Me", file: file, line: line).label
        XCTAssertTrue(label.contains(name), "Me says \(label), not \(name)", file: file, line: line)
    }

    /// Sign out, confirmed; the servers show.
    func signOut() {
        wait(element("me.signOut"), "Sign out").tap()
        let confirm = app.buttons.matching(NSPredicate(format: "label == 'Sign out' AND identifier != 'me.signOut'"))
            .firstMatch
        wait(confirm, "Sign out's confirmation").tap()
        wait(element("servers.address"), "the servers after signing out")
    }
}
