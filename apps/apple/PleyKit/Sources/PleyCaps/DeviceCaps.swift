import AVFoundation
import Combine
import Foundation
import PleyCommon
import PleyModel

/// What this device plays, read once and again when something it depends
/// on changes: the screen's HDR (a TV switched, a display connected), the
/// audio route (AirPods, a receiver), or on the Mac the screen the player's
/// window is on. A new snapshot applies to the next playback session.
@MainActor
public final class DeviceCaps: ObservableObject {
    @Published public private(set) var snapshot: CapsSnapshot?

    private let probe: any CapsProbe
    private var screen: ScreenFacts?
    private var stale = true
    /// Counts changes, so a reading that started before one isn't taken
    /// as current.
    private var generation = 0
    private var reading: Task<CapsSnapshot, Never>?
    private let tokens = NotificationTokens()
    private var logged = false

    public init(probe: any CapsProbe = SystemCapsProbe()) {
        self.probe = probe
        tokens.observe(AVPlayer.eligibleForHDRPlaybackDidChangeNotification) { [weak self] in
            self?.changed()
        }
        #if os(iOS) || os(tvOS)
        tokens.observe(AVAudioSession.routeChangeNotification) { [weak self] in
            self?.audioRouteChanged()
        }
        #endif
    }

    /// The current snapshot, read first when there is none or something
    /// changed since.
    @discardableResult
    public func warmUp() async -> CapsSnapshot {
        // A change while reading makes that reading stale: read once more.
        for _ in 0..<2 {
            if let snapshot, !stale { return snapshot }
            _ = await (reading ?? startReading()).value
        }
        if let snapshot { return snapshot }
        return await startReading().value
    }

    /// The Mac's player window moved to another screen: its HDR and size
    /// are read again.
    public func screenChanged(_ screen: ScreenFacts) {
        guard screen != self.screen else { return }
        self.screen = screen
        changed()
    }

    /// iPhone, iPad and Apple TV: the audio route changed.
    public func audioRouteChanged() {
        changed()
    }

    private func changed() {
        generation += 1
        stale = true
        if reading == nil { startReading() }
    }

    @discardableResult
    private func startReading() -> Task<CapsSnapshot, Never> {
        let (probe, screen, generation) = (self.probe, self.screen, self.generation)
        let task = Task { @MainActor [weak self] in
            let input = await probe.read(screen: screen)
            let snapshot = CapsBuilder.build(input)
            self?.finished(snapshot, input: input, generation: generation)
            return snapshot
        }
        reading = task
        return task
    }

    private func finished(_ snapshot: CapsSnapshot, input: ProbeInput, generation: Int) {
        reading = nil
        self.snapshot = snapshot
        if generation == self.generation { stale = false }
        #if DEBUG
        // Once, so a device's class can be recorded (Tests/PleyCapsTests/Probes).
        // The input holds no personal data.
        if !logged, let json = try? PleyJSON.encoder.encode(input) {
            logged = true
            Log.caps.debug("probe \(String(decoding: json, as: UTF8.self), privacy: .public)")
        }
        #endif
    }
}

/// Notification observers, removed when their owner goes.
final class NotificationTokens: @unchecked Sendable {
    private var tokens: [any NSObjectProtocol] = []

    func observe(_ name: Notification.Name, _ action: @escaping @MainActor () -> Void) {
        tokens.append(NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { _ in
            Task { @MainActor in action() }
        })
    }

    deinit {
        tokens.forEach(NotificationCenter.default.removeObserver)
    }
}
