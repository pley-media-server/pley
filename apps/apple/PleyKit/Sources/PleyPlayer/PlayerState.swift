import Foundation
import PleyModel
import PleySubtitles

/// What the player screens show, from `PlaybackController`.
public struct PlayerState: Equatable, Sendable {
    public enum Phase: Equatable, Sendable {
        /// Asking the server, or opening the media.
        case preparing
        /// Meant to play, waiting for data.
        case buffering
        case playing
        case paused
        /// The media played to its end.
        case ended
        /// It stopped: `message` says why, as a sentence; `canRetry` offers Try again.
        case failed(message: String, canRetry: Bool)
    }

    public var phase: Phase = .preparing
    public var positionMs = 0
    public var durationMs = 0
    public var bufferedMs = 0
    /// The movie ("Arrival (2016)") or the show.
    public var title = ""
    /// An episode's "S1 E2 · Title".
    public var subtitle: String?
    /// How the session plays; `nil` before the server answered.
    public var method: PlaybackMethod?
    /// The audio is converted while the video plays as it is or repackaged.
    public var audioConverted = false
    /// Why it isn't played as it is (the server's reasons).
    public var reasons: [String] = []
    /// What the screen gets of HDR video ("HDR10", "SDR (tone-mapped)").
    public var videoRange: String?
    /// A short note for a few seconds ("The server can't convert this fast enough").
    public var notice: String?
    /// The server stopped the session (an administrator, or the profile elsewhere).
    public var stoppedByServer = false
    /// The Quality menu, the one chosen selected: Original, Automatic, then
    /// the file's qualities (docs/QUALITY.md, "The menu").
    public var qualities: [MenuChoice] = []
    /// What plays, by the menu's words: "Automatic · 8 Mbps 1080p", "Original".
    public var quality = "Automatic"
    /// Why it plays lower than the original when the player chose it
    /// ("Lowered for your connection"); `nil` otherwise.
    public var qualityNote: String?
    /// What the server's conversion makes of the video ("H.264 1280×720"), once it runs.
    public var videoOut: String?
    /// The item playing; it changes when the next episode plays.
    public var itemId: Int?
    /// "Skip Intro" or "Skip Credits" while the playhead is in one; `skip()` skips it.
    public var skipLabel: String?
    /// What plays next, while its card shows.
    public var upNext: UpNextCard?
    /// The file's chapters, the one playing marked.
    public var chapters: [ChapterChoice] = []
    /// The Audio menu: the file's audio tracks, the one playing selected; ids are stream indexes.
    public var audio: [MenuChoice] = []
    /// The Subtitles menu: Off, then the file's subtitle tracks, the one
    /// shown selected; ids are stream indexes (`MenuChoice.off` for Off).
    /// Empty when the file has none.
    public var subtitles: [MenuChoice] = []
    /// The subtitles' offset, in ms (positive: later); see `SubtitleSync`.
    public var subtitleOffsetMs = 0
    /// The subtitles shown can be moved in time: those this player draws, not burned in.
    public var canSyncSubtitles = false
    /// The picture subtitles are burned into the video by the server's conversion.
    public var subtitlesBurnedIn = false
    /// What the subtitle layer draws now.
    public var cues: ShownSubtitles = .none

    public init() {}

    /// "Off", or the subtitles shown ("English (SRT)").
    public var subtitleLabel: String { subtitles.first(where: \.selected)?.label ?? PlayerTexts.subtitlesOff }

    /// "+0.5 s", "−1.2 s", "0.0 s".
    public var subtitleOffsetLabel: String { SubtitleSync.label(subtitleOffsetMs) }

    /// "Direct play", "Direct stream", "Transcoding", "Direct stream · Audio
    /// transcoding", as the other apps say it; `nil` before the server answered.
    public var methodLabel: String? {
        guard let method else { return nil }
        let base = switch method {
        case .direct: "Direct play"
        case .remux: "Direct stream"
        case .transcode: "Transcoding"
        default: method.rawValue.capitalized
        }
        return audioConverted && method != .transcode ? "\(base) · Audio transcoding" : base
    }

    /// Whether the player is past preparing and hasn't failed.
    public var isFailed: Bool {
        if case .failed = phase { return true }
        return false
    }
}

/// The card for what plays next: its words, its picture and its countdown.
public struct UpNextCard: Equatable, Sendable {
    public var itemId: Int
    public var title: String
    /// "Up next · S1 E2".
    public var heading: String
    /// The server's picture of it (a still, else the backdrop), for `NextUp.imageWidth` pixels.
    public var imagePath: String?
    /// Seconds before it plays by itself; `nil` while no countdown runs.
    public var secondsLeft: Int?

    public init(itemId: Int, title: String, heading: String, imagePath: String? = nil, secondsLeft: Int? = nil) {
        self.itemId = itemId
        self.title = title
        self.heading = heading
        self.imagePath = imagePath
        self.secondsLeft = secondsLeft
    }

    /// "Play Next (8)" while counting down, else "Play Next".
    public var playLabel: String { secondsLeft.map { "Play Next (\($0))" } ?? "Play Next" }
}

/// A chapter in the player's Chapters menu.
public struct ChapterChoice: Equatable, Hashable, Sendable, Identifiable {
    /// Its place in the file's chapters, from 0.
    public var index: Int
    /// The chapter's title, else "Chapter 3".
    public var title: String
    public var startMs: Int
    /// The playhead is in it.
    public var current: Bool

    public var id: Int { index }

    public init(index: Int, title: String, startMs: Int, current: Bool = false) {
        self.index = index
        self.title = title
        self.startMs = startMs
        self.current = current
    }

    /// The menu's line: "Intro · 0:10".
    public var label: String { "\(title) · \(Words.clock(ms: startMs))" }
}

/// A choice in one of the player's menus.
public struct MenuChoice: Equatable, Hashable, Sendable, Identifiable {
    /// The Quality menu's Original.
    public static let original = "original"
    /// The Subtitles menu's Off.
    public static let off = "off"

    public var id: String
    public var label: String
    /// A second line ("The file as it is").
    public var detail: String?
    public var selected: Bool

    public init(id: String, label: String, detail: String? = nil, selected: Bool = false) {
        self.id = id
        self.label = label
        self.detail = detail
        self.selected = selected
    }
}

/// The player's words for people.
enum PlayerTexts {
    static let stoppedByServer = "Playback was stopped by the server's administrator."
    static let nothingToPlay = "This item has no file to play."
    static let slowServer = "The server can't convert this video fast enough; a lower quality would play smoothly."
    static let automaticHint = "Dynamically adjust to conditions"
    static let lastEpisode = "This is the last episode"
    static let nothingNext = "Nothing plays next"
    static let subtitlesOff = "Off"
    static let subtitlesFailed = "The subtitles couldn't be loaded."
    static let burnedIn = "Burned into the video"

    /// The menu's Automatic while it plays `playing`: "Automatic · 8 Mbps 1080p".
    static func automatic(_ playing: String) -> String { "Automatic · \(playing)" }

    /// Automatic's second line while it plays below the original: "Now 8 Mbps 1080p".
    static func now(_ playing: String) -> String { "Now \(playing)" }

    /// Said once when Automatic goes down to `label`.
    static func lowered(to label: String) -> String { "Quality lowered to \(label)" }

    /// Why Automatic plays lower.
    static func lowered(_ reason: LoweredFor) -> String {
        switch reason {
        case .network: "Lowered for your connection"
        case .device: "Lowered for this device"
        }
    }

    /// Why a fallback plays something else than the original.
    static func fallback(_ reason: FallbackReason) -> String {
        switch reason {
        case .unplayable: "This device couldn't play the original file."
        case .notSmooth: "The original didn't play smoothly on this device."
        }
    }

    static func stopped(_ message: String?) -> String {
        guard let message, !message.isEmpty else { return stoppedByServer }
        return "\(stoppedByServer) \(message)"
    }

    static func failure(_ failure: Failure, detail: String?) -> String {
        switch failure {
        case .unplayable: "This video can't be played on this device."
        case .stuck: "The video stops at the same place each time."
        case .stopped: "Playback stopped and couldn't start again."
        case .network: "The server couldn't be reached."
        case .engine: "The player stopped unexpectedly."
        }
    }

    /// An episode's "S1 E2".
    static func episodeLabel(season: Int?, episode: Int?) -> String {
        [season.map { "S\($0)" }, episode.map { "E\($0)" }].compactMap { $0 }.joined(separator: " ")
    }
}
