import XCTest

/// Music as the server sees it: the sessions on the dashboard.
extension Admin {
    /// The sessions playing now: each one's id, its item's title and its state.
    static func sessions() async -> [(id: String, title: String, state: String)] {
        let (_, answer) = await call("GET", "/api/activity")
        let sessions = (answer as? [String: Any])?["sessions"] as? [[String: Any]] ?? []
        return sessions.compactMap { session in
            guard let id = session["session_id"] as? String,
                  let title = (session["item"] as? [String: Any])?["title"] as? String else { return nil }
            return (id, title, session["state"] as? String ?? "")
        }
    }

    /// A music playlist of `items`, made as the web does.
    static func makePlaylist(_ title: String, items: [Int]) async -> Int? {
        let ids = items.map(String.init).joined(separator: ",")
        let (status, answer) = await call("POST", "/api/playlists",
                                          body: #"{"title":"\#(title)","kind":"audio","item_ids":[\#(ids)]}"#)
        return status == 201 ? (answer as? [String: Any])?["id"] as? Int : nil
    }

    /// The tracks of the artist called `artist` in library `music`, in their album's order.
    static func tracks(music: Int, artist: String) async -> [Int] {
        guard let id = await titles(library: music)[artist] else { return [] }
        let albums = (await call("GET", "/api/items/\(id)/children").1 as? [String: Any])?["items"] as? [[String: Any]]
        guard let album = albums?.first?["id"] as? Int else { return [] }
        let tracks = (await call("GET", "/api/items/\(album)/children").1 as? [String: Any])?["items"] as? [[String: Any]]
        return (tracks ?? []).compactMap { $0["id"] as? Int }
    }
}

/// Apple TV's music steps: an album played from its page, the Now Playing
/// tab, Next, Shuffle and the sleep timer, and a music playlist played from
/// an entry, then paused with the remote's Play/Pause; each checked on the
/// server.
extension Walk {
    static let musicPlaylist = "UI Music"

    func nowPlaying(says title: String, file: StaticString = #filePath, line: UInt = #line) async {
        wait(element("music.nowPlaying.title"), "Now Playing", timeout: 30, file: file, line: line)
        await eventually("Now Playing says \(title)", file: file, line: line) {
            element("music.nowPlaying.title").label == title
        }
    }

    func musicWalk(music: Int) async {
        await Admin.deletePlaylists(titled: [Self.musicPlaylist])
        openGrid("Music")
        select(titled("Test Artist", in: "library.grid"), "the artist in Music", first: [.down, .right])
        wait(button("artist.play"), "the artist's Play")
        select(titled("Test Album", in: "artist.row.albums"), "the artist's album", first: [.down, .right])
        wait(element("album.title"), "the album's page")
        shot("29-album")
        select(button("album.play"), "the album's Play", first: [.left, .up])
        XCTAssertTrue(app.tabBars.buttons["Now Playing"].waitForExistence(timeout: 20), "the Now Playing tab")
        await nowPlaying(says: "First")
        var first = ""
        await eventually("the dashboard shows First playing") {
            first = await Admin.sessions().first { $0.title == "First" }?.id ?? ""
            return !first.isEmpty
        }
        shot("30-now-playing")

        select(button("music.next"), "Next", first: [.down, .right, .left])
        await nowPlaying(says: "Second")
        await eventually("Second's session takes First's place") {
            let sessions = await Admin.sessions()
            return sessions.map(\.title) == ["Second"] && !sessions.contains { $0.id == first }
        }

        select(button("music.shuffle"), "Shuffle", first: [.down, .left])
        XCTAssertTrue(valueBecomes(button("music.shuffle"), "On"), "Shuffle is on")
        select(button("music.sleep"), "the sleep timer", first: [.right, .down])
        select(menuItem("15 minutes"), "15 minutes", first: [.down])
        XCTAssertTrue(valueBecomes(button("music.sleep"), "15 min left"), "the sleep timer says 15 min left")
        shot("31-now-playing-sleep")
        select(button("music.sleep"), "the sleep timer", first: [.right, .down])
        select(menuItem("Off"), "Off", first: [.up])
        XCTAssertTrue(valueBecomes(button("music.sleep"), "Off"), "the sleep timer is off")

        // A music playlist of the album's tracks: its second entry plays.
        let tracks = await Admin.tracks(music: music, artist: "Test Artist")
        guard let id = await Admin.makePlaylist(Self.musicPlaylist, items: tracks) else {
            XCTFail("the playlist UI Music wasn't made")
            return
        }
        let entries = await Admin.entries(id).map(\.entry)
        guard entries.count == 2 else {
            XCTFail("UI Music has \(entries.count) entries, not 2")
            return
        }
        openPlaylists(first: [.up, .left])
        select(button("tile.playlist.\(id)"), "UI Music in Playlists", first: [.down, .right])
        wait(element("playlist.title"), "UI Music's page")
        select(button("playlist.entry.\(entries[1])"), "UI Music's second entry", first: [.down, .right])
        await nowPlaying(says: "Second")
        await eventually("the dashboard shows Second") { await Admin.sessions().map(\.title) == ["Second"] }
        shot("32-music-playlist")
        // The remote's Play/Pause pauses the music from any tab.
        remote.press(.playPause)
        await eventually("the music pauses") { button("music.toggle").label == "Play" }
        await Admin.deletePlaylists(titled: [Self.musicPlaylist])
    }
}
