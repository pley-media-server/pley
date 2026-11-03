import Foundation

/// Broadcast subtitles (DVB, ETSI EN 300 743, `S_DVBSUB`), a port of
/// packages/picture-subtitles' `dvb.ts`: each sample is a display set of
/// segments. A page composition places regions on the display; each region
/// has a colour table (CLUT) and objects drawn into it as 2, 4 or 8-bit
/// run-length coded pixels, a field at a time. A page in "normal case"
/// only updates what an acquisition point or mode change defined, so a
/// sample is decoded from the last of those.
public final class DVBDecoder: PictureDecoder {
    static let page = 0x10
    static let region = 0x11
    static let clut = 0x12
    static let object = 0x13
    static let display = 0x14
    /// A region's side at most, in pixels: subtitles are drawn for 4K
    /// screens at most, and a file's sizes are outside input.
    static let maxSide = 4096

    public struct Segment: Equatable, Sendable {
        public var type: Int
        public var body: [UInt8]
    }

    struct Page {
        var timeoutS: Int
        /// 0 normal case, 1 acquisition point, 2 mode change.
        var state: Int
        var regions: [(id: Int, x: Int, y: Int)]
    }

    final class Region {
        var width: Int
        var height: Int
        /// 2, 4 or 8 bits a pixel.
        var depth: Int
        var clut: Int
        var fill: Bool
        var background: UInt8
        var objects: [(id: Int, x: Int, y: Int)]
        var pixels: [UInt8]

        init(width: Int, height: Int, depth: Int, clut: Int, fill: Bool, background: UInt8,
             objects: [(id: Int, x: Int, y: Int)]) {
            self.width = width
            self.height = height
            self.depth = depth
            self.clut = clut
            self.fill = fill
            self.background = background
            self.objects = objects
            pixels = [UInt8](repeating: 0, count: width * height)
        }
    }

    private let samples: [[UInt8]]
    private var parsed: [[Segment]?]
    private var parsedCount = 0
    private var parsedBytes = 0
    private var cache = FrameCache()

    public init(samples: [[UInt8]]) {
        self.samples = samples
        parsed = Array(repeating: nil, count: samples.count)
    }

    /// A display set's segments.
    public static func segments(_ data: [UInt8]) -> [Segment] {
        var out: [Segment] = []
        var at = 0
        // data_identifier 0x20 and subtitle_stream_id 0x00 open a PES data field.
        if data.at(0) == 0x20 && data.count > 1 && data[1] == 0x00 { at = 2 }
        while at + 6 <= data.count && data[at] == 0x0F && out.count < Pictures.maxSegments {
            let type = data.at(at + 1)
            let length = data.u16(at + 4)
            out.append(Segment(type: type, body: Array(data[(at + 6)..<min(data.count, at + 6 + length)])))
            at += 6 + length
        }
        return out
    }

    private static func readPage(_ b: [UInt8]) -> Page {
        var regions: [(id: Int, x: Int, y: Int)] = []
        var i = 2
        while i + 6 <= b.count, regions.count < Pictures.maxPictures {
            regions.append((b.at(i), b.u16(i + 2), b.u16(i + 4)))
            i += 6
        }
        return Page(timeoutS: b.at(0), state: (b.at(1) >> 2) & 3, regions: regions)
    }

    /// The colour tables of ETSI EN 300 743 §10 for pixels a CLUT doesn't define.
    static func defaultClut(_ depth: Int) -> [UInt8] {
        let n = 1 << depth
        var out = [UInt8](repeating: 0, count: n * 4)
        func set(_ i: Int, _ r: Double, _ g: Double, _ b: Double, _ a: Double) {
            out[i * 4] = Pictures.clamped(r)
            out[i * 4 + 1] = Pictures.clamped(g)
            out[i * 4 + 2] = Pictures.clamped(b)
            out[i * 4 + 3] = Pictures.clamped(a)
        }
        if depth == 2 {
            set(0, 0, 0, 0, 0)
            set(1, 255, 255, 255, 255)
            set(2, 0, 0, 0, 255)
            set(3, 127, 127, 127, 255)
            return out
        }
        if depth == 4 {
            for i in 0..<16 {
                let v: Double = i < 8 ? 255 : 127
                set(i, i & 1 != 0 ? v : 0, i & 2 != 0 ? v : 0, i & 4 != 0 ? v : 0, i == 0 ? 0 : 255)
            }
            return out
        }
        for i in 0..<256 {
            let r: Double = i & 0x01 != 0 ? 85 : 0
            let g: Double = i & 0x02 != 0 ? 85 : 0
            let b: Double = i & 0x04 != 0 ? 85 : 0
            let hi = i & 0x88
            let r1: Double = i & 0x10 != 0 ? 1 : 0
            let g1: Double = i & 0x20 != 0 ? 1 : 0
            let b1: Double = i & 0x40 != 0 ? 1 : 0
            if i < 8 {
                set(i, r * 3, g * 3, b * 3, i == 0 ? 0 : 64)
            } else if hi == 0x00 {
                set(i, r + r1 * 170, g + g1 * 170, b + b1 * 170, 255)
            } else if hi == 0x08 {
                set(i, r + r1 * 170, g + g1 * 170, b + b1 * 170, 127)
            } else if hi == 0x80 {
                set(i, 127 + r / 2 + r1 * 43, 127 + g / 2 + g1 * 43, 127 + b / 2 + b1 * 43, 255)
            } else {
                set(i, r / 2 + r1 * 43, g / 2 + g1 * 43, b / 2 + b1 * 43, 255)
            }
        }
        return out
    }

    /// Bits read from the front, most significant first; 0 past the end.
    struct Bits {
        private var bit = 0
        private let data: [UInt8]
        private var at: Int
        private let end: Int

        init(_ data: [UInt8], at: Int, end: Int) {
            self.data = data
            self.at = at
            self.end = end
        }

        var done: Bool { at >= end }

        mutating func read(_ n: Int) -> Int {
            var v = 0
            for _ in 0..<n {
                let byte = at < end ? Int(data[at]) : 0
                v = v << 1 | (byte >> (7 - bit)) & 1
                bit += 1
                if bit == 8 {
                    bit = 0
                    at += 1
                }
            }
            return v
        }

        /// The byte after the string: strings end on a byte.
        mutating func align() -> Int {
            if bit != 0 {
                bit = 0
                at += 1
            }
            return at
        }
    }

    /// Runs of one pixel string: `(count, pixel)` pairs, until its end code.
    static func pixelString(_ bits: inout Bits, depth: Int, _ emit: (Int, Int) -> Void) {
        while !bits.done {
            if depth == 2 {
                let v = bits.read(2)
                if v != 0 {
                    emit(1, v)
                } else if bits.read(1) != 0 {
                    let run = bits.read(3) + 3
                    emit(run, bits.read(2))
                } else if bits.read(1) != 0 {
                    emit(1, 0)
                } else {
                    let s = bits.read(2)
                    if s == 0 { return }
                    if s == 1 {
                        emit(2, 0)
                    } else if s == 2 {
                        let run = bits.read(4) + 12
                        emit(run, bits.read(2))
                    } else {
                        let run = bits.read(8) + 29
                        emit(run, bits.read(2))
                    }
                }
            } else if depth == 4 {
                let v = bits.read(4)
                if v != 0 {
                    emit(1, v)
                } else if bits.read(1) == 0 {
                    let run = bits.read(3)
                    if run == 0 { return }
                    emit(run + 2, 0)
                } else if bits.read(1) == 0 {
                    let run = bits.read(2) + 4
                    emit(run, bits.read(4))
                } else {
                    let s = bits.read(2)
                    if s == 0 {
                        emit(1, 0)
                    } else if s == 1 {
                        emit(2, 0)
                    } else if s == 2 {
                        let run = bits.read(4) + 9
                        emit(run, bits.read(4))
                    } else {
                        let run = bits.read(8) + 25
                        emit(run, bits.read(4))
                    }
                }
            } else {
                let v = bits.read(8)
                if v != 0 {
                    emit(1, v)
                } else if bits.read(1) == 0 {
                    let run = bits.read(7)
                    if run == 0 { return }
                    emit(run, 0)
                } else {
                    let run = bits.read(7)
                    emit(run, bits.read(8))
                }
            }
        }
    }

    /// Draws one field of an object's pixel data into `region` at (`ox`, `oy`), every other line from `first`.
    static func drawField(_ data: [UInt8], into region: Region, ox: Int, oy: Int, first: Int) {
        var at = 0
        var x = ox
        var y = oy + first
        // Map tables, for codes shallower than the region.
        var map24 = [0x0, 0x7, 0x8, 0xF]
        var map28 = [0x00, 0x77, 0x88, 0xFF]
        var map48 = (0..<16).map { $0 * 0x11 }
        // Lines past the region's bottom draw nothing: the rest isn't read.
        while at < data.count && y < region.height {
            let type = Int(data[at])
            at += 1
            switch type {
            case 0xF0:
                x = ox
                y += 2
                continue
            case 0x20:
                map24 = [data.at(at) >> 4, data.at(at) & 0xF, data.at(at + 1) >> 4, data.at(at + 1) & 0xF]
                at += 2
                continue
            case 0x21:
                map28 = [data.at(at), data.at(at + 1), data.at(at + 2), data.at(at + 3)]
                at += 4
                continue
            case 0x22:
                map48 = []
                for i in 0..<8 { map48 += [data.at(at + i) >> 4, data.at(at + i) & 0xF] }
                at += 8
                continue
            default:
                break
            }
            let depth = type == 0x10 ? 2 : type == 0x11 ? 4 : type == 0x12 ? 8 : 0
            if depth == 0 { break }
            var bits = Bits(data, at: at, end: data.count)
            let (m24, m28, m48) = (map24, map28, map48)
            func map(_ p: Int) -> Int {
                if depth == region.depth { return p }
                if depth == 2 { return region.depth == 4 ? m24[p & 3] : m28[p & 3] }
                if depth == 4 && region.depth == 8 { return m48[p & 15] }
                // Deeper codes than the region: keep the low bits.
                return p & ((1 << region.depth) - 1)
            }
            pixelString(&bits, depth: depth) { count, pixel in
                if y >= 0 && y < region.height {
                    let stop = min(region.width, x + count)
                    let value = UInt8(truncatingIfNeeded: map(pixel))
                    let row = y * region.width
                    let from = max(0, x)
                    if from < stop {
                        for px in from..<stop { region.pixels[row + px] = value }
                    }
                }
                x += count
            }
            at = bits.align()
        }
    }

    private func segs(_ index: Int) -> [Segment] {
        if let s = parsed[index] { return s }
        let s = Self.segments(samples[index])
        if parsedCount >= Pictures.maxParsed || parsedBytes + samples[index].count > Pictures.maxParsedBytes {
            parsed = Array(repeating: nil, count: parsed.count)
            parsedCount = 0
            parsedBytes = 0
        }
        parsed[index] = s
        parsedCount += 1
        parsedBytes += samples[index].count
        return s
    }

    private func page(_ index: Int) -> Page? {
        segs(index).first { $0.type == Self.page }.map { Self.readPage($0.body) }
    }

    public func ownDurationMs(_ index: Int) -> Int? {
        guard samples.indices.contains(index), let page = page(index), page.timeoutS > 0 else { return nil }
        return page.timeoutS * 1000
    }

    public func decode(_ index: Int) -> PictureFrame? {
        guard samples.indices.contains(index) else { return nil }
        if let kept = cache.get(index) { return kept }
        let frame = render(index)
        cache.put(index, frame)
        return frame
    }

    private static func tables() -> [[UInt8]] { [defaultClut(2), defaultClut(4), defaultClut(8)] }

    private func render(_ index: Int) -> PictureFrame? {
        var from = index
        var walked = samples[index].count
        while from > 0, index - from < Pictures.maxLookBack, walked < Pictures.maxLookBackBytes {
            if let page = page(from), page.state != 0 { break }
            from -= 1
            walked += samples[from].count
        }
        var width = 720
        var height = 576
        var page: Page?
        var regions: [Int: Region] = [:]
        var cluts: [Int: [[UInt8]]] = [:]
        var draws = 0
        var work = 0
        pass: for i in from...index {
            for seg in segs(i) {
                let b = seg.body
                work += 8 * (6 + b.count)
                if work > Pictures.maxWork { break pass }
                switch seg.type {
                case Self.display:
                    if b.count >= 5 {
                        width = b.u16(1) + 1
                        height = b.u16(3) + 1
                    }
                case Self.page:
                    let read = Self.readPage(b)
                    page = read
                    if read.state != 0 { regions.removeAll() }
                case Self.region:
                    guard b.count >= 10 else { break }
                    let id = b.at(0)
                    let fill = b.at(1) & 0x08 != 0
                    let w = b.u16(2)
                    let h = b.u16(4)
                    guard w <= Self.maxSide, h <= Self.maxSide else { break }
                    // Up to 256 regions are kept at once: together, no more than a frame's pixels.
                    let others = regions.reduce(0) { $0 + ($1.key == id ? 0 : $1.value.pixels.count) }
                    guard others + w * h <= Pictures.maxPixels else { break }
                    let depths = [0, 2, 4, 8]
                    let code = (b.at(6) >> 2) & 7
                    let depth = code < depths.count ? depths[code] : 4
                    let clut = b.at(7)
                    let background = UInt8(depth == 8 ? b.at(8) : depth == 4 ? b.at(9) >> 4 : (b.at(9) >> 2) & 3)
                    var objects: [(id: Int, x: Int, y: Int)] = []
                    var o = 10
                    // Each object is drawn once per entry: a few, not thousands.
                    while o + 6 <= b.count, objects.count < Pictures.maxPictures {
                        let objectType = b.at(o + 2) >> 6
                        objects.append((b.u16(o), (b.at(o + 2) & 0x0F) << 8 | b.at(o + 3),
                                        (b.at(o + 4) & 0x0F) << 8 | b.at(o + 5)))
                        o += objectType == 1 || objectType == 2 ? 8 : 6
                    }
                    // Making or filling a region is work, each time a segment asks.
                    let reused = regions[id].map { $0.width == w && $0.height == h && $0.depth == depth } ?? false
                    if !reused || fill {
                        work += w * h
                        if work > Pictures.maxWork { break pass }
                    }
                    let region: Region
                    if let kept = regions[id], kept.width == w, kept.height == h, kept.depth == depth {
                        region = kept
                        region.clut = clut
                        region.fill = fill
                        region.background = background
                        region.objects = objects
                    } else {
                        region = Region(width: w, height: h, depth: depth, clut: clut, fill: fill,
                                        background: background, objects: objects)
                        regions[id] = region
                    }
                    if fill { region.pixels = [UInt8](repeating: background, count: region.pixels.count) }
                case Self.clut:
                    guard b.count >= 2 else { break }
                    let id = b.at(0)
                    var tables = cluts[id] ?? Self.tables()
                    var at = 2
                    while at + 2 <= b.count {
                        let entry = b.at(at)
                        let flags = b.at(at + 1)
                        let y, cr, cb, t: Int
                        if flags & 0x01 != 0 {
                            if at + 6 > b.count { break }
                            (y, cr, cb, t) = (b.at(at + 2), b.at(at + 3), b.at(at + 4), b.at(at + 5))
                            at += 6
                        } else {
                            if at + 4 > b.count { break }
                            y = b.at(at + 2) & 0xFC
                            cr = (b.at(at + 2) & 0x03) << 6 | (b.at(at + 3) >> 2) & 0x30
                            cb = (b.at(at + 3) << 2) & 0xF0
                            t = (b.at(at + 3) & 0x03) * 0x55
                            at += 4
                        }
                        let (r, g, bl) = Pictures.rgb(y: y, cb: cb, cr: cr, hd: height > 576)
                        // Y 0 is transparent whatever T says.
                        let alpha = UInt8(clamping: y == 0 ? 0 : 255 - t)
                        for (table, flag) in [(0, 0x80), (1, 0x40), (2, 0x20)] where flags & flag != 0 {
                            guard entry * 4 + 3 < tables[table].count else { continue }
                            tables[table][entry * 4] = r
                            tables[table][entry * 4 + 1] = g
                            tables[table][entry * 4 + 2] = bl
                            tables[table][entry * 4 + 3] = alpha
                        }
                    }
                    cluts[id] = tables
                case Self.object:
                    guard b.count >= 7 else { break }
                    let id = b.u16(0)
                    let coding = (b.at(2) >> 2) & 3
                    guard coding == 0 else { break }
                    let topLength = b.u16(3)
                    let bottomLength = b.u16(5)
                    let top = Array(b[7..<min(b.count, 7 + topLength)])
                    let bottomStart = min(b.count, 7 + topLength)
                    let bottom = bottomLength > 0
                        ? Array(b[bottomStart..<min(b.count, bottomStart + bottomLength)]) : top
                    for region in regions.values {
                        for o in region.objects where o.id == id && draws < Pictures.maxDraws {
                            draws += 1
                            // A draw walks its fields at most.
                            work += 32 * (top.count + bottom.count)
                            if work > Pictures.maxWork { break pass }
                            Self.drawField(top, into: region, ox: o.x, oy: o.y, first: 0)
                            Self.drawField(bottom, into: region, ox: o.x, oy: o.y, first: 1)
                        }
                    }
                default:
                    break
                }
            }
        }
        guard let page else { return nil }
        var bitmaps: [SubtitleBitmap] = []
        // A page may place a region any number of times, each a picture of its own.
        var pixelsLeft = Pictures.maxPixels
        for placed in page.regions {
            guard let region = regions[placed.id], region.width > 0, region.height > 0 else { continue }
            pixelsLeft -= region.width * region.height
            guard pixelsLeft >= 0 else { break }
            let table = (cluts[region.clut] ?? Self.tables())[region.depth == 2 ? 0 : region.depth == 4 ? 1 : 2]
            var rgba = [UInt8](repeating: 0, count: region.width * region.height * 4)
            var visible = false
            for i in 0..<region.pixels.count {
                let c = Int(region.pixels[i]) * 4
                guard c + 3 < table.count else { continue }
                let o = i * 4
                rgba[o] = table[c]
                rgba[o + 1] = table[c + 1]
                rgba[o + 2] = table[c + 2]
                rgba[o + 3] = table[c + 3]
                if table[c + 3] != 0 { visible = true }
            }
            if visible {
                bitmaps.append(SubtitleBitmap(x: placed.x, y: placed.y, width: region.width, height: region.height,
                                              rgba: rgba))
            }
        }
        return PictureFrame(planeWidth: width, planeHeight: height, bitmaps: bitmaps)
    }
}
