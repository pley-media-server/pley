import Foundation
import Network
import os

/// The network's ceiling Automatic found last, per device and server, so
/// the next title starts at what the network carried (docs/QUALITY.md,
/// "Memory": the device's own, for when the server can't say).
public protocol CeilingMemory: AnyObject, Sendable {
    /// The ceiling in kbps and when it was found (on the player's clock); `nil` for none.
    func read() -> (kbps: Int, atMs: Int64)?
    func write(kbps: Int, atMs: Int64)
    func forget()
}

/// A ceiling kept in memory only (tests, and players nothing should outlive).
public final class MemoryCeiling: CeilingMemory, @unchecked Sendable {
    private let held = OSAllocatedUnfairLock<(kbps: Int, atMs: Int64)?>(initialState: nil)

    public init() {}

    public func read() -> (kbps: Int, atMs: Int64)? { held.withLock { $0 } }
    public func write(kbps: Int, atMs: Int64) { held.withLock { $0 = (kbps, atMs) } }
    public func forget() { held.withLock { $0 = nil } }
}

/// The ceiling of one server (its origin), in `UserDefaults`. Its time is
/// on `SystemPlayerClock`'s clock, the time since the device started: a
/// ceiling from before a restart reads as from the future, and Automatic
/// forgets it, as the Android app's elapsedRealtime does.
public final class DefaultsCeiling: CeilingMemory, @unchecked Sendable {
    // Unchecked Sendable: UserDefaults is thread-safe.
    private let defaults: UserDefaults
    private let key: String

    public init(origin: String, defaults: UserDefaults = .standard) {
        self.defaults = defaults
        key = "pley.ceiling." + origin
    }

    public func read() -> (kbps: Int, atMs: Int64)? {
        guard let held = defaults.dictionary(forKey: key), let kbps = held["kbps"] as? Int,
              let at = (held["at"] as? NSNumber)?.int64Value
        else { return nil }
        return (kbps, at)
    }

    public func write(kbps: Int, atMs: Int64) {
        defaults.set(["kbps": kbps, "at": NSNumber(value: atMs)], forKey: key)
    }

    public func forget() { defaults.removeObject(forKey: key) }
}

/// The quality picked in the menu, remembered per kind of network:
/// cellular, and everything else (the Android app's `qualityOnMobile` and
/// `qualityOnHomeNetwork`). Nothing remembered means Automatic.
public protocol QualityMemory: AnyObject, Sendable {
    func remembered() -> String?
    func remember(_ id: String)
}

/// A quality choice kept in memory only (tests).
public final class MemoryQuality: QualityMemory, @unchecked Sendable {
    private let held: OSAllocatedUnfairLock<String?>

    public init(_ id: String? = nil) {
        held = OSAllocatedUnfairLock(initialState: id)
    }

    public func remembered() -> String? { held.withLock { $0 } }
    public func remember(_ id: String) { held.withLock { $0 = id } }
}

/// The quality choice in `UserDefaults`, one per kind of network.
public final class DefaultsQuality: QualityMemory, @unchecked Sendable {
    // Unchecked Sendable: UserDefaults is thread-safe.
    private let defaults: UserDefaults
    private let cellular: @Sendable () -> Bool

    public init(defaults: UserDefaults = .standard, cellular: @escaping @Sendable () -> Bool = { NetworkKind.shared.cellular }) {
        self.defaults = defaults
        self.cellular = cellular
    }

    private var key: String { cellular() ? "pley.quality.cellular" : "pley.quality.other" }

    public func remembered() -> String? { defaults.string(forKey: key) }
    public func remember(_ id: String) { defaults.set(id, forKey: key) }
}

/// Whether the device is on a cellular network now, from `NWPathMonitor`.
public final class NetworkKind: Sendable {
    public static let shared = NetworkKind()

    private let monitor = NWPathMonitor()
    private let isCellular = OSAllocatedUnfairLock(initialState: false)

    private init() {
        let isCellular = isCellular
        monitor.pathUpdateHandler = { path in
            let cellular = path.usesInterfaceType(.cellular) && !path.usesInterfaceType(.wifi)
                && !path.usesInterfaceType(.wiredEthernet)
            isCellular.withLock { $0 = cellular }
        }
        monitor.start(queue: DispatchQueue(label: "app.pley.apple.network-kind"))
    }

    public var cellular: Bool { isCellular.withLock { $0 } }
}
