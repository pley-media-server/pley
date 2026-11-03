import Foundation

/// Blu-ray subtitles (HDMV PGS, `S_HDMV/PGS`), a port of
/// packages/picture-subtitles' `pgs.ts`: each sample is one display set of
/// segments: a presentation composition (PCS) placing objects on the plane,
/// windows (WDS), palettes (PDS) and run-length coded objects (ODS). An
/// epoch start or acquisition point holds everything it needs; other
/// display sets may reuse palettes and objects defined since.
public final class PGSDecoder: PictureDecoder {
    static let pds = 0x14
    static let ods = 0x15
    static let pcs = 0x16
    static let wds = 0x17
    static let end = 0x80
    /// A picture's side at most, in pixels: subtitles are drawn for 4K
    /// screens at most, and a file's sizes are outside input.
    static let maxSide = 4096

    struct Composition: Equatable {
        struct Placed: Equatable {
            var id: Int
            var x: Int
            var y: Int
            var crop: (x: Int, y: Int, w: Int, h: Int)?

            static func == (a: Placed, b: Placed) -> Bool {
                a.id == b.id && a.x == b.x && a.y == b.y && a.crop.map { [$0.x, $0.y, $0.w, $0.h] }
                    == b.crop.map { [$0.x, $0.y, $0.w, $0.h] }
            }
        }

        var width: Int
        var height: Int
        /// 0x00 normal, 0x40 acquisition point, 0x80 epoch start.
        var state: Int
        var paletteId: Int
        var objects: [Placed]
    }

    struct PaletteEntry: Equatable {
        var index: Int
        var y: Int
        var cr: Int
        var cb: Int
        var a: Int
    }

    struct ObjectPart: Equatable {
        var id: Int
        var first: Bool
        var last: Bool
        var width: Int
        var height: Int
        var data: [UInt8]
    }

    struct DisplaySet: Equatable {
        var pcs: Composition?
        var palettes: [(id: Int, entries: [PaletteEntry])] = []
        var objects: [ObjectPart] = []

        static func == (a: DisplaySet, b: DisplaySet) -> Bool {
            a.pcs == b.pcs && a.objects == b.objects && a.palettes.map(\.id) == b.palettes.map(\.id)
                && a.palettes.map(\.entries) == b.palettes.map(\.entries)
        }
    }

    private let samples: [[UInt8]]
    private var sets: [DisplaySet?]
    private var parsedCount = 0
    private var cache = FrameCache()

    public init(samples: [[UInt8]]) {
        self.samples = samples
        sets = Array(repeating: nil, count: samples.count)
    }

    /// The segments of one display set.
    static func parseDisplaySet(_ data: [UInt8]) -> DisplaySet {
        var set = DisplaySet()
        var at = 0
        var segments = 0
        while at + 3 <= data.count, segments < Pictures.maxSegments {
            segments += 1
            // Some muxers keep the "PG" header of the .sup format: 2 magic bytes, PTS and DTS.
            if data.at(at) == 0x50 && data.at(at + 1) == 0x47 { at += 10 }
            let type = data.at(at)
            let size = data.u16(at + 1)
            let from = min(at + 3, data.count)
            let body = Array(data[from..<min(data.count, max(from, at + 3 + size))])
            at += 3 + size
            switch type {
            case pcs:
                set.pcs = readPcs(body)
            case pds:
                guard body.count >= 2 else { break }
                var entries: [PaletteEntry] = []
                var i = 2
                while i + 5 <= body.count, entries.count < 256 {
                    entries.append(PaletteEntry(index: body.at(i), y: body.at(i + 1), cr: body.at(i + 2),
                                                cb: body.at(i + 3), a: body.at(i + 4)))
                    i += 5
                }
                set.palettes.append((body.at(0), entries))
            case ods:
                guard body.count >= 4 else { break }
                let id = body.u16(0)
                let flags = body.at(3)
                let first = flags & 0x80 != 0
                let last = flags & 0x40 != 0
                if first {
                    guard body.count >= 11 else { break }
                    set.objects.append(ObjectPart(id: id, first: first, last: last, width: body.u16(7),
                                                  height: body.u16(9), data: Array(body[11...])))
                } else {
                    set.objects.append(ObjectPart(id: id, first: first, last: last, width: 0, height: 0,
                                                  data: Array(body[4...])))
                }
            default:
                break
            }
            if type == end { break }
        }
        return set
    }

    private static func readPcs(_ b: [UInt8]) -> Composition? {
        guard b.count >= 11 else { return nil }
        var objects: [Composition.Placed] = []
        var at = 11
        var i = 0
        while i < b.at(10), at + 8 <= b.count {
            // 0x80 is the cropped flag; 0x40 is forced, which says nothing of the crop.
            let cropped = b.at(at + 3) & 0x80 != 0
            var placed = Composition.Placed(id: b.u16(at), x: b.u16(at + 4), y: b.u16(at + 6))
            at += 8
            if cropped, at + 8 <= b.count {
                placed.crop = (b.u16(at), b.u16(at + 2), b.u16(at + 4), b.u16(at + 6))
                at += 8
            }
            objects.append(placed)
            i += 1
        }
        return Composition(width: b.u16(0), height: b.u16(2), state: b.at(7), paletteId: b.at(9), objects: objects)
    }

    /// Decodes PGS run-length data into palette indexes, `width × height`.
    public static func decodeRle(_ rle: [UInt8], width: Int, height: Int) -> [UInt8] {
        var out = [UInt8](repeating: 0, count: max(width * height, 0))
        var x = 0
        var y = 0
        var i = 0
        while i < rle.count && y < height {
            let b = rle[i]
            i += 1
            var color: UInt8 = 0
            var count = 1
            if b != 0 {
                color = b
            } else {
                let flags = rle.at(i)
                i += 1
                if flags == 0 {
                    // End of line.
                    x = 0
                    y += 1
                    continue
                }
                if flags & 0x40 != 0 {
                    count = (flags & 0x3F) << 8 | rle.at(i)
                    i += 1
                } else {
                    count = flags & 0x3F
                }
                if flags & 0x80 != 0 {
                    color = UInt8(rle.at(i))
                    i += 1
                }
            }
            let row = y * width
            let end = min(width, x + count)
            if color != 0 && x < end {
                for px in x..<end { out[row + px] = color }
            }
            x = max(end, x)
        }
        return out
    }

    public func ownDurationMs(_ index: Int) -> Int? { nil }

    public func decode(_ index: Int) -> PictureFrame? {
        guard samples.indices.contains(index) else { return nil }
        if let kept = cache.get(index) { return kept }
        let frame = compose(index)
        cache.put(index, frame)
        return frame
    }

    private func set(_ index: Int) -> DisplaySet {
        if let s = sets[index] { return s }
        let s = Self.parseDisplaySet(samples[index])
        if parsedCount >= Pictures.maxParsed {
            sets = Array(repeating: nil, count: sets.count)
            parsedCount = 0
        }
        sets[index] = s
        parsedCount += 1
        return s
    }

    private func compose(_ index: Int) -> PictureFrame? {
        guard let pcs = set(index).pcs else { return nil }
        if pcs.objects.isEmpty { return PictureFrame(planeWidth: pcs.width, planeHeight: pcs.height, bitmaps: []) }
        // Palettes and objects as they stand at `index`: from the last epoch start or acquisition point.
        var from = index
        while from > 0, index - from < Pictures.maxLookBack {
            if (set(from).pcs?.state ?? 0) & 0xC0 != 0 { break }
            from -= 1
        }
        var palettes: [Int: [Int: PaletteEntry]] = [:]
        var objects: [Int: (width: Int, height: Int, rle: [UInt8])] = [:]
        var pending: [Int: (width: Int, height: Int, parts: [UInt8])] = [:]
        for i in from...index {
            let s = set(i)
            for p in s.palettes {
                for e in p.entries { palettes[p.id, default: [:]][e.index] = e }
            }
            for o in s.objects {
                if o.first { pending[o.id] = (o.width, o.height, []) }
                guard pending[o.id] != nil else { continue }
                pending[o.id]?.parts += o.data
                if o.last, let part = pending.removeValue(forKey: o.id) {
                    objects[o.id] = (part.width, part.height, part.parts)
                }
            }
        }
        guard let palette = palettes[pcs.paletteId] else { return nil }
        let hd = pcs.height > 576
        var lut = [UInt8](repeating: 0, count: 256 * 4)
        for (i, e) in palette where i < 256 {
            let (r, g, b) = Pictures.rgb(y: e.y, cb: e.cb, cr: e.cr, hd: hd)
            lut[i * 4] = r
            lut[i * 4 + 1] = g
            lut[i * 4 + 2] = b
            lut[i * 4 + 3] = UInt8(e.a)
        }
        var bitmaps: [SubtitleBitmap] = []
        var pixelsLeft = Pictures.maxPixels
        for placed in pcs.objects {
            guard let object = objects[placed.id], (1...Self.maxSide).contains(object.width),
                  (1...Self.maxSide).contains(object.height) else { continue }
            guard bitmaps.count < Pictures.maxPictures else { break }
            pixelsLeft -= object.width * object.height
            guard pixelsLeft >= 0 else { break }
            let indexes = Self.decodeRle(object.rle, width: object.width, height: object.height)
            let crop = placed.crop ?? (0, 0, object.width, object.height)
            let w = max(0, min(crop.w, object.width - crop.x))
            let h = max(0, min(crop.h, object.height - crop.y))
            guard w > 0, h > 0 else { continue }
            var pixels = [UInt8](repeating: 0, count: w * h * 4)
            for row in 0..<h {
                let src = (crop.y + row) * object.width + crop.x
                let dst = row * w * 4
                for col in 0..<w {
                    let c = Int(indexes[src + col]) * 4
                    let o = dst + col * 4
                    pixels[o] = lut[c]
                    pixels[o + 1] = lut[c + 1]
                    pixels[o + 2] = lut[c + 2]
                    pixels[o + 3] = lut[c + 3]
                }
            }
            bitmaps.append(SubtitleBitmap(x: placed.x, y: placed.y, width: w, height: h, rgba: pixels))
        }
        return PictureFrame(planeWidth: pcs.width, planeHeight: pcs.height, bitmaps: bitmaps)
    }
}
