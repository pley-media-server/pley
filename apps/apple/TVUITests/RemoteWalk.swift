import XCTest

/// The remote on Home: Play/Pause plays the tile with the focus, and a row
/// keeps the tile it had the focus on when the focus comes back to it.
extension Walk {
    /// The button with the focus.
    var focused: XCUIElement {
        app.buttons.matching(NSPredicate(format: "hasFocus == true")).firstMatch
    }

    /// Home's Recently Added in Movies, its first tile with the focus:
    /// from the Home tab (after a menu's action the focus may be nowhere
    /// the remote can reach from), down to the row, then left to its start.
    private func focusMoviesRow(_ movies: Int, file: StaticString = #filePath,
                                line: UInt = #line) -> XCUIElement {
        openHome()
        let row = wait(element("home.row.recently_added:\(movies)"), "Recently Added in Movies")
        let inRow = row.buttons.matching(NSPredicate(format: "hasFocus == true"))
        // The focus may come down to the rows by itself once the tab is chosen.
        sleep(1)
        for _ in 0..<6 where inRow.count == 0 { press(.down) }
        for _ in 0..<6 where inRow.count == 0 { press(.up) }
        XCTAssertGreaterThan(inRow.count, 0, "the focus reached Recently Added in Movies", file: file, line: line)
        let first = row.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tile.item.'")).firstMatch
        for _ in 0..<10 where !first.hasFocus { press(.left) }
        XCTAssertTrue(first.hasFocus, "the row's first tile has the focus", file: file, line: line)
        return row
    }

    /// Along the movies' row, down a row and back up: the focus is on the
    /// tile it left.
    func rowKeepsItsFocus(movies: Int) {
        let row = focusMoviesRow(movies)
        // The second movie: the row has two, then See all.
        press(.right)
        let left = focused.identifier
        XCTAssertTrue(left.hasPrefix("tile.item."), "a tile has the focus, not \(left)")
        press(.down)
        XCTAssertFalse(row.buttons[left].hasFocus, "the focus went down a row")
        press(.up)
        XCTAssertTrue(row.buttons[left].hasFocus, "back in the row, \(left) has the focus, not \(focused.identifier)")
        shot("08-row-focus")
    }

    /// Play/Pause on a movie's tile opens the player; Menu closes it and
    /// Home is back.
    func playPauseOnATile(movies: Int) async {
        let row = focusMoviesRow(movies)
        let tile = row.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tile.item.'")).firstMatch
        remote.press(.playPause)
        await eventually("Play/Pause opens the player") { !(tile.exists && tile.isHittable) }
        shot("08-play-pause")
        sleep(2)
        // Menu closes the player; a first press may only hide its controls.
        for _ in 0..<3 where !(tile.exists && tile.isHittable) {
            remote.press(.menu)
            for _ in 0..<6 where !(tile.exists && tile.isHittable) { usleep(500_000) }
        }
        XCTAssertTrue(tile.exists && tile.isHittable, "Home is back after the player")
    }
}
