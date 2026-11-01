import Foundation

/// A picture subtitle track as the server's `.mks` holds it.
public struct PictureTrack: Equatable, Sendable {
    /// `S_HDMV/PGS`, `S_VOBSUB` or `S_DVBSUB`.
    public var codecId: String
    public var codecPrivate: [UInt8]?
    public var samples: [PictureSample]

    public init(codecId: String, codecPrivate: [UInt8]?, samples: [PictureSample]) {
        self.codecId = codecId
        self.codecPrivate = codecPrivate
        self.samples = samples
    }
}

/// One sample of the track: where it starts showing, and its bytes.
public struct PictureSample: Equatable, Sendable {
    public var timeMs: Int
    /// From the block group; `nil` when the sample shows until the next one.
    public var durationMs: Int?
    public var data: [UInt8]

    public init(timeMs: Int, durationMs: Int?, data: [UInt8]) {
        self.timeMs = timeMs
        self.durationMs = durationMs
        self.data = data
    }
}

public struct MatroskaError: Error, Equatable, CustomStringConvertible {
    public let description: String
}

/// The Matroska files the server makes of a picture subtitle track
/// (`/api/files/{id}/subtitles/{n}.mks`, docs/API.md): one track of PGS,
/// VobSub or DVB, copied as it is, with the original's times from the start
/// of the file. Only what that needs is read: the track's codec and private
/// data, and every block with its time and duration. A port of
/// packages/picture-subtitles' `mks.ts`.
public enum MatroskaPictureTrack {
    private enum ID {
        static let ebml = 0x1A45_DFA3
        static let segment = 0x1853_8067
        static let info = 0x1549_A966
        static let timecodeScale = 0x2A_D7B1
        static let tracks = 0x1654_AE6B
        static let trackEntry = 0xAE
        static let trackNumber = 0xD7
        static let trackType = 0x83
        static let codecID = 0x86
        static let codecPrivate = 0x63A2
        static let contentEncodings = 0x6D80
        static let contentEncoding = 0x6240
        static let contentCompression = 0x5034
        static let contentCompAlgo = 0x4254
        static let contentCompSettings = 0x4255
        static let cluster = 0x1F43_B675
        static let timecode = 0xE7
        static let simpleBlock = 0xA3
        static let blockGroup = 0xA0
        static let block = 0xA1
        static let blockDuration = 0x9B
    }

    /// Elements whose children are read; everything else is skipped by its size.
    private static let masters: Set<Int> = [ID.segment, ID.info, ID.tracks, ID.trackEntry, ID.contentEncodings,
                                            ID.contentEncoding, ID.contentCompression, ID.cluster, ID.blockGroup]
    /// The Segment's children: where an element of unknown size inside one ends.
    private static let levelOne: Set<Int> = [0x114D_9B74, ID.info, ID.tracks, ID.cluster, 0x1C53_BB6B, 0x1043_A770,
                                             0x1254_C367, 0x1941_A469]
    private static let subtitleTrack = 0x11

    private struct Track {
        var number = 0
        var type = 0
        var codecId = ""
        var codecPrivate: [UInt8]?
        /// Bytes stripped from the start of every frame (ContentCompAlgo 3).
        var stripped: [UInt8]?
        /// Compressed with something this reader can't undo (zlib).
        var compressed = false
    }

    private struct Block {
        var track: Int
        var time: Int
        var data: [UInt8]
    }

    private struct Element {
        var id: Int
        /// `nil`: unknown size (to the end of the parent).
        var size: Int?
        var dataStart: Int
    }

    /// Reads a `.mks` file, or as much of one as there is: a file cut short
    /// gives the samples before the cut. Throws `MatroskaError` for anything
    /// that isn't one.
    public static func read(data: Data) throws -> PictureTrack {
        try read(bytes: [UInt8](data))
    }

    public static func read(bytes: [UInt8]) throws -> PictureTrack {
        var reader = Walker(bytes: bytes)
        guard let header = element(bytes, at: 0, end: bytes.count), header.id == ID.ebml else {
            throw MatroskaError(description: "not a Matroska file")
        }
        reader.walk(header.dataStart + (header.size ?? 0), bytes.count, unknownSize: false)

        let subtitle = reader.tracks.first { $0.type == subtitleTrack } ?? reader.tracks.first
        guard let subtitle else { throw MatroskaError(description: "no track") }
        if subtitle.compressed { throw MatroskaError(description: "compressed subtitle frames") }
        let scale = Double(reader.scaleNs) / 1_000_000
        let ms = { (ticks: Int) in Int((Double(ticks) * scale).rounded()) }
        var samples: [PictureSample] = []
        for b in reader.raw where b.block.track == subtitle.number {
            let data = subtitle.stripped.map { $0 + b.block.data } ?? b.block.data
            samples.append(PictureSample(timeMs: ms(b.block.time), durationMs: b.duration.map(ms), data: data))
        }
        // A stable sort, as JavaScript's.
        samples = samples.enumerated().sorted { a, b in
            a.element.timeMs != b.element.timeMs ? a.element.timeMs < b.element.timeMs : a.offset < b.offset
        }.map(\.element)
        return PictureTrack(codecId: subtitle.codecId, codecPrivate: subtitle.codecPrivate, samples: samples)
    }

    /// The walk through the file's elements, and what it found.
    private struct Walker {
        let bytes: [UInt8]
        var scaleNs = 1_000_000
        var tracks: [Track] = []
        var raw: [(block: Block, duration: Int?)] = []
        var track: Track?
        var compAlgo = 0
        var compSettings: [UInt8]?
        var clusterTime = 0
        var groupBlock: Block?
        var groupDuration: Int?

        init(bytes: [UInt8]) { self.bytes = bytes }

        mutating func endGroup() {
            if let groupBlock { raw.append((groupBlock, groupDuration)) }
            groupBlock = nil
            groupDuration = nil
        }

        /// Reads the elements from `start` to `end`; where it stopped (an
        /// unknown-size cluster ends at the next level-1 element).
        @discardableResult
        mutating func walk(_ start: Int, _ end: Int, unknownSize: Bool) -> Int {
            var at = start
            while at < end {
                guard let el = element(bytes, at: at, end: end) else { return end }
                if unknownSize && levelOne.contains(el.id) { return at }
                // A file still being made (a partial answer, `X-Pley-Complete: 0`)
                // ends inside an element: a block cut short isn't read as a sample.
                if let size = el.size, el.dataStart + size > bytes.count, !masters.contains(el.id) { return end }
                let dataEnd = el.size.map { min(end, el.dataStart + $0) } ?? end
                if el.id == ID.cluster { clusterTime = 0 }
                if el.id == ID.trackEntry { track = Track() }
                if el.id == ID.contentCompression {
                    compAlgo = 0
                    compSettings = nil
                }
                var next = el.size.map { el.dataStart + $0 } ?? end
                if masters.contains(el.id) {
                    let stopped = walk(el.dataStart, dataEnd, unknownSize: el.size == nil && el.id != ID.segment)
                    if el.size == nil { next = stopped }
                } else {
                    let body = Array(bytes[min(el.dataStart, dataEnd)..<dataEnd])
                    switch el.id {
                    case ID.timecodeScale:
                        let scale = uint(body)
                        scaleNs = scale == 0 ? 1_000_000 : scale
                    case ID.trackNumber: track?.number = uint(body)
                    case ID.trackType: track?.type = uint(body)
                    case ID.codecID: track?.codecId = ascii(body)
                    case ID.codecPrivate: track?.codecPrivate = body
                    case ID.contentCompAlgo: compAlgo = uint(body)
                    case ID.contentCompSettings: compSettings = body
                    case ID.timecode: clusterTime = uint(body)
                    case ID.simpleBlock:
                        if let b = block(body, clusterTime: clusterTime) { raw.append((b, nil)) }
                    case ID.block: groupBlock = block(body, clusterTime: clusterTime)
                    case ID.blockDuration: groupDuration = uint(body)
                    default: break
                    }
                }
                if el.id == ID.contentCompression, track != nil {
                    if compAlgo == 3 { track?.stripped = compSettings } else { track?.compressed = true }
                }
                if el.id == ID.trackEntry, let done = track {
                    tracks.append(done)
                    track = nil
                }
                if el.id == ID.blockGroup { endGroup() }
                at = next
            }
            return end
        }
    }

    /// A block's track, absolute time in ticks and its one frame (subtitles are never laced).
    private static func block(_ body: [UInt8], clusterTime: Int) -> Block? {
        guard let v = vint(body, at: 0, stripMarker: true), v.length + 3 <= body.count else { return nil }
        let relative = body.u16(v.length)
        let signed = relative & 0x8000 != 0 ? relative - 0x10000 : relative
        let flags = Int(body[v.length + 2])
        let lacing = (flags >> 1) & 3
        var start = v.length + 3
        // Laced subtitle frames don't occur in practice: the first frame's bytes are kept as they are.
        if lacing != 0 { start += 1 }
        let data = start <= body.count ? Array(body[start...]) : []
        return Block(track: v.value, time: clusterTime + signed, data: data)
    }

    private static func element(_ bytes: [UInt8], at: Int, end: Int) -> Element? {
        guard let id = vint(bytes, at: at, stripMarker: false), at + id.length <= end,
              let size = vint(bytes, at: at + id.length, stripMarker: true)
        else { return nil }
        let dataStart = at + id.length + size.length
        guard dataStart <= end else { return nil }
        return Element(id: id.value, size: size.unknown ? nil : size.value, dataStart: dataStart)
    }

    /// An EBML variable-length integer at `at`: an element id keeps its
    /// length marker (`stripMarker` false), a size or track number loses it.
    private static func vint(_ bytes: [UInt8], at: Int,
                             stripMarker: Bool) -> (value: Int, length: Int, unknown: Bool)? {
        guard at >= 0, at < bytes.count, bytes[at] != 0 else { return nil }
        let first = Int(bytes[at])
        var length = 1
        var mask = 0x80
        while first & mask == 0 {
            mask >>= 1
            length += 1
        }
        guard at + length <= bytes.count else { return nil }
        var value = stripMarker ? first & (mask - 1) : first
        var allOnes = first & (mask - 1) == mask - 1
        for i in 1..<length {
            let b = Int(bytes[at + i])
            if b != 0xFF { allOnes = false }
            value = value &* 256 &+ b
        }
        return (value, length, stripMarker && allOnes)
    }

    private static func uint(_ body: [UInt8]) -> Int {
        body.reduce(0) { $0 &* 256 &+ Int($1) }
    }

    private static func ascii(_ body: [UInt8]) -> String {
        String(decoding: body.prefix { $0 != 0 }, as: UTF8.self)
    }
}
