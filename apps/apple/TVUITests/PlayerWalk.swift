import AVKit
import XCTest

/// The player's extras on Apple TV: Find Subtitles… in the transport
/// bar's Subtitles menu (the test server has no subtitle search set up),
/// Speed, and Picture in Picture keeping the playback.
extension Walk {
    /// A transport bar menu, by its name.
    func transportMenu(_ name: String) -> XCUIElement {
        app.cells.matching(NSPredicate(format: "label == %@", name)).firstMatch
    }

    /// A row of an open menu, by its first words ("Find Subtitles…, Search").
    func menuRow(_ words: String, focused: Bool = false) -> XCUIElement {
        let format = focused ? "label BEGINSWITH %@ AND hasFocus == true" : "label BEGINSWITH %@"
        return app.cells.matching(NSPredicate(format: format, words)).firstMatch
    }

    /// Down an open menu to `words`, and chooses it. The menu is drawn
    /// twice in the tree, so the row is found by its focus.
    func chooseRow(_ words: String, file: StaticString = #filePath, line: UInt = #line) {
        wait(menuRow(words), "\(words) in the menu", file: file, line: line)
        for _ in 0..<10 where !menuRow(words, focused: true).exists { remote.press(.down) }
        XCTAssertTrue(menuRow(words, focused: true).exists, "\(words) has the focus", file: file, line: line)
        remote.press(.select)
    }

    /// Shows the transport bar (a press of Up, or Down then Up from the
    /// info panel) and opens `name`'s menu.
    func openTransportMenu(_ name: String, file: StaticString = #filePath, line: UInt = #line) {
        let menu = transportMenu(name)
        for _ in 0..<3 where !menu.isHittable {
            remote.press(.up)
            _ = menu.waitForExistence(timeout: 2)
        }
        select(menu, "\(name) in the transport bar", first: [.up, .right], file: file, line: line)
    }

    func playerExtras(movies: Int) async {
        openGrid("Movies")
        select(titled("Direct H264", in: "library.grid"), "Direct H264 in Movies", first: [.down, .right])
        let play = wait(button("item.play"), "Direct H264's Play")
        select(play, "Play", first: [.up, .left])
        await eventually("the player covers the page") { !(play.exists && play.isHittable) }
        sleep(3)

        openTransportMenu("Subtitles")
        chooseRow("Find Subtitles…")
        wait(app.staticTexts["Subtitle search isn't set up on this server"], "the search's not-set-up message",
             timeout: 30)
        shot("80-find-subtitles")
        remote.press(.menu)
        sleep(2)

        openTransportMenu("Speed")
        chooseRow("1.5×")
        sleep(1)
        openTransportMenu("Speed")
        wait(menuRow("1.5×, selected"), "1.5× marked in Speed")
        shot("81-speed")
        remote.press(.menu)
        sleep(1)

        if AVPictureInPictureController.isPictureInPictureSupported() {
            let pip = app.descendants(matching: .any).matching(NSPredicate(
                format: "identifier == 'PIP Button' OR label CONTAINS[c] 'Picture in Picture'")).firstMatch
            for _ in 0..<3 where !pip.isHittable {
                remote.press(.up)
                _ = pip.waitForExistence(timeout: 2)
            }
            select(pip, "Picture in Picture in the transport bar", first: [.up, .left])
            await eventually("the page shows behind Picture in Picture") { play.exists && play.isHittable }
            shot("82-picture-in-picture")
            await eventually("the playback goes on in Picture in Picture") {
                await Admin.sessions().contains { $0.title == "Direct H264" }
            }
        } else {
            print("PLEY: this simulator has no Picture in Picture")
        }

        // Menu closes the player; a first press may only hide its controls.
        for _ in 0..<3 where !(play.exists && play.isHittable) {
            remote.press(.menu)
            for _ in 0..<6 where !(play.exists && play.isHittable) { usleep(500_000) }
        }
        XCTAssertTrue(play.exists && play.isHittable, "the page is back after the player")
        back(1)
    }
}
