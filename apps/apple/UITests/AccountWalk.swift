import XCTest

/// The steps of the walk through Search and Me: a search and its recent
/// searches, the profile's track defaults (checked on the server), History,
/// Settings' quality and subtitle size, Diagnostics and About.
extension Walk {
    /// Search: the tab on iPhone, the sidebar's row on iPad.
    func openSearch() {
        let tab = app.tabBars.buttons["Search"]
        if tab.exists {
            tab.tap()
        } else {
            wait(element("shell.sidebar.search"), "Search in the sidebar").tap()
        }
    }

    /// Searches "clip", opens a movie found and comes back; the search is
    /// then among the recent ones.
    func search() {
        openSearch()
        let field = wait(app.searchFields.firstMatch, "the search field")
        field.tap()
        field.typeText("clip")
        let group = wait(element("search.group.movies"), "the Movies found", timeout: 30)
        XCTAssertTrue(group.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Movies ('")).firstMatch.exists,
                      "the group says its count")
        shot("40-search")
        let tile = group.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tile.item.'")).firstMatch
        wait(tile, "a movie found").tap()
        wait(button("item.play"), "the movie's page")
        wait(app.navigationBars.buttons["Search"], "Back").tap()
        // Emptied, the field shows the recent searches.
        let clear = app.searchFields.firstMatch.buttons.firstMatch
        if clear.exists { clear.tap() } else { field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 4)) }
        let recent = wait(button("search.recent.0"), "the recent search")
        XCTAssertTrue(recent.label.contains("clip"), "the recent search is \(recent.label)")
        shot("41-recent")
        // Leaves the field, so the keyboard no longer covers the tab bar.
        let cancel = app.buttons.matching(NSPredicate(format: "label IN {'Cancel', 'Close'}")).firstMatch
        if cancel.exists { cancel.tap() }
    }

    /// Scrolls the page up until `element` can be tapped.
    @discardableResult
    func reveal(_ element: XCUIElement, _ what: String, file: StaticString = #filePath,
                line: UInt = #line) -> XCUIElement {
        for _ in 0..<8 where !(element.exists && element.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(element.exists && element.isHittable, "\(what) didn't show", file: file, line: line)
        return element
    }

    /// Me's Subtitles to Always, checked on the server, then back as it was.
    func trackDefaults() async {
        openMe()
        reveal(element("me.subtitleMode"), "Subtitles in Me").tap()
        wait(app.buttons["Always"], "Always").tap()
        await eventually("the server has subtitles always") { await Admin.subtitleMode() == "always" }
        // Back to Me, where the subtitle language shows for Always.
        if !element("me.name").exists, app.navigationBars.buttons["Me"].exists {
            app.navigationBars.buttons["Me"].tap()
        }
        wait(element("me.subtitleLanguage"), "Subtitle language")
        shot("42-track-defaults")
        await Admin.resetTrackDefaults()
    }

    /// History lists this profile's plays, or says there are none.
    func history() {
        openMe()
        reveal(element("me.history"), "History").tap()
        let list = element("history.list"), empty = element("history.empty")
        XCTAssertTrue(list.waitForExistence(timeout: 20) || empty.exists, "History didn't show")
        shot("43-history")
        wait(app.navigationBars.buttons["Me"], "Back").tap()
    }

    /// Settings: Quality on Wi-Fi 720p and subtitles at 150 %, then
    /// Diagnostics' report and About's version.
    func settings() {
        openMe()
        reveal(element("me.settings"), "Settings").tap()
        let quality = wait(element("settings.quality"), "Quality")
        quality.tap()
        wait(app.buttons["4 Mbps 720p"], "4 Mbps 720p").tap()
        XCTAssertTrue(wait(element("settings.quality"), "Quality").label.contains("720p")
                      || app.staticTexts["4 Mbps 720p"].exists, "Quality says 720p")
        let size = reveal(element("settings.subtitleSize"), "Subtitle size")
        size.tap()
        wait(app.buttons["150 %"], "150 %").tap()
        XCTAssertTrue(app.staticTexts["150 %"].exists || element("settings.subtitleSize").label.contains("150"),
                      "Size says 150 %")
        shot("44-settings")
        reveal(element("settings.diagnostics"), "Diagnostics").tap()
        let json = wait(element("diagnostics.json"), "the caps report")
        XCTAssertTrue(json.label.hasPrefix("{"), "the report is JSON")
        shot("45-diagnostics")
        wait(app.navigationBars.buttons["Settings"], "Back").tap()
        reveal(element("settings.about"), "About").tap()
        let version = wait(element("about.version"), "the version")
        XCTAssertTrue(version.label.hasPrefix("Pley "), "About says \(version.label)")
        shot("46-about")
        wait(app.navigationBars.buttons["Settings"], "Back").tap()
        wait(app.navigationBars.buttons["Me"], "Back").tap()
    }
}
