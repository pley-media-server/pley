import Foundation

/// Files, tracks and times in words, as the web and the Android app say
/// them (apps/android `model/format`: Media.kt, Languages.kt, Durations.kt,
/// Sizes.kt), so every app labels a track or a version the same way.
///
/// The words are in the language shown (`ShownLanguage`), as Android's
/// `FormatWords` has them: codec, format and resolution names stay as they
/// are; units, counts and track words are translated, and numbers are
/// written in the shown locale.
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
        case 2: return localized("Stereo")
        case 3: return "2.1"
        case 6: return "5.1"
        case 7: return "6.1"
        case 8: return "7.1"
        default: return localized("\(channels)ch")
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
            let base = title != nil && lang == nil ? title! : lang ?? title ?? localized("Unknown")
            return base + extra + (details.isEmpty ? "" : " (\(details))")
        case .subtitle:
            let base = lang ?? title ?? localized("Unknown")
            let flags = [codec(s.codec).isEmpty ? nil : codec(s.codec), isForced(s) ? localized("Forced") : nil,
                         s.external ? localized("External") : nil].compactMap { $0 }.joined(separator: ", ")
            return base + extra + (flags.isEmpty ? "" : " (\(flags))")
        default:
            return [resolution(width: s.width, height: s.height), codec(s.codec).isEmpty ? nil : codec(s.codec),
                    dynamicRange(s), (s.bitDepth ?? 0) > 8 ? localized("\(s.bitDepth!)-bit") : nil]
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
        return parts.isEmpty ? localized("Version \(position + 1)") : parts.joined(separator: " · ")
    }

    /// A bitrate in bits a second: "8.2 Mbps", "12 Mbps", "640 kbps"; empty when unknown.
    public static func bitrate(_ bps: Int?) -> String {
        guard let bps, bps > 0 else { return "" }
        if bps >= 1_000_000 {
            let mbps = fixed(Double(bps) / 1_000_000, digits: bps >= 10_000_000 ? 0 : 1)
            return localized("\(mbps) Mbps")
        }
        let kbps = String(Int((Double(bps) / 1000).rounded()))
        return localized("\(kbps) kbps")
    }

    /// `value` with `digits` decimals, rounded as `%f` does, its decimal
    /// separator the shown locale's ("1.5" in English, "1,5" in French),
    /// without grouping, as Android's `fixed` writes it.
    public static func fixed(_ value: Double, digits: Int) -> String {
        let text = String(format: "%.\(digits)f", locale: Locale(identifier: "en_US_POSIX"), value)
        guard digits > 0, let separator = ShownLanguage.locale.decimalSeparator, separator != "." else { return text }
        return text.replacingOccurrences(of: ".", with: separator)
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

    /// "eng" → "English", "pt-BR" → "Portuguese (BR)", in the language
    /// shown ("Anglais" in French); an unknown code comes back
    /// upper-cased; `nil` without one.
    public static func language(_ code: String?) -> String? {
        let raw = (code ?? "").trimmingCharacters(in: .whitespaces)
        guard !raw.isEmpty else { return nil }
        let parts = raw.lowercased().split(whereSeparator: { $0 == "-" || $0 == "_" }).map(String.init)
        guard let english = names[parts[0]] else { return raw.count <= 3 ? raw.uppercased() : raw }
        let name = languageName(parts[0], english: english)
        return parts.count > 1 ? "\(name) (\(parts[1].uppercased()))" : name
    }

    /// What the language shown calls the language of `base` (a code of the
    /// table), `english` in English, as Android's `Languages.kt`: Pley's
    /// own words for the codes that name no one language (and Mandarin,
    /// which Foundation calls Chinese), Foundation's names for the others.
    private static func languageName(_ base: String, english: String) -> String {
        if ShownLanguage.language == .english { return english }
        switch base {
        case "und": return localized(keyed: LocalizedStringResource("language.und", defaultValue: "Unknown",
                                                                     bundle: .atURL(Bundle.module.bundleURL)))
        case "mul": return localized(keyed: LocalizedStringResource("language.mul", defaultValue: "Multiple",
                                                                     bundle: .atURL(Bundle.module.bundleURL)))
        case "zxx": return localized(keyed: LocalizedStringResource("language.zxx", defaultValue: "No linguistic content",
                                                                     bundle: .atURL(Bundle.module.bundleURL)))
        case "qaa": return localized(keyed: LocalizedStringResource("language.qaa", defaultValue: "Original",
                                                                     bundle: .atURL(Bundle.module.bundleURL)))
        case "cmn": return localized(keyed: LocalizedStringResource("language.cmn", defaultValue: "Mandarin",
                                                                     bundle: .atURL(Bundle.module.bundleURL)))
        default: break
        }
        let locale = ShownLanguage.locale
        let code = twoLetterCode(base)
        // Foundation gives nothing, or the code back, for a language it can't name: the English name stands in.
        guard let found = locale.localizedString(forLanguageCode: code), !found.isEmpty,
              found.lowercased() != code, found.lowercased() != base
        else { return english }
        // French and Spanish write languages in lower case ("allemand"); a name on its own starts with a capital.
        return found.prefix(1).uppercased(with: locale) + found.dropFirst()
    }

    /// A language's two-letter code (ISO 639-1), which Foundation names:
    /// "eng" → "en", "chi" → "zh"; a code without one as it is.
    private static func twoLetterCode(_ base: String) -> String {
        if base.count == 2 { return base }
        guard let name = names[base] else { return base }
        return names.first { $0.key.count == 2 && $0.value == name }?.key ?? base
    }

    // MARK: Times and sizes

    /// 1536 → "1.5 KB", 150 MiB → "150 MB": three significant digits, in
    /// units of 1024, up to TB; "1,5 Ko" in French, as Android writes them.
    public static func bytes(_ bytes: Int) -> String {
        var value = Double(bytes)
        var i = 0
        while value >= 1024 && i < 4 {
            value /= 1024
            i += 1
        }
        let number = fixed(value, digits: value >= 100 || i == 0 ? 0 : 1)
        switch i {
        case 0: return localized("\(number) B")
        case 1: return localized("\(number) KB")
        case 2: return localized("\(number) MB")
        case 3: return localized("\(number) GB")
        default: return localized("\(number) TB")
        }
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
        if minutes < 1 { return seconds(max(1, Int((Double(ms) / 1000).rounded()))) }
        if minutes < 60 { return localized("\(minutes) min") }
        return hoursMinutes(minutes)
    }

    /// "29s": a duration under a minute, short.
    static func seconds(_ n: Int) -> String { localized("\(n)s") }

    /// "1h 52m", "2h": a duration of an hour or more, short, from its minutes.
    static func hoursMinutes(_ minutes: Int) -> String {
        let h = minutes / 60, m = minutes % 60
        return m != 0 ? localized("\(h)h \(m)m") : localized("\(h)h")
    }

    /// An episode's "S1 E2"; empty without numbers.
    public static func episode(season: Int?, episode: Int?) -> String {
        switch (season, episode) {
        case (let s?, let e?): localized("S\(s) E\(e)")
        case (let s?, nil): localized("S\(s)")
        case (nil, let e?): localized("E\(e)")
        default: ""
        }
    }

    /// "26 min left", "1h 12m left"; empty without a duration or once nothing is left.
    public static func timeLeft(positionMs: Int, durationMs: Int?) -> String {
        guard let durationMs, durationMs > 0 else { return "" }
        let left = runtime(ms: max(0, durationMs - positionMs))
        return left.isEmpty ? "" : localized("\(left) left")
    }

    /// A duration for VoiceOver, in words: "34 minutes", "1 hour 12 minutes", "45 seconds".
    public static func spokenDuration(ms: Int?) -> String {
        guard let ms, ms > 0 else { return "" }
        let minutes = Int((Double(ms) / 60_000).rounded(.toNearestOrAwayFromZero))
        if minutes < 1 { return localized("\(max(1, Int((Double(ms) / 1000).rounded()))) seconds") }
        let h = minutes / 60, m = minutes % 60
        if h > 0 && m > 0 {
            let hours = localized("\(h) hours"), minutes = localized("\(m) minutes")
            // "1 hour 12 minutes"; Spanish puts "y" between them.
            return localized("\(hours) \(minutes)")
        }
        return h > 0 ? localized("\(h) hours") : localized("\(m) minutes")
    }

    /// "34 minutes left", for VoiceOver; empty once nothing is left.
    public static func spokenTimeLeft(positionMs: Int, durationMs: Int?) -> String {
        guard let durationMs, durationMs > 0 else { return "" }
        let left = spokenDuration(ms: max(0, durationMs - positionMs))
        return left.isEmpty ? "" : localized("\(left) left")
    }

    // MARK: Titles and counts

    /// "1 movie", "0 movies", "12,345 movies": the count with its noun, in
    /// English only (the nouns are the caller's). Words in the language
    /// shown count with `localized("\(n) movies")`, a plural of the catalog.
    public static func pluralize(_ n: Int, _ one: String, _ many: String? = nil) -> String {
        let number = n.formatted(.number.locale(ShownLanguage.locale))
        return "\(number) \(n == 1 ? one : many ?? one + "s")"
    }

    /// A listing's file line, sorted by bitrate: "1080p · 6.6 Mbps · 50.4 GB"; empty without one.
    public static func fileLine(_ media: MediaSummary?) -> String {
        guard let media else { return "" }
        return [resolution(width: media.width, height: media.height) ?? "", bitrate(media.bitrate),
                (media.size ?? 0) > 0 ? bytes(media.size!) : ""].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// "412 movies", "38 shows", "120 artists": what a library holds, by its kind.
    public static func libraryCount(_ library: Library) -> String {
        let n = library.itemCount
        return switch library.kind {
        case .movies: localized("\(n) movies")
        case .shows: localized("\(n) shows")
        case .music: localized("\(n) artists")
        default: localized("\(n) items")
        }
    }

    /// "S1 · E3", "E3", or "" with neither number: the screens' episode label.
    public static func episodeLabel(season: Int?, episode: Int?) -> String {
        switch (season, episode) {
        case (let s?, let e?): localized("S\(s) · E\(e)")
        case (let s?, nil): localized("S\(s)")
        case (nil, let e?): localized("E\(e)")
        default: ""
        }
    }

    /// "Season 2 Episode 5", for VoiceOver; the parts that are known.
    public static func spokenEpisode(season: Int?, episode: Int?) -> String {
        [season.map { $0 == 0 ? localized("Specials") : localized("Season \($0)") },
         episode.map { localized("Episode \($0)") }]
            .compactMap { $0 }.joined(separator: " ")
    }

    /// The short line under a tile, by kind: a year, "3 seasons", "S2 · E5", "Daft Punk · 2001".
    public static func cardSubtitle(_ item: ItemSummary) -> String {
        switch item.kind {
        case .movie:
            return item.year.map(String.init) ?? ""
        case .show:
            if let seasons = item.childCount, seasons > 0 { return localized("\(seasons) seasons") }
            if let episodes = item.leafCount { return localized("\(episodes) episodes") }
            return item.year.map(String.init) ?? ""
        case .season:
            return item.leafCount.map { localized("\($0) episodes") } ?? ""
        case .episode:
            return episodeLabel(season: item.parentIndex, episode: item.index)
        case .artist:
            return item.childCount.map { localized("\($0) albums") } ?? ""
        case .album:
            return [nonEmpty(item.parentTitle), item.year.map(String.init)].compactMap { $0 }.joined(separator: " · ")
        case .track:
            return [nonEmpty(item.grandparentTitle), nonEmpty(item.parentTitle)].compactMap { $0 }
                .joined(separator: " · ")
        default:
            return ""
        }
    }

    /// Started, not finished and not marked watched: its resume point is worth showing.
    public static func isInProgress(_ item: ItemSummary) -> Bool {
        guard let duration = item.durationMs else { return false }
        return !item.userState.watched && item.userState.positionMs > 0 && item.userState.positionMs < duration
    }

    /// How much of `item` has been played, 0 to 1.
    public static func progress(_ item: ItemSummary) -> Double {
        guard item.userState.positionMs > 0, let duration = item.durationMs, duration > 0 else { return 0 }
        return min(1, max(0, Double(item.userState.positionMs) / Double(duration)))
    }

    /// Episodes of a show or season not watched yet; 0 for other kinds.
    public static func unwatchedCount(_ item: ItemSummary) -> Int {
        guard let leaves = item.leafCount else { return 0 }
        let (left, overflow) = leaves.subtractingReportingOverflow(item.viewedLeafCount ?? 0)
        return overflow ? 0 : max(0, left)
    }

    /// What VoiceOver says for a tile, its label and its state apart:
    /// "Glasshouse, The Office, Season 2 Episode 5" and "26 minutes left";
    /// "Arrival, 2016" and "watched"; "Severance, 2 seasons" and "6 unwatched episodes".
    public static func tileSpoken(_ item: ItemSummary) -> (label: String, value: String) {
        let context: String
        if item.kind == .episode {
            context = [nonEmpty(item.grandparentTitle).flatMap { $0 == item.title ? nil : $0 },
                       nonEmpty(spokenEpisode(season: item.parentIndex, episode: item.index))]
                .compactMap { $0 }.joined(separator: ", ")
        } else {
            context = cardSubtitle(item)
        }
        let unwatched = unwatchedCount(item)
        let state: String
        if isInProgress(item) {
            state = spokenTimeLeft(positionMs: item.userState.positionMs, durationMs: item.durationMs)
        } else if item.userState.watched {
            state = localized("watched")
        } else if item.kind == .show || item.kind == .season, unwatched > 0 {
            state = localized("\(unwatched) unwatched episodes")
        } else {
            state = ""
        }
        return ([item.title, context].filter { !$0.isEmpty }.joined(separator: ", "), state)
    }

    private static func nonEmpty(_ text: String?) -> String? {
        guard let text, !text.isEmpty else { return nil }
        return text
    }
}
