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

    /// The servers: the address field, or a server kept from a test
    /// before, which at the largest text sizes pushes the field below the
    /// screen, out of the list.
    var servers: XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'servers.address' OR identifier BEGINSWITH 'servers.server.'"))
            .firstMatch
    }

    /// Signed out of an earlier run, if it left the app signed in.
    func startSignedOut() {
        let address = servers
        if address.waitForExistence(timeout: 8) { return }
        // Signed out at the default text size, where Me is in reach (a test
        // run again after a failure finds the app as that one left it).
        let arguments = app.launchArguments
        if !arguments.isEmpty {
            app.terminate()
            app.launchArguments = []
            app.launch()
            defer {
                app.terminate()
                app.launchArguments = arguments
                app.launch()
                wait(address, "the servers")
            }
            if address.waitForExistence(timeout: 8) { return }
        }
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

    /// At the administrator's Home: with the sign-in an earlier test kept,
    /// or a new one with a code. Only the sign-in walk starts at the
    /// servers; the others don't spend a sign-in each.
    func startSignedIn() async {
        // The app opens where the test before left it: Home, another tab
        // or the sidebar's section, "Who's watching?", or the servers.
        let shown = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'tile.item.' OR identifier BEGINSWITH 'profiles.profile.' "
                + "OR identifier BEGINSWITH 'shell.' "
                + "OR identifier == 'servers.address' OR identifier BEGINSWITH 'servers.server.'")).firstMatch
        let tabs = app.tabBars.firstMatch
        for _ in 0..<40 where !shown.exists && !tabs.exists {
            usleep(500_000)
        }
        let signedIn = tile.exists || element("shell.me").exists || element("shell.sidebar.home").exists
            || app.tabBars.buttons["Home"].exists
        if button("profiles.profile.\(Admin.adminName)").exists {
            chooseSignedIn()
        } else if signedIn {
            openHome()
        } else {
            startSignedOut()
            await signInWithCode()
            chooseSignedIn()
        }
        seeHome(Admin.adminName)
    }

    /// Adds the server by its address and signs in with a code, which the
    /// administrator allows.
    func signInWithCode() async {
        let address = reveal(element("servers.address"), "the address field")
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
        let section = openSection("Me", sidebar: "shell.me")
        let name = element("me.name")
        // Me keeps the pages opened in it (Downloads, Settings): choosing it
        // again, while it shows, goes back to its start.
        if !name.waitForExistence(timeout: 2), app.navigationBars.buttons["Me"].exists {
            section.tap()
        }
        // Me keeps its scroll: its first row may be scrolled away, out of its lazy list.
        for _ in 0..<6 where !name.waitForExistence(timeout: 2) {
            scrollPage(up: false)
        }
        wait(name, "Me")
    }

    /// Scrolls the page a screen's half, up (`true`: to what is below) or
    /// down, dragging inside the app's window: a swipe on the whole app can
    /// start at the screen's bottom edge, where it opens the iPad's Dock,
    /// which then covers the rows below. Right of the middle, clear of the
    /// iPad's sidebar.
    func scrollPage(up: Bool) {
        let window = app.windows.firstMatch
        let low = window.coordinate(withNormalizedOffset: CGVector(dx: 0.7, dy: 0.7))
        let high = window.coordinate(withNormalizedOffset: CGVector(dx: 0.7, dy: 0.3))
        if up {
            low.press(forDuration: 0.05, thenDragTo: high)
        } else {
            high.press(forDuration: 0.05, thenDragTo: low)
        }
    }

    /// Whether `element` can be tapped, and on the iPad clear of the
    /// screen's last 140 points, where the Dock can show over the app.
    func inReach(_ element: XCUIElement) -> Bool {
        guard element.exists, element.isHittable else { return false }
        guard UIDevice.current.userInterfaceIdiom == .pad else { return true }
        return element.frame.maxY < app.windows.firstMatch.frame.maxY - 140
    }

    /// The tab called `tab` on iPhone, the sidebar's row `sidebar` on iPad.
    /// The tab bar is waited for: at the accessibility text sizes it can
    /// be slow to settle, and a tab checked too early looks missing.
    @discardableResult
    func openSection(_ tab: String, sidebar: String) -> XCUIElement {
        let target = UIDevice.current.userInterfaceIdiom == .pad
            ? sidebarRow(sidebar, "\(tab) in the sidebar")
            : wait(app.tabBars.buttons[tab], "the \(tab) tab")
        target.tap()
        return target
    }

    /// The sidebar's row `id`, scrolled to. At the accessibility text
    /// sizes the sidebar runs past the screen, and its list makes only the
    /// rows near what shows: it is dragged up, then down, until the row is
    /// there.
    func sidebarRow(_ id: String, _ what: String, timeout: Double = 20, file: StaticString = #filePath,
                    line: UInt = #line) -> XCUIElement {
        let row = element(id)
        let anyRow = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'shell.sidebar.'")).firstMatch
        let shown = row.waitForExistence(timeout: 3) || anyRow.waitForExistence(timeout: timeout)
        if shown, !inReach(row) {
            let window = app.windows.firstMatch
            let x = anyRow.exists ? anyRow.frame.midX / max(window.frame.width, 1) : 0.1
            let low = window.coordinate(withNormalizedOffset: CGVector(dx: x, dy: 0.7))
            let high = window.coordinate(withNormalizedOffset: CGVector(dx: x, dy: 0.35))
            for step in 0..<18 where !inReach(row) {
                if step < 6 {
                    low.press(forDuration: 0.05, thenDragTo: high)
                } else {
                    high.press(forDuration: 0.05, thenDragTo: low)
                }
            }
        }
        return wait(row, what, timeout: inReach(row) ? 1 : timeout, file: file, line: line)
    }

    /// Home again: the tab on iPhone, the sidebar's row on iPad and Mac.
    func openHome() {
        openSection("Home", sidebar: "shell.sidebar.home")
    }

    func meSays(_ name: String, file: StaticString = #filePath, line: UInt = #line) {
        let label = wait(element("me.name"), "Me", file: file, line: line).label
        XCTAssertTrue(label.contains(name), "Me says \(label), not \(name)", file: file, line: line)
    }

    /// Sign out, confirmed; the servers show.
    func signOut() {
        reveal(element("me.signOut"), "Sign out").tap()
        let confirm = app.buttons.matching(NSPredicate(format: "label == 'Sign out' AND identifier != 'me.signOut'"))
            .firstMatch
        wait(confirm, "Sign out's confirmation").tap()
        wait(servers, "the servers after signing out")
    }

    /// Sign out, checked on the server too: the app's grant is gone from
    /// the administrator's signed-in apps, not only from the screen.
    func signOutEndingTheGrant() async {
        let before = await Admin.appleGrants()
        XCTAssertFalse(before.isEmpty, "the app's grant is listed before signing out")
        signOut()
        await eventually("the app's grant is gone from the server") {
            let now = await Admin.appleGrants()
            return now.count < before.count && now.isSubset(of: before)
        }
    }
}
