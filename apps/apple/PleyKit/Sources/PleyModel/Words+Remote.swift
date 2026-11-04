import Foundation

/// The remote's words, as the web's remote says them
/// (apps/web `components/Remote.tsx`), so every remote names its devices
/// and buttons the same way.
extension Words {
    /// What the remote calls a device the server couldn't name.
    public static let unnamedDevice = "Unknown device"

    /// The remote's sheet and panel: "Playing on your devices".
    public static let remoteTitle = "Playing on your devices"

    /// How far the remote's skip buttons go, back and forward.
    public static let remoteSkipBackMs = 10_000
    public static let remoteSkipForwardMs = 30_000

    /// What the remote calls each session's device, by session id. Two of
    /// the same name (two of the same TV) are numbered, "Living room TV
    /// (2)", so every label and message says which one it means.
    public static func deviceNames(_ sessions: [PlayingSession]) -> [String: String] {
        var total: [String: Int] = [:]
        for session in sessions { total[session.device ?? unnamedDevice, default: 0] += 1 }
        var seen: [String: Int] = [:]
        var names: [String: String] = [:]
        for session in sessions {
            let name = session.device ?? unnamedDevice
            seen[name, default: 0] += 1
            names[session.sessionId] = total[name, default: 0] > 1 ? "\(name) (\(seen[name]!))" : name
        }
        return names
    }

    /// "Remote, 2 devices playing": what VoiceOver reads for the remote's bar and button.
    public static func remoteLabel(_ count: Int) -> String {
        "Remote, \(pluralize(count, "device")) playing"
    }

    /// What a session plays, as the remote names it: the title in full, or
    /// the channel (Live TV without one).
    public static func remoteTitle(_ session: PlayingSession) -> String {
        if session.kind == .live { return session.channel?.name ?? "Live TV" }
        return session.item.map(fullTitle) ?? ""
    }

    /// The remote's buttons for `device`, as VoiceOver reads them.
    public static func remotePrevious(_ device: String, live: Bool) -> String {
        live ? "Previous channel on \(device)" : "Previous on \(device)"
    }

    public static func remoteNext(_ device: String, live: Bool) -> String {
        live ? "Next channel on \(device)" : "Next on \(device)"
    }

    public static func remoteBack(_ device: String) -> String {
        "Back \(remoteSkipBackMs / 1000) seconds on \(device)"
    }

    public static func remoteForward(_ device: String) -> String {
        "Forward \(remoteSkipForwardMs / 1000) seconds on \(device)"
    }

    public static func remotePlay(_ device: String) -> String { "Play on \(device)" }
    public static func remotePause(_ device: String) -> String { "Pause on \(device)" }
    public static func remoteStop(_ device: String) -> String { "Stop on \(device)" }
    public static func remoteSeek(_ device: String) -> String { "Seek on \(device)" }
    public static func remoteProgress(_ device: String) -> String { "Progress on \(device)" }
    public static func remoteWaiting(_ device: String) -> String { "Waiting for \(device)" }

    /// After a command the player didn't show it acted on.
    public static func remoteNoAnswer(_ device: String) -> String { "\(device) didn't answer." }

    /// What the remote says when the server refused a command or a stop
    /// with `status`; `nil` for a refusal said in the server's own words.
    public static func remoteRefusal(status: Int, device: String) -> String? {
        switch status {
        case 404: "\(device) isn't playing any more."
        case 409: "\(device) can't do that."
        case 429: "Too many at once: try again in a moment."
        default: nil
        }
    }
}
