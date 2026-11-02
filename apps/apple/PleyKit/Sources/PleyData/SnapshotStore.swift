import CryptoKit
import Foundation
import PleyCommon
import PleyNetwork

/// The names answers are kept under, per server and profile (Android's
/// `SnapshotKeys`, for what the Apple app reads).
public enum SnapshotKeys {
    /// `GET /api/hubs/home`.
    public static let home = "home"
    /// `GET /api/libraries`.
    public static let libraries = "libraries"
    /// `GET /api/playlists`.
    public static let playlists = "playlists"
    /// `GET /api/watchlist`.
    public static let watchlist = "watchlist"
    /// The first page of `GET /api/history`.
    public static let history = "history:head"
    /// `GET /api/auth/me`: the profile, for starting without the server.
    public static let me = "me"
    /// `GET /api/live`.
    public static let live = "live"
    /// `GET /api/live/groups`.
    public static let liveGroups = "live:groups"

    /// A page of a category's channels matching `query`.
    public static func livePage(_ category: String, _ query: String, _ page: Int) -> String {
        "live:page:\(category):\(page):\(query)"
    }

    /// The page of a category holding `channelId`.
    public static func liveAround(_ category: String, _ channelId: Int) -> String {
        "live:around:\(category):\(channelId)"
    }

    public static func liveChannel(_ channelId: Int) -> String { "live:channel:\(channelId)" }
    /// `GET /api/libraries/{id}`.
    public static func library(_ libraryId: Int) -> String { "library:\(libraryId)" }
    public static func libraryHubs(_ libraryId: Int) -> String { "libhubs:\(libraryId)" }
    /// The first items of a library's listing as `query` (its sort, order and filters) asks.
    public static func libraryHead(_ libraryId: Int, _ query: String) -> String { "library:\(libraryId):\(query):head" }
    public static func collections(_ libraryId: Int) -> String { "collections:\(libraryId)" }
    public static func item(_ itemId: Int) -> String { "item:\(itemId)" }
    public static func children(_ itemId: Int) -> String { "children:\(itemId)" }
    public static func person(_ personId: Int) -> String { "person:\(personId)" }
    public static func playlist(_ playlistId: Int) -> String { "playlist:\(playlistId)" }
    public static func search(_ query: String) -> String { "search:\(query)" }
}

/// Answers saved per server and profile, given back while the server is
/// out of reach (the app's model).
@MainActor
public protocol SnapshotCache: AnyObject {
    /// `load`'s answer, saved as `key`; the one saved while the server can't be reached.
    func cached<T: Codable & Sendable>(_ key: String, _ load: @escaping (PleyAPI) async throws -> T) async throws -> T
}

/// The last answer of each screen on disk (Android's `FileSnapshotStore`):
/// `Application Support/Pley/Snapshots/<server>/<profile>/<key>.json`,
/// where server is the SHA-256 of its origin. While the server is out of
/// reach, the screens show what was saved.
///
/// - A file is written whole to a temporary file and moved over the old
///   one, so a crash leaves the old answer or the new one, never half.
/// - The store holds 25 MB at most: the answers read or written longest
///   ago go first.
/// - Each server keeps the snapshots of its last 3 profiles.
public actor SnapshotStore {
    public static let maxBytes = 25 * 1024 * 1024
    public static let keepProfiles = 3

    private let root: URL
    private let maxBytes: Int
    /// Every snapshot file and its size, the least recently used first; read from disk at first use.
    private var index: [(path: String, size: Int)]?

    /// The store under `root` (`Application Support/Pley/Snapshots` by default).
    public init(root: URL? = nil, maxBytes: Int = SnapshotStore.maxBytes) {
        self.root = root ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Pley", isDirectory: true)
            .appendingPathComponent("Snapshots", isDirectory: true)
        self.maxBytes = maxBytes
    }

    /// A folder name for `origin`: its address could hold anything.
    public static func serverKey(_ origin: String) -> String {
        SHA256.hash(data: Data(origin.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    /// `origin` and `profileId`'s snapshot of `key`, `nil` when there is none.
    public func read(origin: String, profileId: Int, key: String) -> Data? {
        let file = fileOf(origin, profileId, key)
        guard let data = try? Data(contentsOf: file), !data.isEmpty else {
            forget(file.path)
            return nil
        }
        touch(file.path, size: data.count)
        return data
    }

    /// Keeps `data` as `origin` and `profileId`'s snapshot of `key`.
    public func write(origin: String, profileId: Int, key: String, data: Data) {
        let file = fileOf(origin, profileId, key)
        do {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(),
                                                    withIntermediateDirectories: true)
            // Written to a temporary file, then moved over the old one in one step.
            try data.write(to: file, options: [.atomic])
        } catch {
            // A full device: the snapshot is only a head start.
            return
        }
        touch(file.path, size: data.count)
        evict(keeping: file.path)
    }

    /// The profile in use on `origin`: the server keeps the snapshots of
    /// its last 3 profiles only.
    public func activate(origin: String, profileId: Int) {
        let server = root.appendingPathComponent(Self.serverKey(origin), isDirectory: true)
        let mine = server.appendingPathComponent(String(profileId), isDirectory: true)
        let fm = FileManager.default
        try? fm.createDirectory(at: mine, withIntermediateDirectories: true)
        try? fm.setAttributes([.modificationDate: Date()], ofItemAtPath: mine.path)
        let profiles = (try? fm.contentsOfDirectory(at: server, includingPropertiesForKeys: [.contentModificationDateKey],
                                                    options: [.skipsHiddenFiles])) ?? []
        guard profiles.count > Self.keepProfiles else { return }
        let others = profiles.filter { $0.lastPathComponent != mine.lastPathComponent }.sorted { modified($0) > modified($1) }
        for gone in others.dropFirst(Self.keepProfiles - 1) {
            try? fm.removeItem(at: gone)
            forgetFolder(gone.path)
        }
    }

    /// Deletes every snapshot of the server at `origin`.
    public func clear(origin: String) {
        let dir = root.appendingPathComponent(Self.serverKey(origin), isDirectory: true)
        try? FileManager.default.removeItem(at: dir)
        forgetFolder(dir.path)
    }

    /// What the store holds, in bytes.
    public func size() -> Int { loadIndex().reduce(0) { $0 + $1.size } }

    // MARK: Inside

    private func fileOf(_ origin: String, _ profileId: Int, _ key: String) -> URL {
        root.appendingPathComponent(Self.serverKey(origin), isDirectory: true)
            .appendingPathComponent(String(profileId), isDirectory: true)
            .appendingPathComponent(Self.fileName(key))
    }

    /// A key as a file name: letters, digits, `-` and `_` as they are,
    /// every other byte as `~xx`; long ones hashed.
    static func fileName(_ key: String) -> String {
        var name = ""
        for byte in key.utf8 {
            let c = Character(UnicodeScalar(byte))
            if byte < 0x80, c.isLetter || c.isNumber || c == "-" || c == "_" {
                name.append(c)
            } else {
                name += String(format: "~%02x", byte)
            }
        }
        if name.count > 120 {
            name = "~h" + SHA256.hash(data: Data(key.utf8)).map { String(format: "%02x", $0) }.joined()
        }
        return name + ".json"
    }

    private func modified(_ url: URL) -> Date {
        (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
    }

    private func loadIndex() -> [(path: String, size: Int)] {
        if let index { return index }
        var files: [(path: String, size: Int, at: Date)] = []
        let fm = FileManager.default
        // Relative names, joined to `root` as written: the enumerator may name a
        // folder through its real path (/private/var for /var), which wouldn't match.
        if let walk = fm.enumerator(atPath: root.path) {
            while let name = walk.nextObject() as? String {
                guard name.hasSuffix(".json") else { continue }
                let url = root.appendingPathComponent(name)
                let values = try? url.resourceValues(forKeys: [.fileSizeKey, .contentModificationDateKey,
                                                               .isRegularFileKey])
                guard values?.isRegularFile == true else { continue }
                files.append((url.path, values?.fileSize ?? 0, values?.contentModificationDate ?? .distantPast))
            }
        }
        let built = files.sorted { $0.at < $1.at }.map { (path: $0.path, size: $0.size) }
        index = built
        return built
    }

    /// `path` was read or written: last to go.
    private func touch(_ path: String, size: Int) {
        var all = loadIndex().filter { $0.path != path }
        all.append((path, size))
        index = all
    }

    private func forget(_ path: String) {
        index = loadIndex().filter { $0.path != path }
    }

    private func forgetFolder(_ dir: String) {
        let prefix = dir.hasSuffix("/") ? dir : dir + "/"
        index = loadIndex().filter { !$0.path.hasPrefix(prefix) }
    }

    /// Drops the least recently used files until the store fits, never `keeping`.
    private func evict(keeping: String) {
        var all = loadIndex()
        var total = all.reduce(0) { $0 + $1.size }
        var n = 0
        while total > maxBytes, n < all.count {
            if all[n].path == keeping {
                n += 1
                continue
            }
            try? FileManager.default.removeItem(atPath: all[n].path)
            total -= all[n].size
            all.remove(at: n)
        }
        index = all
    }
}
