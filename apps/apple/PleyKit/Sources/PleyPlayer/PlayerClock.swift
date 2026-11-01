import Foundation

/// The player's sense of time: a monotonic clock in ms for cadences and
/// timeouts, the wall clock for reports kept to send later, and sleeping.
/// Tests give a fake one, so the cadence is checked without waiting.
public protocol PlayerClock: Sendable {
    /// Milliseconds on a clock that never goes back (it may stop while the device sleeps).
    func now() -> Int64
    /// The wall clock, for when a report was made.
    func wall() -> Date
    func sleep(ms: Int64) async throws
}

/// The system's clocks.
public struct SystemPlayerClock: PlayerClock {
    public init() {}

    public func now() -> Int64 { Int64(ProcessInfo.processInfo.systemUptime * 1000) }

    public func wall() -> Date { Date() }

    public func sleep(ms: Int64) async throws { try await Task.sleep(for: .milliseconds(ms)) }
}
