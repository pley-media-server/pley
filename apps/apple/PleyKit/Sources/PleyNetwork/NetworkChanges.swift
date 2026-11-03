import Foundation
import Network
import os
import PleyCommon

/// Counts the times the device's network may have changed: a server's
/// proof counts on the network it was made on (docs/API.md, "Telling
/// servers apart"), so each change has every server prove itself again
/// before its next token. Downloads hear of each change too, to hold their
/// pieces back until then.
public final class NetworkChanges: Sendable {
    /// The app's, following the system's path once `follow()` is called.
    public static let shared = NetworkChanges()

    private struct State {
        var count = 0
        var lastPath: String?
        var listeners: [@Sendable () -> Void] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())
    /// Made at once and started by `follow()`, once.
    private let monitor = NWPathMonitor()
    private let started = OSAllocatedUnfairLock(initialState: false)

    public init() {}

    deinit {
        monitor.cancel()
    }

    /// Changes counted so far.
    public var current: Int { state.withLock { $0.count } }

    /// The device may be on another network now.
    public func changed() {
        let listeners = state.withLock { s -> [@Sendable () -> Void] in
            s.count += 1
            return s.listeners
        }
        for listener in listeners { listener() }
    }

    /// `listener` hears each change, on whichever queue saw it.
    public func onChange(_ listener: @escaping @Sendable () -> Void) {
        state.withLock { $0.listeners.append(listener) }
    }

    /// Follows the system's path (`NWPathMonitor`): any interface, gateway
    /// or status that differs from the last path seen counts as a change,
    /// so moving between two Wi-Fi networks does, as joining or leaving
    /// one does. Once; later calls do nothing.
    public func follow() {
        let first = started.withLock { started -> Bool in
            defer { started = true }
            return !started
        }
        guard first else { return }
        // Called on the monitor's own queue: nothing here is the main actor's.
        monitor.pathUpdateHandler = { @Sendable [weak self] path in
            self?.saw(Self.summary(of: path))
        }
        monitor.start(queue: DispatchQueue(label: "app.pley.apple.network-changes"))
        Log.network.debug("following the network for proofs")
    }

    /// A path seen: a change unless it is the first, or the same as the last.
    func saw(_ summary: String) {
        let changed = state.withLock { s -> Bool in
            defer { s.lastPath = summary }
            return s.lastPath != nil && s.lastPath != summary
        }
        if changed { self.changed() }
    }

    /// What tells one network from another in `path`: its status, its
    /// interfaces and its gateways.
    private static func summary(of path: NWPath) -> String {
        let interfaces = path.availableInterfaces.map { "\($0.name):\($0.type)" }.sorted()
        let gateways = path.gateways.map { "\($0)" }.sorted()
        return "\(path.status) \(interfaces.joined(separator: ",")) \(gateways.joined(separator: ","))"
    }
}
