import XCTest

/// The administrator's other device: a TV the test plays on through the
/// server's API, as its player would, and reports for.
extension Admin {
    static let tvName = "Living room TV"

    /// Plays item `id` on the TV, whose player obeys play, pause, seek and
    /// skip: its session id.
    static func playOnTheTv(_ id: Int) async -> String? {
        let body = #"{"item_id":\#(id),"client_id":"check-remote-tv","mode":"direct","#
            + #""controls":["play","pause","seek","skip"],"device_name":"\#(tvName)"}"#
        let (status, answer) = await call("POST", "/api/playback", body: body)
        guard status == 200 else { return nil }
        return (answer as? [String: Any])?["session_id"] as? String
    }

    /// The TV reports `state` at `positionMs`, as its player does after a command.
    static func report(_ id: Int, session: String, state: String, positionMs: Int) async {
        _ = await call("POST", "/api/items/\(id)/progress",
                       body: #"{"position_ms":\#(positionMs),"state":"\#(state)","session_id":"\#(session)"}"#)
    }

    /// The administrator's own sessions whose players are there.
    static func playing() async -> [String] {
        let (_, answer) = await call("GET", "/api/sessions")
        return (answer as? [[String: Any]] ?? []).compactMap { $0["session_id"] as? String }
    }
}

/// The remote: a movie playing on the TV shows in the bar above the tabs
/// (at the page's foot beside the iPad's sidebar), and its sheet pauses
/// it, says when the TV doesn't answer, and stops it once asked.
extension Walk {
    func remoteWalk(movies: Int) async {
        guard let id = await Admin.titles(library: movies)["Direct H264"] else {
            XCTFail("the test media has Direct H264")
            return
        }
        guard let tv = await Admin.playOnTheTv(id) else {
            XCTFail("the TV plays Direct H264")
            return
        }
        await Admin.report(id, session: tv, state: "playing", positionMs: 60_000)
        let bar = wait(element("remote.bar"), "the remote's bar", timeout: 30)
        XCTAssertTrue(bar.label.contains(Admin.tvName), "the bar names the TV: \(bar.label)")
        // One device playing: the bar pauses it without opening the remote.
        XCTAssertEqual(wait(button("remote.bar.toggle"), "the bar's Pause").label, "Pause on \(Admin.tvName)")
        shot("90-remote-bar")

        bar.tap()
        wait(element("remote.session.\(tv)"), "the TV in the remote")
        let toggle = button("remote.toggle.\(tv)")
        XCTAssertEqual(wait(toggle, "Pause").label, "Pause on \(Admin.tvName)")
        shot("91-remote-sheet")

        // The TV pauses and reports, as its player does on the command.
        toggle.tap()
        await Admin.report(id, session: tv, state: "paused", positionMs: 60_000)
        await eventually("Play shows once the TV paused") { toggle.label == "Play on \(Admin.tvName)" }

        // Forward 30 seconds, which the TV doesn't act on: the remote says so.
        tap("remote.forward.\(tv)", "Forward 30 seconds")
        let error = wait(element("remote.error.\(tv)"), "the TV not answering", timeout: 15)
        XCTAssertEqual(error.label, "\(Admin.tvName) didn't answer.")
        shot("92-remote-no-answer")

        // Stop asks first.
        tap("remote.stop.\(tv)", "Stop")
        wait(button("remote.stop.confirm", or: "Stop"), "Stop in the confirmation").tap()
        let stopped = await holds { await !Admin.playing().contains(tv) }
        XCTAssertTrue(stopped, "Stop ends the TV's session at the server")
        // Nothing plays on another device: the sheet and the bar go.
        let gone = await holds(within: 10) { !element("remote.session.\(tv)").exists && !element("remote.bar").exists }
        XCTAssertTrue(gone, "the remote closes once nothing plays elsewhere")
    }
}
