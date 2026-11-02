import XCTest

/// Downloads: Direct H264 downloaded as it is and played from the device
/// (Details says Downloaded, and the server has no session for it), the
/// show's season converted and downloaded, and Settings → Downloads →
/// Delete all downloads.
extension Walk {
    /// The rows of Me → Downloads whose label contains `words`.
    func downloadRows(_ words: String) -> XCUIElementQuery {
        app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'downloads.row.' AND label CONTAINS %@", words))
    }

    /// Waits up to `timeout` seconds for at least `count` rows with `words`, every one finished.
    func waitDownloaded(_ words: String, count: Int, timeout: Double) async {
        let finished = NSPredicate(format: "NOT (label CONTAINS 'Preparing' OR label CONTAINS 'Downloading' "
            + "OR label CONTAINS 'Waiting' OR label CONTAINS %@)", "Didn't")
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            let rows = downloadRows(words)
            if rows.count >= count, rows.matching(finished).count == rows.count { return }
            try? await Task.sleep(for: .seconds(1))
        }
        let labels = downloadRows(words).allElementsBoundByIndex.map(\.label)
        XCTFail("\(words) didn't download: \(labels)")
    }

    func openDownloads() {
        openMe()
        reveal(element("me.downloads"), "Downloads in Me").tap()
        wait(element("downloads.page"), "Downloads")
    }

    func downloads(movies: Int, shows: Int) async {
        // The original, as it is: this device plays it.
        openLibrary(movies, "Movies")
        wait(titled("Direct H264"), "Direct H264 in Movies").tap()
        // A run before that stopped part way may have left it downloaded.
        if element("item.downloaded").waitForExistence(timeout: 5) {
            deleteAllDownloads()
            openLibrary(movies, "Movies")
            wait(titled("Direct H264"), "Direct H264 in Movies").tap()
        }
        reveal(button("item.download"), "Download").tap()
        wait(element("item.downloaded"), "Downloaded on the page", timeout: 90)
        shot("90-item-downloaded")

        openDownloads()
        await waitDownloaded("Direct H264", count: 1, timeout: 10)
        shot("91-downloads")

        // Played from the device: no session at the server, and Details says so.
        downloadRows("Direct H264").firstMatch.tap()
        await eventually("the player covers the list") { !element("downloads.page").isHittable }
        tapInPlayer("player.options", "Options")
        let downloaded = app.descendants(matching: .any).matching(NSPredicate(
            format: "label CONTAINS 'Downloaded'")).firstMatch
        wait(downloaded, "Details' Downloaded")
        shot("92-player-downloaded")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.5)).tap()
        let sessions = await Admin.sessions()
        XCTAssertFalse(sessions.contains { $0.title == "Direct H264" }, "a download plays without a session")
        closePlayer()
        wait(element("downloads.page"), "Downloads after the player")

        // The season, converted by the server for this device, then downloaded.
        openLibrary(shows, "Shows")
        wait(titled("Pley Test Show"), "the show in Shows").tap()
        wait(button("item.play"), "the show's Play")
        reveal(button("item.downloadSeason"), "Download season").tap()
        openDownloads()
        await waitDownloaded("Pley Test Show", count: 3, timeout: 300)
        shot("93-season-downloaded")

        deleteAllDownloads()
    }

    /// Settings → Downloads → Delete all downloads, and Downloads is empty.
    func deleteAllDownloads() {
        openMe()
        reveal(element("me.settings"), "Settings").tap()
        reveal(element("settings.downloads"), "Downloads in Settings").tap()
        wait(element("settings.downloads.quality"), "Downloads' quality")
        shot("94-download-settings")
        reveal(element("settings.downloads.deleteAll"), "Delete all downloads").tap()
        let confirm = app.buttons.matching(NSPredicate(format: "label == 'Delete all'")).firstMatch
        wait(confirm, "Delete all's confirmation").tap()
        openDownloads()
        wait(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Nothing downloaded yet'")).firstMatch,
             "Downloads empty")
    }
}
