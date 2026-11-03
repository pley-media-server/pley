import Foundation

/// DVD subtitles (VobSub, `S_VOBSUB`), a port of packages/picture-subtitles'
/// `vobsub.ts`: each sample is one subpicture unit (SPU): two interlaced
/// fields of 2-bit run-length pixels, then control sequences that say when
/// it shows and stops, which 4 of the 16 palette colours its pixels take,
/// their alpha, and where it goes. The palette and the plane's size come
/// from the track's private data, the `.idx` header (`size: 720x480`,
/// `palette: 000000, ffffff, …`).
public final class VobSubDecoder: PictureDecoder {
    public struct Header: Equatable, Sendable {
        public var width: Int
        public var height: Int
        /// 16 RGB colours.
        public var palette: [[UInt8]]
    }

    public struct SPU: Equatable, Sendable {
        public var x1 = 0
        public var y1 = 0
        public var x2 = -1
        public var y2 = -1
        /// Palette entries for pixel values 0-3.
        public var colors = [0, 1, 2, 3]
        /// Alpha 0-15 for pixel values 0-3.
        public var alpha = [0, 15, 15, 15]
        public var top = 0
        public var bottom = 0
        /// From the stop command, in ms; `nil` without one.
        public var stopMs: Int?
        public var startMs = 0
    }

    private let samples: [[UInt8]]
    private let header: Header
    private var spus: [SPU??]
    private var cache = FrameCache()

    public init(samples: [[UInt8]], codecPrivate: [UInt8]?) {
        self.samples = samples
        header = Self.parseIdx(codecPrivate)
        spus = Array(repeating: nil, count: samples.count)
    }

    public static func parseIdx(_ codecPrivate: [UInt8]?) -> Header {
        // A real .idx header is a few hundred bytes.
        let text = String(decoding: (codecPrivate ?? []).prefix(64 << 10), as: UTF8.self)
        var width = 720
        var height = 576
        if let match = text.firstMatch(of: /(?i)size:\s*(\d+)\s*x\s*(\d+)/),
           let w = Int(match.1), let h = Int(match.2) {
            width = w
            height = h
        }
        var palette: [[UInt8]] = []
        if let match = text.firstMatch(of: /(?i)palette:\s*([0-9a-f,\s]+)/) {
            for hex in match.1.split(separator: ",").prefix(16) {
                if let v = Int(hex.trimmingCharacters(in: .whitespacesAndNewlines), radix: 16) {
                    palette.append([UInt8((v >> 16) & 0xFF), UInt8((v >> 8) & 0xFF), UInt8(v & 0xFF)])
                }
            }
        }
        // A DVD's default greys when the header has none.
        while palette.count < 16 { palette.append(palette.count % 2 == 1 ? [255, 255, 255] : [0, 0, 0]) }
        return Header(width: width, height: height, palette: palette)
    }

    /// Reads the control sequences of an SPU; `nil` when it is cut short.
    public static func parseSpu(_ data: [UInt8]) -> SPU? {
        guard data.count >= 4 else { return nil }
        let size = data.u16(0)
        let ctrl = data.u16(2)
        // An SPU's size is 16 bits; a real one has two or three control
        // sequences, and each is read to the end at most.
        let end = min(size == 0 ? data.count : size, data.count, 0xFFFF)
        var spu = SPU()
        var seq = ctrl
        var seen = Set<Int>()
        while seq + 4 <= end, !seen.contains(seq), seen.count < 16 {
            seen.insert(seq)
            // Delays count in 1024 ticks of a 90 kHz clock.
            let delayMs = Int((Double(data.u16(seq) * 1024) / 90).rounded())
            let next = data.u16(seq + 2)
            var at = seq + 4
            var done = false
            while at < end && !done {
                let cmd = data.at(at)
                at += 1
                switch cmd {
                case 0x00, 0x01: // forced start, start
                    spu.startMs = delayMs
                case 0x02: // stop
                    spu.stopMs = delayMs
                case 0x03:
                    if at + 2 > end { return nil }
                    spu.colors = [data.at(at + 1) & 0x0F, data.at(at + 1) >> 4, data.at(at) & 0x0F, data.at(at) >> 4]
                    at += 2
                case 0x04:
                    if at + 2 > end { return nil }
                    spu.alpha = [data.at(at + 1) & 0x0F, data.at(at + 1) >> 4, data.at(at) & 0x0F, data.at(at) >> 4]
                    at += 2
                case 0x05:
                    if at + 6 > end { return nil }
                    spu.x1 = data.at(at) << 4 | data.at(at + 1) >> 4
                    spu.x2 = (data.at(at + 1) & 0x0F) << 8 | data.at(at + 2)
                    spu.y1 = data.at(at + 3) << 4 | data.at(at + 4) >> 4
                    spu.y2 = (data.at(at + 4) & 0x0F) << 8 | data.at(at + 5)
                    at += 6
                case 0x06:
                    if at + 4 > end { return nil }
                    spu.top = data.u16(at)
                    spu.bottom = data.u16(at + 2)
                    at += 4
                default:
                    // 0xff ends the sequence; an unknown command's length isn't known, so it ends there too.
                    done = true
                }
            }
            if next == seq { break }
            seq = next
        }
        return spu
    }

    /// One field's lines (every other line of the picture) into `out`, starting at line `first`.
    private static func decodeField(_ data: [UInt8], offset: Int, end: Int, width: Int, height: Int, first: Int,
                                    into out: inout [UInt8]) {
        var nibble = offset * 2
        let limit = end * 2
        func read() -> Int {
            if nibble >= limit { return 0 }
            let byte = data.at(nibble >> 1)
            let v = nibble & 1 != 0 ? byte & 0x0F : byte >> 4
            nibble += 1
            return v
        }
        var y = first
        while y < height {
            var x = 0
            while x < width {
                var v = read()
                if v < 0x4 {
                    v = v << 4 | read()
                    if v < 0x10 {
                        v = v << 4 | read()
                        if v < 0x40 { v = v << 4 | read() }
                    }
                }
                let color = UInt8(v & 3)
                var count = v >> 2
                if count == 0 { count = width - x } // To the end of the line.
                let stop = min(width, x + count)
                let row = y * width
                for px in x..<stop { out[row + px] = color }
                x = stop
                if nibble >= limit { break }
            }
            // Lines start on a byte.
            if nibble & 1 != 0 { nibble += 1 }
            y += 2
        }
    }

    private func spu(_ index: Int) -> SPU? {
        if let s = spus[index] { return s }
        let s = Self.parseSpu(samples[index])
        spus[index] = .some(s)
        return s
    }

    public func ownDurationMs(_ index: Int) -> Int? {
        guard samples.indices.contains(index), let s = spu(index), let stop = s.stopMs, stop > s.startMs else {
            return nil
        }
        return stop - s.startMs
    }

    public func decode(_ index: Int) -> PictureFrame? {
        guard samples.indices.contains(index) else { return nil }
        if let kept = cache.get(index) { return kept }
        let frame = render(index)
        cache.put(index, frame)
        return frame
    }

    private func render(_ index: Int) -> PictureFrame? {
        guard let spu = spu(index) else { return nil }
        let width = spu.x2 - spu.x1 + 1
        let height = spu.y2 - spu.y1 + 1
        guard width > 0, height > 0 else {
            return PictureFrame(planeWidth: header.width, planeHeight: header.height, bitmaps: [])
        }
        guard width * height <= Pictures.maxPixels else { return nil }
        let data = samples[index]
        let size = data.u16(0)
        let ctrl = data.u16(2)
        let end = min(ctrl != 0 ? ctrl : size != 0 ? size : data.count, data.count)
        var pixels = [UInt8](repeating: 0, count: width * height)
        Self.decodeField(data, offset: spu.top, end: end, width: width, height: height, first: 0, into: &pixels)
        Self.decodeField(data, offset: spu.bottom, end: end, width: width, height: height, first: 1, into: &pixels)
        let lut: [[UInt8]] = (0..<4).map { v in
            let c = header.palette[spu.colors[v] & 0x0F]
            return [c[0], c[1], c[2], UInt8(spu.alpha[v] * 17)]
        }
        var rgba = [UInt8](repeating: 0, count: width * height * 4)
        for i in 0..<pixels.count {
            let c = lut[Int(pixels[i])]
            let o = i * 4
            rgba[o] = c[0]
            rgba[o + 1] = c[1]
            rgba[o + 2] = c[2]
            rgba[o + 3] = c[3]
        }
        return PictureFrame(planeWidth: header.width, planeHeight: header.height,
                            bitmaps: [SubtitleBitmap(x: spu.x1, y: spu.y1, width: width, height: height, rgba: rgba)])
    }
}
