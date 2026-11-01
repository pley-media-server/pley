import Foundation
import PleyModel

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
    /// Why it isn't played as it is (the server's reasons).
    public var reasons: [String] = []
    /// What the screen gets of HDR video ("HDR10", "SDR (tone-mapped)").
    public var videoRange: String?
    /// A short note for a few seconds ("The server can't convert this fast enough").
    public var notice: String?
    /// The server stopped the session (an administrator, or the profile elsewhere).
    public var stoppedByServer = false

    public init() {}
}

/// The player's words for people.
enum PlayerTexts {
    static let stoppedByServer = "Playback was stopped by the server's administrator."
    static let nothingToPlay = "This item has no file to play."
    static let slowServer = "The server can't convert this video fast enough; a lower quality would play smoothly."

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
