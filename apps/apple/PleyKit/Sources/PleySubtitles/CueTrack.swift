import Foundation

/// Underlined text (WebVTT's `<u>`), an attribute of `TextCue.text`.
public enum SubtitleUnderline: AttributedStringKey {
    public typealias Value = Bool
    public static let name = "app.pley.subtitle.underline"
}

/// One cue of a text track: its time and its lines, `<i>` and `<b>` as
/// the text's `inlinePresentationIntent` (emphasized, strongly
/// emphasized) and `<u>` as `SubtitleUnderline`.
public struct TextCue: Equatable, Sendable {
    public var startMs: Int
    public var endMs: Int
    /// Lines apart by `\n`.
    public var text: AttributedString
    /// Shown at the top of the picture (WebVTT's `line:` in its top half), else at the bottom.
    public var top: Bool

    public init(startMs: Int, endMs: Int, text: AttributedString, top: Bool = false) {
        self.startMs = startMs
        self.endMs = endMs
        self.text = text
        self.top = top
    }

    /// The words without their styles.
    public var plain: String { String(text.characters) }
}

/// A picture shown, told apart from another by its track and sample.
public struct ShownPicture: Equatable, Sendable {
    /// Its track and sample: the same id is the same picture.
    public let id: String
    public let frame: PictureFrame

    public init(id: String, frame: PictureFrame) {
        self.id = id
        self.frame = frame
    }

    public static func == (a: ShownPicture, b: ShownPicture) -> Bool { a.id == b.id }
}

/// What the subtitle layer shows at one moment.
public enum ShownSubtitles: Equatable, Sendable {
    case none
    /// Text cues, earliest first.
    case text([TextCue])
    case picture(ShownPicture)

    public var isEmpty: Bool { self == .none }
}

/// A subtitle track's cues by time: text (the server's WebVTT) or
/// pictures (the server's `.mks`). Times are the track's own.
public protocol CueTrack: AnyObject {
    /// What shows at `ms` of the track's time; pictures are decoded here.
    func shown(at ms: Int) -> ShownSubtitles
    /// The next time after `ms` at which what shows changes; `nil` when nothing changes any more.
    func nextChangeAfter(_ ms: Int) -> Int?
}

/// The cues of one text track, by time. A track still being read out of
/// its file grows: each answer holds every cue found so far, and `merge`
/// keeps whichever reaches further, so an answer that arrives late never
/// takes cues away.
public final class TextCueTrack: CueTrack {
    public let cues: [TextCue]
    /// For each cue, the latest end of it and every cue before it: how far back a cue may still show.
    private let reach: [Int]

    public init(_ cues: [TextCue]) {
        self.cues = cues.filter { $0.endMs > $0.startMs }.enumerated().sorted { a, b in
            a.element.startMs != b.element.startMs ? a.element.startMs < b.element.startMs
                : a.element.endMs != b.element.endMs ? a.element.endMs < b.element.endMs : a.offset < b.offset
        }.map(\.element)
        var latest = Int.min
        reach = self.cues.map { cue in
            latest = max(latest, cue.endMs)
            return latest
        }
    }

    /// Where the last cue ends; 0 without cues.
    public var endMs: Int { reach.last ?? 0 }

    /// The first cue starting after `ms` (binary search).
    private func firstAfter(_ ms: Int) -> Int {
        var lo = 0
        var hi = cues.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if cues[mid].startMs <= ms { lo = mid + 1 } else { hi = mid }
        }
        return lo
    }

    /// The cues showing at `ms`, earliest first.
    public func cuesAt(_ ms: Int) -> [TextCue] {
        var out: [TextCue] = []
        var i = firstAfter(ms) - 1
        while i >= 0, reach[i] > ms {
            if cues[i].endMs > ms { out.append(cues[i]) }
            i -= 1
        }
        return out.reversed()
    }

    public func shown(at ms: Int) -> ShownSubtitles {
        let showing = cuesAt(ms)
        return showing.isEmpty ? .none : .text(showing)
    }

    public func nextChangeAfter(_ ms: Int) -> Int? {
        let after = firstAfter(ms)
        var next = after < cues.count ? cues[after].startMs : nil
        var i = after - 1
        while i >= 0, reach[i] > ms {
            if cues[i].endMs > ms { next = min(next ?? cues[i].endMs, cues[i].endMs) }
            i -= 1
        }
        return next
    }

    /// `more`, a later answer of the same track, unless it reaches less far than this one.
    public func merge(_ more: TextCueTrack) -> TextCueTrack {
        more.endMs > endMs || (more.endMs == endMs && more.cues.count > cues.count) ? more : self
    }
}

/// A picture track ready to show, a port of packages/picture-subtitles'
/// `picture-track.ts`: a sample shows from its time until the next sample,
/// or for its own duration when it carries one (VobSub's stop command,
/// DVB's page time-out, a Matroska block duration); an empty sample
/// clears. Decoded only when shown.
public final class PictureSubtitles: CueTrack {
    /// Codec ids of the picture subtitles the app draws, and the server's (ffprobe's) names for them.
    public static let codecs = ["S_HDMV/PGS": "hdmv_pgs_subtitle", "S_VOBSUB": "dvd_subtitle",
                                "S_DVBSUB": "dvb_subtitle"]

    private let track: PictureTrack
    private let decoder: any PictureDecoder
    private let times: [Int]
    /// Tells this track's pictures apart from another's.
    private let id: String

    public init(track: PictureTrack, decoder: any PictureDecoder, id: String = UUID().uuidString) {
        self.track = track
        self.decoder = decoder
        times = track.samples.map(\.timeMs)
        self.id = id
    }

    /// The decoder of `track`'s codec; `nil` for one this app doesn't draw.
    public static func decoder(for track: PictureTrack) -> (any PictureDecoder)? {
        let data = track.samples.map(\.data)
        switch track.codecId {
        case "S_HDMV/PGS": return PGSDecoder(samples: data)
        case "S_VOBSUB": return VobSubDecoder(samples: data, codecPrivate: track.codecPrivate)
        case "S_DVBSUB": return DVBDecoder(samples: data)
        default: return nil
        }
    }

    /// `track` with its codec's decoder; `nil` for a codec this app doesn't draw.
    public static func of(_ track: PictureTrack, id: String = UUID().uuidString) -> PictureSubtitles? {
        decoder(for: track).map { PictureSubtitles(track: track, decoder: $0, id: id) }
    }

    /// Samples in all.
    public var count: Int { times.count }

    /// The last sample starting at or before `ms`, as a count of those.
    private func upTo(_ ms: Int) -> Int {
        var lo = 0
        var hi = times.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if times[mid] <= ms { lo = mid + 1 } else { hi = mid }
        }
        return lo
    }

    /// The sample showing at `ms` (track time), or -1.
    public func sampleAt(_ ms: Int) -> Int {
        let i = upTo(ms) - 1
        guard i >= 0 else { return -1 }
        return ms < endOf(i) ? i : -1
    }

    /// When sample `i` stops showing; `Int.max` when it shows on.
    public func endOf(_ i: Int) -> Int {
        let start = times[i]
        let next = i + 1 < times.count ? times[i + 1] : Int.max
        let own = track.samples[i].durationMs ?? decoder.ownDurationMs(i)
        if let own, own > 0 { return min(next, start + own) }
        return next
    }

    /// What shows at `ms`: a frame, or `nil` when nothing does (an empty frame included).
    public func frameAt(_ ms: Int) -> (index: Int, frame: PictureFrame)? {
        let i = sampleAt(ms)
        guard i >= 0, let frame = decoder.decode(i), !frame.bitmaps.isEmpty else { return nil }
        return (i, frame)
    }

    /// When what shows changes after `ms`; `nil` when nothing does any more.
    public func nextChange(_ ms: Int) -> Int? {
        let i = sampleAt(ms)
        if i >= 0 {
            let end = endOf(i)
            return end == Int.max ? nil : end
        }
        let after = upTo(ms)
        return after < times.count ? times[after] : nil
    }

    public func shown(at ms: Int) -> ShownSubtitles {
        guard let (index, frame) = frameAt(ms) else { return .none }
        return .picture(ShownPicture(id: "\(id):\(index)", frame: frame))
    }

    public func nextChangeAfter(_ ms: Int) -> Int? { nextChange(ms) }
}
