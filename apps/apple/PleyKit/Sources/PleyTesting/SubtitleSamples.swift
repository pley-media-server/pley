import Foundation

/// Picture subtitles built byte by byte, as packages/picture-subtitles'
/// tests build them: PGS display sets, and the `.mks` of one track the
/// server makes (`/api/files/{id}/subtitles/{index}.mks`).
public enum SubtitleSamples {
    public static func u16(_ v: Int) -> [UInt8] { [UInt8((v >> 8) & 0xFF), UInt8(v & 0xFF)] }

    public static func pgsSegment(_ type: Int, _ body: [UInt8]) -> [UInt8] { [UInt8(type)] + u16(body.count) + body }

    /// A display set placing a `w`×`h` object at (x, y) on a 1920×1080 plane,
    /// with palette entry 1 white and entry 2 half-transparent black.
    public static func pgsDisplaySet(state: Int, objects: Bool, x: Int = 100, y: Int = 900, rle: [UInt8]? = nil,
                                     w: Int = 4, h: Int = 2) -> [UInt8] {
        var pcs = u16(1920) + u16(1080) + [0x10] + u16(1) + [UInt8(state), 0x00, 0x00, objects ? 1 : 0]
        if objects { pcs += u16(0) + [0, 0x00] + u16(x) + u16(y) }
        var out = pgsSegment(0x16, pcs)
        if let rle {
            out += pgsSegment(0x17, [1, 0] + u16(x) + u16(y) + u16(w) + u16(h))
            out += pgsSegment(0x14, [0, 0, 1, 235, 128, 128, 255, 2, 16, 128, 128, 128])
            let data = u16(w) + u16(h) + rle
            let length = data.count
            out += pgsSegment(0x15, u16(0) + [0, 0xC0, UInt8((length >> 16) & 0xFF), UInt8((length >> 8) & 0xFF),
                                             UInt8(length & 0xFF)] + data)
        }
        out += pgsSegment(0x80, [])
        return out
    }

    /// An EBML element, with an 8-byte size.
    public static func el(_ id: Int, _ body: [UInt8]) -> [UInt8] {
        var idBytes: [UInt8] = []
        var v = id
        while v > 0 {
            idBytes.insert(UInt8(v & 0xFF), at: 0)
            v /= 256
        }
        let size = body.count
        let sizeBytes: [UInt8] = [0x01, 0, 0, 0, UInt8((size >> 24) & 0xFF), UInt8((size >> 16) & 0xFF),
                                  UInt8((size >> 8) & 0xFF), UInt8(size & 0xFF)]
        return idBytes + sizeBytes + body
    }

    public static func ascii(_ s: String) -> [UInt8] { Array(s.utf8) }

    /// A block of the track: its time and data, and a duration of its own (a `BlockGroup`).
    public struct Block {
        public var time: Int
        public var data: [UInt8]
        public var duration: Int?

        public init(time: Int, data: [UInt8], duration: Int? = nil) {
            self.time = time
            self.data = data
            self.duration = duration
        }
    }

    /// A `.mks` of one subtitle track, as the server makes them.
    public static func mks(_ codecId: String, _ blocks: [Block], codecPrivate: [UInt8]? = nil) -> [UInt8] {
        let track = el(0xAE, el(0xD7, [1]) + el(0x83, [0x11]) + el(0x86, ascii(codecId))
            + (codecPrivate.map { el(0x63A2, $0) } ?? []))
        var cluster = el(0xE7, [0])
        for b in blocks {
            let block = [0x81] + u16(b.time) + [0x00] + b.data
            if let duration = b.duration {
                cluster += el(0xA0, el(0xA1, block) + el(0x9B, u16(duration)))
            } else {
                cluster += el(0xA3, [0x81] + u16(b.time) + [0x80] + b.data)
            }
        }
        let segment = el(0x1853_8067, el(0x1549_A966, el(0x2A_D7B1, [0x0F, 0x42, 0x40])) + el(0x1654_AE6B, track)
            + el(0x1F43_B675, cluster))
        return el(0x1A45_DFA3, el(0x4282, ascii("matroska"))) + segment
    }

    /// A 4×2 object: colour 1 on both lines.
    public static let rle2x4: [UInt8] = [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0]
}
