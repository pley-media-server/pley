import XCTest

/// The collections' and playlists' steps of Apple TV's walk: a
/// collection's page, a playlist made from a title's actions, added to,
/// an entry moved up, a removal undone, a removal, Rename and Delete, and a
/// library's filters saved as a smart playlist. Each is checked on the
/// server; what the walk makes it deletes, since every device's walk uses
/// the same server.
extension Walk {
    static let playlistTitle = "UI Test"
    static let smartTitle = "UI Smart"

    /// Long press on `element`, then its menu's item `words`, `downs`
    /// presses below the first (the last item when `nil`). A menu's items
    /// don't say they have the focus, so the walk counts presses.
    func longPress(_ element: XCUIElement, _ what: String, then words: String, downs: Int? = nil,
                   first: [XCUIRemote.Button] = [.down, .right, .up, .left],
                   file: StaticString = #filePath, line: UInt = #line) {
        focus(element, what, first: first, file: file, line: line)
        menu(then: words, downs: downs, file: file, line: line)
    }

    /// The top one of two playlist entries: down to the bottom one, then up.
    /// (After a move the entries don't say they have the focus, so the walk counts presses.)
    func toTopOfTwoEntries() {
        press(.down, 3)
        press(.up)
    }

    /// Long press on what has the focus, then its menu's item `words`, as above.
    func menu(then words: String, downs: Int? = nil, file: StaticString = #filePath, line: UInt = #line) {
        remote.press(.select, forDuration: 1.5)
        wait(app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH %@", words)).firstMatch,
             words, file: file, line: line)
        press(.down, downs ?? 8)
        remote.press(.select)
    }

    /// Movies' Collections tab, and "Test collection"'s page with Play;
    /// Movies is left on its Library tab.
    func collectionPage() {
        openLibraryTab("Movies")
        // From the grid's first tile up past Sort to the library's tabs, then
        // along them (left or right in the tab bar would change the tab).
        toFirstTile()
        press(.up, 2)
        select(button("library.tab.collections"), "the Collections tab", first: [.right, .left])
        let tile = app.buttons.matching(NSPredicate(
            format: "identifier BEGINSWITH 'tile.collection.' AND label BEGINSWITH 'Test collection'")).firstMatch
        select(tile, "Test collection in Collections", first: [.down, .left])
        wait(button("collection.play"), "the collection's Play")
        XCTAssertTrue(says("Test collection"), "the collection's title")
        shot("16-collection")
        remote.press(.menu)
        // Up from the collection's tile to the library's tabs, then along them.
        sleep(1)
        press(.up)
        select(button("library.tab.library"), "the Library tab", first: [.left, .right])
        wait(element("library.sort"), "Movies' Library tab")
    }

    /// Add to playlist from title `title`'s actions in the library showing.
    func addToPlaylist(_ title: String) {
        longPress(titled(title, in: "library.grid"), "\(title) in the library", then: "Add to playlist",
                  first: [.left, .right])
        wait(element("addToPlaylist.what"), "Add to playlist's panel")
    }

    func openPlaylists() {
        let tab = app.tabBars.buttons["Playlists"]
        focus(tab, "the Playlists tab", first: [.up, .right])
        remote.press(.select)
        wait(button("playlists.new"), "Playlists")
    }

    /// A movie's actions, Add to playlist and a new playlist "UI Test"; a
    /// second movie added to it; then on its page entry 2 moved up, a
    /// removal undone, a removal, Rename and Delete.
    func playlistWalk(movies: Int) async {
        await Admin.deletePlaylists(titled: [Self.playlistTitle, Self.playlistTitle + " 2"])
        let titles = await Admin.titles(library: movies)
        guard let first = titles["Two Audio"], let second = titles["Direct H264"] else {
            XCTFail("the test media has Two Audio and Direct H264: \(titles.keys.sorted())")
            return
        }

        openGrid("Movies")
        addToPlaylist("Two Audio")
        shot("17-add-to-playlist")
        select(element("addToPlaylist.new.title"), "the new playlist's title")
        type(Self.playlistTitle)
        // The keyboard's Done submits the title, which makes the playlist; else Create does.
        let create = button("addToPlaylist.new.create")
        if create.waitForExistence(timeout: 2) { select(create, "Create Playlist", first: [.down]) }
        var id = 0
        await eventually("the server has UI Test with Two Audio") {
            guard let playlist = await Admin.playlist(titled: Self.playlistTitle),
                  let found = playlist["id"] as? Int else { return false }
            id = found
            return await Admin.entries(found).map(\.item) == [first]
        }

        addToPlaylist("Direct H264")
        select(button("addToPlaylist.playlist.\(id)"), "UI Test in Add to playlist", first: [.down])
        await eventually("UI Test has Direct H264 second") { await Admin.entries(id).map(\.item) == [first, second] }
        let entries = await Admin.entries(id).map(\.entry)
        guard entries.count == 2 else { return }

        openPlaylists()
        select(button("tile.playlist.\(id)"), "UI Test in Playlists", first: [.down, .right])
        wait(element("playlist.title"), "UI Test's page")
        shot("18-playlist")

        // Move: up moves the entry, Select puts it down.
        longPress(button("playlist.entry.\(entries[1])"), "the second entry", then: "Move", downs: 2)
        sleep(1)
        press(.up)
        shot("18-playlist-move")
        press(.select)
        await eventually("the server has the second entry first") {
            await Admin.entries(id).map(\.entry) == [entries[1], entries[0]]
        }

        sleep(1)
        toTopOfTwoEntries()
        menu(then: "Remove")
        // Undo within the 5 seconds: up from the entries, once the menu has
        // gone, is the row of buttons, then along it to Undo.
        sleep(1)
        press(.up)
        for _ in 0..<4 {
            let focused = app.buttons.matching(NSPredicate(format: "hasFocus == true")).firstMatch.identifier
            if focused == "playlist.undo" { break }
            press(["playlist.rename", "playlist.delete"].contains(focused) ? .left : .right)
        }
        remote.press(.select)
        wait(button("playlist.entry.\(entries[1])"), "the entry put back")
        // Past the 5 seconds a removal waits: the server still has both.
        try? await Task.sleep(for: .seconds(6))
        let kept = await Admin.entries(id).map(\.entry)
        XCTAssertEqual(kept, [entries[1], entries[0]], "the undone removal never reached the server")

        toTopOfTwoEntries()
        menu(then: "Remove")
        XCTAssertFalse(button("playlist.entry.\(entries[1])").waitForExistence(timeout: 2), "the entry went")
        await eventually("the server removed it after 5 seconds") { await Admin.entries(id).map(\.entry) == [entries[0]] }

        sleep(1)
        press(.up)
        select(button("playlist.rename"), "Rename", first: [.left, .right])
        let field = wait(app.alerts.textFields.firstMatch, "the new title")
        select(field, "the new title", first: [.up, .down])
        type(" 2")
        select(app.alerts.buttons["Rename"].firstMatch, "Rename in the alert", first: [.down])
        await eventually("the server has the new title") {
            await Admin.playlist(titled: Self.playlistTitle + " 2")?["id"] as? Int == id
        }

        select(button("playlist.delete"), "Delete", first: [.right, .left])
        select(button("playlist.delete.confirm", or: "Delete Playlist"), "Delete's confirmation", first: [.up, .down])
        await eventually("the server deleted it") { await Admin.playlists().allSatisfy { $0["id"] as? Int != id } }
        wait(button("playlists.new"), "Playlists after the delete")
    }

    /// Movies' filters saved as a smart playlist from the row of controls:
    /// its page opens, and the server has its filter. Deleted afterwards.
    func saveAsSmartPlaylist() async {
        await Admin.deletePlaylists(titled: [Self.smartTitle])
        openLibraryTab("Movies")
        // Up from the grid's first tile is Sort; Save as is along that row.
        toFirstTile()
        press(.up)
        select(button("library.saveAs"), "Save as Smart Playlist", first: [.right])
        wait(element("newPlaylist.filters"), "the filters in words")
        shot("19-save-as-panel")
        select(element("newPlaylist.title"), "the smart playlist's title", first: [.up, .down])
        type(Self.smartTitle)
        // The keyboard's Done submits the title, which saves it; else Save does.
        let save = button("newPlaylist.create")
        if save.waitForExistence(timeout: 2) { select(save, "Save", first: [.down]) }
        wait(element("playlist.title"), "the smart playlist's page")
        shot("19-save-as")
        await eventually("the server has the smart playlist") {
            let filter = await Admin.playlist(titled: Self.smartTitle)?["smart_filter"] as? String
            return filter?.contains("sort=year") == true
        }
        XCTAssertFalse(button("playlist.rename").exists, "a smart playlist can't be changed")
        await Admin.deletePlaylists(titled: [Self.smartTitle])
        remote.press(.menu)
        wait(element("library.sort"), "Movies again")
    }
}
