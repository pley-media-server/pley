import XCTest

/// The browsing steps of the walk: Home's See all, a title's actions, a
/// library's sort, filter and A to Z, and the libraries' order.
extension Walk {
    /// A button by its identifier, or else by what it says (a menu's
    /// items are found by their words).
    func button(_ id: String, or label: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "identifier == %@ OR label == %@", id, label)).firstMatch
    }

    var wide: Bool { UIDevice.current.userInterfaceIdiom == .pad }

    /// How the library page says its titles are listed.
    var listing: String { wait(element("library.listing"), "how the titles are listed").label }

    /// Home's first See all goes to its library, listed newest first.
    func seeAll() {
        let seeAll = app.buttons.matching(NSPredicate(format: "identifier ENDSWITH '.seeAll'")).firstMatch
        wait(seeAll, "See all").tap()
        wait(element("library.sort"), "the library's Sort")
        XCTAssertTrue(listing.hasPrefix("Date added, descending"), "See all lists newest first: \(listing)")
        shot("08-see-all")
        wait(app.navigationBars.buttons["Home"], "Back").tap()
        seeHome("the same")
    }

    /// A movie of Home's: Mark as watched and back, and My List and back,
    /// from its context menu, each checked on the server.
    func itemActions(movies: Int) async {
        let row = wait(element("home.row.recently_added:\(movies)"), "Recently Added in Movies")
        let tile = row.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tile.item.'")).firstMatch
        wait(tile, "a movie on Home")
        let id = Int(tile.identifier.dropFirst("tile.item.".count)) ?? 0

        tile.press(forDuration: 1.2)
        wait(button("actions.markWatched", or: "Mark as watched"), "Mark as watched")
        shot("09-actions")
        button("actions.markWatched", or: "Mark as watched").tap()
        await eventually("the server has it watched") { await Admin.watched(id) == true }
        tile.press(forDuration: 1.2)
        wait(button("actions.markUnwatched", or: "Mark as unwatched"), "Mark as unwatched").tap()
        await eventually("the server has it unwatched") { await Admin.watched(id) == false }

        tile.press(forDuration: 1.2)
        wait(button("actions.addToMyList", or: "Add to My List"), "Add to My List").tap()
        await eventually("My List has it") { await Admin.watchlist().contains(id) }
        tile.press(forDuration: 1.2)
        wait(button("actions.removeFromMyList", or: "Remove from My List"), "Remove from My List").tap()
        await eventually("My List doesn't have it") { await !Admin.watchlist().contains(id) }
    }

    /// The library `id`'s page: from the Libraries tab on iPhone, the sidebar on iPad.
    func openLibrary(_ id: Int, _ name: String) {
        if wide {
            wait(element("shell.sidebar.library.\(id)"), "\(name) in the sidebar").tap()
        } else {
            let tab = app.tabBars.buttons["Libraries"]
            wait(tab, "the Libraries tab").tap()
            // Twice goes back to the list from a library left open.
            if !element("libraries.row.\(id)").waitForExistence(timeout: 3) { tab.tap() }
            wait(element("libraries.row.\(id)"), "\(name) in Libraries").tap()
        }
        wait(element("library.sort"), "\(name)'s page")
    }

    /// Sort by year, the other way, Unwatched, and Clear.
    func sortAndFilter() {
        let sort = element("library.sort")
        sort.tap()
        wait(app.buttons["Year"], "Year in Sort").tap()
        XCTAssertTrue(listing.hasPrefix("Year, descending"), "sorted by year: \(listing)")
        sort.tap()
        wait(app.buttons["Ascending"], "Ascending in Sort").tap()
        XCTAssertTrue(listing.hasPrefix("Year, ascending"), "the other way: \(listing)")

        let filter = element("library.filter")
        filter.tap()
        wait(app.buttons["Unwatched"], "Unwatched in Filter").tap()
        shot("10-filters")
        if !wide { tap("library.filter.done", "Done") }
        XCTAssertEqual(listing, "Year, ascending · Unwatched", "filtered")
        shot("11-library")
        filter.tap()
        if wide {
            wait(app.buttons["Clear Filters"], "Clear Filters").tap()
        } else {
            tap("library.filter.clear", "Clear Filters")
            tap("library.filter.done", "Done")
        }
        XCTAssertEqual(listing, "Year, ascending", "the filters cleared")
    }

    /// Clips, sorted by title: down the grid, then C goes back to Clip 01.
    func jumpByLetter() {
        let letter = wait(element("library.alpha.C"), "the letters")
        let grid = element("library.grid")
        for _ in 0..<3 { grid.swipeUp() }
        let first = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Clip 01'")).firstMatch
        XCTAssertFalse(first.exists && first.isHittable, "the grid went down")
        letter.tap()
        XCTAssertTrue(first.waitForExistence(timeout: 5) && first.isHittable, "C went to Clip 01")
        shot("12-letters")
    }

    /// Edit, and the first library dragged to the end: the server keeps
    /// the profile's new order. The server's order again afterwards.
    func reorder() async {
        let before = await Admin.libraries().map(\.id)
        if wide {
            tap("shell.sidebar.edit", "Edit in the sidebar")
        } else {
            let tab = app.tabBars.buttons["Libraries"]
            wait(tab, "the Libraries tab").tap()
            if !element("libraries.list").waitForExistence(timeout: 3) { tab.tap() }
            tap("libraries.edit", "Edit")
        }
        let handles = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Reorder'"))
        wait(handles.firstMatch, "the rows' handles")
        let first = handles.element(boundBy: 0)
        let last = handles.element(boundBy: handles.count - 1)
        first.press(forDuration: 0.6, thenDragTo: last)
        shot("13-libraries")
        await eventually("the server has the new order") {
            let now = await Admin.libraries().map(\.id)
            return now != before && Set(now) == Set(before)
        }
        app.buttons.matching(NSPredicate(format: "label == 'Done'")).firstMatch.tap()
        await Admin.resetLibraryOrder()
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
