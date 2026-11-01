import Foundation

/// Files, tracks and times in words, as the web and the Android app say
/// them (apps/android `model/format`: Media.kt, Languages.kt, Durations.kt,
/// Sizes.kt), so every app labels a track or a version the same way.
public enum Words {
    private static let codecs: [String: String] = [
        "h264": "H.264", "hevc": "HEVC", "h265": "HEVC", "av1": "AV1", "vp9": "VP9", "vp8": "VP8",
        "mpeg4": "MPEG-4", "mpeg2video": "MPEG-2", "mpeg1video": "MPEG-1", "vc1": "VC-1", "wmv3": "WMV",
        "msmpeg4v3": "DivX", "theora": "Theora", "prores": "ProRes", "aac": "AAC", "ac3": "AC3",
        "eac3": "EAC3", "dts": "DTS", "truehd": "TrueHD", "mlp": "MLP", "flac": "FLAC", "alac": "ALAC",
        "mp3": "MP3", "mp2": "MP2", "opus": "Opus", "vorbis": "Vorbis", "wmav2": "WMA",
        "pcm_s16le": "PCM", "pcm_s24le": "PCM", "pcm_s32le": "PCM", "pcm_f32le": "PCM", "subrip": "SRT",
        "srt": "SRT", "ass": "ASS", "ssa": "SSA", "webvtt": "WebVTT", "mov_text": "MOV Text",
        "hdmv_pgs_subtitle": "PGS", "pgssub": "PGS", "dvd_subtitle": "VobSub", "dvdsub": "VobSub",
        "dvb_subtitle": "DVB", "dvb_teletext": "Teletext", "eia_608": "CEA-608",
    ]

    /// ffprobe's codec name as people know it: "h264" → "H.264", "hdmv_pgs_subtitle" → "PGS"; others upper-cased.
    public static func codec(_ codec: String?) -> String {
        guard let codec, !codec.isEmpty else { return "" }
        return codecs[codec.lowercased()] ?? codec.uppercased()
    }

    /// 2 → "Stereo", 6 → "5.1", 8 → "7.1", 5 → "5ch"; the layout's name without a count.
    public static func channels(_ channels: Int?, layout: String? = nil) -> String {
        guard let channels, channels != 0 else { return layout ?? "" }
        switch channels {
        case 1: return "Mono"
        case 2: return "Stereo"
        case 3: return "2.1"
        case 6: return "5.1"
        case 7: return "6.1"
        case 8: return "7.1"
        default: return "\(channels)ch"
        }
    }

    /// "4K", "1440p", "1080p", "720p", "576p", "480p" or "SD", cropped pictures included; `nil` without a size.
    public static func resolution(width: Int?, height: Int?) -> String? {
        let w = width ?? 0, h = height ?? 0
        if w == 0 && h == 0 { return nil }
        if w >= 3800 || h >= 2100 { return "4K" }
        if w >= 2500 || h >= 1400 { return "1440p" }
        if w >= 1800 || h >= 1000 { return "1080p" }
        if w >= 1200 || h >= 700 { return "720p" }
        if h >= 540 { return "576p" }
        if h >= 470 { return "480p" }
        return "SD"
    }

    /// "Dolby Vision P8.1 + HDR10+", "HDR10+", "HDR10", "HLG", "HDR" (an
    /// older server's flag), or `nil` for SDR: the server's names, as the
    /// web and the other apps say them.
    public static func dynamicRange(_ stream: MediaStream?) -> String? {
        guard let stream else { return nil }
        switch stream.dynamicRange {
        case .dolbyVision?:
            let profile = dolbyVisionProfile(stream.dvProfile, stream.dvBlCompat, stream.dvEl).map { " \($0)" } ?? ""
            return "Dolby Vision" + profile + (stream.hdr10plus == true ? " + HDR10+" : "")
        case .hdr10Plus?: return "HDR10+"
        case .hdr10?: return "HDR10"
        case .hlg?: return "HLG"
        case .sdr?: return nil
        default: return stream.hdr == true ? "HDR" : nil
        }
    }

    /// "P8.1", "P5", "P7 FEL": a Dolby Vision profile as the server names it.
    private static func dolbyVisionProfile(_ profile: Int?, _ compat: Int?, _ layer: String?) -> String? {
        guard let profile else { return nil }
        let base = compat.flatMap { profile >= 8 && [1, 2, 4].contains($0) ? ".\($0)" : nil } ?? ""
        let el = layer.flatMap { profile == 7 && !$0.isEmpty ? " \($0.uppercased())" : nil } ?? ""
        return "P\(profile)\(base)\(el)"
    }

    /// Dolby Atmos, as ffprobe names it in the stream's profile or a title says.
    public static func isAtmos(_ stream: MediaStream?) -> Bool {
        guard let stream, stream.kind == .audio else { return false }
        return (stream.profile ?? "").localizedCaseInsensitiveContains("atmos")
            || (stream.title ?? "").localizedCaseInsensitiveContains("atmos")
    }

    /// An audio stream's format: "TrueHD Atmos 7.1", "EAC3 5.1", "AAC Stereo".
    public static func audioFormat(_ stream: MediaStream) -> String {
        [codec(stream.codec), isAtmos(stream) ? "Atmos" : "", channels(stream.channels, layout: stream.channelLayout)]
            .filter { !$0.isEmpty }.joined(separator: " ")
    }

    /// Forced subtitles: flagged so, or only named so ("English (Forced)",
    /// "Signs & Songs"), as the server counts them when it chooses.
    public static func isForced(_ stream: MediaStream) -> Bool {
        if stream.forced { return true }
        let words = (stream.title ?? "").lowercased().split { !($0.isLetter || $0.isNumber) }
        return words.contains { $0 == "forced" || $0 == "signs" }
    }

    /// A track as the pickers list it. Audio: "English (EAC3 5.1)",
    /// "English – Commentary (AAC Stereo)". Subtitles: "French – SDH (SRT,
    /// Forced)". Video: "4K HEVC Dolby Vision P8.1 10-bit".
    public static func stream(_ s: MediaStream) -> String {
        let lang = language(s.language)
        let title = s.title.flatMap { $0.isEmpty ? nil : $0 }
        let extra: String = if let title, let lang, !title.lowercased().contains(lang.lowercased()) {
            " – \(title)"
        } else {
            ""
        }
        switch s.kind {
        case .audio:
            let details = audioFormat(s)
            let base = title != nil && lang == nil ? title! : lang ?? title ?? "Unknown"
            return base + extra + (details.isEmpty ? "" : " (\(details))")
        case .subtitle:
            let base = lang ?? title ?? "Unknown"
            let flags = [codec(s.codec).isEmpty ? nil : codec(s.codec), isForced(s) ? "Forced" : nil,
                         s.external ? "External" : nil].compactMap { $0 }.joined(separator: ", ")
            return base + extra + (flags.isEmpty ? "" : " (\(flags))")
        default:
            return [resolution(width: s.width, height: s.height), codec(s.codec).isEmpty ? nil : codec(s.codec),
                    dynamicRange(s), (s.bitDepth ?? 0) > 8 ? "\(s.bitDepth!)-bit" : nil]
                .compactMap { $0 }.joined(separator: " ")
        }
    }

    /// The audio track a file starts with: the profile's choice, else the default, else the first.
    public static func startingAudio(_ file: MediaFile) -> MediaStream? {
        let audio = file.streams(.audio)
        return audio.first { $0.index == file.selectedAudio } ?? audio.first(where: \.default) ?? audio.first
    }

    /// A version in the picker: "Final Cut · 4K · HEVC · MKV · 58.2 GB", else "Version 2".
    public static func version(_ file: MediaFile, position: Int) -> String {
        let parts = [file.edition ?? "", resolution(width: file.width, height: file.height) ?? "",
                     codec(file.videoCodec), file.container?.uppercased() ?? "",
                     file.size > 0 ? bytes(file.size) : ""].filter { !$0.isEmpty }
        return parts.isEmpty ? "Version \(position + 1)" : parts.joined(separator: " · ")
    }

    /// A bitrate in bits a second: "8.2 Mbps", "12 Mbps", "640 kbps"; empty when unknown.
    public static func bitrate(_ bps: Int?) -> String {
        guard let bps, bps > 0 else { return "" }
        if bps >= 1_000_000 {
            let mbps = Double(bps) / 1_000_000
            return String(format: bps >= 10_000_000 ? "%.0f Mbps" : "%.1f Mbps", locale: Locale(identifier: "en_US_POSIX"),
                          mbps)
        }
        return "\(Int((Double(bps) / 1000).rounded())) kbps"
    }

    /// A file's video and sound in one line: "4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1".
    public static func videoLine(_ file: MediaFile) -> String {
        let video = file.video
        return [resolution(width: file.width ?? video?.width, height: file.height ?? video?.height) ?? "",
                codec(file.videoCodec ?? video?.codec), dynamicRange(video) ?? "",
                startingAudio(file).map(audioFormat) ?? ""]
            .filter { !$0.isEmpty }.joined(separator: " · ")
    }

    // MARK: Languages

    private static let names: [String: String] = [
        "afr": "Afrikaans", "alb": "Albanian", "sqi": "Albanian", "amh": "Amharic", "ara": "Arabic",
        "arm": "Armenian", "hye": "Armenian", "aze": "Azerbaijani", "baq": "Basque", "eus": "Basque",
        "bel": "Belarusian", "ben": "Bengali", "bos": "Bosnian", "bre": "Breton", "bul": "Bulgarian",
        "bur": "Burmese", "mya": "Burmese", "cat": "Catalan", "chi": "Chinese", "zho": "Chinese",
        "cze": "Czech", "ces": "Czech", "dan": "Danish", "dut": "Dutch", "nld": "Dutch", "eng": "English",
        "est": "Estonian", "fao": "Faroese", "fil": "Filipino", "fin": "Finnish", "fre": "French",
        "fra": "French", "geo": "Georgian", "kat": "Georgian", "ger": "German", "deu": "German",
        "gle": "Irish", "glg": "Galician", "gre": "Greek", "ell": "Greek", "guj": "Gujarati",
        "heb": "Hebrew", "hin": "Hindi", "hrv": "Croatian", "hun": "Hungarian", "ice": "Icelandic",
        "isl": "Icelandic", "ind": "Indonesian", "ita": "Italian", "jpn": "Japanese", "kan": "Kannada",
        "kaz": "Kazakh", "khm": "Khmer", "kor": "Korean", "kur": "Kurdish", "lao": "Lao", "lat": "Latin",
        "lav": "Latvian", "lit": "Lithuanian", "ltz": "Luxembourgish", "mac": "Macedonian",
        "mkd": "Macedonian", "mal": "Malayalam", "mao": "Maori", "mri": "Maori", "mar": "Marathi",
        "may": "Malay", "msa": "Malay", "mlt": "Maltese", "mon": "Mongolian", "nep": "Nepali",
        "nob": "Norwegian Bokmål", "nno": "Norwegian Nynorsk", "nor": "Norwegian", "per": "Persian",
        "fas": "Persian", "pol": "Polish", "por": "Portuguese", "pan": "Punjabi", "rum": "Romanian",
        "ron": "Romanian", "rus": "Russian", "slo": "Slovak", "slk": "Slovak", "slv": "Slovenian",
        "som": "Somali", "spa": "Spanish", "srp": "Serbian", "swa": "Swahili", "swe": "Swedish",
        "tam": "Tamil", "tel": "Telugu", "tgl": "Tagalog", "tha": "Thai", "tib": "Tibetan",
        "bod": "Tibetan", "tur": "Turkish", "ukr": "Ukrainian", "urd": "Urdu", "uzb": "Uzbek",
        "vie": "Vietnamese", "wel": "Welsh", "cym": "Welsh", "yid": "Yiddish", "zul": "Zulu",
        "yue": "Cantonese", "cmn": "Mandarin", "nan": "Min Nan", "hat": "Haitian Creole",
        "haw": "Hawaiian", "ibo": "Igbo", "yor": "Yoruba", "hau": "Hausa", "sin": "Sinhala",
        "pus": "Pashto", "tat": "Tatar", "kir": "Kyrgyz", "tuk": "Turkmen", "tgk": "Tajik",
        "und": "Unknown", "mul": "Multiple", "zxx": "No linguistic content", "qaa": "Original",
        // ISO 639-1
        "af": "Afrikaans", "sq": "Albanian", "am": "Amharic", "ar": "Arabic", "hy": "Armenian",
        "az": "Azerbaijani", "eu": "Basque", "be": "Belarusian", "bn": "Bengali", "bs": "Bosnian",
        "bg": "Bulgarian", "my": "Burmese", "ca": "Catalan", "zh": "Chinese", "cs": "Czech",
        "da": "Danish", "nl": "Dutch", "en": "English", "et": "Estonian", "fi": "Finnish",
        "fr": "French", "ka": "Georgian", "de": "German", "ga": "Irish", "gl": "Galician",
        "el": "Greek", "gu": "Gujarati", "he": "Hebrew", "hi": "Hindi", "hr": "Croatian",
        "hu": "Hungarian", "is": "Icelandic", "id": "Indonesian", "it": "Italian", "ja": "Japanese",
        "kn": "Kannada", "kk": "Kazakh", "km": "Khmer", "ko": "Korean", "ku": "Kurdish", "lo": "Lao",
        "la": "Latin", "lv": "Latvian", "lt": "Lithuanian", "lb": "Luxembourgish", "mk": "Macedonian",
        "ml": "Malayalam", "mi": "Maori", "mr": "Marathi", "ms": "Malay", "mt": "Maltese",
        "mn": "Mongolian", "ne": "Nepali", "nb": "Norwegian Bokmål", "nn": "Norwegian Nynorsk",
        "no": "Norwegian", "fa": "Persian", "pl": "Polish", "pt": "Portuguese", "pa": "Punjabi",
        "ro": "Romanian", "ru": "Russian", "sk": "Slovak", "sl": "Slovenian", "so": "Somali",
        "es": "Spanish", "sr": "Serbian", "sw": "Swahili", "sv": "Swedish", "ta": "Tamil",
        "te": "Telugu", "tl": "Tagalog", "th": "Thai", "bo": "Tibetan", "tr": "Turkish",
        "uk": "Ukrainian", "ur": "Urdu", "uz": "Uzbek", "vi": "Vietnamese", "cy": "Welsh",
        "yi": "Yiddish", "zu": "Zulu",
    ]

    /// "eng" → "English", "pt-BR" → "Portuguese (BR)"; an unknown code comes back upper-cased; `nil` without one.
    public static func language(_ code: String?) -> String? {
        let raw = (code ?? "").trimmingCharacters(in: .whitespaces)
        guard !raw.isEmpty else { return nil }
        let parts = raw.lowercased().split(whereSeparator: { $0 == "-" || $0 == "_" }).map(String.init)
        guard let name = names[parts[0]] else { return raw.count <= 3 ? raw.uppercased() : raw }
        return parts.count > 1 ? "\(name) (\(parts[1].uppercased()))" : name
    }

    // MARK: Times and sizes

    /// 1536 → "1.5 KB", 150 MiB → "150 MB": three significant digits, in units of 1024, up to TB.
    public static func bytes(_ bytes: Int) -> String {
        let units = ["B", "KB", "MB", "GB", "TB"]
        var value = Double(bytes)
        var i = 0
        while value >= 1024 && i < units.count - 1 {
            value /= 1024
            i += 1
        }
        let digits = value >= 100 || i == 0 ? 0 : 1
        return String(format: "%.\(digits)f", locale: Locale(identifier: "en_US_POSIX"), value) + " " + units[i]
    }

    /// Milliseconds → "1:02:03" or "2:03".
    public static func clock(ms: Int) -> String {
        let s = max(ms, 0) / 1000
        let h = s / 3600, m = (s % 3600) / 60, sec = s % 60
        let minutes = h > 0 ? String(format: "%02d", m) : String(m)
        return (h > 0 ? "\(h):" : "") + minutes + ":" + String(format: "%02d", sec)
    }

    /// A runtime as the screens show it: "58 min" under an hour, "1h 52m"
    /// from an hour, "29s" below half a minute; empty without one.
    public static func runtime(ms: Int?) -> String {
        guard let ms, ms > 0 else { return "" }
        let minutes = Int((Double(ms) / 60_000).rounded(.toNearestOrAwayFromZero))
        if minutes < 1 { return "\(max(1, Int((Double(ms) / 1000).rounded())))s" }
        if minutes < 60 { return "\(minutes) min" }
        let h = minutes / 60, m = minutes % 60
        return m != 0 ? "\(h)h \(m)m" : "\(h)h"
    }

    /// An episode's "S1 E2"; empty without numbers.
    public static func episode(season: Int?, episode: Int?) -> String {
        [season.map { "S\($0)" }, episode.map { "E\($0)" }].compactMap { $0 }.joined(separator: " ")
    }
}
