import XCTest

/// The title pages' steps of the walk: a movie's audio pick and rating,
/// More Like This and Media info, Play and back; a show, its season and
/// an episode; an artist and an album.
extension Walk {
    /// A button whose words start with `title` (a tile says "Two Audio, 2020").
    func titled(_ title: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", title)).firstMatch
    }

    /// Swipes up the page until `element` is on screen.
    @discardableResult
    func scroll(to element: XCUIElement, _ what: String, file: StaticString = #filePath,
                line: UInt = #line) -> XCUIElement {
        var last: CGRect?
        for _ in 0..<12 {
            if inReach(element) { return element }
            if element.exists, element.isHittable, element.frame == last { return element }
            last = element.exists ? element.frame : nil
            scrollPage(up: true)
        }
        XCTAssertTrue(element.exists && element.isHittable, "\(what) didn't show", file: file, line: line)
        return element
    }

    /// A movie's page: the French track picked in Audio and four stars,
    /// both kept by the server; More Like This and Media info; Play opens
    /// the player, which hides the page, and closing it comes back.
    func moviePage(movies: Int) async {
        let titles = await Admin.titles(library: movies)
        guard let id = titles["Two Audio"], let other = titles["Direct H264"] else {
            XCTFail("the test media has Two Audio and Direct H264: \(titles.keys.sorted())")
            return
        }
        // Alike in genre, so each is More Like This for the other.
        await Admin.genre("Drama", [id, other])
        await Admin.resetChoices(id)

        openLibrary(movies, "Movies")
        wait(titled("Two Audio"), "Two Audio in Movies").tap()
        wait(button("item.play"), "Two Audio's Play")
        shot("14-movie")

        let audio = wait(element("item.audio"), "the Audio picker")
        audio.tap()
        wait(app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'French'")).firstMatch, "French in Audio").tap()
        await eventually("the server keeps the French track") {
            let now = await Admin.audio(id)
            return now.picked && now.selected != nil && now.selected == now.french
        }

        let stars = wait(element("item.rating"), "the rating")
        stars.coordinate(withNormalizedOffset: CGVector(dx: 0.75, dy: 0.5)).tap()
        await eventually("the server keeps four stars") { await Admin.rating(id) == 8 }
        XCTAssertEqual(stars.value as? String, "4 stars", "the rating says four stars")

        scroll(to: element("item.row.related"), "More Like This")
        scroll(to: element("item.mediaInfo"), "Media info")
        shot("15-movie-rows")

        // Play: the player covers the page, which VoiceOver and taps can't reach.
        scrollPage(up: false)
        scrollPage(up: false)
        let play = scroll(to: button("item.play"), "Play")
        play.tap()
        await eventually("the player covers the page") { !(play.exists && play.isHittable) }
        closePlayer()
        await eventually("the page is back") { play.exists && play.isHittable }
    }

    /// Closes the player with the system's Close, shown by a tap.
    func closePlayer(file: StaticString = #filePath, line: UInt = #line) {
        let close = app.buttons.matching(NSPredicate(format: "label IN {'Close', 'Done'} OR identifier == 'Close'"))
            .firstMatch
        for _ in 0..<6 {
            if close.exists && close.isHittable {
                close.tap()
                return
            }
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            _ = close.waitForExistence(timeout: 2)
        }
        XCTFail("the player's Close didn't show", file: file, line: line)
    }

    /// The show's page, its season's page from there, and an episode's
    /// page from the season's.
    func showPages(shows: Int) {
        openLibrary(shows, "Shows")
        wait(titled("Pley Test Show"), "the show in Shows").tap()
        wait(button("item.play"), "the show's Play")
        let first = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'item.episode.'")).firstMatch
        scroll(to: first, "the show's episodes")
        shot("16-show")

        scroll(to: button("item.seasonPage"), "the season's link").tap()
        wait(element("item.title"), "the season's page")
        XCTAssertTrue(element("item.show").waitForExistence(timeout: 10), "the season names its show")
        let episodes = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'item.episode.'"))
        scroll(to: episodes.firstMatch, "the season's episodes")
        // The list makes only the rows near the screen: down to the last.
        let third = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'item.episode.' AND label BEGINSWITH 'Episode 3'"))
            .firstMatch
        scroll(to: third, "the season's third episode")
        shot("17-season")

        scrollPage(up: false)
        scrollPage(up: false)
        scroll(to: episodes.firstMatch, "the season's first episode").tap()
        wait(element("item.seasonLink"), "the episode's season")
        wait(button("item.play"), "the episode's Play")
        shot("18-episode")
    }

    /// The music library's artist, their album, and its tracks; no Play.
    func musicPages(music: Int) {
        openLibrary(music, "Music")
        wait(titled("Test Artist"), "the artist in Music").tap()
        wait(element("artist.name"), "the artist's page")
        shot("19-artist")
        wait(titled("Test Album"), "the artist's album").tap()
        wait(element("album.title"), "the album's page")
        let tracks = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'album.track.'"))
        wait(tracks.firstMatch, "the album's tracks")
        XCTAssertEqual(tracks.count, 2, "the album lists its two tracks")
        XCTAssertTrue(element("album.artist").exists, "the album links its artist")
        XCTAssertFalse(button("item.play").exists, "music has no Play yet")
        shot("20-album")
    }

    /// A person of a title's cast, and their page.
    func personPage(library: Int, name: String, title: String) {
        openLibrary(library, name)
        wait(titled(title), "\(title) in \(name)").tap()
        wait(button("item.play"), "\(title)'s Play")
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'item.cast.'")).firstMatch
        scroll(to: person, "the cast").tap()
        wait(element("person.name"), "the person's page")
        shot("21-person")
    }
}
