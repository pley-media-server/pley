import CryptoKit
import Foundation

/// The Top Shelf's files in the app group's container, under `TopShelf/`:
///
/// - `snapshot.json`: the `ShelfSnapshot`, written whole (a temporary file
///   moved over the old one), so the extension reads the old row or the
///   new one, never half;
/// - the pictures it names, as the server sent them (JPEG, PNG or WebP);
/// - `link.key`: the 32 random bytes the links are signed with, made once.
///
/// The app writes; the extension reads. The group is the Info.plist's
/// `PleyAppGroup` (`group.<bundle ID>`); a build signed without App Groups
/// has no container, and then there is no store and no shelf.
public struct ShelfStore: Sendable {
    /// The Info.plist key naming the app group.
    public static let infoKey = "PleyAppGroup"
    /// A picture larger than this isn't kept.
    public static let maxImageBytes = 4 * 1024 * 1024
    /// A snapshot file larger than this isn't read.
    static let maxSnapshotBytes = 256 * 1024
    static let snapshotName = "snapshot.json"
    static let keyName = "link.key"

    /// The folder: `TopShelf/` in the container.
    public let directory: URL

    public init(directory: URL) {
        self.directory = directory
    }

    /// The store in the app group's container, `nil` when the build has no
    /// app group (signed without App Groups) or the system gives none.
    public static func appGroup(bundle: Bundle = .main) -> ShelfStore? {
        guard let group = bundle.object(forInfoDictionaryKey: infoKey) as? String, !group.isEmpty,
              !group.contains("$("),
              let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
        else { return nil }
        return ShelfStore(directory: container.appendingPathComponent("TopShelf", isDirectory: true))
    }

    // MARK: The snapshot

    /// The snapshot saved, `nil` when there is none or it can't be read.
    public func read() -> ShelfSnapshot? {
        let file = directory.appendingPathComponent(Self.snapshotName)
        guard let size = try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= Self.maxSnapshotBytes,
              let data = try? Data(contentsOf: file),
              let snapshot = try? JSONDecoder().decode(ShelfSnapshot.self, from: data)
        else { return nil }
        return ShelfSnapshot(owner: snapshot.owner, entries: snapshot.entries)
    }

    /// Saves `snapshot` with the new pictures in `images` (by file name).
    /// Pictures already saved that it names stay; the others go.
    public func write(_ snapshot: ShelfSnapshot, images: [String: Data] = [:]) throws {
        let files = FileManager.default
        try files.createDirectory(at: directory, withIntermediateDirectories: true)
        for (name, data) in images where Self.validImageName(name) && data.count <= Self.maxImageBytes {
            try data.write(to: directory.appendingPathComponent(name), options: [.atomic])
        }
        var kept = snapshot
        // A picture that isn't there (too large, not saved) isn't named.
        kept.entries = snapshot.entries.map { entry in
            var entry = entry
            if let name = entry.image, !Self.validImageName(name) || !hasImage(name) { entry.image = nil }
            return entry
        }
        let data = try JSONEncoder().encode(kept)
        try data.write(to: directory.appendingPathComponent(Self.snapshotName), options: [.atomic])
        removePictures(keeping: Set(kept.entries.compactMap(\.image)))
    }

    /// Empties the shelf: the snapshot and its pictures go; the link key stays.
    public func clear() {
        try? FileManager.default.removeItem(at: directory.appendingPathComponent(Self.snapshotName))
        removePictures(keeping: [])
    }

    // MARK: Pictures

    /// The file of the picture `name`, `nil` for a name the store doesn't make.
    public func imageURL(_ name: String) -> URL? {
        guard Self.validImageName(name) else { return nil }
        return directory.appendingPathComponent(name)
    }

    public func hasImage(_ name: String) -> Bool {
        guard let url = imageURL(name) else { return false }
        return FileManager.default.fileExists(atPath: url.path)
    }

    /// The name of a picture saved already from the server's `path`, if any.
    public func savedImage(for path: String) -> String? {
        let stem = Self.imageStem(path)
        return ImageType.allCases.map { "\(stem).\($0.rawValue)" }.first(where: hasImage)
    }

    /// The name to save `data`, read from the server's `path`, under: `nil`
    /// when it isn't a JPEG, PNG or WebP, or is too large.
    public static func imageName(path: String, data: Data) -> String? {
        guard data.count <= maxImageBytes, let type = ImageType(data) else { return nil }
        return "\(imageStem(path)).\(type.rawValue)"
    }

    static func imageStem(_ path: String) -> String {
        SHA256.hash(data: Data(path.utf8)).prefix(8).map { String(format: "%02x", $0) }.joined()
    }

    static func validImageName(_ name: String) -> Bool {
        let parts = name.split(separator: ".")
        guard parts.count == 2, parts[0].count == 16, parts[0].allSatisfy(\.isHexDigit),
              ImageType(rawValue: String(parts[1])) != nil
        else { return false }
        return true
    }

    private func removePictures(keeping: Set<String>) {
        let files = FileManager.default
        guard let names = try? files.contentsOfDirectory(atPath: directory.path) else { return }
        for name in names where Self.validImageName(name) && !keeping.contains(name) {
            try? files.removeItem(at: directory.appendingPathComponent(name))
        }
    }

    /// The picture types the shelf keeps, by their first bytes.
    enum ImageType: String, CaseIterable {
        case jpg, png, webp

        init?(_ data: Data) {
            let head = [UInt8](data.prefix(12))
            if head.starts(with: [0xFF, 0xD8, 0xFF]) {
                self = .jpg
            } else if head.starts(with: [0x89, 0x50, 0x4E, 0x47]) {
                self = .png
            } else if head.count == 12, head.starts(with: Array("RIFF".utf8)),
                      Array(head[8..<12]) == Array("WEBP".utf8) {
                self = .webp
            } else {
                return nil
            }
        }
    }

    // MARK: The link key

    /// The key the links are signed with. The app makes it once (`create`);
    /// the extension only reads it. `nil` when there is none and it isn't
    /// made, or it can't be saved.
    public func linkKey(create: Bool) -> SymmetricKey? {
        let file = directory.appendingPathComponent(Self.keyName)
        if let data = try? Data(contentsOf: file), data.count == 32 { return SymmetricKey(data: data) }
        guard create else { return nil }
        let key = SymmetricKey(size: .bits256)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try key.withUnsafeBytes { Data($0) }.write(to: file, options: [.atomic])
        } catch {
            return nil
        }
        return key
    }
}
