import Foundation
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork

/// Fetches downloads one at a time, oldest first (Android's
/// `DownloadRunner`), on the main actor:
///
/// 1. the item's details, a picture and its text subtitles as WebVTT;
/// 2. `POST /api/conversions`: the original as it is (`direct`), or a
///    file the server makes, looked at every 5 s while Pley runs (the
///    others go on meanwhile) and given up after 15 minutes without progress;
/// 3. the file, in pieces of 256 MiB (`Range`), each a background
///    download task with a token that lasts 10 minutes more, appended to
///    `media.partial` in 1 MiB chunks, then moved to `media.<ext>`.
///
/// A piece that failed goes on from its resume data while its token
/// lasts, and is otherwise asked for again from the partial file's length.
/// A lost network or server makes the download wait; an error the server
/// gives fails it until it is tried again.
@MainActor
final class DownloadRunner {
    struct Timing: Sendable {
        var pieceBytes: Int64 = DownloadPlanner.pieceBytes
        var pollSeconds: Double = DownloadPlanner.pollSeconds
        var conversionPatience: TimeInterval = DownloadPlanner.conversionPatience
        var backoffSeconds: Double = 5
        var spaceMargin: Int64 = DownloadPlanner.spaceMargin
    }

    /// The download being fetched now.
    private(set) var active: String?
    var scope: DownloadScope? {
        didSet { if scope?.origin != oldValue?.origin || scope?.profileId != oldValue?.profileId { pump() } }
    }

    private let store: DownloadStore
    private let transfer: any TransferSession
    private let gate: NetworkGate
    private let settings: AppSettings
    private let timing: Timing
    private let clock: () -> Date
    private var work: Task<Void, Never>?
    private var wake: Task<Void, Never>?
    private var listener: Task<Void, Never>?
    private var lostInARow = 0
    /// The system's handler after a background wake, called once the events are handled.
    var finishedEvents: (() -> Void)?
    /// Progress shown at most every half second.
    private var shownAt: [String: Date] = [:]

    init(store: DownloadStore, transfer: any TransferSession, gate: NetworkGate, settings: AppSettings,
         timing: Timing = Timing(), clock: @escaping () -> Date = { Date() }) {
        self.store = store
        self.transfer = transfer
        self.gate = gate
        self.settings = settings
        self.timing = timing
        self.clock = clock
        let events = transfer.events
        listener = Task { [weak self] in
            for await event in events { await self?.heard(event) }
        }
        gate.onChange { [weak self] in
            Task { @MainActor in self?.pump() }
        }
    }

    deinit {
        listener?.cancel()
        work?.cancel()
        wake?.cancel()
    }

    /// The transfers in flight once the app starts again: their downloads
    /// carry on; the others wait their turn again.
    func settle() async {
        let running = await transfer.running()
        store.settle(running: running)
        if let carried = store.all.first(where: { running.contains($0.key) && $0.state == .downloading }) {
            active = carried.key
        }
        pump()
    }

    // MARK: The queue

    /// Starts the next download when none runs and one may.
    func pump() {
        guard active == nil, let scope else { return }
        let mine = store.all.filter {
            ($0.state == .queued || $0.state == .waiting) && $0.belongs(to: (scope.origin, scope.profileId))
        }
        guard !mine.isEmpty else { return }
        let now = clock()
        guard let next = mine.first(where: { $0.notBefore <= now }) else {
            // Every one left is being made by the server, or waits after a loss: looked at again soon.
            let soonest = mine.map(\.notBefore).min() ?? now
            later(soonest.timeIntervalSince(now))
            return
        }
        guard gate.allows(mobile: settings.downloadOnMobileData) else {
            for r in mine where r.state != .waiting || r.unreachable {
                store.update(r.key) {
                    $0.state = .waiting
                    $0.unreachable = false
                }
            }
            return
        }
        let key = next.key
        active = key
        work = Task { [weak self] in await self?.prepare(key, scope: scope) }
    }

    /// Looks at the queue again in `seconds`.
    private func later(_ seconds: TimeInterval) {
        wake?.cancel()
        let wait = min(max(seconds, 0.05), timing.pollSeconds)
        wake = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled else { return }
            self?.pump()
        }
    }

    /// Stops `key`'s download if it runs (it was removed).
    func cancel(_ key: String) {
        guard active == key else { return }
        work?.cancel()
        transfer.cancel(key: key)
        active = nil
    }

    /// The details, the picture, the subtitles and the server's answer of
    /// `key`, then its first piece.
    private func prepare(_ key: String, scope: DownloadScope) async {
        do {
            guard var r = store.get(key) else { return done(key) }
            store.update(key, persist: false) {
                $0.error = nil
                $0.unreachable = false
            }
            if !r.extrasSaved { r = try await saveExtras(r, api: scope.api) }
            if r.mediaPath == nil {
                guard try await convert(r, scope: scope) != nil else {
                    // The server is making it: the others go on meanwhile.
                    return done(key)
                }
            }
            try Task.checkCancellation()
            try await nextPiece(key)
        } catch is CancellationError {
            if active == key { done(key) }
        } catch {
            failed(key, error)
        }
    }

    /// The download stopped running: the next one may start.
    private func done(_ key: String) {
        guard active == key else { return }
        active = nil
        work = nil
        pump()
    }

    // MARK: Extras

    /// The details (for playing and listing offline), a picture and the
    /// text subtitles, saved beside the file. Subtitles the server hasn't
    /// read whole yet are asked for again once the file is down.
    private func saveExtras(_ r: DownloadRecord, api: PleyAPI) async throws -> DownloadRecord {
        let detail = try await api.item(r.itemId)
        let dir = store.dir(r)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try JSONEncoder().encode(detail).write(to: store.detail(r), options: .atomic)
        let image = await savePicture(r, detail: detail, api: api)
        let file = detail.files.first { $0.id == r.fileId }
        // Text tracks only: picture subtitles aren't downloaded, as on Android.
        let tracks = file?.streams(.subtitle).filter { $0.textBased == true } ?? []
        var saved: [Int] = []
        var partial: [Int] = []
        for track in tracks {
            guard let whole = try? await saveSubtitle(r, index: track.index, api: api) else { continue }
            saved.append(track.index)
            if !whole { partial.append(track.index) }
        }
        guard let updated = store.update(r.key, {
            $0.extrasSaved = true
            $0.image = image || $0.image
            $0.subtitles = saved
            $0.partialSubtitles = partial
            $0.durationMs = file?.durationMs ?? detail.durationMs ?? $0.durationMs
        }) else { throw CancellationError() }
        return updated
    }

    /// A still, else the poster or the backdrop, 640 px wide: whether there is one.
    func savePicture(_ r: DownloadRecord, detail: ItemDetail, api: PleyAPI) async -> Bool {
        let s = detail.summary
        guard let path = [s.thumbUrl, s.posterUrl, s.backdropUrl].compactMap({ $0 }).first(where: { !$0.isEmpty }),
              let data = try? await api.image(path: path, width: DownloadPlanner.pictureWidth),
              store.get(r.key) != nil
        else { return false }
        do {
            try FileManager.default.createDirectory(at: store.dir(r), withIntermediateDirectories: true)
            try data.write(to: store.picture(r), options: .atomic)
            return true
        } catch {
            return false
        }
    }

    /// A text track as the server's WebVTT, saved as far as it goes: whether it came whole.
    private func saveSubtitle(_ r: DownloadRecord, index: Int, api: PleyAPI) async throws -> Bool {
        let (data, complete) = try await api.subtitleText(fileId: r.fileId, index: index)
        // Never into the folder of a download deleted meanwhile.
        guard store.get(r.key) != nil else { throw CancellationError() }
        let file = store.subtitle(r, index)
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: file, options: .atomic)
        return complete
    }

    /// The subtitle tracks of `r` that came cut short, asked for again.
    func finishSubtitles(_ key: String, api: PleyAPI) async {
        guard let r = store.get(key), !r.partialSubtitles.isEmpty else { return }
        var whole: Set<Int> = []
        for index in r.partialSubtitles {
            if (try? await saveSubtitle(r, index: index, api: api)) == true { whole.insert(index) }
        }
        guard !whole.isEmpty else { return }
        store.update(key) { $0.partialSubtitles.removeAll { whole.contains($0) } }
    }

    // MARK: The server's file

    /// `r` with the address of its file, once the server said which (the
    /// original, or the file it made whole); `nil` while it is being made.
    private func convert(_ r: DownloadRecord, scope: DownloadScope) async throws -> String? {
        let api = scope.api
        let info: DownloadInfo
        do {
            var known: DownloadInfo?
            if let id = r.conversion {
                do {
                    known = try await api.conversion(id)
                } catch PleyError.api(status: 404, _, _) {
                    // The server restarted, or forgot it: asked for again.
                }
            }
            if let known {
                info = known
            } else {
                let request = PlaybackRequest(itemId: r.itemId, fileId: r.fileId, maxBitrate: r.maxBitrateKbps,
                                              maxHeight: r.maxHeight, mode: .auto, caps: await scope.caps())
                info = try await api.convert(request)
            }
        } catch PleyError.api(status: let status, _, _) where status == 501 || status == 404 {
            // A server that can't convert: the original, unless a smaller file was asked for.
            guard r.maxHeight == nil else { throw Failure(DownloadPlanner.noConversions) }
            return try keep(r.key) {
                $0.mediaPath = PleyAPI.fileStreamPath($0.fileId)
                $0.converted = false
                $0.converting = nil
            }.mediaPath
        }
        let now = clock()
        switch info.state {
        case .direct, .done:
            let path = PleyAPI.isDownloadPath(info.url) ? info.url : PleyAPI.fileStreamPath(r.fileId)
            let converted = info.state == .done
            return try keep(r.key) {
                $0.mediaPath = path
                $0.converted = converted
                $0.conversion = info.id ?? $0.conversion
                $0.converting = nil
                $0.convertingSince = nil
                if converted { $0.container = "mp4" }
                if let size = info.size, size > 0 {
                    if $0.sizeBytes != size { try? FileManager.default.removeItem(at: self.store.partial($0)) }
                    $0.sizeBytes = size
                }
            }.mediaPath
        case .failed:
            throw Failure(DownloadPlanner.conversionFailed)
        default:
            let progress = min(max(info.progress, 0), 1)
            let moved = r.converting.map { progress > $0 } ?? true
            if !moved, let since = r.convertingSince, now.timeIntervalSince(since) > timing.conversionPatience {
                throw Failure(DownloadPlanner.conversionStalled)
            }
            _ = try keep(r.key) {
                $0.state = .queued
                $0.conversion = info.id
                $0.converting = progress
                if moved || $0.convertingSince == nil { $0.convertingSince = now }
                $0.notBefore = now.addingTimeInterval(self.timing.pollSeconds)
            }
            return nil
        }
    }

    /// `change` made to `key`'s download, which must still be there.
    private func keep(_ key: String, _ change: (inout DownloadRecord) -> Void) throws -> DownloadRecord {
        guard let r = store.update(key, change) else { throw CancellationError() }
        return r
    }

    // MARK: Pieces

    /// Asks for the next piece of `key`'s file, or puts the file in place when it is all down.
    private func nextPiece(_ key: String) async throws {
        guard let scope, var r = store.get(key), r.belongs(to: (scope.origin, scope.profileId)) else {
            // Another profile signed in meanwhile: it waits for its own.
            store.update(key) {
                $0.state = .queued
                $0.piece = nil
            }
            return done(key)
        }
        let have = store.partialBytes(r)
        guard let range = DownloadPlanner.piece(have: have, size: r.sizeBytes, pieceBytes: timing.pieceBytes) else {
            return finish(key)
        }
        let left = r.sizeBytes > 0 ? r.sizeBytes - have : timing.pieceBytes
        if store.freeBytes() < left + timing.spaceMargin { throw Failure(DownloadPlanner.noSpace) }
        guard let tokens = scope.tokens else { throw PleyError.signedOut }
        guard let path = r.mediaPath, PleyAPI.isDownloadPath(path), let url = scope.api.client.url(path) else {
            throw Failure(DownloadPlanner.message(status: 404))
        }
        // Over plain http, the host must stand for private addresses only.
        try await scope.api.client.checkPlayerURL(url)
        let (token, expires) = try await tokens.accessToken(lasting: DownloadPlanner.tokenLasting)
        try Task.checkCancellation()
        let mobile = settings.downloadOnMobileData
        var request = URLRequest(url: url)
        request.setValue("bytes=\(range.lowerBound)-\(range.upperBound)", forHTTPHeaderField: "Range")
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("*/*", forHTTPHeaderField: "Accept")
        request.allowsCellularAccess = mobile
        request.allowsExpensiveNetworkAccess = mobile
        request.allowsConstrainedNetworkAccess = mobile
        r = try keep(key) {
            $0.state = .downloading
            $0.unreachable = false
            $0.downloadedBytes = have
            $0.piece = .init(start: range.lowerBound, end: range.upperBound, tokenExpires: expires)
        }
        transfer.start(request, key: key)
    }

    private func heard(_ event: TransferEvent) async {
        switch event {
        case .progress(let key, let written):
            guard let piece = store.get(key)?.piece else { return }
            let now = clock()
            if let at = shownAt[key], now.timeIntervalSince(at) < 0.5 { return }
            shownAt[key] = now
            store.update(key, persist: false) { $0.downloadedBytes = piece.start + written }
        case .finished(let finished):
            await pieceEnded(finished)
        case .allDelivered:
            let handler = finishedEvents
            finishedEvents = nil
            handler?()
        }
    }

    /// A piece ended: appended and the next asked for, or what went wrong dealt with.
    private func pieceEnded(_ f: Finished) async {
        let key = f.key
        defer { if let file = f.file { try? FileManager.default.removeItem(at: file) } }
        guard let r = store.get(key), let piece = r.piece else { return }
        if active == nil { active = key }
        guard active == key else { return }
        if f.cancelled {
            store.update(key) { $0.piece = nil }
            return done(key)
        }
        guard f.privateRoute else {
            return failed(key, Failure(DownloadPlanner.notPrivate))
        }
        if f.failure != nil {
            let mobile = settings.downloadOnMobileData
            if let data = f.resumeData, DownloadPlanner.resumes(tokenExpires: piece.tokenExpires, now: clock()),
               gate.allows(mobile: mobile) {
                transfer.resume(data, key: key)
                return
            }
            return lost(key)
        }
        lostInARow = 0
        let have = store.partialBytes(r)
        switch DownloadPlanner.check(status: f.status, contentRange: f.headers["content-range"], start: piece.start,
                                     size: r.sizeBytes) {
        case .append(let total):
            guard have == piece.start, let file = f.file else { return restartPiece(key) }
            let partial = store.partial(r)
            do {
                try await Task.detached { try Self.append(file, to: partial) }.value
            } catch {
                return failed(key, Failure(DownloadPlanner.noSpace))
            }
            store.update(key) {
                if let total, total > 0 { $0.sizeBytes = total }
                $0.downloadedBytes = Self.size(partial)
                $0.piece = nil
            }
            await continueAfterPiece(key)
        case .whole:
            guard let file = f.file else { return restartPiece(key) }
            let partial = store.partial(r)
            try? FileManager.default.removeItem(at: partial)
            do {
                try FileManager.default.moveItem(at: file, to: partial)
            } catch {
                return failed(key, Failure(DownloadPlanner.noSpace))
            }
            store.update(key) {
                $0.sizeBytes = Self.size(partial)
                $0.downloadedBytes = $0.sizeBytes
                $0.piece = nil
            }
            await continueAfterPiece(key)
        case .complete:
            finish(key)
        case .restart:
            // The file changed on the server: it starts again.
            try? FileManager.default.removeItem(at: store.partial(r))
            restartPiece(key)
        case .refused(let status):
            if status == 404, r.converted {
                // The server pruned the converted file, or forgot it: asked for again.
                store.update(key) {
                    $0.state = .queued
                    $0.mediaPath = nil
                    $0.piece = nil
                    $0.notBefore = self.clock().addingTimeInterval(self.timing.pollSeconds)
                }
                return done(key)
            }
            if status >= 500 || status == 0 { return lost(key) }
            failed(key, Failure(DownloadPlanner.message(status: status)))
        }
    }

    private func continueAfterPiece(_ key: String) async {
        do {
            try await nextPiece(key)
        } catch {
            failed(key, error)
        }
    }

    private func restartPiece(_ key: String) {
        store.update(key) {
            $0.state = .queued
            $0.piece = nil
            $0.downloadedBytes = 0
        }
        done(key)
    }

    /// All of `key`'s file is down: it goes in place, and is done.
    private func finish(_ key: String) {
        guard let r = store.get(key) else { return done(key) }
        let fm = FileManager.default
        let media = store.media(r)
        do {
            try? fm.removeItem(at: media)
            try fm.moveItem(at: store.partial(r), to: media)
        } catch {
            if !fm.fileExists(atPath: media.path) { return failed(key, Failure(DownloadStore.missing)) }
        }
        store.update(key) {
            $0.state = .done
            $0.sizeBytes = Self.size(media)
            $0.downloadedBytes = $0.sizeBytes
            $0.piece = nil
            $0.error = nil
            $0.converting = nil
        }
        Log.downloads.info("a download finished")
        if let api = scope?.api { Task { await finishSubtitles(key, api: api) } }
        done(key)
    }

    /// The network went, or the server: it waits a little longer each
    /// time, while the others have their turn.
    private func lost(_ key: String) {
        lostInARow += 1
        let away = gate.allows(mobile: settings.downloadOnMobileData)
        let wait = timing.backoffSeconds * Double(min(lostInARow, 12))
        store.update(key) {
            $0.state = .waiting
            $0.unreachable = away
            $0.piece = nil
            $0.downloadedBytes = self.store.partialBytes($0)
            $0.notBefore = self.clock().addingTimeInterval(wait)
        }
        done(key)
    }

    private func failed(_ key: String, _ error: any Error) {
        if let error = error as? PleyError, Self.isUnreachable(error) { return lost(key) }
        let message: String = switch error {
        case let failure as Failure: failure.message
        case PleyError.api(let status, _, _): DownloadPlanner.message(status: status)
        case PleyError.signedOut: DownloadPlanner.message(status: 401)
        case PleyError.notAvailable: "The server can't make it now: try again later"
        default: DownloadPlanner.message(status: 0)
        }
        if case PleyError.notAvailable = error { return lost(key) }
        store.update(key) {
            $0.state = .failed
            $0.error = message
            $0.piece = nil
        }
        done(key)
    }

    static func isUnreachable(_ error: PleyError) -> Bool {
        switch error {
        case .offline, .timedOut, .unreachable, .localNetworkBlocked: true
        case .api(let status, _, _), .unreadable(let status): status == 503 || status == 502 || status == 504
        default: false
        }
    }

    /// Appends `piece` to `partial` in 1 MiB chunks, never the whole in memory.
    nonisolated static func append(_ piece: URL, to partial: URL) throws {
        let fm = FileManager.default
        if !fm.fileExists(atPath: partial.path) { fm.createFile(atPath: partial.path, contents: nil) }
        let input = try FileHandle(forReadingFrom: piece)
        defer { try? input.close() }
        let output = try FileHandle(forWritingTo: partial)
        defer { try? output.close() }
        try output.seekToEnd()
        while true {
            let chunk = try autoreleasepool { try input.read(upToCount: 1 << 20) }
            guard let chunk, !chunk.isEmpty else { break }
            try output.write(contentsOf: chunk)
        }
    }

    nonisolated static func size(_ url: URL) -> Int64 { DownloadStore.size(of: url) }
}

/// A download that can't go on by itself: `message` says why, for the list.
struct Failure: LocalizedError, Sendable {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}
