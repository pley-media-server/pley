import Foundation
import os
import PleyModel

/// Times a speed test's body as it arrives: from its first byte, until
/// `readMs` after it or all `expected` bytes came (a port of the Android
/// app's `SpeedReader`). Fed from the network's own thread.
package final class SpeedReader: Sendable {
    private struct State {
        var startedAt: UInt64?
        var lastAt: UInt64 = 0
        var total = 0
    }

    private let expected: Int
    private let readMs: Int
    private let nanos: @Sendable () -> UInt64
    private let state = OSAllocatedUnfairLock(initialState: State())

    package init(expected: Int, readMs: Int,
                 nanos: @escaping @Sendable () -> UInt64 = { DispatchTime.now().uptimeNanoseconds }) {
        self.expected = expected
        self.readMs = readMs
        self.nanos = nanos
    }

    /// `count` bytes came now; false once it has read enough.
    package func read(_ count: Int) -> Bool {
        let now = nanos()
        return state.withLock { s in
            if s.startedAt == nil { s.startedAt = now }
            s.lastAt = now
            s.total += count
            return s.total < expected && now - s.startedAt! < UInt64(readMs) * 1_000_000
        }
    }

    /// What was measured so far; `nil` before the first byte.
    package var sample: SpeedSample? {
        state.withLock { s in
            guard let startedAt = s.startedAt else { return nil }
            // At least a millisecond: a body that came at once is fast, not infinitely so.
            let ms = max(Int((s.lastAt - startedAt) / 1_000_000), 1)
            return SpeedSample(bytes: s.total, ms: ms, complete: s.total >= expected && ms < readMs)
        }
    }
}
