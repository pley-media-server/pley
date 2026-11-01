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
/// kept in a cache bounded by bytes. An older iPad has little memory to spare.
public actor ImageLoader {
    /// The cache's bound, in bytes of decoded pixels.
    public static let cacheBytes = 48 << 20

    /// The types decoded; anything else is refused, whatever the server said.
    static let types: Set<String> = [UTType.jpeg.identifier, UTType.png.identifier, UTType.webP.identifier,
                                     UTType.gif.identifier]

    private let fetch: @Sendable (String, Int?) async throws -> Data
    private let cache = Cache()
    private var running: [String: Task<CGImage, any Error>] = [:]
    private nonisolated(unsafe) var observer: (any NSObjectProtocol)?

    /// Pictures of the server `api` talks to.
    public init(api: PleyAPI) {
        self.init { path, width in try await api.image(path: path, width: width) }
    }

    /// Pictures from `fetch` (path, width in pixels) → bytes.
    public init(fetch: @escaping @Sendable (String, Int?) async throws -> Data) {
        self.fetch = fetch
        #if os(iOS) || os(tvOS)
        let cache = self.cache
        observer = NotificationCenter.default.addObserver(
            forName: UIApplication.didReceiveMemoryWarningNotification, object: nil, queue: nil
        ) { _ in cache.empty() }
        #endif
    }

    deinit {
        if let observer { NotificationCenter.default.removeObserver(observer) }
    }

    /// The picture the server named at `path`, at most `widthPx` pixels
    /// wide. Requests for the same picture at once share one fetch.
    public func image(path: String, widthPx: Int) async throws -> CGImage {
        let width = max(1, widthPx)
        let key = "\(width) \(path)"
        if let hit = cache.get(key) { return hit }
        if let task = running[key] { return try await task.value }
        let fetch = self.fetch
        let task = Task<CGImage, any Error> {
            let data = try await fetch(path, width)
            return try Self.decode(data, maxWidth: width)
        }
        running[key] = task
        defer { running[key] = nil }
        let image = try await task.value
        cache.put(key, image)
        return image
    }

    /// Empties the cache.
    public func empty() {
        cache.empty()
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

/// `NSCache` bounded by the bytes of its pictures. NSCache is safe to use
/// from any thread.
private final class Cache: @unchecked Sendable {
    private let cache: NSCache<NSString, CGImage> = {
        let cache = NSCache<NSString, CGImage>()
        cache.totalCostLimit = ImageLoader.cacheBytes
        return cache
    }()

    func get(_ key: String) -> CGImage? { cache.object(forKey: key as NSString) }

    func put(_ key: String, _ image: CGImage) {
        cache.setObject(image, forKey: key as NSString, cost: image.bytesPerRow * image.height)
    }

    func empty() { cache.removeAllObjects() }
}
