import XCTest

/// The browsing steps of Apple TV's walk: Home's See all, a title's
/// actions, and a library tab's sort, filter and letters.
extension Walk {
    /// A button by its identifier, or else by what it says (a menu's
    /// items are found by their words).
    func button(_ id: String, or label: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "identifier == %@ OR label == %@", id, label))
            .firstMatch
    }

    /// Whether something on the screen says `text` (a menu's own label isn't read).
    func says(_ text: String, timeout: Double = 5) -> Bool {
        app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", text)).firstMatch
            .waitForExistence(timeout: timeout)
    }

    /// Whether `element`'s accessibility value comes to be `value` within 10 s.
    func valueBecomes(_ element: XCUIElement, _ value: String) -> Bool {
        let predicate = NSPredicate(format: "value == %@", value)
        return XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: predicate, object: element)],
                                timeout: 10) == .completed
    }

    /// Home's first See all goes to its library, listed newest first.
    func seeAll() {
        let seeAll = app.buttons.matching(NSPredicate(format: "identifier ENDSWITH '.seeAll'")).firstMatch
        select(seeAll, "See all", first: [.down, .right])
        let sort = wait(element("library.sort"), "the library's Sort")
        // Assistive technology reads the menus by name, not only what they show.
        XCTAssertEqual(sort.label, "Sort", "Sort has its name")
        let filter = element("library.filter")
        if filter.exists { XCTAssertEqual(filter.label, "Filter", "Filter has its name") }
        shot("08-see-all")
        XCTAssertTrue(valueBecomes(sort, "Date added"), "See all lists newest first")
        remote.press(.menu)
        seeHome("the same")
    }

    /// Long press on the first movie of a row of Home's, then the menu's
    /// item `words`, found by its words: a movie part way through has
    /// Resume and Play from start where another has Play.
    private func act(_ tile: XCUIElement, _ words: String, shot name: String? = nil) {
        // Home reads its rows again after each action: once it has, up to
        // the tabs and down again lands on the first row's first tile.
        sleep(1)
        if !focused(tile) {
            for _ in 0..<4 { remote.press(.up) }
            remote.press(.down)
        }
        focus(tile, "the movie", first: [.left, .down])
        // `hasFocus` can say yes of the first tile while its neighbour has
        // the focus. The tile is its row's first: Left from anywhere in the
        // row comes to it, and stops there.
        press(.left, 3)
        remote.press(.select, forDuration: 1.5)
        wait(app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", words)).firstMatch, words)
        if let name { shot(name) }
        // The focus is on a cell around the item, which has no words of
        // its own: down from the first until the cell holds `words`.
        let item = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", words)).firstMatch
        let focus = app.descendants(matching: .any).matching(NSPredicate(format: "hasFocus == true")).firstMatch
        func onItem() -> Bool {
            guard item.exists, focus.exists else { return false }
            let middle = CGPoint(x: item.frame.midX, y: item.frame.midY)
            return focus.frame.contains(middle)
        }
        for _ in 0..<6 where !onItem() { press(.down) }
        XCTAssertTrue(onItem(), "\(words) has the focus")
        remote.press(.select)
    }

    /// Mark as watched, and Add to My List, each checked on the server.
    func itemActions(movies: Int) async {
        // Back from See all, Home may show rows further down: up to the first.
        let rowElement = element("home.row.recently_added:\(movies)")
        for _ in 0..<8 where !rowElement.exists { remote.press(.up) }
        let row = wait(rowElement, "Recently Added in Movies")
        let tile = row.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tile.item.'")).firstMatch
        wait(tile, "a movie on Home")
        let id = Int(tile.identifier.dropFirst("tile.item.".count)) ?? 0
        act(tile, "Mark as watched", shot: "09-actions")
        await eventually("the server has it watched") { await Admin.watched(id) == true }
        act(tile, "Add to My List")
        await eventually("My List has it") { await Admin.watchlist().contains(id) }
        // Back as it was, as the administrator.
        _ = await Admin.call("POST", "/api/items/\(id)/unwatched")
        _ = await Admin.call("DELETE", "/api/watchlist/\(id)")
    }

    /// A library's page: its tab, or, when the tabs ran out, its row under Libraries.
    func openLibraryTab(_ name: String) {
        let tab = app.tabBars.buttons[name]
        if tab.waitForExistence(timeout: 5) {
            focus(tab, "the \(name) tab", first: [.up, .right, .left])
            remote.press(.select)
            // A library's tab chosen, the bar still fits: Search hasn't gone off the left.
            tabsFit()
        } else {
            let libraries = app.tabBars.buttons["Libraries"]
            focus(libraries, "the Libraries tab", first: [.up, .right, .left])
            remote.press(.select)
            let row = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'libraries.row.' AND label BEGINSWITH %@",
                                                       name)).firstMatch
            select(row, "\(name) under Libraries", first: [.down])
        }
        wait(element("library.tab.library"), "\(name)'s page")
    }

    /// Presses `button` `times` times, a moment apart.
    func press(_ button: XCUIRemote.Button, _ times: Int = 1) {
        for _ in 0..<times {
            remote.press(button)
            usleep(400_000)
        }
    }

    /// Down to the grid's first tile.
    func toFirstTile() {
        let tile = self.tile
        wait(tile, "a tile")
        for _ in 0..<6 where !tile.hasFocus { press(.down) }
        XCTAssertTrue(tile.hasFocus, "the first tile has the focus")
    }

    /// Sort by year, the other way, Unwatched, and Clear. A menu doesn't
    /// say it has the focus, so the walk counts presses: up from the first
    /// tile is Sort, and a menu opens on its first item.
    func sortAndFilter() {
        toFirstTile()
        press(.up)
        press(.select)
        press(.down)
        press(.select)
        XCTAssertTrue(valueBecomes(element("library.sort"), "Year"), "sorted by year")
        press(.right)
        press(.select)
        XCTAssertTrue(says("Ascending"), "the other way")
        press(.right)
        press(.select)
        press(.down)
        press(.select)
        XCTAssertTrue(valueBecomes(element("library.filter"), "Unwatched"), "filtered")
        shot("11-library")
        press(.right)
        press(.select)
        XCTAssertTrue(valueBecomes(element("library.filter"), ""), "the filters cleared")
    }

    /// Clips, sorted by title: C, three along from "#" in the row above the
    /// grid, goes to Clip 01.
    func jumpByLetter() {
        wait(element("library.alpha.C"), "the letters")
        toFirstTile()
        press(.up)
        // To "#", then along to C.
        press(.left, 6)
        press(.right, 3)
        press(.select)
        let first = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Clip 01'")).firstMatch
        wait(first, "Clip 01")
        shot("12-letters")
    }

    /// Waits up to 10 seconds for `condition`, asked once a second.
    func eventually(_ what: String, file: StaticString = #filePath, line: UInt = #line,
                    _ condition: () async -> Bool) async {
        for _ in 0..<10 {
            if await condition() { return }
            try? await Task.sleep(for: .seconds(1))
        }
        XCTFail("\(what): not after 10 seconds", file: file, line: line)
    }
}
