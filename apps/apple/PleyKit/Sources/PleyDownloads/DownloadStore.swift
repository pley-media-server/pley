import CryptoKit
import Foundation
import PleyCommon
import PleyData
import PleyModel

/// A download, as the index keeps it.
struct DownloadRecord: Codable, Hashable, Sendable {
    /// The piece of the file on its way: its byte range, and when the token it carries ends.
    struct Piece: Codable, Hashable, Sendable {
        var start: Int64
        var end: Int64
        var tokenExpires: Date
    }

    /// The server's origin, as it signs in.
    var origin: String
    var profileId: Int
    var itemId: Int
    var fileId: Int
    var kind: ItemKind
    var title: String
    var showId: Int?
    var showTitle: String?
    var seasonIndex: Int?
    var episodeIndex: Int?
    var year: Int?
    var durationMs: Int?
    /// The file's container ("mp4"): its saved name's extension.
    var container: String?
    var sizeBytes: Int64 = 0
    var downloadedBytes: Int64 = 0
    var state: DownloadState = .queued
    /// The details, the picture and the subtitles are saved.
    var extrasSaved = false
    /// A picture is saved (`picture.jpg`).
    var image = false
    /// The text subtitle tracks saved as WebVTT (`subtitles/<index>.vtt`).
    var subtitles: [Int] = []
    /// Those of `subtitles` the server hadn't read whole yet: asked for again.
    var partialSubtitles: [Int] = []
    var addedAt: Date
    /// Where it was left, played here.
    var positionMs = 0
    var watched = false
    var error: String?
    /// Settings → Downloads → Quality when it was asked for: `nil` for the original.
    var maxHeight: Int?
    var maxBitrateKbps: Int?
    /// The server's conversion, once asked for.
    var conversion: String?
    /// How far the server has come making it, 0 to 1, while it does.
    var converting: Double?
    /// When the conversion last moved on (it gives up after 15 minutes without).
    var convertingSince: Date?
    /// Where the file comes from on the server, once the server said
    /// (the original's stream, or the converted file).
    var mediaPath: String?
    /// The server made the file for this device.
    var converted = false
    /// Not looked at again before this: its conversion is being made, or it waits after a loss.
    var notBefore = Date.distantPast
    /// It waits because the server didn't answer, rather than for a network.
    var unreachable = false
    var piece: Piece?

    var key: String { DownloadStore.key(origin: origin, profileId: profileId, itemId: itemId) }

    func belongs(to scope: (origin: String, profileId: Int)?) -> Bool {
        guard let scope else { return false }
        return origin == scope.origin && profileId == scope.profileId
    }
}

/// Where downloads are kept (Android's `DownloadStore`):
/// `Application Support/Pley/Downloads/<server>/<profile>/<item>/`, where
/// server is the first 16 hex of its origin's SHA-256, holding
/// `media.<ext>`, `media.partial`, `detail.json`, `picture.jpg` and
/// `subtitles/<index>.vtt`; and one `index.json` of them all, written
/// whole (a temporary file moved over the old one), so a crash leaves the
/// old index or the new one. The folder is left out of backups.
@MainActor
final class DownloadStore {
    static let index = "index.json"
    /// Why a download marked whole has no file: its folder went.
    static var missing: String { localized("The downloaded file is gone") }

    let root: URL
    /// Every download of every server and profile, oldest first.
    private(set) var all: [DownloadRecord] = []
    /// Told after every change, saved or not.
    var onChange: (() -> Void)?
    private let freeSpace: @Sendable (URL) -> Int64

    init(root: URL, freeSpace: @escaping @Sendable (URL) -> Int64 = DownloadStore.systemFreeSpace) {
        self.root = root
        self.freeSpace = freeSpace
        load()
    }

    /// The default folder, in the app's own Application Support (inside the Mac's sandbox container).
    nonisolated static var defaultRoot: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Pley", isDirectory: true)
            .appendingPathComponent("Downloads", isDirectory: true)
    }

    nonisolated static func key(origin: String, profileId: Int, itemId: Int) -> String {
        "\(serverKey(origin))/\(profileId)/\(itemId)"
    }

    /// A folder name for `origin`: its address could hold anything.
    nonisolated static func serverKey(_ origin: String) -> String {
        String(SHA256.hash(data: Data(origin.utf8)).map { String(format: "%02x", $0) }.joined().prefix(16))
    }

    // MARK: Paths

    func dir(_ r: DownloadRecord) -> URL {
        root.appendingPathComponent(Self.serverKey(r.origin), isDirectory: true)
            .appendingPathComponent(String(r.profileId), isDirectory: true)
            .appendingPathComponent(String(r.itemId), isDirectory: true)
    }

    func media(_ r: DownloadRecord) -> URL { dir(r).appendingPathComponent("media.\(Self.extension(r.container))") }
    func partial(_ r: DownloadRecord) -> URL { dir(r).appendingPathComponent("media.partial") }
    func detail(_ r: DownloadRecord) -> URL { dir(r).appendingPathComponent("detail.json") }
    func picture(_ r: DownloadRecord) -> URL { dir(r).appendingPathComponent("picture.jpg") }

    func subtitle(_ r: DownloadRecord, _ index: Int) -> URL {
        dir(r).appendingPathComponent("subtitles", isDirectory: true).appendingPathComponent("\(index).vtt")
    }

    /// Where finished pieces wait to be appended: inside the downloads folder, out of backups.
    var pieces: URL { root.appendingPathComponent("pieces", isDirectory: true) }

    /// The bytes of `r`'s partial file.
    func partialBytes(_ r: DownloadRecord) -> Int64 {
        Self.size(of: partial(r))
    }

    nonisolated static func size(of url: URL) -> Int64 {
        ((try? FileManager.default.attributesOfItem(atPath: url.path)[.size]) as? NSNumber)?.int64Value ?? 0
    }

    private static func `extension`(_ container: String?) -> String {
        // The first of several names ("mov,mp4,m4a" as ffprobe gives some).
        let first = (container ?? "").lowercased().split(separator: ",").first.map(String.init) ?? ""
        let clean = first.filter { $0.isASCII && ($0.isLetter || $0.isNumber) }
        return clean.isEmpty ? "media" : clean
    }

    // MARK: Changes

    func get(_ key: String) -> DownloadRecord? { all.first { $0.key == key } }

    /// Adds the records not there yet, at the end; returns those added.
    @discardableResult
    func add(_ records: [DownloadRecord]) -> [DownloadRecord] {
        var known = Set(all.map(\.key))
        let added = records.filter { known.insert($0.key).inserted }
        guard !added.isEmpty else { return [] }
        all += added
        save()
        onChange?()
        return added
    }

    /// Changes the download of `key`, saved unless `persist` is false; `nil` when it is gone.
    @discardableResult
    func update(_ key: String, persist: Bool = true, _ change: (inout DownloadRecord) -> Void) -> DownloadRecord? {
        guard let n = all.firstIndex(where: { $0.key == key }) else { return nil }
        change(&all[n])
        if persist { save() }
        onChange?()
        return all[n]
    }

    /// Removes `key`'s download and its folder.
    func remove(_ key: String) {
        guard let r = get(key) else { return }
        try? FileManager.default.removeItem(at: dir(r))
        all.removeAll { $0.key == key }
        save()
        onChange?()
    }

    /// Removes every download of the server at `origin`.
    func removeServer(_ origin: String) {
        let gone = all.filter { $0.origin == origin }
        guard !gone.isEmpty else { return }
        try? FileManager.default.removeItem(at: root.appendingPathComponent(Self.serverKey(origin), isDirectory: true))
        all.removeAll { $0.origin == origin }
        save()
        onChange?()
    }

    /// Removes every download, and every file under the folder.
    func clear() {
        all = []
        let fm = FileManager.default
        for url in (try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)) ?? [] {
            try? fm.removeItem(at: url)
        }
        save()
        onChange?()
    }

    /// The bytes the downloads take.
    nonisolated static func usedBytes(_ root: URL) -> Int64 {
        var total: Int64 = 0
        guard let walk = FileManager.default.enumerator(at: root, includingPropertiesForKeys: [.fileSizeKey]) else {
            return 0
        }
        for case let url as URL in walk {
            total += Int64((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
        }
        return total
    }

    /// What is free where the downloads go.
    func freeBytes() -> Int64 {
        try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return freeSpace(root)
    }

    nonisolated static func systemFreeSpace(_ url: URL) -> Int64 {
        #if os(macOS) || os(iOS)
        if let important = try? url.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
            .volumeAvailableCapacityForImportantUsage, important > 0 {
            return important
        }
        #endif
        return Int64((try? url.resourceValues(forKeys: [.volumeAvailableCapacityKey]).volumeAvailableCapacity) ?? 0)
    }

    // MARK: The index

    /// Reads the index once. A download cut short by the app ending keeps
    /// its piece until the transfers in flight are known (`settle`).
    private func load() {
        let file = root.appendingPathComponent(Self.index)
        let read = (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode([DownloadRecord].self, from: $0) }
        all = (read ?? []).map { r in
            var r = r
            if r.state == .done, !FileManager.default.fileExists(atPath: media(r).path) {
                r.state = .failed
                r.error = Self.missing
            }
            return r
        }
        excludeFromBackup()
    }

    /// The transfers still in flight are `running`: the downloads the app
    /// was fetching when it ended, and that no transfer carries on, wait
    /// their turn again from what their partial file holds.
    func settle(running: Set<String>) {
        var changed = false
        for n in all.indices where all[n].state == .downloading || all[n].state == .waiting {
            guard !running.contains(all[n].key) else { continue }
            all[n].state = .queued
            all[n].piece = nil
            all[n].downloadedBytes = partialBytes(all[n])
            changed = true
        }
        if changed {
            save()
            onChange?()
        }
    }

    /// Writes the index whole: a temporary file, then moved over the old
    /// one. A device too full to take it keeps the one before; the next
    /// save writes it all.
    private func save() {
        let fm = FileManager.default
        let file = root.appendingPathComponent(Self.index)
        let temp = root.appendingPathComponent(Self.index + ".tmp")
        do {
            try fm.createDirectory(at: root, withIntermediateDirectories: true)
            excludeFromBackup()
            try JSONEncoder().encode(all).write(to: temp)
            if fm.fileExists(atPath: file.path) {
                _ = try fm.replaceItemAt(file, withItemAt: temp)
            } else {
                try fm.moveItem(at: temp, to: file)
            }
        } catch {
            try? fm.removeItem(at: temp)
            Log.downloads.error("the downloads' index couldn't be written")
        }
    }

    private func excludeFromBackup() {
        guard FileManager.default.fileExists(atPath: root.path) else { return }
        var url = root
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? url.setResourceValues(values)
    }
}
