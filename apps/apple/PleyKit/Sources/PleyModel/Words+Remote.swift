import Foundation

/// The remote's words, as the web's remote says them
/// (apps/web `components/Remote.tsx`), so every remote names its devices
/// and buttons the same way.
extension Words {
    /// What the remote calls a device the server couldn't name.
    public static let unnamedDevice = "Unknown device"

    /// The remote's sheet and panel: the profile's other devices.
    public static let remoteTitle = "Your devices"

    /// How far the remote's skip buttons go, back and forward.
    public static let remoteSkipBackMs = 10_000
    public static let remoteSkipForwardMs = 30_000

    /// A device that is on and plays nothing: its row's second line.
    public static let readyToPlay = "Ready to play"
    /// Its controls.
    public static let nothingPlaying = "Nothing playing"
    public static let castHint = "Find something to watch or listen to, then choose Play on… and this device."

    /// The remote's track pickers, and the subtitles' choice of none.
    public static let remoteAudioTitle = "Audio"
    public static let remoteSubtitlesTitle = "Subtitles"
    public static let remoteSubtitlesOff = "Off"

    /// What the remote calls each device, by id. Two of the same name (two
    /// of the same TV) are numbered, "Living room TV (2)", in the order of
    /// their ids, which stays as they play and stop, so every label and
    /// message says which one it means.
    public static func deviceNames(_ devices: [(id: String, name: String?)]) -> [String: String] {
        let sorted = devices.sorted { $0.id < $1.id }
        var total: [String: Int] = [:]
        for device in sorted { total[device.name ?? unnamedDevice, default: 0] += 1 }
        var seen: [String: Int] = [:]
        var names: [String: String] = [:]
        for device in sorted {
            let name = device.name ?? unnamedDevice
            seen[name, default: 0] += 1
            names[device.id] = total[name, default: 0] > 1 ? "\(name) (\(seen[name, default: 0]))" : name
        }
        return names
    }

    /// "Remote, 2 devices": what VoiceOver reads for the remote's bar and button.
    public static func remoteLabel(_ count: Int) -> String {
        "Remote, \(pluralize(count, "device"))"
    }

    /// A channel as the remote names it: its number and name, "101 News".
    public static func remoteChannel(_ channel: PlayingChannel) -> String {
        [channel.number ?? "", channel.name].filter { !$0.isEmpty }.joined(separator: " ")
    }

    /// What a device plays, on its row and in "Play on…": the title in
    /// full, or the channel with its number.
    public static func remotePlaying(_ session: PlayingSession) -> String {
        if session.kind == .live { return session.channel.map(remoteChannel) ?? "Live TV" }
        return session.item.map(fullTitle) ?? ""
    }

    /// The heading of a device's controls, on two lines: an episode's show
    /// with its season and number ("Show · S1 · E2"), then its title; a
    /// track's title, then its artist; a channel's number and name, then
    /// what's on; anything else, its title in full.
    public static func remoteHeading(_ session: PlayingSession) -> (title: String, subtitle: String?) {
        if session.kind == .live {
            let now = session.channel?.now.flatMap { $0.isEmpty ? nil : $0 }
            return (session.channel.map(remoteChannel) ?? "Live TV", now)
        }
        guard let item = session.item else { return ("", nil) }
        switch item.kind {
        case .episode:
            let show = [item.grandparentTitle ?? "", episodeLabel(season: item.parentIndex, episode: item.index)]
                .filter { !$0.isEmpty }.joined(separator: " · ")
            if show.isEmpty { return (item.title, nil) }
            return (show, item.title)
        case .track:
            return (item.title, item.grandparentTitle.flatMap { $0.isEmpty ? nil : $0 })
        default:
            return (fullTitle(item), nil)
        }
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
    public static func remoteAudio(_ device: String) -> String { "Audio on \(device)" }
    public static func remoteSubtitles(_ device: String) -> String { "Subtitles on \(device)" }

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
