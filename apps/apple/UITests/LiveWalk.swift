import UIKit
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
        wait(button("profiles.profile.\(Admin.adminName)"), "Who's watching?", timeout: 30).tap()
        if element("profiles.pin").waitForExistence(timeout: 3) {
            element("profiles.pin").tap()
            element("profiles.pin").typeText(Admin.adminPin)
            tapAfterTyping("profiles.pin.continue", "Continue", then: tile)
        }
    }

    /// Live TV: from Libraries on iPhone, the sidebar on iPad.
    func openLiveTv() {
        if UIDevice.current.userInterfaceIdiom == .pad {
            wait(element("shell.sidebar.liveTv"), "Live TV in the sidebar", timeout: 30).tap()
        } else {
            openSection("Libraries", sidebar: "shell.sidebar.home")
            wait(element("libraries.liveTv"), "Live TV in Libraries", timeout: 30).tap()
        }
        // The view is kept per profile: a run before may have left Channels.
        let guide = element("live.guide")
        if !guide.waitForExistence(timeout: 30), element("live.cards").exists {
            wait(app.buttons["Guide"], "the Guide view").tap()
        }
        wait(guide, "the guide", timeout: 30)
    }

    /// Chooses `category` in the categories (the sheet on iPhone).
    func chooseCategory(_ key: String) {
        if UIDevice.current.userInterfaceIdiom != .pad {
            tap("live.categories", "Categories")
        }
        wait(element("live.category.\(key)"), "the category \(key)").tap()
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
            app.tap()
            shown = await holds(within: 15) { label.exists && label.label.contains(name) }
        }
        XCTAssertTrue(shown, "the player shows \(label.exists ? label.label : "nothing"), not \(name)",
                      file: file, line: line)
    }

    /// Shows the player's overlay and taps `id` in it.
    func tapInPlayer(_ id: String, _ what: String) {
        // The overlay hides 3 seconds after the last tap: a tap on the picture
        // shows it for 3 more, and `id` is tapped at once.
        for _ in 0..<3 {
            app.tap()
            if element(id).waitForExistence(timeout: 2) {
                element(id).tap()
                return
            }
        }
        XCTFail("\(what) didn't show")
    }

    /// Eventually true, or false after `timeout` seconds.
    func holds(within timeout: Double = 20, _ check: () async -> Bool) async -> Bool {
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            if await check() { return true }
            try? await Task.sleep(for: .milliseconds(500))
        }
        return await check()
    }
}
