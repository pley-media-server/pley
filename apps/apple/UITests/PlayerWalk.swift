import AVKit
import XCTest

/// The player's extras: Find Subtitles… (the test server has no subtitle
/// search set up), the subtitle style, Speed, and Picture in Picture
/// keeping the playback.
extension Walk {
    /// A menu row by its first words (a submenu says its second line too: "Speed, 1×").
    func menuRow(_ words: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", words)).firstMatch
    }

    /// Plays `title` from Movies, and waits for the player to cover the page.
    func playMovie(_ title: String, movies: Int) async -> XCUIElement {
        openLibrary(movies, "Movies")
        wait(titled(title), "\(title) in Movies").tap()
        let play = wait(button("item.play"), "\(title)'s Play")
        play.tap()
        await eventually("the player covers the page") { !(play.exists && play.isHittable) }
        let started = await holds(within: 30) { await Admin.sessions().contains { $0.title == title } }
        XCTAssertTrue(started, "the session starts at the server")
        return play
    }

    /// Options, then `row` in it (a submenu, or an item).
    func options(_ row: String) {
        tapInPlayer("player.options", "Options")
        let item = wait(menuRow(row), "\(row) in Options")
        // A long menu scrolls.
        for _ in 0..<4 where !item.isHittable { app.swipeUp() }
        item.tap()
    }

    func playerExtras(movies: Int) async {
        // The movie is 90 s long: Picture in Picture goes first, before 1.5× brings its end nearer.
        let play = await playMovie("Direct H264", movies: movies)

        // Picture in Picture: the player goes, the page shows, and the playback goes on.
        // The simulator may have none (`isPictureInPictureSupported`), and AVKit then shows no button.
        if AVPictureInPictureController.isPictureInPictureSupported() {
            let pip = app.buttons.matching(NSPredicate(
                format: "identifier == 'PIP Button' OR label CONTAINS[c] 'Picture in Picture'")).firstMatch
            // A tap shows the controls (another hides them, and they hide by themselves after a
            // few seconds): the button is tapped as soon as it can be.
            var tapped = false
            for _ in 0..<5 where !tapped {
                app.tap()
                for _ in 0..<8 where !tapped {
                    if pip.exists && pip.isHittable {
                        pip.tap()
                        tapped = true
                    } else {
                        usleep(250_000)
                    }
                }
            }
            XCTAssertTrue(tapped, "Picture in Picture's button didn't show")
            await eventually("the page shows behind Picture in Picture") { play.exists && play.isHittable }
            shot("82-picture-in-picture")
            let kept = await holds(within: 4) { await Admin.sessions().contains { $0.title == "Direct H264" } }
            XCTAssertTrue(kept, "the playback goes on in Picture in Picture")
            // Play again: the one in Picture in Picture ends first.
            play.tap()
            await eventually("the player covers the page again") { !(play.exists && play.isHittable) }
        } else {
            print("PLEY: this simulator has no Picture in Picture")
        }

        options("Find Subtitles…")
        wait(app.staticTexts["Subtitle search isn't set up on this server"], "the search's not-set-up message",
             timeout: 30)
        shot("80-find-subtitles")
        tap("subtitleSearch.cancel", "Cancel")

        // Text subtitles bring their style into Options.
        options("English")
        options("Subtitle style")
        wait(menuRow("Size, System"), "Size in Subtitle style")
        shot("83-subtitle-style")
        menuRow("Size").tap()
        wait(menuRow("150 %"), "150 % in Size").tap()
        options("Subtitle style")
        wait(menuRow("Size, 150 %"), "150 % beside Size")
        menuRow("Size").tap()
        wait(menuRow("System"), "System in Size").tap()
        options("Off")

        options("Speed")
        wait(menuRow("1.5×"), "1.5× in Speed").tap()
        options("Speed")
        wait(menuRow("1.5×, selected"), "1.5× marked in Speed")
        shot("81-speed")
        // Away from the menu, which closes it.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.5)).tap()

        closePlayer()
        await eventually("the page is back") { play.exists && play.isHittable }
        let ended = await holds { await Admin.sessions().isEmpty }
        XCTAssertTrue(ended, "closing the player ends the session")
    }
}
