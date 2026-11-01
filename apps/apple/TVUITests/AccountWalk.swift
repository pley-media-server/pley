import XCTest

/// The steps of the walk through Search and Settings on Apple TV: a search
/// and its recent searches, the profile's track defaults (checked on the
/// server), History, quality and subtitle size, Diagnostics, About, and
/// Always start as across a relaunch.
extension Walk {
    /// Moves the focus down Settings' long column, a press at a time, until
    /// `element` has it.
    func focusDown(_ element: XCUIElement, _ what: String, file: StaticString = #filePath, line: UInt = #line) {
        wait(element, what, file: file, line: line)
        for direction in [XCUIRemote.Button.down, .up] {
            for _ in 0..<30 where !element.hasFocus {
                remote.press(direction)
            }
            if element.hasFocus { return }
        }
        XCTFail("couldn't move the focus to \(what)", file: file, line: line)
    }

    /// Picks `choice` in the list a setting's row opens.
    func pick(_ id: String, _ choice: String, _ what: String, file: StaticString = #filePath, line: UInt = #line) {
        focusDown(element(id), what, file: file, line: line)
        remote.press(.select)
        let button = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", choice)).firstMatch
        focus(button, choice, first: [.down, .up], file: file, line: line)
        remote.press(.select)
        sleep(1)
    }

    /// The Search tab, first of all.
    func openSearch() {
        let tab = app.tabBars.buttons["Search"]
        focus(tab, "the Search tab", first: [.up, .left])
        remote.press(.select)
    }

    /// Searches "clip", opens a movie found and comes back; with the field
    /// emptied, the search is among the recent ones.
    func search() {
        openSearch()
        // The system's keyboard row, under the field, takes the typing.
        focus(app.keyboards.firstMatch, "the keyboard", first: [.down])
        app.typeText("clip")
        let group = wait(element("search.group.movies"), "the Movies found", timeout: 30)
        let tile = group.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tile.item.'")).firstMatch
        focus(tile, "a movie found", first: [.down])
        shot("40-search")
        remote.press(.select)
        wait(button("item.play"), "the movie's page")
        remote.press(.menu)
        focus(app.keyboards.firstMatch, "the keyboard", first: [.up])
        app.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 4))
        let recent = wait(button("search.recent.0"), "the recent search")
        XCTAssertTrue(recent.label.contains("clip"), "the recent search is \(recent.label)")
        // Down from the keyboard lands under the key last used, Clear here.
        focus(recent, "the recent search", first: [.down, .left])
        shot("41-recent")
    }

    /// Subtitles to Always, checked on the server, then back as it was.
    func trackDefaults() async {
        openSettings()
        pick("me.subtitleMode", "Always", "Subtitles")
        await eventually("the server has subtitles always") { await Admin.subtitleMode() == "always" }
        wait(element("me.subtitleLanguage"), "Subtitle language")
        shot("42-track-defaults")
        await Admin.resetTrackDefaults()
    }

    /// History lists this profile's plays, or says there are none.
    func history() {
        openSettings()
        focusDown(element("me.history"), "History")
        remote.press(.select)
        let list = element("history.list"), empty = element("history.empty")
        XCTAssertTrue(list.waitForExistence(timeout: 20) || empty.exists, "History didn't show")
        shot("43-history")
        remote.press(.menu)
        wait(element("me.name"), "Settings")
    }

    /// Quality 720p and subtitles at 150 %, then Diagnostics and About.
    func settings() {
        openSettings()
        pick("settings.quality", "4 Mbps 720p", "Quality")
        XCTAssertEqual(element("settings.quality").value as? String, "4 Mbps 720p", "Quality says 720p")
        pick("settings.subtitleSize", "150 %", "Size")
        XCTAssertEqual(element("settings.subtitleSize").value as? String, "150 %", "Size says 150 %")
        shot("44-settings")
        focusDown(element("settings.diagnostics"), "Diagnostics")
        remote.press(.select)
        wait(element("diagnostics.json"), "the caps report")
        shot("45-diagnostics")
        remote.press(.menu)
        focusDown(element("settings.about"), "About")
        remote.press(.select)
        let version = wait(element("about.version"), "the version")
        XCTAssertTrue(version.label.hasPrefix("Pley "), "About says \(version.label)")
        shot("46-about")
        remote.press(.menu)
        wait(element("me.name"), "Settings")
    }

    /// Relaunched, the app asks who's watching; with Always start as the
    /// administrator, it opens signed in as them instead.
    func alwaysStartAs() {
        app.terminate()
        app.launch()
        wait(button("profiles.profile.Kids"), "Who's watching? at start", timeout: 30)
        shot("47-start-profiles")
        chooseAtStart(Admin.adminName, pin: Admin.adminPin)
        // The app opens on the section shown last, Settings here.
        openSettings()
        meSays(Admin.adminName)
        pick("settings.startAs", Admin.adminName, "Always start as")
        XCTAssertEqual(element("settings.startAs").value as? String, Admin.adminName, "Always start as is set")
        app.terminate()
        app.launch()
        wait(app.tabBars.buttons["Settings"], "the tabs at start", timeout: 30)
        XCTAssertFalse(button("profiles.profile.Kids").exists, "Who's watching? stays away")
        shot("48-start-signed-in")
        openSettings()
        meSays(Admin.adminName)
        pick("settings.startAs", "Ask who's watching", "Always start as")
        // Up to the tabs, from the foot of Settings' long column.
        remote.press(.menu)
    }

    /// Chooses `name` in "Who's watching?" at start: the profile in use goes
    /// on without its PIN, as Switch profile's Menu does; another asks for it.
    func chooseAtStart(_ name: String, pin: String) {
        select(button("profiles.profile.\(name)"), "\(name) in Who's watching?", first: [.right, .left])
        let field = element("profiles.pin")
        guard field.waitForExistence(timeout: 5) else { return }
        select(field, "\(name)'s PIN")
        type(pin)
        let next = button("profiles.pin.continue")
        if next.exists { select(next, "Continue") }
    }
}
