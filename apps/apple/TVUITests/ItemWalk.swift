import XCTest

/// The title pages' steps of Apple TV's walk: a movie's audio pick and
/// rating, More Like This and Media info, Play and back; a show, its
/// season and an episode; an artist and an album.
extension Walk {
    /// A button whose words start with `title` (a tile says "Two Audio, 2020").
    /// Inside `container` when given: other tabs' pages stay in the tree.
    func titled(_ title: String, in container: String? = nil) -> XCUIElement {
        let scope = container.map { element($0).descendants(matching: .button) } ?? app.buttons
        return scope.matching(NSPredicate(format: "label BEGINSWITH %@", title)).firstMatch
    }

    /// A library tab's grid of every title, from Recommended if it opens there.
    func openGrid(_ name: String) {
        openLibraryTab(name)
        if !element("library.grid").waitForExistence(timeout: 3) {
            select(element("library.tab.library"), "\(name)'s Library", first: [.down, .up])
        }
        wait(element("library.grid"), "\(name)'s titles")
        // Its titles are read again after an edit or a mark: once they are.
        sleep(2)
    }

    /// A list's choice by its words.
    func menuItem(_ words: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", words)).firstMatch
    }

    /// Menu (Back) until the library tab's own page shows, at most `times`
    /// presses, and once more, which takes the focus up to the tabs.
    func back(_ times: Int) {
        let page = element("library.tab.library")
        for _ in 0..<times {
            if page.exists && page.isHittable { break }
            remote.press(.menu)
            usleep(1_000_000)
        }
        remote.press(.menu)
        usleep(1_000_000)
    }

    /// A movie's page: the French track picked in Audio and four stars,
    /// both kept by the server; More Like This and Media info; Play opens
    /// the player over the page, and Menu closes it.
    func moviePage(movies: Int) async {
        let titles = await Admin.titles(library: movies)
        guard let id = titles["Two Audio"], let other = titles["Direct H264"] else {
            XCTFail("the test media has Two Audio and Direct H264: \(titles.keys.sorted())")
            return
        }
        // Alike in genre, so each is More Like This for the other.
        await Admin.genre("Drama", [id, other])
        await Admin.resetChoices(id)
        openGrid("Movies")
        select(titled("Two Audio", in: "library.grid"), "Two Audio in Movies", first: [.down, .right])
        let play = wait(button("item.play"), "Two Audio's Play")
        shot("14-movie")

        select(button("item.audio"), "the Audio button", first: [.right])
        let french = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'French'")).firstMatch
        select(french, "French in Audio", first: [.down, .up])
        await eventually("the server keeps the French track") {
            let now = await Admin.audio(id)
            return now.picked && now.selected != nil && now.selected == now.french
        }

        select(button("item.rating"), "the rating button", first: [.left, .right])
        select(menuItem("4 stars"), "4 stars", first: [.down, .up])
        await eventually("the server keeps four stars") { await Admin.rating(id) == 8 }

        XCTAssertTrue(element("item.row.related").waitForExistence(timeout: 10), "More Like This shows")
        XCTAssertTrue(element("item.mediaInfo").exists, "Media info shows")
        focus(element("item.mediaInfo"), "Media info", first: [.down])
        shot("15-movie-rows")

        select(play, "Play", first: [.up, .left])
        await eventually("the player covers the page") { !(play.exists && play.isHittable) }
        sleep(2)
        // Menu closes the player; a first press may only hide its controls.
        for _ in 0..<3 where !(play.exists && play.isHittable) {
            remote.press(.menu)
            for _ in 0..<6 where !(play.exists && play.isHittable) { usleep(500_000) }
        }
        XCTAssertTrue(play.exists && play.isHittable, "the page is back after the player")
        back(1)
    }

    /// The show's page, its season's page from there, and an episode's
    /// page from the season's.
    func showPages() {
        openGrid("Shows")
        select(titled("Pley Test Show", in: "library.grid"), "the show in Shows", first: [.down, .right])
        wait(button("item.play"), "the show's Play")
        let episodes = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'item.episode.'"))
        wait(episodes.firstMatch, "the show's episodes")
        shot("16-show")

        select(element("item.seasonPage"), "the season's button", first: [.down, .right])
        wait(element("item.title"), "the season's page")
        wait(episodes.firstMatch, "the season's episodes")
        XCTAssertEqual(episodes.count, 3, "the season lists its three episodes")
        shot("17-season")

        select(episodes.firstMatch, "the first episode", first: [.down, .left])
        wait(button("item.play"), "the episode's Play")
        XCTAssertTrue(says("Season 1"), "the episode names its season")
        shot("18-episode")
        back(3)

    }

    /// The music library's artist, their album, and its tracks; no Play.
    func musicPages() {
        openGrid("Music")
        select(titled("Test Artist", in: "library.grid"), "the artist in Music", first: [.down, .right])
        wait(element("artist.name"), "the artist's page")
        shot("19-artist")
        select(titled("Test Album", in: "artist.row.albums"), "the artist's album", first: [.down, .right])
        wait(element("album.title"), "the album's page")
        let tracks = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'album.track.'"))
        wait(tracks.firstMatch, "the album's tracks")
        XCTAssertEqual(tracks.count, 2, "the album lists its two tracks")
        XCTAssertTrue(element("album.artist").exists, "the album has its artist")
        XCTAssertFalse(button("item.play").exists, "music has no Play yet")
        shot("20-album")
        back(2)
    }

    /// A person of a title's cast, and their page.
    func personPage(library: String, title: String) {
        openGrid(library)
        select(titled(title, in: "library.grid"), "\(title) in \(library)", first: [.down, .right])
        wait(button("item.play"), "\(title)'s Play")
        let person = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'item.cast.'")).firstMatch
        select(person, "the cast", first: [.down, .right])
        wait(element("person.name"), "the person's page")
        shot("21-person")
        back(2)
    }
}
