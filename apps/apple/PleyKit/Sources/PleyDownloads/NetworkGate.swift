import Foundation
import Network
import os

/// Whether downloads may use the network now: one there is, and, unless
/// mobile data is allowed (Settings → Downloads), one that isn't
/// expensive (cellular, or a phone's hotspot). Follows the system's path
/// (`NWPathMonitor`) and says when it changes.
public final class NetworkGate: Sendable {
    /// What the path allows: whether there is one, and whether it is expensive.
    public struct Path: Hashable, Sendable {
        public var satisfied: Bool
        public var expensive: Bool

        public init(satisfied: Bool, expensive: Bool) {
            self.satisfied = satisfied
            self.expensive = expensive
        }
    }

    private let path: OSAllocatedUnfairLock<Path>
    private let monitor: NWPathMonitor?
    private let listeners = OSAllocatedUnfairLock<[@Sendable () -> Void]>(initialState: [])

    /// The system's path, followed from now on.
    public static func system() -> NetworkGate {
        NetworkGate(monitor: NWPathMonitor())
    }

    /// A fixed path, for the tests and the command-line check.
    public init(fixed: Path = Path(satisfied: true, expensive: false)) {
        path = OSAllocatedUnfairLock(initialState: fixed)
        monitor = nil
    }

    private init(monitor: NWPathMonitor) {
        path = OSAllocatedUnfairLock(initialState: Path(satisfied: true, expensive: false))
        self.monitor = monitor
        // Called on the monitor's own queue: nothing here is the main actor's.
        monitor.pathUpdateHandler = { @Sendable [weak self] update in
            self?.set(Path(satisfied: update.status == .satisfied, expensive: update.isExpensive))
        }
        monitor.start(queue: DispatchQueue(label: "app.pley.apple.downloads.network"))
    }

    deinit {
        monitor?.cancel()
    }

    public var current: Path { path.withLock { $0 } }

    /// Whether downloads may go now, with mobile data allowed or not.
    public func allows(mobile: Bool) -> Bool {
        let now = current
        return now.satisfied && (mobile || !now.expensive)
    }

    /// `listener` hears every change of the path, on the monitor's queue.
    public func onChange(_ listener: @escaping @Sendable () -> Void) {
        listeners.withLock { $0.append(listener) }
    }

    /// Sets the path (the monitor does; tests too).
    public func set(_ new: Path) {
        let changed = path.withLock { old in
            defer { old = new }
            return old != new
        }
        guard changed else { return }
        for listener in listeners.withLock({ $0 }) { listener() }
    }
}
