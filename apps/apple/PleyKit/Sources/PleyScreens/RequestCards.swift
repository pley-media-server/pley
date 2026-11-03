import Foundation
import PleyModel

/// A TMDB title as every Discover grid and row shows it, its words ready.
public struct DiscoverCard: Hashable, Sendable, Identifiable {
    public var key: String
    public var title: String
    /// "2024 · Movie".
    public var subtitle: String
    /// The poster's path on the server.
    public var posterPath: String?
    public var badge: String?
    /// What VoiceOver says: "Dune: Part Two, 2024, Movie, Requested".
    public var spoken: String
    public var screen: Screen
    /// The title as read, for what its page shows before it's read.
    public var source: DiscoverTitle

    public var id: String { key }

    public init(_ title: DiscoverTitle) {
        key = title.key
        self.title = title.title
        let kind = RequestWords.kind(title.kind)
        subtitle = [title.year.map(String.init), kind].compactMap { $0 }.joined(separator: " · ")
        posterPath = title.posterUrl
        badge = RequestRules.badge(title)
        spoken = [title.title, title.year.map(String.init), kind, badge].compactMap { $0 }.joined(separator: ", ")
        screen = .discoverTitle(title.kind, tmdbId: title.tmdbId)
        source = title
    }
}

/// A stage pill's words and tone, with its download's bar.
public struct PillWords: Hashable, Sendable {
    public var text: String
    public var tone: PillTone
    /// How far the download is, 0-1, while there is one.
    public var progress: Double?
    public var paused: Bool
    public var warning: Bool
    /// The line under a failed request's pill.
    public var line: String?

    public static func of(_ request: MediaRequest, manage: Bool, now: Int) -> PillWords {
        let pill = StagePill.of(request, manage: manage)
        let state = request.download?.state
        return PillWords(text: RequestWords.pill(pill, now: now), tone: pill.tone, progress: request.downloadProgress,
                         paused: state == .paused, warning: state == .warning,
                         line: RequestWords.pillLine(pill, now: now))
    }
}

/// A request as a card shows it, its words ready.
public struct RequestCard: Hashable, Sendable, Identifiable {
    public var id: Int
    public var title: String
    /// "2024 · Movie · Seasons 1–3".
    public var line: String
    public var posterPath: String?
    public var pill: PillWords
    /// "Ready, in a library you can't open: ask an administrator".
    public var elsewhere: String?
    /// "Ana · 3 days ago · 2 people follow it".
    public var asked: String
    /// The decline's reason, quoted.
    public var reason: String?
    /// Managers: the note the person left.
    public var note: String?
    /// Managers: "Radarr · HD-1080p · /movies".
    public var sentTo: String?
    public var buttons: [CardAction]
    public var menu: [CardAction]
    /// Where the poster and title go.
    public var opens: Screen?
    /// Where Watch goes.
    public var watch: Screen?
    /// What VoiceOver says: "title, stage".
    public var spoken: String
    /// The request as read, for the actions.
    public var request: MediaRequest

    public static func of(_ request: MediaRequest, manage: Bool, mayAsk: Bool, now: Int) -> RequestCard {
        let seasons = request.seasons.flatMap { $0.isEmpty ? nil : RequestWords.seasons($0) }
        let line = [request.year.map(String.init), RequestWords.kind(request.kind), seasons].compactMap { $0 }
            .joined(separator: " · ")
        let pill = PillWords.of(request, manage: manage, now: now)
        let watchers = manage && request.watchers > 0 ? RequestWords.watchers(request.watchers) : nil
        let reason = request.reason?.trimmingCharacters(in: .whitespacesAndNewlines)
        let actions = CardAction.of(request)
        return RequestCard(
            id: request.id, title: request.title, line: line, posterPath: request.posterUrl, pill: pill,
            elsewhere: request.readyElsewhere ? RequestWords.readyElsewhere : nil,
            asked: [RequestWords.asked(request, manage: manage, now: now), watchers].compactMap { $0 }
                .joined(separator: " · "),
            reason: request.status == .declined && reason?.isEmpty == false ? "\u{201C}\(reason!)\u{201D}" : nil,
            note: manage ? request.note.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 } : nil,
            sentTo: manage ? RequestWords.sentTo(request) : nil,
            buttons: actions.buttons, menu: actions.menu,
            opens: RequestRules.opens(request, mayAsk: mayAsk),
            watch: request.readyToWatch ? request.itemId.map { .item($0) } : nil,
            spoken: "\(request.title), \(pill.text)", request: request)
    }
}

/// What requests say of a library's movie or show (`GET /api/items/{id}/request`):
/// the stage pill of its newest active request, and Request more seasons
/// (`more`) when some can be asked for and asking works.
public struct ItemRequestBlock: Hashable, Sendable {
    public var kind: TitleKind
    public var tmdbId: Int
    public var requestId: Int?
    /// The request's pill, while one is active.
    public var pill: PillWords?
    /// Request more seasons shows.
    public var more: Bool
    /// Something of it downloads or goes into the library: read again every 10 s.
    public var moving: Bool
    /// Where Request more seasons goes: the Discover page, its picker open.
    public var picker: Screen { .discoverTitle(kind, tmdbId: tmdbId, pick: true) }

    public static func of(_ answer: ItemRequest, mayAsk: Bool, manage: Bool, now: Int) -> ItemRequestBlock? {
        let request = answer.request
        let more = mayAsk && !(answer.askable ?? []).isEmpty
        guard request != nil || more else { return nil }
        return ItemRequestBlock(kind: answer.kind, tmdbId: answer.tmdbId, requestId: request?.id,
                                pill: request.map { PillWords.of($0, manage: manage, now: now) }, more: more,
                                moving: request?.moving == true)
    }
}
