import Foundation

/// The remote's words, as the web's remote says them
/// (apps/web `components/Remote.tsx`), so every remote names its devices
/// and buttons the same way, in the language shown.
extension Words {
    /// What the remote calls a device the server couldn't name.
    public static var unnamedDevice: String { localized("Unknown device") }

    /// The remote's sheet and panel: the profile's other devices.
    public static var remoteTitle: String { localized("Your devices") }

    /// How far the remote's skip buttons go, back and forward.
    public static let remoteSkipBackMs = 30_000
    public static let remoteSkipForwardMs = 30_000

    /// A device that is on and plays nothing: its row's second line.
    public static var readyToPlay: String { localized("Ready to play") }
    /// Its controls.
    public static var nothingPlaying: String { localized("Nothing playing") }
    public static var castHint: String { localized("Send something here with Play on…") }

    /// Beside a page's Play: what plays it on another device. Live TV
    /// watches; Shuffle shuffles there.
    public static var playOn: String { localized("Play on…") }
    public static var watchOn: String { localized("Watch on…") }
    public static var shuffleOn: String { localized("Shuffle on…") }

    /// The remote's track and quality pickers, and the subtitles' choice of none.
    public static var remoteAudioTitle: String { localized("Audio") }
    public static var remoteSubtitlesTitle: String { localized("Subtitles") }
    public static var remoteQualityTitle: String { localized("Quality") }
    /// Between a channel's down and up on the remote, as on a TV remote's rocker.
    public static var remoteChannelTitle: String { localized("Channel") }
    public static var remoteSubtitlesOff: String { localized("Off") }

    /// Stop, and its confirmation's buttons.
    public static var remoteStopTitle: String { localized("Stop") }
    public static var remoteCancel: String { localized("Cancel") }

    /// What Stop asks first: "Stop playing on Living room TV?".
    public static func remoteStopQuestion(_ device: String) -> String { localized("Stop playing on \(device)?") }

    /// Under a channel's progress: the programme after the one on.
    public static func remoteNextProgramme(_ title: String) -> String { localized("Next: \(title)") }

    /// What the remote calls each device, by id. Two of the same name (two
    /// of the same TV) are numbered, "Living room TV (2)", in the order of
    /// their ids, which stays as they play and stop, so every label and
    /// message says which one it means.
    public static func deviceNames(_ devices: [(id: String, name: String?)]) -> [String: String] {
        let sorted = devices.sorted { $0.id < $1.id }
        let unnamed = unnamedDevice
        var total: [String: Int] = [:]
        for device in sorted { total[device.name ?? unnamed, default: 0] += 1 }
        var seen: [String: Int] = [:]
        var names: [String: String] = [:]
        for device in sorted {
            let name = device.name ?? unnamed
            seen[name, default: 0] += 1
            names[device.id] = total[name, default: 0] > 1 ? "\(name) (\(seen[name, default: 0]))" : name
        }
        return names
    }

    /// "Remote, 2 devices": what VoiceOver reads for the remote's bar and button.
    public static func remoteLabel(_ count: Int) -> String {
        localized("Remote, \(count) devices")
    }

    /// A channel as the remote names it: its number and name, "101 News".
    public static func remoteChannel(_ channel: PlayingChannel) -> String {
        [channel.number ?? "", channel.name].filter { !$0.isEmpty }.joined(separator: " ")
    }

    /// What a device plays, on its row and in "Play on…": the title in
    /// full, or the channel with its number.
    public static func remotePlaying(_ session: PlayingSession) -> String {
        if session.kind == .live { return session.channel.map(remoteChannel) ?? localized("Live TV") }
        return session.item.map(fullTitle) ?? ""
    }

    /// The heading of a device's controls, on two lines: an episode's show
    /// with its season and number ("Show · S1 · E2"), then its title; a
    /// track's title, then its artist; a channel's number and name, then
    /// what's on; anything else, its title in full.
    public static func remoteHeading(_ session: PlayingSession) -> (title: String, subtitle: String?) {
        if session.kind == .live {
            let now = session.channel?.now.flatMap { $0.isEmpty ? nil : $0 }
            return (session.channel.map(remoteChannel) ?? localized("Live TV"), now)
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
        live ? localized("Previous channel on \(device)") : localized("Previous on \(device)")
    }

    public static func remoteNext(_ device: String, live: Bool) -> String {
        live ? localized("Next channel on \(device)") : localized("Next on \(device)")
    }

    public static func remoteBack(_ device: String) -> String {
        localized("Back \(remoteSkipBackMs / 1000) seconds on \(device)")
    }

    public static func remoteForward(_ device: String) -> String {
        localized("Forward \(remoteSkipForwardMs / 1000) seconds on \(device)")
    }

    /// Skip Intro or Skip Credits on the remote, as VoiceOver reads it: "Skip intro on Living room TV".
    public static func remoteSkip(_ kind: MarkerKind, device: String) -> String {
        kind == .intro ? localized("Skip intro on \(device)") : localized("Skip credits on \(device)")
    }

    public static func remotePlay(_ device: String) -> String { localized("Play on \(device)") }
    public static func remotePause(_ device: String) -> String { localized("Pause on \(device)") }
    public static func remoteStop(_ device: String) -> String { localized("Stop on \(device)") }
    public static func remoteSeek(_ device: String) -> String { localized("Seek on \(device)") }
    public static func remoteProgress(_ device: String) -> String { localized("Progress on \(device)") }
    public static func remoteWaiting(_ device: String) -> String { localized("Waiting for \(device)") }
    public static func remoteAudio(_ device: String) -> String { localized("Audio on \(device)") }
    public static func remoteSubtitles(_ device: String) -> String { localized("Subtitles on \(device)") }
    public static func remoteQuality(_ device: String) -> String { localized("Quality on \(device)") }

    /// After a command the player didn't show it acted on.
    public static func remoteNoAnswer(_ device: String) -> String { localized("\(device) didn't answer.") }

    /// What the remote says when the server refused a command or a stop
    /// with `status`; `nil` for a refusal said in the server's own words.
    public static func remoteRefusal(status: Int, device: String) -> String? {
        switch status {
        case 404: localized("\(device) isn't playing any more.")
        case 409: localized("\(device) can't do that.")
        case 429: localized("Too many at once: try again in a moment.")
        default: nil
        }
    }

    /// After "Play on…": where it went.
    public static func playingOn(_ device: String) -> String { localized("Playing on \(device)") }

    /// As something cast from another device starts here: that device, as
    /// the server names it, else "another device".
    public static func playingFrom(_ device: String?) -> String {
        let device = device.flatMap { $0.isEmpty ? nil : $0 } ?? localized("another device")
        return localized("Playing from \(device)")
    }

    /// What "Play on…" says when the server refused it with `status`;
    /// `nil` for a refusal said in the server's own words.
    public static func castRefusal(status: Int, device: String) -> String? {
        switch status {
        case 404: localized("\(device) isn't on any more")
        case 409: localized("\(device) can't play that")
        case 429: localized("Too many at once: try again in a moment")
        default: nil
        }
    }
}
