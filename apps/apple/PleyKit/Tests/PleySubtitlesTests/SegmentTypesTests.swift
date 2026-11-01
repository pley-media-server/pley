import Foundation
@testable import PleySubtitles
import Testing

/// Each decoder's segment types and codes, beyond pictures.test.ts.
@Suite("Picture subtitles: segment types")
struct SegmentTypesTests {
    // MARK: PGS

    @Test func pgsReadsTheSupHeaderSomeMuxersKeep() throws {
        // "PG", PTS and DTS before each segment.
        let plain = pgsDisplaySet(state: 0x80, objects: true, rle: rle2x4)
        var withHeaders: [UInt8] = []
        var at = 0
        while at < plain.count {
            let size = Int(plain[at + 1]) << 8 | Int(plain[at + 2])
            withHeaders += [0x50, 0x47, 0, 0, 0, 0, 0, 0, 0, 0] + plain[at..<(at + 3 + size)]
            at += 3 + size
        }
        let frame = try #require(PGSDecoder(samples: [withHeaders]).decode(0))
        #expect(frame.bitmaps.count == 1)
    }

    @Test func pgsJoinsAnObjectSentInSeveralSegments() throws {
        // The object's RLE split in two ODS: first (0x80) then last (0x40).
        let rle = rle2x4
        let data = u16(4) + u16(2) + rle
        let length = data.count
        let firstPart = Array(data.prefix(7))
        let lastPart = Array(data.dropFirst(7))
        var set = pgsSegment(0x16, u16(1920) + u16(1080) + [0x10] + u16(1) + [0x80, 0, 0, 1] + u16(0) + [0, 0]
            + u16(10) + u16(20))
        set += pgsSegment(0x14, [0, 0, 1, 235, 128, 128, 255])
        set += pgsSegment(0x15, u16(0) + [0, 0x80, UInt8((length >> 16) & 0xFF), UInt8((length >> 8) & 0xFF),
                                         UInt8(length & 0xFF)] + firstPart)
        set += pgsSegment(0x15, u16(0) + [0, 0x40] + lastPart)
        set += pgsSegment(0x80, [])
        let b = try #require(PGSDecoder(samples: [set]).decode(0)?.bitmaps.first)
        #expect([b.x, b.y, b.width, b.height] as [Int] == [10, 20, 4, 2])
        #expect(b.rgba.enumerated().filter { $0.offset % 4 == 3 }.allSatisfy { $0.element == 255 })
    }

    @Test func pgsCropsAnObjectAsTheCompositionSays() throws {
        var pcs = u16(1920) + u16(1080) + [0x10] + u16(1) + [0x80, 0, 0, 1]
        // Object 0, window 0, cropped (0x40), at (100, 900); crop from (2, 0), 2×2.
        pcs += u16(0) + [0, 0x40] + u16(100) + u16(900) + u16(2) + u16(0) + u16(2) + u16(2)
        var set = pgsSegment(0x16, pcs)
        set += pgsSegment(0x14, [0, 0, 1, 235, 128, 128, 255, 2, 16, 128, 128, 128])
        let data = u16(4) + u16(2) + [1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0]
        set += pgsSegment(0x15, u16(0) + [0, 0xC0, 0, 0, UInt8(data.count)] + data)
        set += pgsSegment(0x80, [])
        let b = try #require(PGSDecoder(samples: [set]).decode(0)?.bitmaps.first)
        #expect([b.width, b.height] as [Int] == [2, 2])
        // The top line's right half is colour 0 (transparent), the bottom line's colour 2.
        #expect(b.rgba[3] == 0)
        #expect(Array(b.rgba[8..<12]) == [0, 0, 0, 128])
    }

    @Test func pgsTakesAPaletteUpdateInANormalCase() throws {
        let start = pgsDisplaySet(state: 0x80, objects: true, rle: rle2x4)
        // Palette 0's entry 1 becomes half transparent, the objects stay.
        var update = pgsSegment(0x16, u16(1920) + u16(1080) + [0x10] + u16(2) + [0x00, 0x80, 0x00, 1] + u16(0)
            + [0, 0] + u16(100) + u16(900))
        update += pgsSegment(0x14, [0, 1, 1, 235, 128, 128, 64])
        update += pgsSegment(0x80, [])
        let decoder = PGSDecoder(samples: [start, update])
        #expect(try #require(decoder.decode(0)?.bitmaps.first).rgba[3] == 255)
        #expect(try #require(decoder.decode(1)?.bitmaps.first).rgba[3] == 64)
    }

    @Test func pgsWithoutAPaletteOrCompositionDecodesNothing() {
        #expect(PGSDecoder(samples: [pgsSegment(0x80, [])]).decode(0) == nil)
        // Objects placed, but no palette defined in the epoch.
        #expect(PGSDecoder(samples: [pgsDisplaySet(state: 0x80, objects: true)]).decode(0) == nil)
        #expect(PGSDecoder(samples: []).decode(0) == nil)
    }

    // MARK: VobSub

    @Test func vobSubWithoutAHeaderUsesTheDVDsPlaneAndGreys() {
        let header = VobSubDecoder.parseIdx(nil)
        #expect([header.width, header.height] as [Int] == [720, 576])
        #expect(header.palette.count == 16)
        #expect(header.palette[0] == [0, 0, 0])
        #expect(header.palette[1] == [255, 255, 255])
    }

    @Test func vobSubForcedStartAndAnUnknownCommandEndTheSequence() throws {
        // Forced start (0x00) at 0, then a command it doesn't know: what follows isn't read.
        let ctrl: [UInt8] = u16(0) + u16(4) + [0x00, 0x07, 0x05, 0, 0, 0, 0, 0, 0]
        let spu = u16(4 + ctrl.count) + u16(4) + ctrl
        let s = try #require(VobSubDecoder.parseSpu(spu))
        #expect(s.startMs == 0)
        #expect([s.x1, s.x2] as [Int] == [0, -1])
        #expect(s.stopMs == nil)
        let decoder = VobSubDecoder(samples: [spu], codecPrivate: nil)
        #expect(decoder.decode(0)?.bitmaps.isEmpty == true)
        #expect(decoder.ownDurationMs(0) == nil)
    }

    @Test func vobSubCutShortIsNotDecoded() {
        #expect(VobSubDecoder.parseSpu([0, 8]) == nil)
        // A colour command whose bytes are missing.
        let ctrl: [UInt8] = u16(0) + u16(4) + [0x03]
        #expect(VobSubDecoder.parseSpu(u16(4 + ctrl.count) + u16(4) + ctrl) == nil)
    }

    @Test func vobSubStopShowsForItsOwnDurationInTheTimeline() throws {
        let track = PictureTrack(codecId: "S_VOBSUB", codecPrivate: PicturesTests.idx,
                                 samples: [PictureSample(timeMs: 1_000, durationMs: nil, data: PicturesTests.spu()),
                                           PictureSample(timeMs: 10_000, durationMs: nil, data: PicturesTests.spu())])
        let subs = try #require(PictureSubtitles.of(track))
        let shown = Int((Double(175 * 1024) / 90).rounded())
        #expect(subs.endOf(0) == 1_000 + shown)
        #expect(subs.frameAt(1_500)?.index == 0)
        #expect(subs.frameAt(1_000 + shown) == nil)
        #expect(subs.nextChange(1_500) == 1_000 + shown)
        #expect(subs.nextChange(1_000 + shown) == 10_000)
    }

    // MARK: DVB

    /// One region of `depth` bits (2, 4 or 8) of 4×2 at (0, 0), filled by `line` on both fields.
    private func dvb(depth: Int, line: [UInt8], clut: [UInt8] = [], state: Int = 2, display: [UInt8]? = nil,
                     timeout: UInt8 = 0) -> [UInt8] {
        let code: UInt8 = depth == 2 ? 1 : depth == 4 ? 2 : 3
        var out: [UInt8] = [0x20, 0x00]
        if let display { out += PicturesTests.dvbSegment(0x14, display) }
        out += PicturesTests.dvbSegment(0x10, [timeout, UInt8(state << 2), 0, 0] + u16(0) + u16(0))
        out += PicturesTests.dvbSegment(0x11, [0, 0x00] + u16(4) + u16(2) + [code << 2, 0, 0, 0] + u16(0)
            + [0, 0, 0, 0])
        if !clut.isEmpty { out += PicturesTests.dvbSegment(0x12, [0, 0] + clut) }
        out += PicturesTests.dvbSegment(0x13, u16(0) + [0x00] + u16(line.count) + u16(0) + line)
        return out + [0xFF]
    }

    @Test func dvbDrawsTwoBitStringsWithTheDefaultTable() throws {
        // 2-bit: pixel 1 ×4 ("01" ×4), end of string ("00 0 0 00"), end of line.
        let frame = try #require(DVBDecoder(samples: [dvb(depth: 2, line: [0x10, 0x55, 0x00, 0xF0])]).decode(0))
        let b = try #require(frame.bitmaps.first)
        // The default 2-bit table's entry 1 is opaque white.
        #expect(Array(b.rgba[0..<4]) == [255, 255, 255, 255])
        #expect(Array(b.rgba[12..<16]) == [255, 255, 255, 255])
    }

    @Test func dvbDrawsEightBitStringsAndLongRuns() throws {
        // 8-bit: a run of 4 × pixel 9 ("0000 0000", "1", 4 in 7 bits, 9), then the end ("0000 0000", "0", 0 in 7).
        let line: [UInt8] = [0x12, 0x00, 0x84, 0x09, 0x00, 0x00, 0xF0]
        let clut: [UInt8] = [9, 0x20 | 0x01, 235, 128, 128, 0]
        let b = try #require(DVBDecoder(samples: [dvb(depth: 8, line: line, clut: clut)]).decode(0)?.bitmaps.first)
        #expect(Array(b.rgba[0..<4]) == [255, 255, 255, 255])
        #expect(Array(b.rgba[12..<16]) == [255, 255, 255, 255])
    }

    @Test func dvbMapsShallowerCodesThroughTheMapTable() throws {
        // A 2-to-4-bit map table (0x20) sending pixel 1 to entry 3, then a 2-bit string of 4 × pixel 1.
        let line: [UInt8] = [0x20, 0x03, 0x00, 0x10, 0x55, 0x00, 0xF0]
        let b = try #require(DVBDecoder(samples: [dvb(depth: 4, line: line)]).decode(0)?.bitmaps.first)
        // The default 4-bit table's entry 3 is opaque yellow.
        #expect(Array(b.rgba[0..<4]) == [255, 255, 0, 255])
    }

    @Test func dvbReadsTwoByteClutEntriesAndTheDisplaysSize() throws {
        // A 1920×1080 display definition, and entry 1 of the 4-bit table in 2 bytes: Y 0xFC, opaque.
        let display: [UInt8] = [0] + u16(1919) + u16(1079)
        let clut: [UInt8] = [1, 0x40, 0xFC, 0x00]
        let decoder = DVBDecoder(samples: [dvb(depth: 4, line: [0x11, 0x11, 0x11, 0x00, 0x00, 0xF0], clut: clut,
                                               display: display)])
        let frame = try #require(decoder.decode(0))
        #expect([frame.planeWidth, frame.planeHeight] as [Int] == [1920, 1080])
        let b = try #require(frame.bitmaps.first)
        let (r, g, bl) = Pictures.rgb(y: 0xFC, cb: 0, cr: 0, hd: true)
        #expect(Array(b.rgba[0..<4]) == [r, g, bl, 255])
        // No time-out: it shows until the next sample.
        #expect(decoder.ownDurationMs(0) == nil)
    }

    @Test func dvbDecodesANormalCasePageFromTheAcquisitionPointBefore() throws {
        let first = dvb(depth: 4, line: [0x11, 0x11, 0x11, 0x00, 0x00, 0xF0], clut: [1, 0x40 | 0x01, 81, 240, 90, 0])
        // A normal-case page placing the same region elsewhere, with nothing else.
        let normal: [UInt8] = [0x20, 0x00] + PicturesTests.dvbSegment(0x10, [0, 0, 0, 0] + u16(30) + u16(40)) + [0xFF]
        let b = try #require(DVBDecoder(samples: [first, normal]).decode(1)?.bitmaps.first)
        #expect([b.x, b.y] as [Int] == [30, 40])
        #expect(b.rgba[3] == 255)
    }

    @Test func dvbShowsNoRegionWithNothingVisible() throws {
        // Pixel 0 everywhere: transparent in every table.
        let frame = try #require(DVBDecoder(samples: [dvb(depth: 4, line: [0x11, 0x00, 0x18, 0x00, 0xF0])]).decode(0))
        #expect(frame.bitmaps.isEmpty)
        #expect(DVBDecoder(samples: [[0x20, 0x00, 0xFF]]).decode(0) == nil)
    }

    // MARK: Matroska

    @Test func matroskaPutsBackTheBytesStrippedFromEveryFrame() throws {
        // ContentCompression with algorithm 3 (header stripping) of [0xAB].
        let compression = el(0x5034, el(0x4254, [3]) + el(0x4255, [0xAB]))
        let track = el(0xAE, el(0xD7, [1]) + el(0x83, [0x11]) + el(0x86, ascii("S_HDMV/PGS"))
            + el(0x6D80, el(0x6240, compression)))
        let cluster = el(0xE7, [0]) + el(0xA3, [0x81] + u16(100) + [0x80, 0x01, 0x02])
        let file = el(0x1A45_DFA3, []) + el(0x1853_8067, el(0x1654_AE6B, track) + el(0x1F43_B675, cluster))
        let read = try MatroskaPictureTrack.read(bytes: file)
        #expect(read.samples.map(\.data) == [[0xAB, 0x01, 0x02]])
        #expect(read.samples.map(\.timeMs) == [100])
    }

    @Test func matroskaRefusesFramesCompressedWithZlib() {
        let compression = el(0x5034, el(0x4254, [0]))
        let track = el(0xAE, el(0xD7, [1]) + el(0x83, [0x11]) + el(0x86, ascii("S_VOBSUB"))
            + el(0x6D80, el(0x6240, compression)))
        let file = el(0x1A45_DFA3, []) + el(0x1853_8067, el(0x1654_AE6B, track))
        #expect(throws: MatroskaError(description: "compressed subtitle frames")) {
            try MatroskaPictureTrack.read(bytes: file)
        }
    }

    @Test func matroskaReadsClustersOfUnknownSize() throws {
        let track = el(0xAE, el(0xD7, [1]) + el(0x83, [0x11]) + el(0x86, ascii("S_DVBSUB")))
        // Two clusters of unknown size (0x01FFFFFFFFFFFFFF), the second at 2 s.
        let unknown: [UInt8] = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
        let one = [0x1F, 0x43, 0xB6, 0x75] + unknown + el(0xE7, [0]) + el(0xA3, [0x81] + u16(10) + [0x80, 7])
        let two = [0x1F, 0x43, 0xB6, 0x75] + unknown + el(0xE7, u16(2000)) + el(0xA3, [0x81] + u16(5) + [0x80, 8])
        let file = el(0x1A45_DFA3, []) + [0x18, 0x53, 0x80, 0x67] + unknown + el(0x1654_AE6B, track) + one + two
        let read = try MatroskaPictureTrack.read(bytes: file)
        #expect(read.codecId == "S_DVBSUB")
        #expect(read.samples.map(\.timeMs) == [10, 2005])
        #expect(read.samples.map(\.data) == [[7], [8]])
    }
}
