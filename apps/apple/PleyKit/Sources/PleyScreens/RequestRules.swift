import Foundation
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer

// Requests' rules, as every client keeps them: pure functions of what the
// server answered. Their words are `RequestWords`'.

// MARK: - Ways in

/// The ways into requests, for Me, Search and the sidebar: Discover, and
/// Your requests (Requests for managers) carrying one number, in the
/// danger colour while failed requests wait for a manager.
public struct RequestsEntry: Hashable, Sendable {
    /// Discover shows: requests are on and open to this account (perhaps only to say "Sign in again").
    public var discover = false
    /// Your requests shows, or for managers Requests.
    public var requests = false
    /// It's the managers' Requests, opening on Needs attention.
    public var manage = false
    /// The number on the way into requests; 0 for none.
    public var badge = 0
    /// Managers: of `badge`, the failed requests waiting for one.
    public var failed = 0

    public init(discover: Bool = false, requests: Bool = false, manage: Bool = false, badge: Int = 0, failed: Int = 0) {
        self.discover = discover
        self.requests = requests
        self.manage = manage
        self.badge = badge
        self.failed = failed
    }

    public static let none = RequestsEntry()

    public static func of(_ gate: RequestsGate) -> RequestsEntry {
        guard let summary = gate.summary else { return .none }
        let requests = summary.requestsEntry
        return RequestsEntry(discover: summary.discover, requests: requests, manage: requests && summary.manage,
                             badge: requests ? summary.badge : 0,
                             failed: requests && summary.manage ? summary.failed : 0)
    }

    /// The number is in the danger colour.
    public var alarm: Bool { failed > 0 }

    /// Some way in shows.
    public var shows: Bool { discover || requests }

    /// The number said for VoiceOver; `nil` without one.
    public var spoken: String? {
        badge > 0 ? RequestWords.count(manage: manage, badge: badge, failed: failed) : nil
    }

    /// The badge's number: "99+" past 99; `nil` at 0.
    public var badgeText: String? { RequestWords.badgeText(badge) }

    /// "Requests" for managers, else "Your requests".
    public var requestsTitle: String { manage ? RequestWords.manage : RequestWords.yourRequests }
}

// MARK: - Problems

/// Why a request route refused, as the screens say it. A `503` with
/// Pley's JSON is an answer, never the server being away: only `offline` is.
public enum RequestProblem: Hashable, Sendable {
    /// `503 unavailable`: "Requests are off".
    case off
    /// `403 no_access`.
    case noAccess
    /// `403 insufficient_scope`: this sign-in lacks the scope; signing in again asks for it.
    case signInAgain(Scope)
    /// `503 tmdb_key`: TMDB refused the server's key. No Retry: an administrator fixes it.
    case tmdbKey
    /// `503 tmdb_down`, with Retry.
    case tmdbDown
    /// `403 restricted`: above the account's age limit; the text says so.
    case restricted(String)
    /// `409 declined`: declined in the last 30 days; asking again works from `retryAt`.
    case declined(retryAt: Int?)
    /// `429 too_many_requests`: past the limit until `retryAt`; `nil`: what was asked is more than the limit itself.
    case limit(retryAt: Int?)
    /// Another `409`: the server's message, and the request as it is now when it sent it.
    case conflict(String, MediaRequest?)
    /// `404`: the title or the request isn't there (any more).
    case notFound
    /// The server can't be reached: why, in a sentence.
    case offline(String)
    /// Anything else: its sentence.
    case other(String)

    public enum Scope: Sendable { case requests, approvals }

    /// Trying again may help: a Retry shows.
    public var retry: Bool {
        switch self {
        case .tmdbDown, .offline, .other: true
        default: false
        }
    }

    /// Why Discover's pages can't show, from `gate`: signed in before
    /// requests, requests off, no access; `nil` otherwise.
    public static func of(_ gate: RequestsGate) -> RequestProblem? {
        switch gate {
        case .signInAgain: .signInAgain(.requests)
        case .hidden: .off
        case .noAccess: .noAccess
        case .open(let summary): summary.mayAsk ? nil : .noAccess
        case .unknown: nil
        }
    }

    public static func of(_ error: any Error) -> RequestProblem {
        guard let error = error as? PleyError else { return .other(asSentence(describe(error))) }
        if LateReports.unreachable(error) { return .offline(asSentence(error.userMessage)) }
        switch error {
        case .insufficientScope(let scope):
            return .signInAgain(scope == RequestSummary.approvalsScope ? .approvals : .requests)
        case .notAvailable(let code, _, let setup):
            if code == "unavailable" || !setup { return .off }
            if code == "tmdb_key" { return .tmdbKey }
            if code == "tmdb_down" { return .tmdbDown }
            return .other(asSentence(error.userMessage))
        case .api(let status, let code, let message):
            return of(status: status, code: code, message: message, retryAt: nil, request: nil, error: error)
        case .refused(let status, let code, let message, let retryAt, let request):
            return of(status: status, code: code, message: message, retryAt: retryAt, request: request, error: error)
        default:
            return .other(asSentence(error.userMessage))
        }
    }

    private static func of(status: Int, code: String, message: String?, retryAt: Int?, request: MediaRequest?,
                           error: PleyError) -> RequestProblem {
        switch code {
        case "unavailable": return .off
        case "tmdb_key": return .tmdbKey
        case "tmdb_down": return .tmdbDown
        case "no_access": return .noAccess
        case "restricted": return .restricted(message ?? "")
        case "declined": return .declined(retryAt: retryAt)
        default: break
        }
        if status == 429 || code == "too_many_requests" { return .limit(retryAt: retryAt) }
        if status == 409 { return .conflict(message ?? "", request) }
        if status == 404 { return .notFound }
        return .other(asSentence(error.userMessage))
    }

    /// What it says: `failure` is the page's own title for what failed
    /// ("Couldn't load Discover"), over the sentence of an offline or other
    /// error.
    public func words(manage: Bool, summary: RequestSummary?, failure: String? = nil,
                      now: Int = Int(Date().timeIntervalSince1970)) -> ProblemWords {
        switch self {
        case .off:
            ProblemWords(title: RequestWords.offTitle, body: RequestWords.offBody, symbol: "safari")
        case .noAccess:
            ProblemWords(title: RequestWords.noAccessTitle, body: RequestWords.noAccessBody, symbol: "lock")
        case .signInAgain(let scope):
            ProblemWords(title: scope == .approvals ? RequestWords.signInApprovals : RequestWords.signInTitle,
                         symbol: "rectangle.portrait.and.arrow.right", signInAgain: true)
        case .tmdbKey:
            ProblemWords(title: RequestWords.tmdbKey,
                         body: manage ? RequestWords.tmdbKeyManage : RequestWords.tmdbKeyOthers,
                         symbol: "exclamationmark.triangle")
        case .tmdbDown:
            ProblemWords(title: RequestWords.tmdbDown, body: RequestWords.tmdbDownBody,
                         symbol: "bolt.horizontal.circle", retry: true)
        case .restricted(let text):
            ProblemWords(title: text, symbol: "lock")
        case .declined(let retryAt):
            ProblemWords(title: retryAt.map { RequestWords.askAgainAt($0, now: now) } ?? RequestWords.declinedOnly,
                         symbol: "exclamationmark.circle")
        case .limit(let retryAt):
            ProblemWords(title: retryAt.map { RequestWords.askMoreAt($0, now: now) }
                             ?? RequestWords.atMost(seasonsLimit: summary?.seasonsLimit,
                                                    moviesLimit: summary?.moviesLimit),
                         symbol: "exclamationmark.circle")
        case .conflict(let message, _):
            ProblemWords(title: message, symbol: "exclamationmark.circle")
        case .notFound:
            ProblemWords(title: RequestWords.notFound, body: RequestWords.notFoundBody, symbol: "exclamationmark.circle")
        case .offline(let sentence):
            failure.map { ProblemWords(title: $0, body: sentence, symbol: "wifi.slash", retry: true) }
                ?? ProblemWords(title: sentence, symbol: "wifi.slash", retry: true)
        case .other(let sentence):
            failure.map { ProblemWords(title: $0, body: sentence, symbol: "exclamationmark.triangle", retry: true) }
                ?? ProblemWords(title: sentence, symbol: "exclamationmark.triangle", retry: true)
        }
    }

    /// What a toast says of it: its title, with its body when it has one.
    public func toast(manage: Bool, summary: RequestSummary?, now: Int = Int(Date().timeIntervalSince1970)) -> String {
        let words = words(manage: manage, summary: summary, now: now)
        return [words.title, words.body].compactMap { $0 }.joined(separator: " ")
    }
}

/// A problem's words, as a page or a toast shows them.
public struct ProblemWords: Hashable, Sendable {
    public var title: String
    public var body: String?
    /// Its SF Symbol.
    public var symbol: String
    /// A Retry shows.
    public var retry: Bool
    /// A Sign in shows.
    public var signInAgain: Bool

    public init(title: String, body: String? = nil, symbol: String, retry: Bool = false, signInAgain: Bool = false) {
        self.title = title
        self.body = body
        self.symbol = symbol
        self.retry = retry
        self.signInAgain = signInAgain
    }
}

// MARK: - The main action

/// Whether asking works for a title now.
public enum Ask: Hashable, Sendable {
    case yes
    /// The account may not ask: "Requests aren't open to this account."
    case noAccess
    /// Nothing left to ask for: the button is disabled, and the allowance line shows.
    case limit
}

/// A title page's one main action, the first that applies.
public enum MainAction: Hashable, Sendable {
    /// This sign-in came before requests: only "Sign in again".
    case signInAgain
    /// Above the account's age limit: no button, the restriction's text.
    case restricted(String?)
    /// In a library: Open, where the item is known.
    case open(itemId: Int?)
    /// No button: "Declined: “{reason}”. You can ask again in 12 days."
    case declined(reason: String?, againAt: Int?)
    /// Requested, inert, with the request's pill.
    case mine(MediaRequest?)
    /// Following, inert, with the request's pill, and Stop following.
    case following(MediaRequest?)
    /// Someone asked for it already: Tell me too, which costs nothing.
    case tellMeToo(Ask)
    /// Some of it is here or asked for: Open (with an item), and Request more seasons unless `more` is `nil`.
    case partial(itemId: Int?, more: Ask?)
    case request(Ask)
}

/// Requests' pure rules.
public enum RequestRules {
    /// The longest reason a decline takes (the server's limit), in UTF-16 units.
    public static let declineReasonMax = 500

    /// `title` as one of its requests now says it is: active → requested,
    /// declined → declined, available → in the library, gone → no badge. A
    /// request no longer there (`nil`) says nothing of the title, which
    /// others may still ask for or a library have: only the request is let go of.
    public static func patched(_ title: DiscoverTitle, with request: MediaRequest?) -> DiscoverTitle {
        var title = title
        guard let request else {
            title.requestId = nil
            return title
        }
        if request.stage == .gone {
            title.status = .none
            title.requestId = nil
        } else if request.status == .available {
            title.status = .library
            title.itemId = request.itemId ?? title.itemId
        } else if request.status == .declined {
            title.status = .declined
            title.requestId = nil
        } else if request.active {
            title.status = .requested
            title.requestId = request.id
        }
        return title
    }

    /// Whether `change` may be about `title`: its request, or the title itself.
    public static func touches(_ title: DiscoverTitle, _ change: RequestChange) -> Bool {
        change.all || title.requestId.map(change.ids.contains) == true || change.titles.contains(title.key)
    }

    /// Whether asking works for `kind` under `gate`, with `allowance` left.
    public static func askFor(_ kind: TitleKind, gate: RequestsGate, allowance: Allowance?) -> Ask {
        if !gate.mayAsk || gate.summary?.access == .off { return .noAccess }
        if let allowance, !allowance.allows(kind) { return .limit }
        return .yes
    }

    /// `detail`'s main action, for what `gate` lets this sign-in do.
    public static func mainAction(_ detail: DiscoverDetail, gate: RequestsGate) -> MainAction {
        if case .signInAgain = gate { return .signInAgain }
        let ask = askFor(detail.kind, gate: gate, allowance: Allowance(detail: detail, summary: gate.summary))
        switch detail.status {
        case .restricted:
            return .restricted(detail.restriction)
        case .library:
            return .open(itemId: detail.itemId)
        case .declined:
            return .declined(reason: detail.declined?.reason, againAt: detail.declined.map(\.againAt).flatMap { $0 > 0 ? $0 : nil })
        case .requested:
            let active = activeRequest(detail)
            if active?.mine == true { return .mine(active) }
            if detail.watching || active?.watching == true { return .following(active) }
            // Following costs nothing: only access counts.
            return .tellMeToo(ask == .noAccess ? .noAccess : .yes)
        case .partial:
            return .partial(itemId: detail.itemId, more: moreSeasons(detail) ? ask : nil)
        default:
            return .request(ask)
        }
    }

    /// A title's main action from what a row knows of it (the hero's,
    /// before its page is read): a request it has is taken as this account's.
    public static func titleAction(_ title: DiscoverTitle, gate: RequestsGate, allowance: Allowance?) -> MainAction {
        if case .signInAgain = gate { return .signInAgain }
        let ask = askFor(title.kind, gate: gate, allowance: allowance)
        switch title.status {
        case .restricted: return .restricted(nil)
        case .library: return .open(itemId: title.itemId)
        case .declined: return .declined(reason: nil, againAt: nil)
        case .requested: return title.requestId != nil ? .mine(nil) : .tellMeToo(ask == .noAccess ? .noAccess : .yes)
        case .partial: return .partial(itemId: title.itemId, more: ask)
        default: return .request(ask)
        }
    }

    /// The active request `detail` names (its `request_id`), else the newest active one this account sees.
    public static func activeRequest(_ detail: DiscoverDetail) -> MediaRequest? {
        detail.requests.first { $0.id == detail.requestId } ?? detail.requests.first { $0.active }
    }

    /// The request `season` shows the stage of: the one asking for it, among the page's.
    public static func requestOf(_ season: DiscoverSeason, in detail: DiscoverDetail) -> MediaRequest? {
        guard let id = season.requestId else { return nil }
        return detail.requests.first { $0.id == id }
    }

    /// Tell me too: the movie, or the seasons others asked for, which costs nothing.
    public static func tellMeToo(_ detail: DiscoverDetail) -> NewRequest {
        guard detail.kind == .show else { return NewRequest(kind: detail.kind, tmdbId: detail.tmdbId) }
        let seasons = (detail.seasons ?? []).filter { $0.state == .requested }.map(\.number)
        return NewRequest(kind: detail.kind, tmdbId: detail.tmdbId, seasons: seasons.isEmpty ? nil : seasons)
    }

    /// Asking again for `request` as it was (Request again).
    public static func askAgain(_ request: MediaRequest) -> NewRequest {
        let seasons = request.kind == .show ? request.seasons.flatMap { $0.isEmpty ? nil : $0 } : nil
        return NewRequest(kind: request.kind, tmdbId: request.tmdbId, seasons: seasons)
    }

    /// Whether `season` may be asked for: missing, partly in a library or
    /// not aired, not declined, the title not restricted.
    public static func askable(_ season: DiscoverSeason, in detail: DiscoverDetail) -> Bool {
        [SeasonState.missing, .partial, .unaired].contains(season.state) && !season.declined
            && detail.status != .restricted
    }

    /// Missing or partly here, and not the specials: what the picker ticks first.
    static func wanted(_ season: DiscoverSeason) -> Bool {
        season.number != 0 && (season.state == .missing || season.state == .partial)
    }

    /// "Request more seasons" shows: a season other than the specials is askable and missing or partly here.
    public static func moreSeasons(_ detail: DiscoverDetail) -> Bool {
        (detail.seasons ?? []).contains { wanted($0) && askable($0, in: detail) }
    }

    /// A decline's reason as the server takes it: trimmed, empty as
    /// `nil`, and cut to `declineReasonMax` UTF-16 units without splitting
    /// a character (an emoji alone would reach the server as "?").
    public static func declineReason(_ reason: String) -> String? {
        let cut = cutReason(reason).trimmingCharacters(in: .whitespacesAndNewlines)
        return cut.isEmpty ? nil : cut
    }

    /// `reason` cut, as the field keeps it while it's typed.
    public static func cutReason(_ reason: String) -> String {
        guard reason.utf16.count > declineReasonMax else { return reason }
        var out = ""
        var units = 0
        for character in reason {
            let count = character.utf16.count
            if units + count > declineReasonMax { break }
            out.append(character)
            units += count
        }
        return out
    }

    /// Where a request's card opens: the library's title once it's
    /// there, else its Discover page while asking works; `nil`: nowhere.
    public static func opens(_ request: MediaRequest, mayAsk: Bool) -> Screen? {
        let item = request.itemId.map { Screen.item($0) }
        if item != nil && (request.stage == .available || request.stage == .partial) { return item }
        if mayAsk { return .discoverTitle(request.kind, tmdbId: request.tmdbId) }
        return item
    }

    /// Where a title's poster opens: the library's title when a library
    /// this account can open has it whole, since its Discover page would
    /// offer only Open; else its Discover page. A show partly there opens
    /// its Discover page, where more seasons can be asked for.
    public static func opens(_ title: DiscoverTitle) -> Screen {
        if title.status == .library, let itemId = title.itemId { return .item(itemId) }
        return .discoverTitle(title.kind, tmdbId: title.tmdbId)
    }

    /// The badge of a title's poster.
    public static func badge(_ title: DiscoverTitle) -> String? { RequestWords.badge(title.status, itemId: title.itemId) }
}

// MARK: - The season picker

/// One season in the picker: tickable when `askable`.
public struct PickerSeason: Hashable, Sendable, Identifiable {
    public var season: DiscoverSeason
    public var askable: Bool
    public var id: Int { season.number }

    public init(season: DiscoverSeason, askable: Bool) {
        self.season = season
        self.askable = askable
    }
}

/// "Which seasons?": every season, specials last; `picked` the ones
/// ticked. More than `left` ticked disables the button and shows the allowance line.
public struct SeasonPicker: Hashable, Sendable {
    public var rows: [PickerSeason]
    public var picked: Set<Int>
    public var left: Int?

    public init(rows: [PickerSeason], picked: Set<Int>, left: Int?) {
        self.rows = rows
        self.picked = picked
        self.left = left
    }

    public var count: Int { picked.count }
    /// More ticked than may be asked for.
    public var overLimit: Bool { left.map { count > $0 } ?? false }
    /// Request can be pressed.
    public var ready: Bool { count > 0 && !overLimit }
    /// What was picked, lowest first, for the request.
    public var asked: [Int] { picked.sorted() }
    /// The button's words: "Request 3 seasons", or "Pick a season.".
    public var button: String { RequestWords.pickerButton(count) }

    /// `number` ticked or unticked; seasons that can't be asked for stay as they are.
    public mutating func toggle(_ number: Int) {
        guard rows.contains(where: { $0.season.number == number && $0.askable }) else { return }
        if picked.contains(number) { picked.remove(number) } else { picked.insert(number) }
    }

    /// `detail`'s seasons: those askable and missing or partly here (not
    /// the specials) are ticked lowest first, up to `left`; unaired seasons
    /// and specials are offered unticked.
    public static func of(_ detail: DiscoverDetail, left: Int?) -> SeasonPicker {
        let all = (detail.seasons ?? []).sorted { a, b in
            (a.number == 0) != (b.number == 0) ? b.number == 0 : a.number < b.number
        }
        let rows = all.map { PickerSeason(season: $0, askable: RequestRules.askable($0, in: detail)) }
        let wanted = rows.filter { $0.askable && RequestRules.wanted($0.season) }.map(\.season.number)
        let picked = left.map { Array(wanted.prefix(max($0, 0))) } ?? wanted
        return SeasonPicker(rows: rows, picked: Set(picked), left: left)
    }

    /// The picker for `detail` read again: the picks still askable are kept.
    public func rebuilt(from detail: DiscoverDetail, left: Int?) -> SeasonPicker {
        var fresh = SeasonPicker.of(detail, left: left)
        let askable = Set(fresh.rows.filter(\.askable).map(\.season.number))
        fresh.picked = picked.intersection(askable)
        return fresh
    }
}

// MARK: - A card's actions

/// What a request card offers: Watch once it's ready where the person can
/// open it, then exactly the server's actions in its order, those this app
/// doesn't know left out. Approve comes with Approve… (profile, folder and
/// seasons), and Decline asks for an optional reason. On Apple TV's panel
/// (`panel`), Approve is plain and Decline comes with Decline with a reason….
public enum CardAction: String, Hashable, Sendable, CaseIterable {
    case watch, withdraw, remove, stopFollowing, approve, approveWith, tryAgain, decline, declineWithReason, handled
    case delete, askAgain

    public var title: String {
        switch self {
        case .watch: RequestWords.watch
        case .withdraw: RequestWords.withdraw
        case .remove: RequestWords.remove
        case .stopFollowing: RequestWords.stopFollowing
        case .approve: RequestWords.approve
        case .approveWith: RequestWords.approveWith
        case .tryAgain: RequestWords.tryAgain
        case .decline: RequestWords.decline
        case .declineWithReason: RequestWords.declineWithReason
        case .handled: RequestWords.handled
        case .delete: RequestWords.delete
        case .askAgain: RequestWords.askAgain
        }
    }

    /// Its SF Symbol.
    public var symbol: String {
        switch self {
        case .watch: "play.fill"
        case .approve: "checkmark"
        case .approveWith: "slider.horizontal.3"
        case .tryAgain: "arrow.clockwise"
        case .decline, .withdraw: "xmark"
        case .declineWithReason: "text.bubble"
        case .remove, .delete: "trash"
        case .stopFollowing: "bookmark.slash"
        case .handled: "checkmark.circle"
        case .askAgain: "arrow.triangle.2.circlepath"
        }
    }

    /// It asks first: "Withdraw your request for “X”?", "Remove “X” from your requests?", "Delete this request?".
    public var confirms: Bool { self == .withdraw || self == .remove || self == .delete }

    /// It's drawn as the main button: Watch, Approve, Try again, Request again.
    public var prominent: Bool { self == .watch || self == .approve || self == .tryAgain || self == .askAgain }

    /// It takes something away: drawn as destructive.
    public var destructive: Bool { self == .delete || self == .remove || self == .withdraw }

    /// The question it asks first, for `title`; `nil` when it asks none.
    public func confirmation(_ title: String) -> String? {
        switch self {
        case .withdraw: RequestWords.withdrawConfirm(title)
        case .remove: RequestWords.removeConfirm(title)
        case .delete: RequestWords.deleteConfirm
        default: nil
        }
    }

    /// Those under More actions.
    static let menu: Set<CardAction> = [.approveWith, .handled, .delete]

    /// `request`'s actions: the buttons, and those under More actions.
    public static func of(_ request: MediaRequest) -> (buttons: [CardAction], menu: [CardAction]) {
        var all: [CardAction] = request.readyToWatch ? [.watch] : []
        var seen: Set<RequestAction> = []
        for action in request.actions where seen.insert(action).inserted {
            switch action {
            case .withdraw: all.append(request.status == .declined ? .remove : .withdraw)
            case .unwatch: all.append(.stopFollowing)
            case .approve: all += [.approve, .approveWith]
            case .retry: all.append(.tryAgain)
            case .decline: all.append(.decline)
            case .handled: all.append(.handled)
            case .delete: all.append(.delete)
            case .askAgain: all.append(.askAgain)
            default: break
            }
        }
        return (all.filter { !menu.contains($0) }, all.filter { menu.contains($0) })
    }

    /// Apple TV's panel for `request` (Android TV's): exactly the server's
    /// actions in its order. Approve is plain, the settings' choices
    /// applying, and Decline comes with Decline with a reason…; Open and
    /// Details are the panel's own.
    public static func panel(_ request: MediaRequest) -> [CardAction] {
        var all: [CardAction] = []
        var seen: Set<RequestAction> = []
        for action in request.actions where seen.insert(action).inserted {
            switch action {
            case .withdraw: all.append(request.status == .declined ? .remove : .withdraw)
            case .unwatch: all.append(.stopFollowing)
            case .approve: all.append(.approve)
            case .retry: all.append(.tryAgain)
            case .decline: all += [.decline, .declineWithReason]
            case .handled: all.append(.handled)
            case .delete: all.append(.delete)
            case .askAgain: all.append(.askAgain)
            default: break
            }
        }
        return all
    }
}

// MARK: - Lists' filters

/// Which requests a list shows: managers' Needs attention, all of them, or those with one status.
public enum RequestView: Hashable, Sendable, CaseIterable, Identifiable {
    case attention, all, pending, approved, available, declined, failed

    public var id: String { key }

    public var status: RequestStatus? {
        switch self {
        case .attention, .all: nil
        case .pending: .pending
        case .approved: .approved
        case .available: .available
        case .declined: .declined
        case .failed: .failed
        }
    }

    /// Its name in accessibility identifiers: `requests.view.attention`.
    public var key: String {
        switch self {
        case .attention: "attention"
        case .all: "all"
        default: status?.rawValue ?? "all"
        }
    }

    public var title: String {
        switch self {
        case .attention: RequestWords.attention
        case .all: RequestWords.filterAll
        default: status.map(RequestWords.filter) ?? RequestWords.filterAll
        }
    }

    /// The status views, in the order their chips show.
    public static let statuses: [RequestView] = [.pending, .approved, .available, .declined, .failed]
}

/// A list's filters: the view, movies or shows, one person's (managers), a title, only this account's.
public struct RequestFilter: Hashable, Sendable {
    public var view: RequestView
    public var kind: TitleKind?
    public var userId: Int?
    public var q: String
    public var mine: Bool

    public init(view: RequestView = .all, kind: TitleKind? = nil, userId: Int? = nil, q: String = "",
                mine: Bool = false) {
        self.view = view
        self.kind = kind
        self.userId = userId
        self.q = q
        self.mine = mine
    }

    private var trimmed: String { q.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Some filter narrows the list beyond its view.
    public var narrowed: Bool { kind != nil || userId != nil || !trimmed.isEmpty || mine }

    /// Less than all shows (or all that needs attention): with none, none match.
    public var filtered: Bool { narrowed || view.status != nil }

    public func query(offset: Int, limit: Int) -> RequestQuery {
        RequestQuery(statuses: view.status.map { [$0] } ?? [], attention: view == .attention, kind: kind,
                     userId: userId, q: trimmed.isEmpty ? nil : trimmed, mine: mine, oldest: view == .attention,
                     offset: offset, limit: limit)
    }

    /// Whether `request` still belongs in the list, as it is now (after an action on it).
    public func matches(_ request: MediaRequest) -> Bool {
        if let kind, request.kind != kind { return false }
        if let userId, request.requestedBy?.id != userId { return false }
        if !trimmed.isEmpty, request.title.range(of: trimmed, options: .caseInsensitive) == nil { return false }
        if mine && !request.mine && !request.watching { return false }
        if view == .attention { return request.status == .pending || (request.status == .failed && request.retryAt == nil) }
        if let status = view.status { return request.status == status }
        return true
    }
}

// MARK: - Pages of titles

/// Titles of a list or of a search as they're read page by page: each
/// title once, more to read while `page < totalPages` (a short page, even
/// an empty one, isn't the end).
public struct TitlePages: Hashable, Sendable {
    public var titles: [DiscoverTitle] = []
    /// The last page read; 0 before the first.
    public var page = 0
    public var totalPages = 1
    public var loading = false
    public var problem: RequestProblem?
    /// The last pages read on their own brought fewer titles than wanted:
    /// more are read only when asked for (scrolling near the end).
    public var stopped = false

    public init(titles: [DiscoverTitle] = [], page: Int = 0, totalPages: Int = 1, loading: Bool = false,
                problem: RequestProblem? = nil, stopped: Bool = false) {
        self.titles = titles
        self.page = page
        self.totalPages = totalPages
        self.loading = loading
        self.problem = problem
        self.stopped = stopped
    }

    public var more: Bool { page < totalPages }

    /// Nothing more is read unless asked for: every page read, or the reading stopped short or failed.
    public var settled: Bool { !loading && (problem != nil || (page > 0 && (!more || stopped))) }

    /// `answer` added: the first page replaces what was there; later ones add what's new.
    public func with(_ answer: DiscoverPage) -> TitlePages {
        let all = answer.page <= 1 ? answer.results : titles + answer.results
        var seen: Set<String> = []
        let unique = all.filter { seen.insert($0.key).inserted }
        return TitlePages(titles: unique, page: max(answer.page, 1), totalPages: answer.totalPages)
    }

    /// Another page is wanted without scrolling: fewer than a `screenful`, and more to read.
    public func short(_ screenful: Int) -> Bool {
        more && !loading && problem == nil && !stopped && titles.count < screenful
    }
}

// MARK: - Approving

/// What Approve… starts on, and what it sends.
public enum Approving {
    /// The profile the dialog starts on: anime's for anime, else the settings'.
    public static func profile(_ options: ApprovalOptions?, for request: MediaRequest) -> Int? {
        guard let options else { return nil }
        return request.anime ? options.animeQualityProfile ?? options.qualityProfile : options.qualityProfile
    }

    /// The folder the dialog starts on.
    public static func folder(_ options: ApprovalOptions?, for request: MediaRequest) -> String? {
        guard let options else { return nil }
        return request.anime ? options.animeRootFolder ?? options.rootFolder : options.rootFolder
    }

    /// Shows: the seasons asked for, all ticked to start with.
    public static func seasons(_ request: MediaRequest) -> [Int] {
        request.kind == .show ? (request.seasons ?? []).sorted() : []
    }

    /// What approving sends: only what differs from the settings'
    /// choices, and the seasons when some were left out.
    public static func approval(options: ApprovalOptions?, request: MediaRequest, profile: Int?, folder: String?,
                                seasons: Set<Int>?) -> Approval {
        let all = Self.seasons(request)
        let picked = seasons?.sorted() ?? []
        return Approval(qualityProfile: profile == Self.profile(options, for: request) ? nil : profile,
                        rootFolder: folder == Self.folder(options, for: request) ? nil : folder,
                        seasons: request.kind == .show && !picked.isEmpty && picked.count < all.count ? picked : nil)
    }
}
