import UIKit
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

    /// A playlist of the album's tracks, made as the web does.
    static func makePlaylist(_ title: String, items: [Int]) async -> Int? {
        let ids = items.map(String.init).joined(separator: ",")
        let (status, answer) = await call("POST", "/api/playlists",
                                          body: #"{"title":"\#(title)","kind":"audio","item_ids":[\#(ids)]}"#)
        return status == 201 ? (answer as? [String: Any])?["id"] as? Int : nil
    }
}

/// The music steps: an album played from its page, the mini player, Next,
/// Now Playing with Shuffle and the sleep timer, and a music playlist
/// played from an entry, each checked on the server.
extension Walk {
    static let musicPlaylist = "UI Music"

    /// The mini player: one element for VoiceOver, "Now playing, First, Test Artist".
    var mini: XCUIElement { element("music.mini") }

    func waitForTrack(_ title: String, file: StaticString = #filePath, line: UInt = #line) async {
        wait(mini, "the mini player", timeout: 30, file: file, line: line)
        await eventually("the mini player says \(title)", file: file, line: line) {
            mini.label.hasPrefix("Now playing, \(title)")
        }
    }

    /// One of the mini player's buttons, at its trailing end: VoiceOver
    /// reads the mini player as one element, so the test taps where they are.
    func tapMini(_ fromTrailing: CGFloat) {
        let frame = mini.frame
        mini.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0.5))
            .withOffset(CGVector(dx: frame.width - fromTrailing, dy: 0)).tap()
    }

    /// Next is the last button; Play/Pause is before it.
    func tapMiniNext() { tapMini(34) }
    func tapMiniToggle() { tapMini(82) }

    func musicWalk(music: Int) async {
        await Admin.deletePlaylists(titled: [Self.musicPlaylist])
        openLibrary(music, "Music")
        wait(titled("Test Artist"), "the artist in Music").tap()
        wait(button("artist.play"), "the artist's Play")
        wait(titled("Test Album"), "the artist's album").tap()
        wait(element("album.title"), "the album's page")
        tap("album.play", "the album's Play")
        await waitForTrack("First")
        var first = ""
        await eventually("the dashboard shows First playing") {
            first = await Admin.sessions().first { $0.title == "First" }?.id ?? ""
            return !first.isEmpty
        }
        let playing = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'album.track.' AND label CONTAINS ', playing'"))
        wait(playing.firstMatch, "the track playing marked on the album")
        shot("30-music-mini")

        tapMiniNext()
        await waitForTrack("Second")
        await eventually("Second's session takes First's place") {
            let sessions = await Admin.sessions()
            return sessions.map(\.title) == ["Second"] && !sessions.contains { $0.id == first }
        }

        // Now Playing: Shuffle and the sleep timer.
        mini.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)).tap()
        wait(element("music.nowPlaying.title"), "Now Playing")
        XCTAssertEqual(element("music.nowPlaying.title").label, "Second", "Now Playing says the track playing")
        shot("31-now-playing")
        let shuffle = wait(element("music.shuffle"), "Shuffle")
        shuffle.tap()
        await eventually("Shuffle is on") { shuffle.isSelected || (shuffle.value as? String) == "1" }
        let sleepTimer = wait(element("music.sleep"), "the sleep timer")
        sleepTimer.tap()
        wait(app.buttons["15 minutes"].firstMatch, "15 minutes in the sleep timer").tap()
        await eventually("the sleep timer says 15 min left") { (sleepTimer.value as? String) == "15 min left" }
        shot("32-now-playing-sleep")
        sleepTimer.tap()
        wait(app.buttons["Off"].firstMatch, "Off in the sleep timer").tap()
        tap("music.nowPlaying.done", "Done")

        // A music playlist of the artist's tracks: an entry plays from there.
        openLibrary(music, "Music")
        addToPlaylist("Test Artist")
        let name = wait(element("addToPlaylist.new.title"), "the new playlist's title")
        name.tap()
        name.typeText(Self.musicPlaylist)
        tap("addToPlaylist.new.create", "Create")
        var id = 0
        await eventually("the server has UI Music with the two tracks") {
            guard let playlist = await Admin.playlist(titled: Self.musicPlaylist),
                  let found = playlist["id"] as? Int else { return false }
            id = found
            return await Admin.entries(found).count == 2
        }
        let entries = await Admin.entries(id).map(\.entry)
        if entries.count == 2 {
            openPlaylists()
            wait(button("tile.playlist.\(id)"), "UI Music in Playlists").tap()
            wait(element("playlist.entry.\(entries[1])"), "UI Music's second entry").tap()
            await waitForTrack("Second")
            await eventually("the dashboard shows Second") { await Admin.sessions().map(\.title) == ["Second"] }
            shot("33-music-playlist")
        }
        tapMiniToggle()
        await eventually("the music pauses") { await Admin.sessions().first?.state == "paused" }
        await Admin.deletePlaylists(titled: [Self.musicPlaylist])
    }
}
