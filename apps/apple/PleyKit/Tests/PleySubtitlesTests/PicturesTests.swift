import Foundation
@testable import PleySubtitles
import PleyTesting
import Testing

// MARK: Builders, as pictures.test.ts makes them

typealias Block = SubtitleSamples.Block

func u16(_ v: Int) -> [UInt8] { SubtitleSamples.u16(v) }

func pgsSegment(_ type: Int, _ body: [UInt8]) -> [UInt8] { SubtitleSamples.pgsSegment(type, body) }

func pgsDisplaySet(state: Int, objects: Bool, x: Int = 100, y: Int = 900, rle: [UInt8]? = nil, w: Int = 4,
                   h: Int = 2) -> [UInt8] {
    SubtitleSamples.pgsDisplaySet(state: state, objects: objects, x: x, y: y, rle: rle, w: w, h: h)
}

func el(_ id: Int, _ body: [UInt8]) -> [UInt8] { SubtitleSamples.el(id, body) }

func ascii(_ s: String) -> [UInt8] { SubtitleSamples.ascii(s) }

func mks(_ codecId: String, _ blocks: [Block], codecPrivate: [UInt8]? = nil) -> [UInt8] {
    SubtitleSamples.mks(codecId, blocks, codecPrivate: codecPrivate)
}

let rle2x4 = SubtitleSamples.rle2x4

// MARK: The cases of pictures.test.ts

@Suite("Picture subtitles (pictures.test.ts)")
struct PicturesTests {
    @Test func pgsDecodesRunsLineEndsAndLongRuns() {
        // Line 1: 2 × colour 1, 2 × colour 0. Line 2: 4 × colour 2 (short coloured run).
        let rle: [UInt8] = [1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0]
        #expect(PGSDecoder.decodeRle(rle, width: 4, height: 2) == [1, 1, 0, 0, 2, 2, 2, 2])
        let long: [UInt8] = [0, 0xC1, 0x00, 7, 0, 0]
        let decoded = PGSDecoder.decodeRle(long, width: 300, height: 1)
        #expect(!decoded.allSatisfy { $0 == 7 })
        #expect(decoded.prefix(256).allSatisfy { $0 == 7 })
    }

    @Test func pgsPlacesAnObjectOnThePlaneInItsPalettesColours() throws {
        let set = pgsDisplaySet(state: 0x80, objects: true, rle: [1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0])
        let frame = try #require(PGSDecoder(samples: [set]).decode(0))
        #expect(frame.planeWidth == 1920)
        #expect(frame.planeHeight == 1080)
        #expect(frame.bitmaps.count == 1)
        let b = frame.bitmaps[0]
        #expect([b.x, b.y, b.width, b.height] as [Int] == [100, 900, 4, 2])
        #expect(Array(b.rgba[0..<4]) == [255, 255, 255, 255])
        #expect(b.rgba[3 + 4 * 2] == 0)
        #expect(Array(b.rgba[16..<20]) == [0, 0, 0, 128])
    }

    @Test func pgsDecodesANormalCaseDisplaySetFromItsEpochsDefinitions() throws {
        let start = pgsDisplaySet(state: 0x80, objects: true, rle: rle2x4)
        let clear = pgsDisplaySet(state: 0x00, objects: false)
        let again = pgsDisplaySet(state: 0x00, objects: true, x: 200, y: 950)
        let decoder = PGSDecoder(samples: [start, clear, again])
        #expect(try #require(decoder.decode(1)).bitmaps.isEmpty)
        let reused = try #require(decoder.decode(2)?.bitmaps.first)
        #expect([reused.x, reused.y] as [Int] == [200, 950])
        #expect(reused.rgba[3] == 255)
    }

    @Test func matroskaReadsTheTrackItsSamplesTimesAndDurations() throws {
        let set = pgsDisplaySet(state: 0x80, objects: true, rle: rle2x4)
        let track = try MatroskaPictureTrack.read(bytes: mks("S_HDMV/PGS", [
            Block(time: 1000, data: set),
            Block(time: 3000, data: pgsDisplaySet(state: 0, objects: false)),
            Block(time: 5000, data: set, duration: 1500),
        ]))
        #expect(track.codecId == "S_HDMV/PGS")
        #expect(track.samples.map(\.timeMs) == [1000, 3000, 5000])
        #expect(track.samples.map(\.durationMs) == [nil, nil, 1500])
        let subs = try #require(PictureSubtitles.of(track))
        #expect(subs.frameAt(500) == nil)
        #expect(subs.frameAt(1500)?.index == 0)
        #expect(subs.frameAt(3200) == nil)
        #expect(subs.frameAt(6000)?.index == 2)
        #expect(subs.frameAt(6600) == nil)
        #expect(subs.nextChange(1500) == 3000)
        #expect(subs.nextChange(4000) == 5000)
        #expect(subs.nextChange(5100) == 6500)
    }

    @Test func matroskaReadsAFileCutShortUpToTheCutWithoutTheSampleCutInTwo() throws {
        let set = pgsDisplaySet(state: 0x80, objects: true, rle: rle2x4)
        let whole = mks("S_HDMV/PGS", [
            Block(time: 1000, data: set),
            Block(time: 3000, data: pgsDisplaySet(state: 0, objects: false)),
            Block(time: 5000, data: set),
        ])
        // Up to the middle of the last block.
        let cut = Array(whole.prefix(whole.count - set.count / 2))
        let track = try MatroskaPictureTrack.read(bytes: cut)
        #expect(track.samples.map(\.timeMs) == [1000, 3000])
        #expect(track.samples[0].data == set)
        // Cut inside the track list: no track yet.
        #expect(throws: MatroskaError(description: "no track")) {
            try MatroskaPictureTrack.read(bytes: Array(whole.prefix(60)))
        }
    }

    @Test func matroskaRefusesWhatIsntMatroska() {
        #expect(throws: MatroskaError(description: "not a Matroska file")) {
            try MatroskaPictureTrack.read(bytes: [1, 2, 3, 4])
        }
    }

    static let idx = ascii("size: 720x480\npalette: 000000, ffffff, 808080, ff0000, 00ff00, 0000ff, 000000, 000000, "
        + "000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000\n")

    /// An SPU of a 4×2 picture at (10, 400): top line pattern colour,
    /// bottom line emphasis 1, shown for 175 × 1024 / 90 ms.
    static func spu() -> [UInt8] {
        let top: [UInt8] = [0x11, 0x00] // count 4 colour 1
        let bottom: [UInt8] = [0x12, 0x00] // count 4 colour 2
        let pixels = top + bottom
        let ctrlAt = 4 + pixels.count
        let stopSeqAt = ctrlAt + 4 + 1 + 3 + 3 + 7 + 5 + 1
        let ctrl: [UInt8] = u16(0) + u16(stopSeqAt) + [0x01,
                                                      0x03, 0x32, 0x10, // colours: e2=3 e1=2 p=1 b=0
                                                      0x04, 0xFF, 0xF0, // alpha: e2 15, e1 15, p 15, b 0
                                                      0x05, 0x00, 0xA0, 0x0D, 0x19, 0x01, 0x91, // x 10..13, y 400..401
                                                      0x06] + u16(4) + u16(6) + [0xFF]
        let stop: [UInt8] = u16(175) + u16(stopSeqAt) + [0x02, 0xFF]
        let all = pixels + ctrl + stop
        return u16(4 + all.count) + u16(ctrlAt) + all
    }

    @Test func vobSubReadsTheIdxHeader() {
        let h = VobSubDecoder.parseIdx(Self.idx)
        #expect([h.width, h.height] as [Int] == [720, 480])
        #expect(h.palette[1] == [255, 255, 255])
        #expect(h.palette[3] == [255, 0, 0])
    }

    @Test func vobSubReadsControlSequencesPlaceColoursAndWhenItStops() throws {
        let s = try #require(VobSubDecoder.parseSpu(Self.spu()))
        #expect([s.x1, s.x2, s.y1, s.y2] as [Int] == [10, 13, 400, 401])
        #expect(s.colors == [0, 1, 2, 3])
        #expect(s.alpha == [0, 15, 15, 15])
        #expect(s.stopMs == Int((Double(175 * 1024) / 90).rounded()))
    }

    @Test func vobSubDecodesInterlacedFieldsInThePalettesColours() throws {
        let decoder = VobSubDecoder(samples: [Self.spu()], codecPrivate: Self.idx)
        let frame = try #require(decoder.decode(0))
        #expect([frame.planeWidth, frame.planeHeight] as [Int] == [720, 480])
        let b = try #require(frame.bitmaps.first)
        #expect([b.x, b.y, b.width, b.height] as [Int] == [10, 400, 4, 2])
        #expect(Array(b.rgba[0..<4]) == [255, 255, 255, 255])
        #expect(Array(b.rgba[16..<20]) == [128, 128, 128, 255])
        #expect(decoder.ownDurationMs(0) == Int((Double(175 * 1024) / 90).rounded()))
    }

    static func dvbSegment(_ type: Int, _ body: [UInt8]) -> [UInt8] {
        [0x0F, UInt8(type)] + u16(1) + u16(body.count) + body
    }

    /// A mode-change page with one 4-bit region of 4×2 at (60, 500), CLUT
    /// entry 1 red, the object filled with entry 1.
    static func dvbDisplaySet(timeout: UInt8 = 5) -> [UInt8] {
        let page = dvbSegment(0x10, [timeout, 2 << 2, 0, 0] + u16(60) + u16(500))
        let region = dvbSegment(0x11, [0, 0x00] + u16(4) + u16(2) + [2 << 5 | 2 << 2, 0, 0, 0] + u16(0) + [0, 0, 0, 0])
        // CLUT 0: entry 1 for 4-bit tables, full range: Y 81 Cr 240 Cb 90 (red), opaque.
        let clut = dvbSegment(0x12, [0, 0, 1, 0x40 | 0x01, 81, 240, 90, 0])
        // 4-bit string: 4 × pixel 1 ("0001" ×4), then end of string (0000 0 000), end of line.
        let line: [UInt8] = [0x11, 0x11, 0x11, 0x00, 0x00, 0xF0]
        let object = dvbSegment(0x13, u16(0) + [0x00] + u16(line.count) + u16(0) + line)
        let end = dvbSegment(0x80, [])
        return [0x20, 0x00] + page + region + clut + object + end + [0xFF]
    }

    @Test func dvbSplitsADisplaySetIntoSegments() {
        #expect(DVBDecoder.segments(Self.dvbDisplaySet()).map(\.type) == [0x10, 0x11, 0x12, 0x13, 0x80])
    }

    @Test func dvbDrawsObjectsIntoRegionsPlacedByThePage() throws {
        let decoder = DVBDecoder(samples: [Self.dvbDisplaySet()])
        let frame = try #require(decoder.decode(0))
        #expect([frame.planeWidth, frame.planeHeight] as [Int] == [720, 576])
        let b = try #require(frame.bitmaps.first)
        #expect([b.x, b.y, b.width, b.height] as [Int] == [60, 500, 4, 2])
        let (r, g, bl) = Pictures.rgb(y: 81, cb: 90, cr: 240, hd: false)
        #expect(Array(b.rgba[0..<4]) == [r, g, bl, 255])
        // The bottom field is the top one: both lines drawn.
        #expect(b.rgba[4 * 4 + 3] == 255)
        #expect(decoder.ownDurationMs(0) == 5000)
    }
}
