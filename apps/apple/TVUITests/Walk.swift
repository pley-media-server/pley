import XCTest

/// Steps through Apple TV's screens with the remote, as a person does, by
/// the screens' accessibility identifiers (`<screen>.<part>[.<id>]`). The
/// focus moves one press at a time, checked after each.
@MainActor
struct Walk {
    let app: XCUIApplication
    let test: XCTestCase
    let remote = XCUIRemote.shared

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

    /// Moves the focus to `element`: along each direction in turn, a press at
    /// a time, until it has it.
    func focus(_ element: XCUIElement, _ what: String, first: [XCUIRemote.Button] = [.down, .right, .up, .left],
               file: StaticString = #filePath, line: UInt = #line) {
        wait(element, what, file: file, line: line)
        if focused(element) { return }
        if steer(to: element) { return }
        for direction in first + [.down, .right, .up, .left] {
            for _ in 0..<10 {
                remote.press(direction)
                if focused(element) { return }
            }
        }
        XCTFail("couldn't move the focus to \(what)", file: file, line: line)
    }

    /// Moves the focus toward `element` by where it is on the screen from
    /// what has the focus: a press up or down while it is in another row,
    /// left or right in its row. False when the focus stops moving, or the
    /// element is off the screen, where a lazy grid doesn't make it: the
    /// caller sweeps then. A fixed sweep misses a tile in the middle of a
    /// grid's top row, and where a title lands in a grid isn't fixed.
    func steer(to element: XCUIElement) -> Bool {
        let focus = app.descendants(matching: .any).matching(NSPredicate(format: "hasFocus == true")).firstMatch
        var last = CGRect.null
        for _ in 0..<30 {
            guard focus.exists, element.exists else { return false }
            let from = focus.frame, to = element.frame
            if from == last || to.isEmpty { return false }
            last = from
            let dx = to.midX - from.midX, dy = to.midY - from.midY
            if abs(dy) > max(from.height, to.height) / 2 {
                remote.press(dy > 0 ? .down : .up)
            } else if abs(dx) > max(from.width, to.width) / 2 {
                remote.press(dx > 0 ? .right : .left)
            } else {
                return focused(element)
            }
            if focused(element) { return true }
        }
        return false
    }

    /// Whether `element` has the focus. Asked of an element that is gone,
    /// such as a tile in a grid the page is drawing again after the
    /// server's news, `hasFocus` fails the test; this says no, and the
    /// next press finds the element drawn again.
    func focused(_ element: XCUIElement) -> Bool {
        element.exists && element.hasFocus
    }

    func select(_ element: XCUIElement, _ what: String, first: [XCUIRemote.Button] = [.down, .right, .up, .left],
                file: StaticString = #filePath, line: UInt = #line) {
        focus(element, what, first: first, file: file, line: line)
        remote.press(.select)
    }

    /// A screenshot, kept with the test's results and, when `PLEY_SHOTS` is
    /// set, written there as `tv-<name>.png`.
    func shot(_ name: String) {
        let screenshot = XCUIScreen.main.screenshot()
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = "tv-\(name)"
        attachment.lifetime = .keepAlways
        test.add(attachment)
        if let folder = Admin.shots {
            try? screenshot.pngRepresentation.write(to: URL(fileURLWithPath: folder)
                .appendingPathComponent("tv-\(name).png"))
        }
    }

    /// Types `text` into the focused field with the system keyboard, then
    /// leaves the keyboard.
    func type(_ text: String) {
        sleep(1)
        app.typeText(text)
        let done = app.buttons.matching(NSPredicate(format: "label ==[c] 'done'")).firstMatch
        if done.exists {
            select(done, "the keyboard's Done")
        } else {
            remote.press(.menu)
        }
        sleep(1)
    }

    // MARK: Steps

    /// Signed out of an earlier run, if it left the app signed in.
    func startSignedOut() {
        let add = button("servers.add")
        if add.waitForExistence(timeout: 8) { return }
        if tile.exists || app.tabBars.buttons["Settings"].exists {
            openSettings()
            signOut()
        } else if button("profiles.profile.\(Admin.adminName)").exists {
            choose(Admin.adminName, pin: Admin.adminPin)
            openSettings()
            signOut()
        }
        wait(add, "the servers")
    }

    /// At the administrator's Home: with the sign-in an earlier test kept,
    /// or a new one with a code. Only the sign-in walk starts at the
    /// servers; the others don't spend a sign-in each.
    func startSignedIn() async {
        let shown = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'tile.item.' OR identifier BEGINSWITH 'profiles.profile.' "
                + "OR identifier == 'servers.add'")).firstMatch
        _ = shown.waitForExistence(timeout: 20)
        if button("profiles.profile.\(Admin.adminName)").exists {
            chooseSignedIn()
        } else if !tile.exists {
            startSignedOut()
            await signInWithCode()
            chooseSignedIn()
        }
        seeHome(Admin.adminName)
    }

    /// Adds the server by its address; the sign-in code shows, and the
    /// administrator allows it.
    func signInWithCode() async {
        select(button("servers.add"), "Add a server")
        select(element("servers.address"), "the address field")
        type(Admin.origin)
        shot("01-add-server")
        let connect = button("servers.connect")
        if connect.exists { select(connect, "Connect") }
        let code = wait(element("signIn.userCode"), "the code", timeout: 30).label
        XCTAssertFalse(code.isEmpty, "a code shows")
        shot("02-code")
        let allowed = await Admin.allow(code: code)
        XCTAssertTrue(allowed, "the administrator allowed the code")
    }

    /// Chooses `name` in "Who's watching?" and types its PIN.
    func choose(_ name: String, pin: String) {
        select(button("profiles.profile.\(name)"), "\(name) in Who's watching?", first: [.right, .left])
        select(element("profiles.pin"), "\(name)'s PIN")
        type(pin)
        let next = button("profiles.pin.continue")
        if next.exists { select(next, "Continue") }
    }

    /// Home, with its rows of tiles.
    func seeHome(_ who: String) {
        wait(tile, "\(who)'s Home", timeout: 30)
    }

    /// Opens the first tile and comes back with Menu.
    func openATile() {
        select(tile, "a tile", first: [.down, .right])
        wait(button("item.play"), "the item's Play")
        shot("05-item")
        remote.press(.menu)
        seeHome("the same")
    }

    /// The tab bar, which hides while the focus is down in a page: Up until
    /// it shows.
    func showTabs() {
        for _ in 0..<8 where !app.tabBars.buttons.firstMatch.exists {
            remote.press(.up)
            _ = app.tabBars.buttons.firstMatch.waitForExistence(timeout: 1)
        }
    }

    /// The Settings tab, which holds the profile and Sign out.
    func openSettings() {
        showTabs()
        let tab = app.tabBars.buttons["Settings"]
        focus(tab, "the Settings tab", first: [.up, .right])
        remote.press(.select)
        wait(element("me.name"), "Settings")
    }

    func openHome() {
        showTabs()
        let tab = app.tabBars.buttons["Home"]
        focus(tab, "the Home tab", first: [.up, .left])
        remote.press(.select)
    }

    func meSays(_ name: String, file: StaticString = #filePath, line: UInt = #line) {
        let label = wait(element("me.name"), "Settings", file: file, line: line).label
        XCTAssertTrue(label.contains(name), "Settings says \(label), not \(name)", file: file, line: line)
    }

    /// Sign out, confirmed; the servers show.
    func signOut() {
        focusDown(element("me.signOut"), "Sign out")
        remote.press(.select)
        let confirm = app.alerts.buttons["Sign out"].firstMatch
        select(confirm, "Sign out's confirmation", first: [.up, .down])
        wait(button("servers.add"), "the servers after signing out")
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
