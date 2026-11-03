import Foundation

// What picture subtitles become once decoded (a Swift port of
// packages/picture-subtitles, `pictures.ts`): bitmaps placed on the
// subtitle plane (1920×1080 for a Blu-ray's PGS, 720×576 for a DVD's
// VobSub), drawn over the video at the same fractions.

/// One picture on the plane.
public struct SubtitleBitmap: Equatable, Sendable {
    public var x: Int
    public var y: Int
    public var width: Int
    public var height: Int
    /// RGBA, `width × height × 4` bytes, not premultiplied.
    public var rgba: [UInt8]

    public init(x: Int, y: Int, width: Int, height: Int, rgba: [UInt8]) {
        self.x = x
        self.y = y
        self.width = width
        self.height = height
        self.rgba = rgba
    }
}

/// What a picture track shows at one moment.
public struct PictureFrame: Equatable, Sendable {
    /// The plane the bitmaps are placed on.
    public var planeWidth: Int
    public var planeHeight: Int
    /// Empty: nothing shows, the screen clears.
    public var bitmaps: [SubtitleBitmap]

    public init(planeWidth: Int, planeHeight: Int, bitmaps: [SubtitleBitmap]) {
        self.planeWidth = planeWidth
        self.planeHeight = planeHeight
        self.bitmaps = bitmaps
    }
}

/// A decoder for one track's samples, which may need earlier samples to decode a later one.
public protocol PictureDecoder: AnyObject {
    /// The frame sample `index` shows; `nil` when it can't be decoded.
    func decode(_ index: Int) -> PictureFrame?
    /// How long sample `index` shows when the sample itself says (VobSub's
    /// stop command, DVB's page time-out), else `nil`: until the next sample.
    func ownDurationMs(_ index: Int) -> Int?
}

public enum Pictures {
    /// Limited-range YCbCr to RGB: BT.709 for HD planes, BT.601 for SD (as FFmpeg's decoders).
    public static func rgb(y: Int, cb: Int, cr: Int, hd: Bool) -> (UInt8, UInt8, UInt8) {
        let yy = 1.164383 * Double(y - 16)
        let u = Double(cb - 128)
        let v = Double(cr - 128)
        let r, g, b: Double
        if hd {
            r = yy + 1.792741 * v
            g = yy - 0.213249 * u - 0.532909 * v
            b = yy + 2.112402 * u
        } else {
            r = yy + 1.596027 * v
            g = yy - 0.391762 * u - 0.812968 * v
            b = yy + 2.017232 * u
        }
        return (round(r), round(g), round(b))
    }

    /// `x` kept within 0…255 and rounded half up, as JavaScript's `Math.round`.
    static func round(_ x: Double) -> UInt8 {
        x <= 0 ? 0 : x >= 255 ? 255 : UInt8((x + 0.5).rounded(.down))
    }

    /// `x` as a `Uint8ClampedArray` stores it: within 0…255, rounded half to even.
    static func clamped(_ x: Double) -> UInt8 {
        x <= 0 ? 0 : x >= 255 ? 255 : UInt8(x.rounded(.toNearestOrEven))
    }

    /// Decoded frames kept per track: a few around the playhead are enough,
    /// as seeking decodes again.
    static let cacheSize = 24

    /// The most pixels a frame decodes, all its pictures together: a 4K
    /// screen's worth. A file can place a 4096² picture many times over, and
    /// each pixel costs 4 bytes once coloured.
    static let maxPixels = 3_840 * 2_160

    /// The most bytes the frames kept per track hold together.
    static let cacheBytes = 128 << 20

    /// Pictures placed at one moment, at most: a Blu-ray places two, a DVB
    /// page a few. Each one is drawn on its own.
    static let maxPictures = 64

    /// Segments read from one sample: a real display set has tens.
    static let maxSegments = 512

    /// Samples a decode reads back to its epoch's start or acquisition
    /// point, at most: real tracks have one every few samples, and a long
    /// animated epoch (karaoke) a few hundred.
    static let maxLookBack = 512

    /// The bytes of the samples a decode reads back, at most: real samples
    /// are tens of KB, and parsing runs on the main actor.
    static let maxLookBackBytes = 16 << 20

    /// Parsed samples kept for the next decodes, at most: more than one
    /// look-back, which would otherwise parse again each time.
    static let maxParsed = 1_024

    /// The bytes of the parsed samples kept, at most: a parsed sample holds
    /// about its own size again.
    static let maxParsedBytes = 64 << 20

    /// DVB object draws in one decode, at most: a page draws a few. Each
    /// walks the object's data, on the main actor.
    static let maxDraws = 256

    /// The work one DVB decode does, at most, on the main actor: each pixel
    /// of a region made or filled, and 32 for each byte of pixel codes drawn
    /// (walking codes costs far more than filling). Eight 4K screens.
    static let maxWork = 8 * maxPixels

    /// Samples read from a track's file, at most: a film's has a few thousand.
    static let maxSamples = 100_000
}

/// The few frames decoded last, oldest dropped first.
struct FrameCache {
    private var order: [Int] = []
    private var frames: [Int: PictureFrame?] = [:]
    private var bytes = 0

    /// The frame of `index` (maybe `nil`, kept as such), or `.none` when not decoded yet.
    func get(_ index: Int) -> PictureFrame?? { frames[index] }

    mutating func put(_ index: Int, _ frame: PictureFrame?) {
        if let kept = frames[index] {
            bytes -= Self.size(kept)
            order.removeAll { $0 == index }
        }
        order.append(index)
        frames[index] = .some(frame)
        bytes += Self.size(frame)
        // The frame just decoded stays, whatever its size.
        while order.count > 1, order.count > Pictures.cacheSize + 1 || bytes > Pictures.cacheBytes {
            if let dropped = frames.removeValue(forKey: order.removeFirst()) { bytes -= Self.size(dropped) }
        }
    }

    private static func size(_ frame: PictureFrame?) -> Int {
        frame?.bitmaps.reduce(0) { $0 + $1.rgba.count } ?? 0
    }
}

/// Big-endian numbers out of a byte array, 0 past its end.
extension Array where Element == UInt8 {
    @inline(__always) func at(_ i: Int) -> Int { i >= 0 && i < count ? Int(self[i]) : 0 }
    @inline(__always) func u16(_ i: Int) -> Int { at(i) << 8 | at(i + 1) }
}
