import Foundation
import PleyModel

/// Why a fallback plays something other than the original.
public enum FallbackReason: Sendable, Equatable {
    /// The device couldn't play the original file.
    case unplayable
    /// The original didn't play smoothly here.
    case notSmooth
}

/// Why playback gave up.
public enum Failure: Sendable, Equatable {
    /// Neither the original nor a conversion plays here.
    case unplayable
    /// It stops at the same spot each time.
    case stuck
    /// It stopped and couldn't get going again.
    case stopped
    /// The server couldn't be reached.
    case network
    /// The engine failed in a way no new session helps with.
    case engine
}

/// The web and Android players' recovery ladder (Android's `Recovery`),
/// with AVPlayer's own case:
///
/// - Direct play that fails to open or decode: a remux when the video is
///   H.264 or HEVC and only the container or the audio failed, else a
///   transcode; each once.
/// - Playlists or segments answering 404 or 410, or an unreadable
///   playlist: a new session at the position, at most 3 times.
/// - Network errors after the engine's own retries: a new session, at most 3.
/// - 12 s standing still while meant to play: a direct play with data
///   becomes a transcode; a stream asks the server first (`checkServer`).
/// - A remux that shows no picture for 5 s while its time runs on (HEVC
///   copied into MPEG-TS by a transcoder without fMP4, which AVPlayer plays
///   without picture): a transcode, once.
///
/// Budgets start over after 5 s of playback and on Retry. Not thread-safe:
/// the controller calls it on the main actor.
public struct Recovery: Sendable {
    public enum Action: Sendable, Equatable {
        /// A new session at the position; `mode` is asked for from now on when set.
        case newSession(mode: PlaybackMode?, reason: FallbackReason?)
        /// Ask the server how its conversion is doing, then call `afterServerCheck`.
        case checkServer
        /// The server can't keep up: a lower quality would help.
        case suggestLowerQuality
        case fail(Failure, detail: String?)
    }

    public static let recoveredAfterMs = 5_000
    public static let maxRecreate = 3
    public static let maxNetwork = 3
    public static let maxStuck = 1
    /// Videos a remux copies, which AVPlayer decodes in another container.
    static let copyableVideo: Set<String> = ["h264", "hevc"]

    /// The mode asked for instead of the original's, after a fallback.
    public private(set) var fallbackMode: PlaybackMode?
    public private(set) var fallbackReason: FallbackReason?
    /// The switch for dropped frames happened (once per item).
    public private(set) var struggleSwitched = false
    public private(set) var recreate = 0
    public private(set) var network = 0
    public private(set) var stuck = 0
    private var remuxTried = false
    private var transcodeTried = false
    private var playedFrom: Int?

    public init() {}

    /// Where playback is now: 5 s played on from where it last (re)started means the trouble is over.
    public mutating func onPosition(_ positionMs: Int) {
        guard let from = playedFrom, positionMs >= from else {
            playedFrom = positionMs
            return
        }
        if positionMs - from >= Self.recoveredAfterMs {
            recreate = 0
            network = 0
            stuck = 0
            playedFrom = positionMs
        }
    }

    /// A seek or a new session: progress counts from where it lands.
    public mutating func onJump() {
        playedFrom = nil
    }

    /// What to do about `error` in a session played as `method`, whose video is `videoCodec`.
    public mutating func decide(_ error: EngineError, method: PlaybackMethod, videoCodec: String?) -> Action {
        switch error {
        case .unsupported(let detail, let audio, let container):
            return fallback(method: method, videoCodec: videoCodec, partial: audio || container, detail: detail)
        case .http(let status, let detail):
            return status == 404 || status == 410 ? expired(detail) : networkError(detail)
        case .playlist(let detail):
            return expired(detail)
        case .network(let detail):
            return networkError(detail)
        case .other(let detail):
            return .fail(.engine, detail: detail)
        }
    }

    /// Stood still for 12 s while meant to play; `starving` when it waits for data.
    public mutating func onStall(method: PlaybackMethod, starving: Bool) -> Action {
        if !starving && method == .direct && !transcodeTried {
            transcodeTried = true
            return switchTo(.transcode, .notSmooth)
        }
        if method != .direct { return .checkServer }
        return afterStall(starving: starving)
    }

    /// The server's conversion runs at `speed` (`nil` when it can't say).
    public mutating func afterServerCheck(speed: Double?, starving: Bool) -> Action {
        if let speed, speed < 1 { return .suggestLowerQuality }
        return afterStall(starving: starving)
    }

    /// A remux shows no picture while its time runs on: a transcode, once.
    public mutating func onNoVideo(method: PlaybackMethod) -> Action? {
        guard method == .remux, !transcodeTried else { return nil }
        transcodeTried = true
        return switchTo(.transcode, .unplayable)
    }

    /// The decoder drops frames: one switch to a transcode per item at the
    /// server's own quality; `nil` when it switched already or transcodes.
    public mutating func onStruggling(method: PlaybackMethod) -> Action? {
        guard !struggleSwitched, method != .transcode else { return nil }
        struggleSwitched = true
        transcodeTried = true
        return switchTo(.transcode, .notSmooth)
    }

    /// Another quality chosen: its own mode again, as on the web (dropped frames don't switch twice).
    public mutating func qualityChanged() {
        fallbackMode = nil
        fallbackReason = nil
    }

    /// Retry after an error: every new-session budget again.
    public mutating func retry() {
        recreate = 0
        network = 0
        stuck = 0
        playedFrom = nil
    }

    /// Another item: everything starts over.
    public mutating func newItem() {
        retry()
        fallbackMode = nil
        fallbackReason = nil
        struggleSwitched = false
        remuxTried = false
        transcodeTried = false
    }

    private mutating func fallback(method: PlaybackMethod, videoCodec: String?, partial: Bool, detail: String) -> Action {
        let copyable = videoCodec.map { Self.copyableVideo.contains($0.lowercased()) } ?? false
        if method == .direct && partial && copyable && !remuxTried {
            remuxTried = true
            return switchTo(.remux, .unplayable)
        }
        if method != .transcode && !transcodeTried {
            transcodeTried = true
            return switchTo(.transcode, .unplayable)
        }
        return .fail(.unplayable, detail: detail)
    }

    private mutating func switchTo(_ mode: PlaybackMode, _ reason: FallbackReason) -> Action {
        fallbackMode = mode
        fallbackReason = reason
        return .newSession(mode: mode, reason: reason)
    }

    private mutating func expired(_ detail: String) -> Action {
        guard recreate < Self.maxRecreate else { return .fail(.stopped, detail: detail) }
        recreate += 1
        return .newSession(mode: nil, reason: nil)
    }

    private mutating func networkError(_ detail: String) -> Action {
        guard network < Self.maxNetwork else { return .fail(.network, detail: detail) }
        network += 1
        return .newSession(mode: nil, reason: nil)
    }

    private mutating func afterStall(starving: Bool) -> Action {
        if !starving {
            guard stuck < Self.maxStuck else { return .fail(.stuck, detail: nil) }
            stuck += 1
            return .newSession(mode: nil, reason: nil)
        }
        guard recreate < Self.maxRecreate else { return .fail(.stopped, detail: nil) }
        recreate += 1
        return .newSession(mode: nil, reason: nil)
    }
}
