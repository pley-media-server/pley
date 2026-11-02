import XCTest

/// The screens at the largest accessibility text size, with music playing
/// so the mini player shows under each: an album playing, Now Playing,
/// Home, a movie's page, a library, a show and its episodes, a collection,
/// Playlists, Search, Me, Settings and About, each looked at (`large-*`
/// screenshots) and its main control checked to be on screen and
/// reachable. It changes nothing on the server but music's session.
extension Walk {
    /// The launch arguments that set the text size to the largest accessibility size.
    static let largestText = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]

    func largeTextPass(libraries: [String: Int]) {
        let movies = libraries["Movies"] ?? 0, shows = libraries["Shows"] ?? 0
        seeHome(Admin.adminName)
        openLibrary(libraries["Music"] ?? 0, "Music")
        wait(titled("Test Artist"), "the artist in Music").tap()
        wait(titled("Test Album"), "the artist's album").tap()
        reveal(button("album.play"), "the album's Play").tap()
        let mini = wait(element("music.mini"), "the mini player", timeout: 30)
        XCTAssertTrue(mini.isHittable, "the mini player can be reached")
        shot("large-00-album-playing")
        mini.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)).tap()
        reveal(button("music.toggle"), "Now Playing's Play/Pause")
        shot("large-00-now-playing")
        tap("music.nowPlaying.done", "Done")
        openHome()
        seeHome(Admin.adminName)
        XCTAssertTrue(tile.isHittable, "a tile can be reached on Home")
        shot("large-01-home")

        tile.tap()
        reveal(button("item.play"), "the item's Play")
        shot("large-02-item")
        scrollPage(up: true)
        scrollPage(up: true)
        shot("large-03-item-below")
        back(to: "Home")

        openLibrary(movies, "Movies")
        XCTAssertTrue(element("library.sort").isHittable, "Sort can be reached")
        shot("large-04-library")
        wait(titled("Two Audio"), "a title in Movies")

        openLibrary(shows, "Shows")
        wait(titled("Pley Test Show"), "the show in Shows").tap()
        reveal(button("item.play"), "the show's Play")
        shot("large-05-show")
        let episode = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'item.episode.'")).firstMatch
        scroll(to: episode, "the show's episodes")
        shot("large-06-episodes")
        back(to: "Shows")

        openLibrary(movies, "Movies")
        libraryTab("Collections")
        let collection = app.buttons.matching(NSPredicate(
            format: "identifier BEGINSWITH 'tile.collection.' AND label BEGINSWITH 'Test collection'")).firstMatch
        wait(collection, "Test collection in Collections").tap()
        reveal(button("collection.play"), "the collection's Play")
        shot("large-07-collection")
        back(to: "Movies")
        libraryTab("Library")

        openPlaylists()
        shot("large-08-playlists")

        openSearch()
        wait(app.searchFields.firstMatch, "the search field")
        shot("large-09-search")

        openMe()
        shot("large-10-me")
        reveal(element("me.settings"), "Settings").tap()
        wait(element("settings.quality"), "Quality")
        shot("large-11-settings")
        reveal(element("settings.about"), "About").tap()
        wait(element("about.version"), "the version")
        shot("large-12-about")
        back(to: "Settings")
        back(to: "Me")
        reveal(element("me.signOut"), "Sign out")
        shot("large-13-me-below")
    }
}
