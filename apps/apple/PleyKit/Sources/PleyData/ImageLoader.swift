import CoreGraphics
import Foundation
import ImageIO
import PleyNetwork
import UniformTypeIdentifiers

#if os(iOS) || os(tvOS)
import UIKit
#endif

/// Why a picture wasn't shown.
public enum ImageError: Error, Sendable, Equatable {
    /// Not a JPEG, PNG, WebP or GIF, or not a picture at all.
    case notAPicture
}

/// The server's pictures, made small enough to show: fetched once at the
/// width asked, decoded to at most that many pixels wide (ImageIO's
/// thumbnails, so a large poster never sits in memory whole), and
/// kept in a cache bounded by bytes. An older iPad has little memory to spare:
///
/// - The cache's budget follows the device's memory (`budget`): 32 MiB on
///   a device with 3 GB or less (the iPad 5th generation has 2 GB).
/// - It is emptied on a memory warning and halved when the app goes to the
///   background (iPhone, iPad and Apple TV).
/// - Requests for one picture at once share one fetch, which is cancelled
///   when the last of them is: tiles scrolled past quickly don't queue
///   downloads nobody waits for.
/// - At most `maxFetches` fetches run at once; later ones wait, the newest
///   first, so the tiles on screen now come before those scrolled past.
public actor ImageLoader {
    /// Fetches running at once.
    public static let maxFetches = 6

    /// The cache's bound, in bytes of decoded pixels, on a device with
    /// `physicalMemory` bytes: 32 MiB up to 3 GB, else 64 MiB on Apple TV
    /// and 96 MiB on iPhone, iPad and Mac.
    public static func budget(physicalMemory: UInt64, appleTV: Bool) -> Int {
        if physicalMemory <= 3 << 30 { return 32 << 20 }
        return appleTV ? 64 << 20 : 96 << 20
    }

    /// This device's budget.
    public static var deviceBudget: Int {
        #if os(tvOS)
        budget(physicalMemory: ProcessInfo.processInfo.physicalMemory, appleTV: true)
        #else
        budget(physicalMemory: ProcessInfo.processInfo.physicalMemory, appleTV: false)
        #endif
    }

    /// The width in pixels a picture `points` wide asks for on a screen of
    /// `scale`: rounded up to a multiple of 64, so near sizes share one
    /// cache entry, and never more than `limit`.
    public static func pixels(points: Double, scale: Double, limit: Int = 4096) -> Int {
        let exact = max(1, points * max(scale, 1))
        return min(limit, Int((exact / 64).rounded(.up)) * 64)
    }

    /// The types decoded; anything else is refused, whatever the server said.
    static let types: Set<String> = [UTType.jpeg.identifier, UTType.png.identifier, UTType.webP.identifier,
                                     UTType.gif.identifier]

    /// One fetch and the requests waiting for it.
    private struct Flight {
        let id: Int
        let task: Task<CGImage, any Error>
        var waiters: Int
    }

    private let fetch: @Sendable (String, Int?) async throws -> Data
    private let cache: Cache
    private var flights: [String: Flight] = [:]
    private var flightCount = 0
    /// Fetches running, and those waiting for a turn, oldest first.
    private var active = 0
    private var queue: [(id: Int, turn: CheckedContinuation<Void, any Error>)] = []
    private var queueCount = 0
    private nonisolated(unsafe) var observers: [any NSObjectProtocol] = []

    /// Pictures of the server `api` talks to.
    public init(api: PleyAPI) {
        self.init { path, width in try await api.image(path: path, width: width) }
    }

    /// Pictures from `fetch` (path, width in pixels) → bytes, kept in a
    /// cache of `cacheBytes` (this device's budget by default).
    public init(cacheBytes: Int = ImageLoader.deviceBudget,
                fetch: @escaping @Sendable (String, Int?) async throws -> Data) {
        self.fetch = fetch
        let cache = Cache(limit: cacheBytes)
        self.cache = cache
        #if os(iOS) || os(tvOS)
        observers = [
            NotificationCenter.default.addObserver(
                forName: UIApplication.didReceiveMemoryWarningNotification, object: nil, queue: nil
            ) { _ in cache.empty() },
            NotificationCenter.default.addObserver(
                forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: nil
            ) { _ in cache.trim(to: cache.limit / 2) },
        ]
        #endif
    }

    deinit {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
    }

    /// The picture the server named at `path`, at most `widthPx` pixels
    /// wide. Requests for the same picture at once share one fetch; it is
    /// cancelled when every request for it is.
    public func image(path: String, widthPx: Int) async throws -> CGImage {
        let width = max(1, widthPx)
        let key = "\(width) \(path)"
        if let hit = cache.get(key) { return hit }
        let flight: Flight
        if var shared = flights[key] {
            shared.waiters += 1
            flights[key] = shared
            flight = shared
        } else {
            flightCount += 1
            let fetch = self.fetch
            let task = Task<CGImage, any Error> {
                try await self.turn()
                defer { self.done() }
                try Task.checkCancellation()
                let data = try await fetch(path, width)
                try Task.checkCancellation()
                return try Self.decode(data, maxWidth: width)
            }
            flight = Flight(id: flightCount, task: task, waiters: 1)
            flights[key] = flight
        }
        let id = flight.id
        do {
            let image = try await withTaskCancellationHandler {
                try await flight.task.value
            } onCancel: {
                Task { await self.leave(key, id: id) }
            }
            if flights[key]?.id == id { flights[key] = nil }
            cache.put(key, image)
            return image
        } catch {
            if flights[key]?.id == id { flights[key] = nil }
            throw error
        }
    }

    /// Empties the cache.
    public func empty() {
        cache.empty()
    }

    /// The cache's size in bytes, and its bound.
    public var cacheUsage: (bytes: Int, limit: Int) { (cache.bytes, cache.limit) }

    /// Fetches running now.
    public var running: Int { active }

    /// Keeps at most `bytes` of the pictures used last.
    public func trim(to bytes: Int) {
        cache.trim(to: bytes)
    }

    /// A request for `key` was cancelled: the fetch is too when nobody else waits.
    private func leave(_ key: String, id: Int) {
        guard var flight = flights[key], flight.id == id else { return }
        flight.waiters -= 1
        if flight.waiters <= 0 {
            flights[key] = nil
            flight.task.cancel()
        } else {
            flights[key] = flight
        }
    }

    /// Waits for one of the `maxFetches` turns; the newest waiter goes first.
    private func turn() async throws {
        if active < Self.maxFetches {
            active += 1
            return
        }
        queueCount += 1
        let id = queueCount
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (turn: CheckedContinuation<Void, any Error>) in
                queue.append((id, turn))
            }
        } onCancel: {
            Task { await self.giveUp(id) }
        }
    }

    /// A fetch waiting for its turn was cancelled.
    private func giveUp(_ id: Int) {
        guard let index = queue.firstIndex(where: { $0.id == id }) else { return }
        queue.remove(at: index).turn.resume(throwing: CancellationError())
    }

    /// A fetch ended: its turn goes to the newest waiting.
    private func done() {
        if let next = queue.popLast() {
            next.turn.resume()
        } else {
            active -= 1
        }
    }

    /// `data` decoded to at most `maxWidth` pixels wide, never larger than
    /// it is; only the four picture types.
    ///
    /// ImageIO's thumbnail size caps the longer side, so a tall poster asked
    /// for at its width would come out narrower than the tile it fills. The
    /// cap is worked out from the picture's own size so its width is the one
    /// limited.
    static func decode(_ data: Data, maxWidth: Int) throws -> CGImage {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithData(data as CFData, options),
              let type = CGImageSourceGetType(source) as String?, types.contains(type),
              CGImageSourceGetCount(source) > 0
        else { throw ImageError.notAPicture }
        let thumbnail = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: longerSide(source, maxWidth: maxWidth),
        ] as CFDictionary
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, thumbnail) else {
            throw ImageError.notAPicture
        }
        return image
    }

    /// The longer side a thumbnail of `source` may have so that it is at
    /// most `maxWidth` pixels wide, as shown (after its orientation), and no
    /// larger than the picture. `maxWidth` when the size isn't known.
    private static func longerSide(_ source: CGImageSource, maxWidth: Int) -> Int {
        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              var width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.intValue,
              var height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.intValue,
              width > 0, height > 0
        else { return maxWidth }
        // EXIF orientations 5 to 8 turn the picture a quarter: its width is shown as its height.
        if let orientation = (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue,
           (5...8).contains(orientation) {
            swap(&width, &height)
        }
        let longer = max(width, height)
        let scaled = (Double(maxWidth) * Double(longer) / Double(width)).rounded(.up)
        return min(longer, max(1, Int(scaled)))
    }
}

/// The pictures used last, bounded by their bytes: the one used longest
/// ago goes first. Safe to use from any thread (the notifications empty
/// and trim it from theirs).
private final class Cache: @unchecked Sendable {
    let limit: Int
    private let lock = NSLock()
    private var images: [String: CGImage] = [:]
    /// Keys, the one used longest ago first.
    private var order: [String] = []
    private var used = 0

    init(limit: Int) {
        self.limit = max(0, limit)
    }

    var bytes: Int { lock.withLock { used } }

    func get(_ key: String) -> CGImage? {
        lock.withLock {
            guard let image = images[key] else { return nil }
            touch(key)
            return image
        }
    }

    func put(_ key: String, _ image: CGImage) {
        let cost = image.bytesPerRow * image.height
        lock.withLock {
            if let old = images[key] {
                used -= old.bytesPerRow * old.height
                order.removeAll { $0 == key }
            }
            // A picture bigger than the whole cache isn't kept.
            guard cost <= limit else {
                images[key] = nil
                return
            }
            images[key] = image
            order.append(key)
            used += cost
            evict(to: limit)
        }
    }

    func trim(to bytes: Int) {
        lock.withLock { evict(to: max(0, bytes)) }
    }

    func empty() {
        lock.withLock {
            images = [:]
            order = []
            used = 0
        }
    }

    private func touch(_ key: String) {
        guard let index = order.lastIndex(of: key), index != order.count - 1 else { return }
        order.remove(at: index)
        order.append(key)
    }

    private func evict(to bytes: Int) {
        var drop = 0
        while used > bytes, drop < order.count {
            if let image = images.removeValue(forKey: order[drop]) {
                used -= image.bytesPerRow * image.height
            }
            drop += 1
        }
        order.removeFirst(drop)
    }
}
