import Foundation
import PleyCaps
import PleyModel

/// One audio option of the media AVPlayer opened (its `.audible` media
/// selection group), in the group's order, which is the file's track order.
public struct EngineAudioOption: Equatable, Sendable {
    /// Its place in the group.
    public var position: Int
    /// Its language as the file names it ("en", "fre"…); `nil` when it names none.
    public var language: String?

    public init(position: Int, language: String?) {
        self.position = position
        self.language = language
    }
}

/// The player's audio choices: the menu's words, and which of the media's
/// options a server stream is. Pure: it reads the file and what the engine
/// lists.
///
/// In direct play the file's own tracks are switched by AVPlayer. The
/// server numbers streams as ffprobe does, and an MP4's audio tracks come
/// in the same order in AVPlayer's audible group, so a stream is the
/// option at its place among the file's audio streams, checked against
/// the language; when that check fails, the one option of its language, if
/// exactly one has it. Subtitle files beside the media (from 1000) aren't
/// in the file.
public enum TrackChoices {
    /// Streams beside the media are numbered from 1000.
    static let externalFrom = 1000

    /// The audio menu: each of `file`'s audio streams by its label, the one
    /// playing (`playing`, the server's `audio_index`) selected.
    public static func audioMenu(_ file: MediaFile?, playing: Int?) -> [MenuChoice] {
        let audio = file?.streams(.audio) ?? []
        let selected = PlaybackPlan.playedAudio(file, index: playing)?.index
        return audio.map { stream in
            MenuChoice(id: String(stream.index), label: Words.stream(stream), selected: stream.index == selected)
        }
    }

    /// The file's own audio streams, in the file's order.
    static func ownAudio(_ file: MediaFile?) -> [MediaStream] {
        (file?.streams(.audio) ?? []).filter { !$0.external && $0.index < externalFrom }.sorted { $0.index < $1.index }
    }

    /// The option of `options` that is `file`'s audio stream `index`; `nil` when none is surely it.
    public static func option(for index: Int, in file: MediaFile?, options: [EngineAudioOption]) -> EngineAudioOption? {
        let own = ownAudio(file)
        guard let at = own.firstIndex(where: { $0.index == index }) else { return nil }
        let stream = own[at]
        if at < options.count, agrees(stream, options[at]) { return options[at] }
        let candidates = options.filter { agrees(stream, $0) && languageKnown(stream, $0) }
        return candidates.count == 1 ? candidates[0] : nil
    }

    /// The stream of `file` that `option` is, the other way round.
    public static func stream(for option: EngineAudioOption, in file: MediaFile?,
                              options: [EngineAudioOption]) -> MediaStream? {
        ownAudio(file).first { stream in self.option(for: stream.index, in: file, options: options) == option }
    }

    /// Whether `stream` can be `option`: the same language where both name one.
    static func agrees(_ stream: MediaStream, _ option: EngineAudioOption) -> Bool {
        guard let ours = normalized(stream.language), let theirs = normalized(option.language) else { return true }
        return ours == theirs
    }

    private static func languageKnown(_ stream: MediaStream, _ option: EngineAudioOption) -> Bool {
        normalized(stream.language) != nil && normalized(option.language) != nil
    }

    /// A language's ISO 639-2 code, from ISO 639-1 ("en"), 639-2/T or /B ("fra", "fre") or a BCP 47 tag ("en-US").
    static func normalized(_ language: String?) -> String? {
        guard let language else { return nil }
        let base = language.lowercased().split(whereSeparator: { $0 == "-" || $0 == "_" }).first.map(String.init) ?? ""
        if base.isEmpty || base == "und" || base == "mul" || base == "zxx" { return nil }
        if let three = twoToThree[base] { return three }
        return bibliographic[base] ?? base
    }

    /// ISO 639-1 to 639-2 for the languages files name most.
    private static let twoToThree: [String: String] = [
        "en": "eng", "fr": "fre", "de": "ger", "es": "spa", "it": "ita", "pt": "por", "nl": "dut", "sv": "swe",
        "da": "dan", "no": "nor", "nb": "nob", "nn": "nno", "fi": "fin", "pl": "pol", "cs": "cze", "sk": "slo",
        "hu": "hun", "ro": "rum", "el": "gre", "tr": "tur", "ru": "rus", "uk": "ukr", "ja": "jpn", "ko": "kor",
        "zh": "chi", "ar": "ara", "he": "heb", "hi": "hin", "th": "tha", "vi": "vie", "id": "ind", "ms": "may",
        "is": "ice", "ga": "gle", "cy": "wel", "hr": "hrv", "sr": "srp", "sl": "slv", "bg": "bul", "et": "est",
        "lv": "lav", "lt": "lit", "fa": "per", "ta": "tam", "te": "tel", "ca": "cat", "eu": "baq", "gl": "glg",
    ]

    /// ISO 639-2/T codes to their /B twins, so either spelling matches.
    private static let bibliographic: [String: String] = [
        "fra": "fre", "deu": "ger", "nld": "dut", "ces": "cze", "slk": "slo", "ron": "rum", "ell": "gre",
        "zho": "chi", "isl": "ice", "cym": "wel", "fas": "per", "eus": "baq", "msa": "may", "sqi": "alb",
        "hye": "arm", "kat": "geo", "mkd": "mac", "mya": "bur", "bod": "tib", "mri": "mao",
    ]
}
