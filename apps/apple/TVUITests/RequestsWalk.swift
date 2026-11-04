import XCTest

/// Requests on Apple TV, against the fixture server (pley-apple-fixtures),
/// which answers for TMDB as the test server can't: the news of a request
/// ready to watch popping up, Discover's tab with its hero, a row's See
/// all, a show's page and Which seasons?, the requests with an
/// administrator approving and declining with a reason in a request's
/// panel, and Search's Discover row with its See all.
///
/// check-screens.sh starts the fixture server beside the walk and passes:
///
/// - `PLEY_FIXTURES_ORIGIN`: its address.
/// - `PLEY_FIXTURES_LOG`: the file it writes what it was asked, a line a
///   request (`POST /api/requests/49/approve`), without tokens.
enum FixtureHost {
    static var origin: String { Admin.env["PLEY_FIXTURES_ORIGIN"] ?? "" }

    /// What the fixture server was asked so far, a line each.
    static var asked: [String] {
        guard let path = Admin.env["PLEY_FIXTURES_LOG"], !path.isEmpty,
              let text = try? String(contentsOfFile: path, encoding: .utf8) else { return [] }
        return text.split(separator: "\n").map(String.init)
    }
}

extension Walk {
    /// Signs in to the fixture server as Alex, its administrator, by a code
    /// it allows by itself: out of the test server first.
    func signInToFixtures() {
        startSignedOut()
        select(button("servers.add"), "Add a server")
        select(element("servers.address"), "the address field")
        type(FixtureHost.origin)
        let connect = button("servers.connect")
        if connect.exists { select(connect, "Connect") }
        let alex = button("profiles.profile.Alex")
        wait(alex, "the fixture's Who's watching?", timeout: 30)
        select(alex, "Alex in Who's watching?", first: [.right, .left])
        seeHome("Alex")
    }

    /// The news that Arrival is ready, told once the remote rests: it takes
    /// the focus on Open, which opens Arrival's page.
    func requestNews() {
        let notice = wait(element("notice"), "the news of a request ready", timeout: 30)
        let text = element("notice.text").label
        XCTAssertTrue(text.contains("Arrival"), "the news says \(text)")
        let open = button("notice.open")
        _ = open.waitForExistence(timeout: 5)
        let focused = NSPredicate(format: "hasFocus == true")
        test.wait(for: [XCTNSPredicateExpectation(predicate: focused, object: open)], timeout: 5)
        shot("90-request-news")
        // A press in its first moments is taken as meant for the page.
        sleep(1)
        remote.press(.select)
        wait(element("item.page"), "Arrival's page from the news")
        XCTAssertFalse(notice.exists, "the news is gone once opened")
        remote.press(.menu)
        seeHome("Alex")
    }

    /// The Discover tab, after Home.
    func openDiscover() {
        showTabs()
        let tab = app.tabBars.buttons["Discover"]
        wait(tab, "the Discover tab")
        let home = app.tabBars.buttons["Home"]
        XCTAssertLessThan(home.frame.minX, tab.frame.minX, "Discover comes after Home")
        focus(tab, "the Discover tab", first: [.up, .right, .left])
        remote.press(.select)
        wait(element("discover.page"), "Discover")
    }

    /// Discover's hero for the first trending title, The Wild Robot: its
    /// Request, which asks for it; then a row's See all, the grid of its
    /// titles to its end, and back.
    func discoverHeroAndGrid() {
        let title = wait(element("discover.hero.title"), "the hero", timeout: 30)
        XCTAssertEqual(title.label, "The Wild Robot")
        XCTAssertTrue(element("discover.requests").exists, "the way to the requests shows")
        shot("91-discover")
        let primary = button("discover.hero.primary")
        XCTAssertEqual(primary.label, "Request")
        select(primary, "the hero's Request", first: [.down, .up])
        let asked = NSPredicate(format: "label != 'Request'")
        let gone = XCTNSPredicateExpectation(predicate: asked, object: primary)
        test.wait(for: [gone], timeout: 20)
        serverAsked("POST /api/requests", "The Wild Robot asked for on the server")
        shot("92-discover-asked")

        let row = wait(element("discover.row.trending"), "the Trending row")
        _ = row
        select(button("discover.row.trending.seeAll"), "Trending's See all", first: [.down])
        wait(element("discoverList.page"), "Trending's grid", timeout: 30)
        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'discoverList.card.'")).firstMatch
        wait(card, "a title in the grid")
        let titles = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'discoverList.card.'"))
        XCTAssertEqual(titles.count, 6, "the grid has Trending's six titles, each once")
        focus(card, "a title in the grid", first: [.down])
        shot("93-discover-grid")
        // The list ended where the server said: it didn't stop short, so no Show more.
        XCTAssertFalse(button("discoverGrid.more").exists, "Show more shows at the end of the list")
        remote.press(.menu)
        wait(element("discover.hero.title"), "Discover again")
    }

    /// Breaking Bad's page, two of its seasons asked for: Request more
    /// seasons opens Which seasons?, which Menu closes; season 3, which may
    /// be asked for, opens it at that season, where Select unticks and ticks
    /// it, and Request asks for the seasons ticked.
    func discoverShowAndSeasons() {
        let card = button("discover.card.trending.show:1396")
        select(card, "Breaking Bad in Trending", first: [.down, .right])
        wait(element("discoverTitle.page"), "Breaking Bad's page", timeout: 30)
        XCTAssertEqual(element("discoverTitle.title").label, "Breaking Bad")
        shot("94-discover-title")

        let more = button("discoverTitle.requestMore")
        select(more, "Request more seasons", first: [.right, .down])
        let picker = wait(element("picker"), "Which seasons?")
        shot("95-which-seasons")
        remote.press(.menu)
        let closed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: picker)
        test.wait(for: [closed], timeout: 10)

        select(button("discoverTitle.season.3"), "season 3", first: [.down, .right])
        wait(element("picker"), "Which seasons? from season 3")
        let row = button("picker.season.3")
        let focused = NSPredicate(format: "hasFocus == true")
        test.wait(for: [XCTNSPredicateExpectation(predicate: focused, object: row)], timeout: 5)
        // Every season that may be asked for starts ticked: Select unticks it, and ticks it again.
        XCTAssertTrue(row.isSelected, "season 3 starts ticked")
        remote.press(.select)
        test.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "selected == false"), object: row)],
                  timeout: 5)
        remote.press(.select)
        test.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "selected == true"), object: row)],
                  timeout: 5)
        shot("96-which-seasons-picked")
        select(button("picker.ask"), "Request in Which seasons?", first: [.down, .right])
        let asked = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),
                                              object: element("picker"))
        test.wait(for: [asked], timeout: 20)
        serverAsked("POST /api/requests", times: 2, "season 3 asked for on the server")
        remote.press(.menu)
        wait(element("discover.hero.title"), "Discover again")
    }

    /// The requests, as an administrator: Needs attention's cards; Arcane's
    /// panel approves it, and The Substance's declines it with a reason.
    func requestsAndApproving() {
        let way = button("discover.requests")
        select(way, "the way to the requests", first: [.up])
        wait(element("requests.page"), "the requests", timeout: 30)
        let arcane = button("request.49")
        wait(arcane, "Arcane waiting in Needs attention", timeout: 30)
        shot("97-requests")

        select(arcane, "Arcane's card", first: [.down])
        wait(element("requests.panel"), "Arcane's panel")
        shot("98-request-panel")
        select(button("request.panel.approve"), "Approve", first: [.down, .up])
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),
                                             object: element("requests.panel"))
        test.wait(for: [gone], timeout: 20)
        serverAsked("POST /api/requests/49/approve", "Arcane approved on the server")

        let substance = button("request.44")
        select(substance, "The Substance's card", first: [.up, .down])
        wait(element("requests.panel"), "The Substance's panel")
        select(button("request.panel.declineWithReason"), "Decline with a reason…", first: [.down, .up])
        let reason = wait(element("request.panel.reason"), "the reason's field")
        select(reason, "the reason's field", first: [.down, .up])
        type("Not on this server")
        select(button("request.panel.decline"), "Decline", first: [.down, .up])
        let closed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),
                                               object: element("requests.panel"))
        test.wait(for: [closed], timeout: 20)
        serverAsked("POST /api/requests/44/decline", "The Substance declined on the server")
        shot("99-requests-decided")
        remote.press(.menu)
    }

    /// Search's Discover row, after what the libraries have, and its See all.
    func searchDiscover() {
        showTabs()
        openSearch()
        focus(app.keyboards.firstMatch, "the keyboard", first: [.down])
        app.typeText("the")
        wait(element("search.discover.row"), "Search's Discover row", timeout: 30)
        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'search.discover.card.'"))
            .firstMatch
        wait(card, "a title in Search's Discover row", timeout: 30)
        let seeAll = button("search.discover.row.seeAll")
        for _ in 0..<12 where !seeAll.exists || !seeAll.isHittable {
            remote.press(.down)
        }
        select(seeAll, "Search's See all", first: [.down, .right])
        wait(element("discoverResults.page"), "Search's grid", timeout: 30)
        wait(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'discoverResults.card.'")).firstMatch,
             "a title in Search's grid")
        shot("100-search-discover")
        remote.press(.menu)
    }

    /// The fixture server is asked `line` (`POST /api/requests`) `times` in
    /// all within 10 seconds: it writes its log as it goes.
    func serverAsked(_ line: String, times: Int = 1, _ what: String, file: StaticString = #filePath,
                     line number: UInt = #line) {
        for _ in 0..<20 {
            if FixtureHost.asked.filter({ $0 == line }).count >= times { return }
            usleep(500_000)
        }
        XCTFail("\(what): not after 10 seconds", file: file, line: number)
    }
}
