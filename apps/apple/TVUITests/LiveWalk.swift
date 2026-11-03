import Foundation
import XCTest

/// Live TV as the administrator sees it from the test, through the server.
extension Admin {
    /// The channel named `name`'s id, from All channels.
    static func channelId(_ name: String) async -> Int? {
        let query = name.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? name
        let (_, answer) = await call("GET", "/api/live/browse?category=all&q=\(query)&limit=20")
        let channels = (answer as? [String: Any])?["channels"] as? [[String: Any]] ?? []
        return channels.first { $0["name"] as? String == name }?["id"] as? Int
    }

    /// The administrator's favourite channels' ids.
    static func favoriteChannels() async -> Set<Int> {
        let (_, answer) = await call("GET", "/api/live/browse?category=favorites&limit=100")
        let channels = (answer as? [String: Any])?["channels"] as? [[String: Any]] ?? []
        return Set(channels.compactMap { $0["id"] as? Int })
    }

    /// Takes the star off `id`, so the walk starts from none.
    static func unfavorite(_ id: Int) async {
        _ = await call("DELETE", "/api/live/channels/\(id)/favorite")
    }

    /// Watches `id` as another device: its keyed playlist's address.
    static func watch(_ id: Int) async -> String? {
        let (status, answer) = await call("POST", "/api/live/channels/\(id)/watch",
                                          body: #"{"client_id":"check-other-device","keyed_urls":true}"#)
        guard status == 200 else { return nil }
        return (answer as? [String: Any])?["url"] as? String
    }

    /// What the keyed playlist at `path` answers now.
    static func playlistStatus(_ path: String) async -> Int {
        guard let url = URL(string: origin + path),
              let (_, response) = try? await URLSession.shared.data(from: url) else { return 0 }
        return (response as? HTTPURLResponse)?.statusCode ?? 0
    }

    /// How many sessions watch any channel, for everyone.
    static func liveViewers() async -> Int {
        let (_, answer) = await call("GET", "/api/live/tuners")
        let tuners = answer as? [[String: Any]] ?? []
        return tuners.reduce(0) { $0 + (($1["viewers"] as? [Any])?.count ?? 0) }
    }
}

extension Walk {
    /// The profile signed in, in "Who's watching?": it goes on without its PIN, which is typed if asked.
    func chooseSignedIn() {
        select(button("profiles.profile.\(Admin.adminName)"), "Who's watching?", first: [.right, .left])
        if element("profiles.pin").waitForExistence(timeout: 3) {
            select(element("profiles.pin"), "the PIN")
            type(Admin.adminPin)
            let next = button("profiles.pin.continue")
            if next.exists { select(next, "Continue") }
        }
    }

    /// The Live TV tab, after Home.
    func openLiveTv() {
        let tab = app.tabBars.buttons["Live TV"]
        focus(tab, "the Live TV tab", first: [.up, .right, .left])
        remote.press(.select)
        wait(element("live.guide"), "the guide", timeout: 30)
    }

    /// The tab bar's ends are on the screen: Search, first, and Settings,
    /// last, neither cut off nor scrolled away.
    func tabsFit(file: StaticString = #filePath, line: UInt = #line) {
        let screen = app.windows.firstMatch.frame
        for name in ["Search", "Settings"] {
            let tab = app.tabBars.buttons[name]
            XCTAssertTrue(tab.exists && screen.contains(tab.frame),
                          "the \(name) tab is off the screen: \(tab.frame) of \(screen)", file: file, line: line)
        }
    }

    /// A category of the column down the left.
    /// Left from the tab bar would change tabs, and up from the column's top
    /// would leave it, so the focus goes down into the page, left into the
    /// column, then up or down towards the category.
    func chooseCategory(_ key: String) {
        let target = wait(element("live.category.\(key)"), "the category \(key)")
        let focused = app.buttons.matching(NSPredicate(format: "hasFocus == true")).firstMatch
        if app.tabBars.buttons.matching(NSPredicate(format: "hasFocus == true")).count > 0 {
            remote.press(.down)
        }
        for _ in 0..<4 where !focused.identifier.hasPrefix("live.category.") {
            remote.press(.left)
        }
        for _ in 0..<30 where !target.hasFocus {
            guard focused.exists, target.exists else { break }
            remote.press(focused.frame.midY > target.frame.midY ? .up : .down)
        }
        XCTAssertTrue(target.hasFocus, "couldn't move the focus to the category \(key)")
        remote.press(.select)
        sleep(1)
    }

    /// Waits for the live player to say `state` (playing, failed…).
    func livePlayer(says state: String, timeout: Double = 40, file: StaticString = #filePath,
                    line: UInt = #line) async {
        let label = element("live.player.state")
        let said = await holds(within: timeout) { label.exists && label.label == state }
        XCTAssertTrue(said, "the live player says \(label.exists ? label.label : "nothing"), not \(state)",
                      file: file, line: line)
    }

    /// The player's channel name holds `name` (shown again when hidden).
    func liveChannel(is name: String, file: StaticString = #filePath, line: UInt = #line) async {
        let label = element("live.channelName")
        var shown = await holds(within: 8) { label.exists && label.label.contains(name) }
        if !shown {
            remote.press(.playPause)
            shown = await holds(within: 15) { label.exists && label.label.contains(name) }
        }
        XCTAssertTrue(shown, "the player shows \(label.exists ? label.label : "nothing"), not \(name)",
                      file: file, line: line)
    }

    /// Select on the picture opens the overlay; then `id` in it.
    func selectInOverlay(_ id: String, _ what: String) {
        if !element(id).exists { remote.press(.select) }
        select(element(id), what, first: [.right, .left])
    }

    /// Whether `check` comes true within `timeout` seconds.
    func holds(within timeout: Double = 20, _ check: () async -> Bool) async -> Bool {
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            if await check() { return true }
            try? await Task.sleep(for: .milliseconds(500))
        }
        return await check()
    }
}
