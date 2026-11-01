import Foundation
import PleyModel

/// When a session's progress reports go out (docs/API.md, "Reporting"),
/// as the web and Android players send them:
///
/// - `playing` when the first frame shows;
/// - every 10 s while the player wants to play (waiting for data counts as
///   playing);
/// - `paused` at once on a pause, then every 30 s while paused, since the
///   server forgets a player it hasn't heard from for 90 s;
/// - 1 s after a seek, in the state of the moment.
///
/// `stopped` at the end is the session's own (`ServerPlaybackSession.end`).
/// Pure: it is told the time, and says what is due.
public struct ProgressSchedule: Sendable, Equatable {
    public static let playingEveryMs: Int64 = 10_000
    public static let pausedEveryMs: Int64 = 30_000
    public static let afterSeekMs: Int64 = 1_000

    private var shown = false
    private var lastAt: Int64?
    private var seekDue: Int64?
    private var wantsToPlay = true

    public init() {}

    /// The first frame of the session showed: `playing`, once.
    public mutating func firstFrame(at now: Int64) -> ProgressState? {
        guard !shown else { return nil }
        shown = true
        lastAt = now
        return wantsToPlay ? .playing : .paused
    }

    /// The player wants to play (`true`) or was paused. A pause reports at
    /// once; playing again starts the 10 s over.
    public mutating func wantsToPlay(_ wants: Bool, at now: Int64) -> ProgressState? {
        guard wants != wantsToPlay else { return nil }
        wantsToPlay = wants
        guard shown else { return nil }
        if wants {
            lastAt = now
            return nil
        }
        seekDue = nil
        lastAt = now
        return .paused
    }

    /// A seek: a report 1 s later (a later seek moves it).
    public mutating func sought(at now: Int64) {
        seekDue = now + Self.afterSeekMs
    }

    /// What is due at `now`, if anything; it counts as sent.
    public mutating func due(at now: Int64) -> ProgressState? {
        guard shown else { return nil }
        let state: ProgressState = wantsToPlay ? .playing : .paused
        if let seekDue, now >= seekDue {
            self.seekDue = nil
            lastAt = now
            return state
        }
        let every = wantsToPlay ? Self.playingEveryMs : Self.pausedEveryMs
        guard let lastAt, now - lastAt >= every else { return nil }
        self.lastAt = now
        return state
    }

    /// A new session at the same place (a recovery, an expired session): its first frame reports again.
    public mutating func restart() {
        shown = false
        lastAt = nil
        seekDue = nil
    }
}
