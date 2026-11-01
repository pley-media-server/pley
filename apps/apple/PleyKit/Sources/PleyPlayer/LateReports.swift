import CryptoKit
import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// `paused` and `stopped` reports that couldn't reach the server, kept to
/// send once it answers again, each with its `age_ms` (docs/API.md,
/// "Reporting"): the server then keeps whatever happened to the item since.
///
/// One file per server and profile in `Application Support/Pley`, named by
/// a hash of the two: `late-reports-<sha256>.json`. It holds the item, the
/// position, the duration, the state and when the report was made, never a
/// token or a session. The newest report per item is kept, for at most 100
/// items and 90 days.
public actor LateReports {
    /// One report kept.
    public struct Entry: Codable, Hashable, Sendable {
        public var itemId: Int
        public var positionMs: Int
        public var durationMs: Int?
        public var state: ProgressState
        /// When it was made, unix ms.
        public var madeAt: Int64
    }

    public static let maxItems = 100
    public static let maxAgeMs: Int64 = 90 * 24 * 3600 * 1000

    private let file: URL
    private let clock: any PlayerClock
    private var entries: [Entry]?

    /// The reports of `origin` and `profileId`, in `directory` (the app's
    /// `Application Support/Pley` by default).
    public init(origin: String, profileId: Int?, directory: URL? = nil, clock: any PlayerClock = SystemPlayerClock()) {
        let base = directory ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Pley", isDirectory: true)
        let key = SHA256.hash(data: Data("\(origin)\n\(profileId.map(String.init) ?? "")".utf8))
            .map { String(format: "%02x", $0) }.joined()
        file = base.appendingPathComponent("late-reports-\(key).json")
        self.clock = clock
    }

    /// The reports kept, oldest first.
    public func pending() -> [Entry] { load() }

    /// Keeps `report` of `itemId` to send later, made now.
    public func keep(itemId: Int, _ report: ProgressReport) {
        var all = load().filter { $0.itemId != itemId }
        all.append(Entry(itemId: itemId, positionMs: report.positionMs, durationMs: report.durationMs,
                         state: report.state, madeAt: Int64(clock.wall().timeIntervalSince1970 * 1000)))
        if all.count > Self.maxItems { all.removeFirst(all.count - Self.maxItems) }
        save(all)
    }

    /// Sends what is kept, oldest first, without a session and with its
    /// age. A report the server answered (2xx, or a 4xx refusal) is done;
    /// one that can't reach it stays, with the rest after it.
    public func send(using api: PleyAPI) async {
        var left = load()
        guard !left.isEmpty else { return }
        while let entry = left.first {
            let age = max(Int64(clock.wall().timeIntervalSince1970 * 1000) - entry.madeAt, 0)
            let report = ProgressReport(positionMs: entry.positionMs, durationMs: entry.durationMs, state: entry.state,
                                        ageMs: Int(age))
            do {
                try await api.reportProgress(itemId: entry.itemId, report)
            } catch let error where !Self.answered(error) {
                break
            } catch {}
            left.removeFirst()
        }
        save(left)
        Log.player.info("late reports sent; \(left.count, privacy: .public) left")
    }

    /// Whether `error` means the server wasn't reached (as opposed to an answer).
    public static func unreachable(_ error: any Error) -> Bool {
        switch error as? PleyError {
        case .offline, .timedOut, .unreachable, .localNetworkBlocked: true
        default: false
        }
    }

    /// Whether the server answered with 2xx or a 4xx refusal: the report is done with.
    static func answered(_ error: any Error) -> Bool {
        switch error as? PleyError {
        case .api(let status, _, _), .unreadable(let status): (400..<500).contains(status)
        case .sessionStopped, .insufficientScope: true
        default: false
        }
    }

    private func load() -> [Entry] {
        if let entries { return entries }
        let now = Int64(clock.wall().timeIntervalSince1970 * 1000)
        let read = (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode([Entry].self, from: $0) } ?? []
        let fresh = read.filter { now - $0.madeAt <= Self.maxAgeMs }.sorted { $0.madeAt < $1.madeAt }
        entries = fresh
        return fresh
    }

    private func save(_ all: [Entry]) {
        entries = all
        do {
            if all.isEmpty {
                try? FileManager.default.removeItem(at: file)
                return
            }
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try JSONEncoder().encode(all).write(to: file, options: [.atomic])
        } catch {
            Log.player.error("late reports couldn't be kept")
        }
    }
}
