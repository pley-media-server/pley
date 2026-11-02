import XCTest

extension Admin {
    /// Puts `id` (30 s long) part way through for the administrator, so it leads Continue Watching (a resume point is kept from 15 s).
    static func startWatching(_ id: Int) async -> Bool {
        _ = await call("POST", "/api/items/\(id)/unwatched")
        let body = #"{"position_ms":16000,"duration_ms":30000,"state":"stopped"}"#
        return await call("POST", "/api/items/\(id)/progress", body: body).0 == 204
    }

    /// Puts `id` back unwatched at its start, out of the administrator's Continue Watching.
    static func stopWatching(_ id: Int) async {
        _ = await call("POST", "/api/items/\(id)/unwatched")
        let body = #"{"position_ms":0,"duration_ms":30000,"state":"stopped"}"#
        _ = await call("POST", "/api/items/\(id)/progress", body: body)
        _ = await call("DELETE", "/api/continue-watching/\(id)")
    }
}

extension Walk {
    /// The links the app made for the first Top Shelf title (the app's
    /// test hook, `-PleyShelfLinks YES`): its page and its playback.
    func shelfLinks(first id: Int) async -> (open: URL, play: URL)? {
        let open = element("test.shelf.open")
        let wanted = NSPredicate(format: "label CONTAINS %@", "/shelf/open/\(id)?")
        let found = XCTNSPredicateExpectation(predicate: wanted, object: open)
        if await XCTWaiter.fulfillment(of: [found], timeout: 30) != .completed {
            shot("80-shelf-missing")
            let count = element("test.shelf.count")
            XCTFail("the shelf's first title isn't \(id): count \(count.exists ? count.label : "absent"), "
                + "first \(open.exists ? open.label : "absent")")
            return nil
        }
        guard let page = URL(string: open.label), let play = URL(string: element("test.shelf.play").label) else {
            XCTFail("the shelf's links aren't addresses")
            return nil
        }
        return (page, play)
    }

    /// Opens `url` as the Top Shelf does. The simulator starts the app
    /// again with it, so "Who's watching?" shows first: the link waits
    /// through it, and acts once the administrator, the profile it was
    /// made for, is chosen again.
    func openFromShelf(_ url: URL) {
        app.open(url)
        if button("profiles.profile.\(Admin.adminName)").waitForExistence(timeout: 10) {
            chooseSignedIn()
        }
    }

    /// The Top Shelf's links: an unsigned one does nothing; the signed
    /// `open` shows the title's page over Home, and `play` plays it.
    /// Two Audio (`id`) leads Continue Watching.
    func shelfLinks(_ id: Int) async {
        guard let links = await shelfLinks(first: id) else { return }
        XCTAssertNotEqual(element("test.shelf.count").label, "0", "the shelf holds Continue Watching")

        // The same link with another signature, or without one: Home stays.
        var forged = URLComponents(url: links.open, resolvingAgainstBaseURL: false)!
        forged.queryItems = forged.queryItems?.map {
            $0.name == "sig" ? URLQueryItem(name: "sig", value: String(repeating: "0", count: 32)) : $0
        }
        openFromShelf(forged.url!)
        seeHome("Home after a link signed with another key")
        forged.queryItems = forged.queryItems?.filter { $0.name != "sig" }
        openFromShelf(forged.url!)
        seeHome("Home after an unsigned link")
        sleep(3)
        XCTAssertFalse(button("item.play").exists, "an unsigned link opened a page")
        shot("80-shelf-unsigned")

        openFromShelf(links.open)
        wait(button("item.play"), "Two Audio's page from its Top Shelf link", timeout: 30)
        XCTAssertTrue(app.staticTexts["Two Audio"].exists, "the page is Two Audio's")
        shot("81-shelf-open")
        remote.press(.menu)
        seeHome("Home under the page")

        openFromShelf(links.play)
        await eventually("the play link plays Two Audio") {
            await Admin.sessions().contains { $0.title == "Two Audio" }
        }
        shot("82-shelf-play")
        // Menu closes the player; a first press may only hide its controls.
        for _ in 0..<3 where !(tile.exists && tile.isHittable) {
            remote.press(.menu)
            for _ in 0..<6 where !(tile.exists && tile.isHittable) { usleep(500_000) }
        }
        await eventually("closing the player ends its session") {
            await !Admin.sessions().contains { $0.title == "Two Audio" }
        }
        seeHome("Home after the player")
    }
}
