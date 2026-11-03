import Foundation

/// The player's sense of time: a monotonic clock in ms for cadences and
/// timeouts, the wall clock for reports kept to send later, and sleeping.
/// Tests give a fake one, so the cadence is checked without waiting.
public protocol PlayerClock: Sendable {
    /// Milliseconds on a clock that never goes back.
    func now() -> Int64
    /// The wall clock, for when a report was made.
    func wall() -> Date
    func sleep(ms: Int64) async throws
}

/// The system's clocks. `now()` is the time since the device started,
/// counting the time it slept (`CLOCK_MONOTONIC`, as Android's
/// `elapsedRealtime`): a phone locked for 20 minutes has been away 20
/// minutes. `systemUptime` stops while the device sleeps.
public struct SystemPlayerClock: PlayerClock {
    public init() {}

    public func now() -> Int64 { Int64(clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1_000_000) }

    public func wall() -> Date { Date() }

    public func sleep(ms: Int64) async throws { try await Task.sleep(for: .milliseconds(ms)) }
}
