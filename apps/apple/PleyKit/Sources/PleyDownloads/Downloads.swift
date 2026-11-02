import Combine
import Foundation
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork

/// The app's downloads (iPhone, iPad and Mac), to watch without the
/// server: asked for here, fetched by `DownloadRunner` through a
/// background `URLSession`, kept per server and profile in the app's own
/// Application Support, and played by the player through `local`.
///
/// The background session is made again at every launch (`live()`), so
/// the pieces the system finished while the app was away are handed back.
@MainActor
public final class Downloads: DownloadsService {
    public let available = true
    public private(set) var entries: [DownloadEntry] = []
    public private(set) var storage = DownloadStorage()
    public var changed: AnyPublisher<Void, Never> { subject.eraseToAnyPublisher() }

    let store: DownloadStore
    private let runner: DownloadRunner
    private let settings: AppSettings
    private let clock: () -> Date
    private let subject = PassthroughSubject<Void, Never>()
    private var scope: DownloadScope?
    private var localMedia: ScopedLocalMedia?
    private var storageTask: Task<Void, Never>?
    private var storageAt = Date.distantPast

    /// The downloads in `root`, through `transfer` and `gate`. Tests make
    /// their own; the app's are `live()`.
    init(root: URL, transfer: any TransferSession, gate: NetworkGate, settings: AppSettings,
         timing: DownloadRunner.Timing = DownloadRunner.Timing(), clock: @escaping () -> Date = { Date() },
         freeSpace: @escaping @Sendable (URL) -> Int64 = DownloadStore.systemFreeSpace) {
        store = DownloadStore(root: root, freeSpace: freeSpace)
        runner = DownloadRunner(store: store, transfer: transfer, gate: gate, settings: settings, timing: timing,
                                clock: clock)
        self.settings = settings
        self.clock = clock
        store.onChange = { [weak self] in self?.storeChanged() }
        Task { [weak self] in await self?.runner.settle() }
        storeChanged()
    }

    /// The app's: in `Application Support/Pley/Downloads`, through the
    /// background session named after the app's bundle.
    public static func live(settings: AppSettings, gate: ConnectionGate) -> Downloads {
        let root = DownloadStore.defaultRoot
        let bundle = Bundle.main.bundleIdentifier ?? "app.pley.apple"
        let transfer = PieceTransfer.background(bundle: bundle, folder: root.appendingPathComponent("pieces"),
                                                gate: gate)
        return Downloads(root: root, transfer: transfer, gate: .system(), settings: settings)
    }

    /// A download made with a foreground session (the command-line check,
    /// which has no bundle for a background one), in `root`.
    public static func foreground(root: URL, settings: AppSettings, gate: ConnectionGate,
                                  pieceBytes: Int64 = 256 << 20) -> (Downloads, PieceTransfer) {
        let transfer = PieceTransfer(configuration: .ephemeral, folder: root.appendingPathComponent("pieces"),
                                     gate: gate)
        var timing = DownloadRunner.Timing()
        timing.pieceBytes = pieceBytes
        return (Downloads(root: root, transfer: transfer, gate: NetworkGate(), settings: settings, timing: timing),
                transfer)
    }

    // MARK: DownloadsService

    public func signedIn(_ scope: DownloadScope?) {
        self.scope = scope
        localMedia = scope.map { ScopedLocalMedia(downloads: self, origin: $0.origin, profileId: $0.profileId) }
        runner.scope = scope
        storeChanged()
    }

    public var local: (any LocalMedia)? { localMedia }

    public func download(_ itemIds: [Int]) async throws {
        guard let scope else { return }
        let wanted = itemIds.reduce(into: [Int]()) { if !$0.contains($1) { $0.append($1) } }
            .filter { store.get(key(scope, $0)) == nil }
        guard !wanted.isEmpty else { return }
        let caps = await scope.caps()
        var added: [(DownloadRecord, ItemDetail)] = []
        for id in wanted {
            let detail = try await scope.api.item(id)
            guard let record = DownloadPlanner.record(detail, caps: caps, origin: scope.origin,
                                                      profileId: scope.profileId, quality: settings.downloadQuality,
                                                      now: clock()) else { continue }
            added.append((record, detail))
        }
        if added.isEmpty, wanted.count == 1 { throw Failure(DownloadPlanner.nothingToDownload) }
        store.add(added.map(\.0))
        runner.pump()
        // Their pictures now, not each in its turn: a season's list shows its stills at once.
        let api = scope.api
        for (record, detail) in added {
            Task { [weak self] in
                guard let self, await self.runner.savePicture(record, detail: detail, api: api) else { return }
                self.store.update(record.key) { $0.image = true }
            }
        }
    }

    public func remove(_ itemId: Int) {
        guard let scope, let r = store.get(key(scope, itemId)) else { return }
        runner.cancel(r.key)
        store.remove(r.key)
        forget(r, api: scope.api)
        runner.pump()
    }

    public func retry(_ itemId: Int) {
        guard let scope else { return }
        store.update(key(scope, itemId)) {
            $0.state = .queued
            $0.error = nil
            // A failed conversion is asked for anew: the server makes a failed one again.
            if $0.mediaPath == nil { $0.conversion = nil }
            $0.converting = nil
            $0.convertingSince = nil
            $0.notBefore = .distantPast
        }
        runner.pump()
    }

    public func removeAll() {
        for r in store.all { runner.cancel(r.key) }
        let gone = store.all
        store.clear()
        if let scope { for r in gone where r.belongs(to: (scope.origin, scope.profileId)) { forget(r, api: scope.api) } }
    }

    public func forgetServer(_ origin: String) {
        for r in store.all where r.origin == origin { runner.cancel(r.key) }
        store.removeServer(origin)
        runner.pump()
    }

    public func resume() {
        for r in store.all where r.state == .waiting { store.update(r.key, persist: false) { $0.notBefore = .distantPast } }
        runner.pump()
        guard let scope else { return }
        let api = scope.api
        let short = store.all.filter {
            $0.state == .done && !$0.partialSubtitles.isEmpty && $0.belongs(to: (scope.origin, scope.profileId))
        }
        for r in short { Task { await runner.finishSubtitles(r.key, api: api) } }
    }

    /// iPhone and iPad: the system woke the app for the background
    /// session's events; `completion` is called once they are handled.
    public func handleBackgroundEvents(_ completion: @escaping () -> Void) {
        runner.backgroundEvents(completion)
    }

    // MARK: Inside

    /// Tells the server that `r`, deleted, no longer wants the file it was
    /// making, which then stops. A file made stays for whoever asks next.
    private func forget(_ r: DownloadRecord, api: PleyAPI) {
        guard let id = r.conversion, r.state != .done, r.mediaPath == nil else { return }
        Task { try? await api.cancelConversion(id) }
    }

    private func key(_ scope: DownloadScope, _ itemId: Int) -> String {
        DownloadStore.key(origin: scope.origin, profileId: scope.profileId, itemId: itemId)
    }

    func record(_ itemId: Int, origin: String, profileId: Int) -> DownloadRecord? {
        store.get(DownloadStore.key(origin: origin, profileId: profileId, itemId: itemId))
    }

    func records(origin: String, profileId: Int) -> [DownloadRecord] {
        store.all.filter { $0.belongs(to: (origin, profileId)) }
    }

    var downloadStore: DownloadStore { store }

    private func storeChanged() {
        let mine = scope.map { s in store.all.filter { $0.belongs(to: (s.origin, s.profileId)) } } ?? []
        entries = mine.sorted { $0.addedAt > $1.addedAt }.map(entry)
        subject.send()
        measure()
    }

    /// The storage line, measured at most once a second (off the main actor).
    private func measure() {
        guard storageTask == nil else { return }
        let root = store.root
        let wait = max(0, 1 - clock().timeIntervalSince(storageAt))
        storageTask = Task { [weak self] in
            if wait > 0 { try? await Task.sleep(for: .seconds(wait)) }
            let used = await Task.detached { DownloadStore.usedBytes(root) }.value
            guard let self else { return }
            self.storageAt = self.clock()
            self.storageTask = nil
            let measured = DownloadStorage(usedBytes: used, freeBytes: self.store.freeBytes())
            if measured != self.storage {
                self.storage = measured
                self.subject.send()
            }
        }
    }

    private func entry(_ r: DownloadRecord) -> DownloadEntry {
        DownloadEntry(
            itemId: r.itemId, fileId: r.fileId, kind: r.kind, title: r.title, showId: r.showId, showTitle: r.showTitle,
            seasonIndex: r.seasonIndex, episodeIndex: r.episodeIndex, year: r.year, durationMs: r.durationMs,
            sizeBytes: r.sizeBytes, downloadedBytes: r.state == .done ? r.sizeBytes : r.downloadedBytes, state: r.state,
            image: r.image ? store.picture(r) : nil, addedAt: r.addedAt, positionMs: r.positionMs, watched: r.watched,
            error: r.error,
            converting: r.mediaPath == nil && r.state != .done && r.state != .failed ? r.converting : nil,
            serverAway: r.state == .waiting && r.unreachable)
    }
}

/// One profile's downloads, as the player takes them.
@MainActor
final class ScopedLocalMedia: LocalMedia {
    private weak var downloads: Downloads?
    let origin: String
    let profileId: Int

    init(downloads: Downloads, origin: String, profileId: Int) {
        self.downloads = downloads
        self.origin = origin
        self.profileId = profileId
    }

    func file(_ fileId: Int) -> LocalFile? {
        guard let downloads, let r = downloads.records(origin: origin, profileId: profileId)
            .first(where: { $0.fileId == fileId && $0.state == .done }) else { return nil }
        let store = downloads.downloadStore
        var subtitles: [Int: URL] = [:]
        for index in r.subtitles where FileManager.default.fileExists(atPath: store.subtitle(r, index).path) {
            subtitles[index] = store.subtitle(r, index)
        }
        return LocalFile(url: store.media(r), subtitles: subtitles, converted: r.converted)
    }

    func detail(_ itemId: Int) async -> ItemDetail? {
        guard let downloads, let r = downloads.record(itemId, origin: origin, profileId: profileId), r.extrasSaved
        else { return nil }
        let file = downloads.downloadStore.detail(r)
        guard var detail = await Task.detached(operation: {
            (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode(ItemDetail.self, from: $0) }
        }).value else { return nil }
        // Where it was left here is the latest this device knows.
        detail.summary.userState.positionMs = r.positionMs
        detail.summary.userState.watched = r.watched
        return detail
    }

    func played(_ itemId: Int, positionMs: Int, durationMs: Int?) {
        guard let downloads else { return }
        let (position, watched) = DownloadPlanner.played(positionMs: positionMs, durationMs: durationMs)
        downloads.downloadStore.update(DownloadStore.key(origin: origin, profileId: profileId, itemId: itemId)) {
            $0.positionMs = position
            $0.watched = $0.watched || watched
        }
    }

    func nextAfter(_ itemId: Int) async -> ItemSummary? {
        guard let downloads else { return nil }
        let mine = downloads.records(origin: origin, profileId: profileId)
        guard let now = mine.first(where: { $0.itemId == itemId }),
              let next = DownloadPlanner.nextEpisode(mine, after: now) else { return nil }
        var summary = await detail(next.itemId)?.summary
            ?? ItemSummary(id: next.itemId, libraryId: 0, kind: next.kind, title: next.title)
        summary.index = next.episodeIndex
        summary.parentIndex = next.seasonIndex
        summary.grandparentId = next.showId
        summary.grandparentTitle = next.showTitle
        summary.userState.positionMs = next.positionMs
        summary.userState.watched = next.watched
        return summary
    }
}
