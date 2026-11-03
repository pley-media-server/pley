import XCTest

/// The collections' and playlists' steps of the walk: a collection's page,
/// a playlist made from a title's actions, added to, reordered, an entry
/// removed and put back, then removed, renamed and deleted, and a
/// library's filters saved as a smart playlist. Each is checked on the
/// server; what the walk makes it deletes, since every device's walk uses
/// the same server.
extension Walk {
    static let playlistTitle = "UI Test"
    static let smartTitle = "UI Smart"

    /// The library's tab called `title` (Recommended, Library, Collections).
    func libraryTab(_ title: String) {
        let tabs = wait(element("library.tabs"), "the library's tabs")
        wait(tabs.buttons[title], "\(title) in the tabs").tap()
    }

    /// Back to `page`: the navigation bar's Back is named after the page under it.
    func back(to page: String) {
        let back = app.navigationBars.buttons.matching(NSPredicate(format: "label IN %@", [page, "Back"])).firstMatch
        wait(back, "Back to \(page)").tap()
    }

    /// Movies' Collections tab, and "Test collection"'s page with Play;
    /// Movies is left on its Library tab.
    func collectionPage(movies: Int) {
        openLibrary(movies, "Movies")
        libraryTab("Collections")
        let tile = app.buttons.matching(NSPredicate(
            format: "identifier BEGINSWITH 'tile.collection.' AND label BEGINSWITH 'Test collection'")).firstMatch
        wait(tile, "Test collection in Collections").tap()
        wait(button("collection.play"), "the collection's Play")
        XCTAssertTrue(element("collection.title").label.contains("Test collection"), "the collection's title")
        shot("16-collection")
        back(to: "Movies")
        libraryTab("Library")
        wait(element("library.sort"), "Movies' Library tab")
    }

    /// Add to playlist from title `title`'s actions in the library showing.
    func addToPlaylist(_ title: String) {
        let tile = wait(titled(title), "\(title) in the library")
        tile.press(forDuration: 1.2)
        wait(button("actions.addToPlaylist", or: "Add to playlist…"), "Add to playlist").tap()
        wait(element("addToPlaylist.what"), "Add to playlist's sheet")
    }

    /// Playlists: the tab on iPhone, the sidebar's row on iPad.
    func openPlaylists() {
        let tab = openSection("Playlists", sidebar: "shell.sidebar.playlists")
        // On iPhone, twice goes back to the list from a playlist left open.
        if !wide && !element("playlists.new").waitForExistence(timeout: 3) { tab.tap() }
        wait(element("playlists.new"), "Playlists")
    }

    /// An entry's context menu, and its item `id` or `label`.
    func entryMenu(_ entry: Int, _ id: String, or label: String) {
        let row = wait(element("playlist.entry.\(entry)"), "entry \(entry)")
        let item = button(id, or: label)
        // The menu can miss a press made while the list still moves: one more.
        for _ in 0..<3 where !item.exists {
            row.press(forDuration: 1.2)
            if item.waitForExistence(timeout: 5) { break }
        }
        wait(item, label).tap()
    }

    /// A movie's actions, Add to playlist and New playlist "UI Test"; a
    /// second movie added to it; then on its page entry 2 moved up, a
    /// removal undone, a removal, Rename and Delete.
    func playlistWalk(movies: Int) async {
        await Admin.deletePlaylists(titled: [Self.playlistTitle, Self.playlistTitle + " 2"])
        let titles = await Admin.titles(library: movies)
        guard let first = titles["Two Audio"], let second = titles["Direct H264"] else {
            XCTFail("the test media has Two Audio and Direct H264: \(titles.keys.sorted())")
            return
        }

        openLibrary(movies, "Movies")
        addToPlaylist("Two Audio")
        shot("17-add-to-playlist")
        let name = wait(element("addToPlaylist.new.title"), "the new playlist's title")
        name.tap()
        name.typeText(Self.playlistTitle)
        tap("addToPlaylist.new.create", "Create")
        var id = 0
        await eventually("the server has UI Test with Two Audio") {
            guard let playlist = await Admin.playlist(titled: Self.playlistTitle),
                  let found = playlist["id"] as? Int else { return false }
            id = found
            return await Admin.entries(found).map(\.item) == [first]
        }

        addToPlaylist("Direct H264")
        let row = wait(button("addToPlaylist.playlist.\(id)"), "UI Test in Add to playlist")
        // The sheet is still rising when its rows appear; a tap then lands where the row was.
        sleep(1)
        shot("17-add-to-playlist-2")
        row.tap()
        await eventually("UI Test has Direct H264 second") {
            await Admin.entries(id).map(\.item) == [first, second]
        }
        let entries = await Admin.entries(id).map(\.entry)
        guard entries.count == 2 else { return }

        openPlaylists()
        wait(button("tile.playlist.\(id)"), "UI Test in Playlists").tap()
        wait(element("playlist.title"), "UI Test's page")
        wait(element("playlist.entry.\(entries[1])"), "UI Test's second entry")
        shot("18-playlist")
        // Play and Shuffle keep their words and a button's height in the list's header.
        let play = wait(button("playlist.play"), "the playlist's Play")
        XCTAssertLessThan(play.frame.height, 80, "Play is \(play.frame.height) pt high")
        XCTAssertLessThan(button("playlist.shuffle").frame.height, 80, "Shuffle stays a button's height")
        XCTAssertEqual(play.label, "Play", "Play keeps its title")

        entryMenu(entries[1], "playlist.entry.moveUp", or: "Move Up")
        await eventually("the server has the second entry first") {
            await Admin.entries(id).map(\.entry) == [entries[1], entries[0]]
        }

        entryMenu(entries[1], "playlist.entry.removeFromMenu", or: "Remove")
        tap("toast.undo", "Undo in the toast")
        wait(element("playlist.entry.\(entries[1])"), "the entry put back")
        // Past the 5 seconds a removal waits: the server still has both.
        try? await Task.sleep(for: .seconds(6))
        let kept = await Admin.entries(id).map(\.entry)
        XCTAssertEqual(kept, [entries[1], entries[0]], "the undone removal never reached the server")

        entryMenu(entries[1], "playlist.entry.removeFromMenu", or: "Remove")
        XCTAssertFalse(element("playlist.entry.\(entries[1])").waitForExistence(timeout: 2), "the entry went")
        await eventually("the server removed it after 5 seconds") { await Admin.entries(id).map(\.entry) == [entries[0]] }

        tap("playlist.more", "More")
        wait(button("playlist.rename", or: "Rename…"), "Rename").tap()
        let field = wait(app.alerts.textFields.firstMatch, "the new title")
        field.tap()
        field.typeText(" 2")
        wait(app.alerts.buttons["Rename"].firstMatch, "Rename in the alert").tap()
        await eventually("the server has the new title") {
            await Admin.playlist(titled: Self.playlistTitle + " 2")?["id"] as? Int == id
        }
        await eventually("the page says it") { element("playlist.title").label.contains(Self.playlistTitle + " 2") }

        tap("playlist.more", "More")
        wait(button("playlist.delete", or: "Delete…"), "Delete").tap()
        wait(button("playlist.delete.confirm", or: "Delete Playlist"), "Delete's confirmation").tap()
        await eventually("the server deleted it") { await Admin.playlists().allSatisfy { $0["id"] as? Int != id } }
        wait(element("playlists.new"), "Playlists after the delete")
    }

    /// Movies' filters saved as a smart playlist from More: its page opens,
    /// and the server has its filter. Deleted afterwards.
    func saveAsSmartPlaylist(movies: Int) async {
        await Admin.deletePlaylists(titled: [Self.smartTitle])
        openLibrary(movies, "Movies")
        tap("library.more", "More")
        wait(button("library.saveAs", or: "Save as Smart Playlist…"), "Save as Smart Playlist").tap()
        let name = wait(element("newPlaylist.title"), "the smart playlist's title")
        XCTAssertFalse(element("newPlaylist.filters").label.isEmpty, "the filters in words")
        name.tap()
        name.typeText(Self.smartTitle)
        shot("19-save-as")
        tapAfterTyping("newPlaylist.create", "Save", then: element("playlist.title"))
        wait(element("playlist.title"), "the smart playlist's page")
        await eventually("the server has the smart playlist") {
            let filter = await Admin.playlist(titled: Self.smartTitle)?["smart_filter"] as? String
            return filter?.contains("sort=year") == true
        }
        XCTAssertFalse(element("playlist.more").exists, "a smart playlist can't be changed")
        await Admin.deletePlaylists(titled: [Self.smartTitle])
        back(to: "Movies")
        wait(element("library.sort"), "Movies again")
    }
}
