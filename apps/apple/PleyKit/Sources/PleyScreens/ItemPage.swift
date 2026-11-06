import Foundation
import PleyModel

/// What an item page is made from: the item, the file it plays, and what
/// was loaded beside it (Android's `PageInput`).
public struct PageInput: Sendable {
    public var detail: ItemDetail
    /// The file shown and played: the one picked, else the one this device plays best.
    public var file: MediaFile?
    /// Shows and seasons: the episode Play plays, once asked (`onDeckKnown`).
    public var onDeck: ItemSummary?
    public var onDeckKnown: Bool
    /// The show's seasons (a show's own, a season's siblings); `nil` while they load.
    public var seasons: [ItemSummary]?
    public var selectedSeason: Int?
    /// The selected season's episodes; `nil` while they load.
    public var episodes: [ItemSummary]?
    public var related: [ItemSummary]
    public var partOf: [CollectionRef]
    /// In My List as last known here; `nil`: as the details say.
    public var inWatchlist: Bool?
    public var locale: Locale

    public init(detail: ItemDetail, file: MediaFile? = nil, onDeck: ItemSummary? = nil, onDeckKnown: Bool = false,
                seasons: [ItemSummary]? = nil, selectedSeason: Int? = nil, episodes: [ItemSummary]? = nil,
                related: [ItemSummary] = [], partOf: [CollectionRef] = [], inWatchlist: Bool? = nil,
                locale: Locale = .current) {
        self.detail = detail
        self.file = file ?? detail.files.first
        self.onDeck = onDeck
        self.onDeckKnown = onDeckKnown
        self.seasons = seasons
        self.selectedSeason = selectedSeason
        self.episodes = episodes
        self.related = related
        self.partOf = partOf
        self.inWatchlist = inWatchlist
        self.locale = locale
    }
}

/// One part of the line of facts under the title; the screens put "·"
/// between them.
public enum MetaPart: Hashable, Sendable {
    /// A year, an air date, "3 seasons", "2021-2025", an edition, a runtime.
    case text(String)
    /// "PG-13", in a box.
    case contentRating(String)
    /// The critics' rating, "7.6", after a star.
    case rating(String)
    /// "4K", "DV P8.1", "Atmos".
    case badges([String])

    /// As VoiceOver reads it.
    public var spoken: String {
        switch self {
        case .text(let text): text
        case .contentRating(let rating): localized("rated \(rating)")
        case .rating(let rating): localized("\(rating) out of 10")
        case .badges(let badges): badges.joined(separator: ", ")
        }
    }
}

/// What the main button does: Play, Resume, a show's next episode, or nothing.
public struct PrimaryAction: Hashable, Sendable {
    public enum Kind: Hashable, Sendable { case play, resume, unavailable }

    public var kind: Kind
    /// Shows and seasons: the episode it plays, "S1 · E3".
    public var episode: String?
    /// How far it was played, 0 to 1.
    public var progress: Double
    /// "1h 33m left" while resuming; empty otherwise.
    public var timeLeft: String

    public init(_ kind: Kind, episode: String? = nil, progress: Double = 0, timeLeft: String = "") {
        self.kind = kind
        self.episode = episode
        self.progress = progress
        self.timeLeft = timeLeft
    }

    /// "Play", "Resume", "Play S1 · E3", "Resume S2 · E5", "Nothing to play".
    public var label: String {
        switch kind {
        case .unavailable: localized("Nothing to play")
        case .play: episode.map { localized("Play \($0)") } ?? localized("Play")
        case .resume: episode.map { localized("Resume \($0)") } ?? localized("Resume")
        }
    }
}

/// Where an episode or a season is: its show, and an episode's season.
public struct ItemContext: Hashable, Sendable {
    public var showId: Int?
    public var showTitle: String?
    public var seasonId: Int?
    /// "Season 2", "Specials".
    public var seasonTitle: String?
    /// An episode's "S2 · E5".
    public var episodeLabel: String?
    /// An episode's "Season 2 · Episode 5".
    public var episodeLongLabel: String?
}

/// A choice in a picker: a version (its file id) or a track (its stream index).
public struct TrackChoice: Hashable, Sendable, Identifiable {
    public var id: Int
    public var label: String
    public var selected: Bool
}

/// The version, audio and subtitle pickers (Android's `TrackChoices`). A
/// list with nothing to choose is empty.
public struct TrackChoices: Hashable, Sendable {
    /// Versions, when there are several.
    public var versions: [TrackChoice]
    /// Audio tracks, when there are two or more; none is selected while the audio is automatic.
    public var audio: [TrackChoice]
    /// Subtitle tracks; "Off" goes first on screen, chosen when none of these is.
    public var subtitles: [TrackChoice]
    public var versionId: Int?
    /// No track is picked: each device plays its best of the language, and
    /// "Automatic (English)" shows first, ticked.
    public var automaticAudio: Bool
    /// "Automatic (English)", or "Automatic" when the tracks don't say.
    public var automaticLabel: String
    /// The picked audio track; `nil` while automatic.
    public var audioIndex: Int?
    /// The subtitle track; `nil`: off.
    public var subtitleIndex: Int?

    public var versionLabel: String? { versions.first(where: \.selected)?.label }
    public var audioLabel: String? {
        automaticAudio ? automaticLabel : audio.first(where: \.selected)?.label
    }
    public var subtitleLabel: String { subtitles.first(where: \.selected)?.label ?? localized("Off") }
}

/// "Directors: A, B, C, D and 2 more".
public struct CrewLine: Hashable, Sendable {
    public var role: CreditRole
    /// "Director", "Directors", "Created by".
    public var label: String
    /// Four at most.
    public var people: [Credit]
    /// How many more there are.
    public var more: Int

    /// "A, B, C, D and 2 more".
    public var names: String {
        let names = people.map(\.name).joined(separator: ", ")
        return more > 0 ? names + " " + localized("and \(more) more") : names
    }
}

/// The Details list: who made it, and facts.
public struct ItemDetails: Hashable, Sendable {
    public var crew: [CrewLine]
    /// When it differs from the title.
    public var originalTitle: String?
    /// Not for episodes, whose date is in the line of facts.
    public var released: String?
    public var studio: String?
    /// "Network" for shows and episodes, else "Studio".
    public var studioLabel: String
    /// Times played, when any.
    public var plays: Int?
    /// Each version in long form, the one that plays selected.
    public var versions: [TrackChoice]

    /// The facts under Details, labelled: original title, released, studio
    /// or network, plays, and the versions when there are several.
    public var rows: [InfoRow] {
        var rows: [InfoRow] = []
        if let originalTitle { rows.append(InfoRow(label: localized("Original title"), value: originalTitle)) }
        if let released { rows.append(InfoRow(label: localized("Released"), value: released)) }
        if let studio { rows.append(InfoRow(label: studioLabel, value: studio)) }
        if let plays { rows.append(InfoRow(label: localized("Plays"), value: String(plays))) }
        if versions.count > 1 {
            rows.append(InfoRow(label: localized("Versions"), value: versions.map(\.label).joined(separator: ", ")))
        }
        return rows
    }

    public var isEmpty: Bool {
        crew.isEmpty && originalTitle == nil && released == nil && studio == nil && plays == nil && versions.isEmpty
    }
}

/// A fact about a file.
public struct InfoRow: Hashable, Sendable {
    /// "File", "Container", "Size", "Duration", "Bitrate", "Video", "Audio".
    public var label: String
    public var value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

/// A stream of a file, as the web's table has it.
public struct StreamInfo: Hashable, Sendable {
    /// "Video", "Audio", "Subtitles".
    public var kind: String
    public var codec: String
    public var language: String?
    public var title: String?
    /// "4K · 3840×2160 · 23.976 fps · 10-bit · Dolby Vision P8.1 · Main 10".
    public var details: String
}

/// A file in Media info.
public struct FileInfo: Hashable, Sendable, Identifiable {
    public var id: Int
    /// Its edition, or "Version 2" when there are several; `nil` for the only one.
    public var title: String?
    public var rows: [InfoRow]
    public var streams: [StreamInfo]
}

/// A season among a show's: "Season 2", "Specials", with "8 episodes".
public struct SeasonTab: Hashable, Sendable, Identifiable {
    public var id: Int
    public var title: String
    public var episodes: String?
}

/// An episode in a list, its words worked out.
public struct EpisodeLine: Hashable, Sendable, Identifiable {
    public var id: Int { item.id }
    public var item: ItemSummary
    /// "5. Glasshouse".
    public var title: String
    /// "58 min" or "26 min left".
    public var length: String
    /// "58 min · Mar 5, 2024".
    public var line: String
    /// "March 5, 2024".
    public var aired: String?
    /// "Season 2 · Episode 5".
    public var longLabel: String
    public var summary: String?
    public var progress: Double
    public var watched: Bool
    /// Its still, else its show's backdrop.
    public var pictures: [String]
    /// What VoiceOver says: "Episode 5, Glasshouse" and "58 minutes, watched".
    public var spokenLabel: String
    public var spokenValue: String
}

/// A show's seasons and the episodes of one.
public struct SeasonsSection: Hashable, Sendable {
    /// The show's seasons; empty on an episode's page, which lists its own season.
    public var seasons: [SeasonTab]
    public var selected: Int?
    /// The selected season's episodes; `nil` while they load.
    public var episodes: [EpisodeLine]?
    /// The episode to start from: the one Play plays, or the page's own.
    public var focus: Int?

    public var selectedTab: SeasonTab? { seasons.first { $0.id == selected } }
}

/// An item's page (movie, show, season, episode), every word worked out:
/// a port of Android's `itemPage`.
public struct ItemPage: Hashable, Sendable {
    public var id: Int
    public var kind: ItemKind
    /// The item as listings carry it: its pictures, and what its actions act on.
    public var item: ItemSummary
    /// A season's is "Season 2" or "Specials".
    public var title: String
    public var context: ItemContext?
    public var meta: [MetaPart]
    public var genres: [String]
    public var partOf: [CollectionRef]
    public var primary: PrimaryAction
    /// "From start" plays from the beginning what Play would resume.
    public var fromStart: Bool
    /// Watched: a movie or episode, or every episode of a show or season.
    public var watched: Bool
    /// Whether it can be marked watched (movies, shows, seasons, episodes).
    public var markable: Bool
    /// In My List; `nil` where My List doesn't take it.
    public var myList: Bool?
    /// The profile's own rating, 1 to 10 (a half star each); `nil`: unrated.
    public var userRating: Int?
    public var rateable: Bool
    public var tagline: String?
    public var summary: String?
    public var tracks: TrackChoices?
    /// The first trailer among the extras.
    public var trailer: ItemSummary?
    public var details: ItemDetails
    public var mediaInfo: [FileInfo]
    public var extras: [ItemSummary]
    /// "Cast", or "Guest Stars" for an episode's.
    public var castTitle: String
    /// Each person once, in billing order.
    public var cast: [Credit]
    /// More Like This: movies and shows.
    public var related: [ItemSummary]
    public var seasons: SeasonsSection?
    /// Server paths of its poster and backdrop, the best first.
    public var posters: [String]
    public var backdrops: [String]

    /// The line of facts for VoiceOver.
    public var spokenMeta: String { meta.map(\.spoken).joined(separator: ", ") }

    /// The page of `input`'s item.
    public init(_ input: PageInput) {
        let d = input.detail
        let kind = d.kind
        let video = kind == .movie || kind == .episode || kind == .extra
        let container = kind == .show || kind == .season
        let state = d.userState
        let resumable = video && !state.watched && state.positionMs > 0
        if video && d.files.isEmpty {
            primary = PrimaryAction(.unavailable)
        } else if resumable {
            primary = PrimaryAction(.resume, progress: Self.fraction(state.positionMs, d.durationMs),
                                    timeLeft: Words.timeLeft(positionMs: state.positionMs, durationMs: d.durationMs))
        } else if video {
            primary = PrimaryAction(.play)
        } else {
            let own = kind == .season && input.selectedSeason == d.id ? input.episodes : nil
            primary = Self.containerPrimary(d, onDeck: input.onDeck, ownEpisodes: own)
        }
        id = d.id
        self.kind = kind
        item = d.summary
        title = kind == .season ? Words.seasonTitle(d.title, index: d.summary.index) : d.title
        context = Self.context(d)
        meta = Self.meta(d, file: input.file, seasons: input.seasons, locale: input.locale)
        genres = d.genres
        partOf = input.partOf
        fromStart = primary.kind == .resume
        if container {
            let leaves = d.summary.leafCount ?? 0
            watched = leaves > 0 && (d.summary.viewedLeafCount ?? 0) >= leaves
        } else {
            watched = state.watched
        }
        markable = [.movie, .episode, .show, .season].contains(kind)
        myList = kind == .movie || kind == .show ? (input.inWatchlist ?? d.inWatchlist) : nil
        userRating = state.rating.map { Int($0.rounded()) }.map { min(max($0, 0), 10) }.flatMap { $0 > 0 ? $0 : nil }
        rateable = kind == .movie || kind == .show || kind == .episode
        tagline = d.tagline.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
        summary = d.summary.summary.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
        tracks = Self.trackChoices(d, file: input.file)
        trailer = d.extras.first { $0.extraType == "trailer" }
        details = Self.details(d, file: input.file)
        mediaInfo = Self.mediaInfo(d.files)
        extras = Self.distinct(d.extras)
        castTitle = kind == .episode ? localized("Guest Stars") : localized("Cast")
        var seenCast = Set<Int>()
        cast = d.cast.filter { seenCast.insert($0.id).inserted }
        related = kind == .movie || kind == .show ? Self.distinct(input.related).filter { $0.id != d.id } : []
        seasons = Self.seasons(input, locale: input.locale)
        posters = Tile.posters(d.summary)
        backdrops = Self.backdrops(d.summary)
    }

    // MARK: Inside

    private static func fraction(_ position: Int, _ duration: Int?) -> Double {
        guard let duration, duration > 0 else { return 0 }
        return min(1, max(0, Double(position) / Double(duration)))
    }

    private static func distinct(_ items: [ItemSummary]) -> [ItemSummary] {
        var seen = Set<Int>()
        return items.filter { seen.insert($0.id).inserted }
    }

    /// A show's or season's button: its next episode, "Resume S2 · E5" or "Play S1 · E1".
    private static func containerPrimary(_ d: ItemDetail, onDeck: ItemSummary?,
                                         ownEpisodes: [ItemSummary]?) -> PrimaryAction {
        guard let onDeck else {
            let leaves = d.summary.leafCount
            let nothing = d.kind == .season ? ownEpisodes?.isEmpty == true || leaves == 0 : leaves == nil || leaves == 0
            return PrimaryAction(nothing ? .unavailable : .play)
        }
        let resuming = onDeck.userState.positionMs > 0 && !onDeck.userState.watched
        let label = Words.episodeLabel(season: onDeck.parentIndex, episode: onDeck.index)
        return PrimaryAction(resuming ? .resume : .play, episode: label.isEmpty ? nil : label,
                             progress: resuming ? Words.progress(onDeck) : 0,
                             timeLeft: resuming ? Words.timeLeft(positionMs: onDeck.userState.positionMs,
                                                                 durationMs: onDeck.durationMs) : "")
    }

    private static func nonEmpty(_ text: String?) -> String? {
        guard let text, !text.isEmpty else { return nil }
        return text
    }

    private static func context(_ d: ItemDetail) -> ItemContext? {
        let s = d.summary
        switch d.kind {
        case .episode:
            let label = Words.episodeLabel(season: s.parentIndex, episode: s.index)
            let long = Words.episodeLongLabel(season: s.parentIndex, episode: s.index)
            return ItemContext(showId: s.grandparentId, showTitle: nonEmpty(s.grandparentTitle), seasonId: s.parentId,
                               seasonTitle: s.parentIndex.map { Words.seasonTitle(nil, index: $0) }
                                   ?? nonEmpty(s.parentTitle),
                               episodeLabel: nonEmpty(label), episodeLongLabel: nonEmpty(long))
        case .season:
            return ItemContext(showId: s.parentId, showTitle: nonEmpty(s.parentTitle))
        default:
            return nil
        }
    }

    /// The line of facts.
    private static func meta(_ d: ItemDetail, file: MediaFile?, seasons: [ItemSummary]?,
                             locale: Locale) -> [MetaPart] {
        var out: [MetaPart] = []
        func text(_ value: String?) {
            if let value, !value.trimmingCharacters(in: .whitespaces).isEmpty { out.append(.text(value)) }
        }
        let s = d.summary
        switch d.kind {
        case .show:
            let count = s.childCount ?? seasons?.filter { ($0.index ?? 1) > 0 }.count
            if let count, count > 0 { text(localized("\(count) seasons")) }
            text(yearRange(d.year, seasons))
        case .season:
            text(d.year.map(String.init))
            if let leaves = s.leafCount, leaves > 0 { text(localized("\(leaves) episodes")) }
        case .episode:
            text(Words.date(s.releaseDate, locale: locale))
            text(file?.edition)
            text(Words.runtime(ms: d.durationMs))
        default:
            text(d.year.map(String.init))
            text(file?.edition)
            text(Words.runtime(ms: d.durationMs))
        }
        if let rating = d.contentRating, !rating.trimmingCharacters(in: .whitespaces).isEmpty {
            out.append(.contentRating(rating))
        }
        if let critics = s.rating, critics > 0 { out.append(.rating(Words.rating(critics))) }
        let badges = Words.qualityBadges(file)
        if !badges.isEmpty { out.append(.badges(badges)) }
        return out
    }

    /// "2021-2025" from a show's seasons, or its own year.
    private static func yearRange(_ year: Int?, _ seasons: [ItemSummary]?) -> String? {
        let years = (seasons ?? []).compactMap(\.year) + [year].compactMap { $0 }
        guard let first = years.min(), let last = years.max() else { return nil }
        return last > first ? "\(first)-\(last)" : String(first)
    }

    /// The pickers: versions when there are several, audio when two or more, subtitles when any.
    private static func trackChoices(_ d: ItemDetail, file: MediaFile?) -> TrackChoices? {
        guard let file else { return nil }
        let audio = file.streams(.audio)
        let subtitles = file.streams(.subtitle)
        if d.files.count < 2 && audio.count < 2 && subtitles.isEmpty { return nil }
        let defaultAudio = audio.first(where: \.default) ?? audio.first
        let automatic = !file.audioPicked
        let start = file.selectedAudio ?? defaultAudio?.index
        let shown = automatic ? nil : start
        let language = Words.language(audio.first { $0.index == start }?.language)
        let versions = d.files.count > 1
            ? d.files.enumerated().map { n, f in
                TrackChoice(id: f.id, label: Words.version(f, position: n), selected: f.id == file.id)
            } : []
        let subtitle = file.selectedSubtitle.flatMap { index in subtitles.contains { $0.index == index } ? index : nil }
        return TrackChoices(
            versions: versions,
            audio: audio.count > 1
                ? audio.map { TrackChoice(id: $0.index, label: Words.stream($0), selected: $0.index == shown) } : [],
            subtitles: subtitles.map { TrackChoice(id: $0.index, label: Words.stream($0), selected: $0.index == subtitle) },
            versionId: file.id,
            automaticAudio: automatic,
            automaticLabel: language.map { localized("Automatic (\($0))") }
                ?? localized(keyed: LocalizedStringResource("audio.automaticPlain", defaultValue: "Automatic",
                                                            bundle: .atURL(Bundle.module.bundleURL))),
            audioIndex: shown,
            subtitleIndex: subtitle)
    }

    /// At most this many names per crew line, then "and N more".
    public static let crewNames = 4

    private static func details(_ d: ItemDetail, file: MediaFile?) -> ItemDetails {
        let crew: [CrewLine] = [CreditRole.creator, .director, .writer].compactMap { role in
            var seen = Set<Int>()
            let people = d.crew.filter { $0.role == role && seen.insert($0.id).inserted }
            guard !people.isEmpty else { return nil }
            return CrewLine(role: role, label: Words.crewLabel(role, plural: people.count > 1),
                            people: Array(people.prefix(crewNames)), more: max(0, people.count - crewNames))
        }
        let original = d.originalTitle.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty || $0 == d.title
            ? nil : $0 }
        let released = d.kind == .episode ? nil : nonEmpty(d.summary.releaseDate).map { Words.date($0) }
        return ItemDetails(
            crew: crew, originalTitle: original, released: released,
            studio: d.studio.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 },
            studioLabel: d.kind == .episode || d.kind == .show ? localized("Network") : localized("Studio"),
            plays: d.userState.playCount > 0 ? d.userState.playCount : nil,
            versions: d.files.map { TrackChoice(id: $0.id, label: Words.versionDescription($0), selected: $0.id == file?.id) })
    }

    /// Media info: an app gets file names only, and only the name shows, whatever comes.
    private static func mediaInfo(_ files: [MediaFile]) -> [FileInfo] {
        files.enumerated().map { n, f in
            let video = f.streams(.video).first
            var rows = [InfoRow(label: localized("File"), value: Words.fileName(f.path)),
                        InfoRow(label: localized("Container"), value: f.container?.uppercased() ?? "—")]
            if f.size > 0 { rows.append(InfoRow(label: localized("Size"), value: Words.bytes(f.size))) }
            if let ms = f.durationMs, ms > 0 {
                rows.append(InfoRow(label: localized("Duration"), value: Words.duration(ms: ms)))
            }
            if let bitrate = f.bitrate, bitrate > 0 {
                rows.append(InfoRow(label: localized("Bitrate"), value: Words.bitrate(bitrate)))
            }
            if let codec = f.videoCodec, !codec.isEmpty {
                let value = [Words.codec(codec), Words.resolution(width: f.width, height: f.height) ?? "",
                             Words.dynamicRange(video) ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
                rows.append(InfoRow(label: localized("Video"), value: value))
            }
            if let codec = f.audioCodec, !codec.isEmpty {
                rows.append(InfoRow(label: localized("Audio"), value: Words.codec(codec)))
            }
            let order: [StreamKind: Int] = [.video: 0, .audio: 1, .subtitle: 2]
            let streams = f.streams.filter { order[$0.kind] != nil }
                .sorted { (order[$0.kind]!, $0.index) < (order[$1.kind]!, $1.index) }
                .map { s in
                    let kind = s.kind == .video ? localized("Video")
                        : s.kind == .audio ? localized("Audio") : localized("Subtitles")
                    return StreamInfo(kind: kind, codec: Words.codec(s.codec), language: Words.language(s.language),
                                      title: nonEmpty(s.title), details: streamDetails(s))
                }
            let title = nonEmpty(f.edition) ?? (files.count > 1 ? localized("Version \(n + 1)") : nil)
            return FileInfo(id: f.id, title: title, rows: rows, streams: streams)
        }
    }

    private static func streamDetails(_ s: MediaStream) -> String {
        var parts: [String] = []
        switch s.kind {
        case .video:
            if let resolution = Words.resolution(width: s.width, height: s.height) { parts.append(resolution) }
            if let w = s.width, let h = s.height, w > 0, h > 0 { parts.append("\(w)×\(h)") }
            if let rate = s.frameRate, rate > 0 {
                let rounded = (rate * 100).rounded() / 100
                let text = rounded == rounded.rounded() ? String(Int(rounded)) : String(rounded)
                parts.append("\(text) fps")
            }
            if let depth = s.bitDepth, depth > 0 { parts.append("\(depth)-bit") }
            if let range = Words.dynamicRange(s) { parts.append(range) }
            if let profile = nonEmpty(s.profile) { parts.append(profile) }
        case .audio:
            let channels = Words.channels(s.channels, layout: s.channelLayout)
            if !channels.isEmpty { parts.append(channels) }
            if let rate = s.sampleRate, rate > 0 {
                parts.append(String(format: "%.1f kHz", locale: Locale(identifier: "en_US_POSIX"), Double(rate) / 1000))
            }
        default:
            parts.append(s.textBased == false ? localized("Image-based") : localized("Text"))
            if s.external { parts.append(localized("External")) }
        }
        let bitrate = Words.bitrate(s.bitrate)
        if !bitrate.isEmpty { parts.append(bitrate) }
        if s.forced { parts.append(localized("Forced")) }
        if s.default { parts.append(localized("Default")) }
        return parts.joined(separator: " · ")
    }

    private static func seasons(_ input: PageInput, locale: Locale) -> SeasonsSection? {
        let d = input.detail
        let tabs: [SeasonTab]
        switch d.kind {
        case .show, .season:
            tabs = (input.seasons ?? []).map { season in
                SeasonTab(id: season.id, title: Words.seasonTitle(season.title, index: season.index),
                          episodes: season.leafCount.map { localized("\($0) episodes") })
            }
        case .episode:
            tabs = []
        default:
            return nil
        }
        let focus = d.kind == .episode ? d.id : input.onDeck.flatMap { $0.parentId == input.selectedSeason ? $0.id : nil }
        var seen = Set<Int>()
        let episodes = input.episodes.map { list in
            list.filter { seen.insert($0.id).inserted }.map { episodeLine($0, locale: locale) }
        }
        return SeasonsSection(seasons: tabs, selected: input.selectedSeason, episodes: episodes, focus: focus)
    }

    /// An episode's words for lists and rows.
    public static func episodeLine(_ e: ItemSummary, locale: Locale = ShownLanguage.locale) -> EpisodeLine {
        let inProgress = Words.isInProgress(e)
        let runtime = Words.runtime(ms: e.durationMs)
        let length = inProgress ? Words.timeLeft(positionMs: e.userState.positionMs, durationMs: e.durationMs) : runtime
        let spokenLength = inProgress
            ? Words.spokenTimeLeft(positionMs: e.userState.positionMs, durationMs: e.durationMs)
            : Words.spokenDuration(ms: e.durationMs)
        let number = e.index.map { localized("Episode \($0)") }
        return EpisodeLine(
            item: e,
            title: e.index.map { "\($0). \(e.title)" } ?? e.title,
            length: length,
            line: [length, Words.date(e.releaseDate, locale: locale)].filter { !$0.isEmpty }.joined(separator: " · "),
            aired: nonEmpty(Words.longDate(e.releaseDate, locale: locale)),
            longLabel: Words.episodeLongLabel(season: e.parentIndex, episode: e.index),
            summary: e.summary.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 },
            progress: inProgress ? Words.progress(e) : 0,
            watched: e.userState.watched,
            pictures: Tile.stills(e),
            spokenLabel: [number, e.title].compactMap { $0 }.joined(separator: ", "),
            spokenValue: [spokenLength, e.userState.watched ? localized("watched") : ""].filter { !$0.isEmpty }
                .joined(separator: ", "))
    }

    /// The season a show page opens on: the next episode's, else the first that isn't Specials, else the first.
    public static func defaultSeason(_ seasons: [ItemSummary], onDeck: ItemSummary?) -> Int? {
        if let parent = onDeck?.parentId, seasons.contains(where: { $0.id == parent }) { return parent }
        return seasons.first { ($0.index ?? 1) > 0 }?.id ?? seasons.first?.id
    }

    /// A backdrop: its own, else its show's (an episode's, a season's), else its own still.
    static func backdrops(_ item: ItemSummary) -> [String] {
        var out: [String] = []
        if let own = nonEmpty(item.backdropUrl) { out.append(own) }
        let show = item.kind == .episode ? item.grandparentId : item.kind == .season ? item.parentId : nil
        if let show { out.append("/api/items/\(show)/images/backdrop") }
        if let thumb = nonEmpty(item.thumbUrl), !out.contains(thumb) { out.append(thumb) }
        return out
    }
}
