import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// How often a screen with a request that downloads or goes into the
/// library reads it again (download progress sends no events), how long
/// changes are gathered, and how often an open list is read while the
/// event stream is down.
public enum RequestTimings {
    public static let movingSeconds: Double = 10
    public static let changeDebounce: Double = 0.5
    public static let listPollSeconds: Double = 30
}

/// A title's page, its words ready.
public struct DiscoverTitlePage: Hashable, Sendable {
    public var title: String
    /// The title in its own language, when it differs.
    public var originalTitle: String?
    /// "2024", "1h 42m" or "5 seasons", "★ 8.4", "Movie": the line under the title.
    public var meta: [String]
    /// The age rating, drawn boxed.
    public var certification: String?
    /// "Animation · Science Fiction · Family".
    public var genres: String?
    public var tagline: String?
    public var overview: String
    public var posterPath: String?
    public var backdropPath: String?

    public static func of(_ detail: DiscoverDetail) -> DiscoverTitlePage {
        var meta: [String] = []
        if let year = detail.year { meta.append(String(year)) }
        if detail.kind == .show {
            let seasons = (detail.seasons ?? []).filter { $0.number > 0 }.count
            if seasons > 0 { meta.append(Words.pluralize(seasons, "season")) }
        } else if let minutes = detail.runtimeMinutes, minutes > 0 {
            meta.append(Words.duration(ms: minutes * 60_000))
        }
        if let rating = detail.rating, rating > 0 { meta.append("\u{2605} \(Words.rating(rating))") }
        meta.append(RequestWords.kind(detail.kind))
        let original = detail.originalTitle?.trimmingCharacters(in: .whitespaces)
        let tagline = detail.tagline?.trimmingCharacters(in: .whitespaces)
        return DiscoverTitlePage(
            title: detail.title, originalTitle: original?.isEmpty == false && original != detail.title ? original : nil,
            meta: meta, certification: detail.certification.flatMap { $0.isEmpty ? nil : $0 },
            genres: detail.genres.isEmpty ? nil : detail.genres.joined(separator: " · "),
            tagline: tagline?.isEmpty == false ? tagline : nil, overview: detail.overview,
            posterPath: detail.posterUrl, backdropPath: detail.backdropUrl)
    }
}

/// A season's row on a title's page.
public struct SeasonRow: Hashable, Sendable, Identifiable {
    public var number: Int
    public var name: String
    /// "In library", "4 of 10 episodes in library", "Airs Nov 4, 2026".
    public var state: String
    /// Its request's pill, when that request is on the page.
    public var pill: PillWords?
    /// Its state's SF Symbol, without a pill.
    public var symbol: String
    public var posterPath: String?
    public var id: Int { number }
}

/// A cast member on a title's page.
public struct CastCard: Hashable, Sendable, Identifiable {
    public var index: Int
    public var name: String
    public var character: String?
    public var photoPath: String?
    public var id: Int { index }
}

/// A title found on TMDB (Android's `DiscoverTitleViewModel`): its
/// details, its one main action, its seasons with their states, and the
/// season picker; the requests for it (this account's and those it
/// follows; everyone's for managers) with their actions. While it shows,
/// the page reads itself again when one of its requests changes, when many
/// did, and every 10 s while one of them downloads or goes into the library.
@MainActor
public final class DiscoverTitleModel: ObservableObject {
    public enum State: Hashable {
        case loading
        case ready
        case failed(ProblemWords)
    }

    @Published public private(set) var state: State = .loading
    @Published public private(set) var detail: DiscoverDetail?
    @Published public private(set) var page: DiscoverTitlePage?
    @Published public private(set) var action: MainAction?
    /// The lines of the action block: the declined line, "Someone asked
    /// for it already.", no access, the restriction, the allowance lines.
    @Published public private(set) var lines: [String] = []
    /// The allowance lines are red: nothing is left to ask for.
    @Published public private(set) var limitReached = false
    /// Requested or Following: the stage pill of the request, under the button.
    @Published public private(set) var actionPill: PillWords?
    /// "Your requests", or "Requests" for managers.
    @Published public private(set) var requestsHeading = RequestWords.yourRequests
    @Published public private(set) var requests: [RequestCard] = []
    @Published public private(set) var seasons: [SeasonRow] = []
    @Published public private(set) var cast: [CastCard] = []
    /// Reading the page again failed: what shows is from before.
    @Published public private(set) var stale: ProblemWords?
    /// An ask or an action is on its way: the buttons wait.
    @Published public private(set) var busy = false
    /// "Which seasons?", while it shows.
    @Published public var picker: SeasonPicker?

    public let kind: TitleKind
    public let tmdbId: Int
    /// The cards' actions and their dialogs.
    public let runner: RequestActionRunner
    public var now: () -> Int = { Int(Date().timeIntervalSince1970) }
    /// Seconds between reads while a request moves (tests shorten it).
    public var movingEvery = RequestTimings.movingSeconds
    public var debounce = RequestTimings.changeDebounce

    private let app: AppModel
    /// The picker was asked for when the page opened (Request more seasons), and hasn't opened yet.
    private var pickPending: Bool
    private var listeners: Set<AnyCancellable> = []
    private var soon: Task<Void, Never>?
    /// Until when the change of the page's own action, which the app's
    /// events gather a moment (`ServerEvents.window`), is read already.
    private var ownChangeUntil: Date?
    private var reading: Task<Void, Never>?
    private var again = false

    public init(app: AppModel, kind: TitleKind, tmdbId: Int, pick: Bool = false) {
        self.app = app
        self.kind = kind
        self.tmdbId = tmdbId
        pickPending = pick
        runner = RequestActionRunner(app: app)
        runner.onOutcome = { [weak self] _, _, outcome in
            guard let self else { return }
            if case .failed(let problem) = outcome { self.failed(problem) } else { Task { await self.loadActed() } }
        }
        app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, self.follows(change.requests) || change.everything else { return }
            if let until = self.ownChangeUntil {
                self.ownChangeUntil = nil
                if Date() < until { return }
            }
            self.loadSoon()
        }.store(in: &listeners)
        // The gate opening (a summary read late, signing in again) reads the
        // page; a count moving (what's left to ask) only says it again.
        var kind = app.requests.gate.kind
        app.requests.$summary.dropFirst().map { RequestsGate.of($0) }.removeDuplicates().sink { [weak self] gate in
            let same = gate.kind == kind
            kind = gate.kind
            Task { @MainActor in
                guard let self else { return }
                if same, let detail = self.detail { self.show(detail) } else { await self.load() }
            }
        }.store(in: &listeners)
    }

    /// The gate this page follows.
    public var gate: RequestsGate { app.requests.gate }

    /// Whether a change is the page's: many changed, the title, or one of its requests.
    func follows(_ change: RequestChange) -> Bool {
        change.about(kind, tmdbId: tmdbId) || requests.contains { change.ids.contains($0.id) }
    }

    /// Reads the page, then again every 10 s while one of its requests
    /// moves, in the foreground, until the view goes (its `.task` is cancelled).
    public func run() async {
        await load()
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(movingEvery))
            guard !Task.isCancelled else { return }
            if app.active, requests.contains(where: { $0.request.moving }) { await load() }
        }
    }

    /// Reads the page; what shows stays while it reads again, and when that
    /// fails. Asked while a read is on its way, it waits for that one, which
    /// then reads once more: every caller returns with the page as it is now.
    public func load() async {
        if let reading {
            again = true
            await reading.value
            return
        }
        // The read lets go of `reading` itself, as its last loop ends: a
        // call that comes after that starts a read of its own, rather than
        // waiting on one that won't look again.
        let task = Task {
            repeat {
                again = false
                await read()
            } while again
            reading = nil
        }
        reading = task
        await task.value
    }

    private func read() async {
        if gate == .unknown { await app.requests.refreshNow() }
        let gate = self.gate
        switch gate {
        case .unknown:
            state = .loading
            return
        case .open:
            break
        case .hidden, .noAccess, .signInAgain:
            // The page can't show: signed in before requests, only Sign in shows.
            let problem = RequestProblem.of(gate) ?? .off
            state = .failed(problem.words(manage: false, summary: gate.summary))
            return
        }
        let kind = kind, tmdbId = tmdbId
        do {
            let detail = try await app.requestsAPI().discoverDetail(kind, tmdbId: tmdbId)
            stale = nil
            show(detail)
        } catch {
            let problem = RequestProblem.of(error)
            if case .offline = problem { app.wentOffline() }
            let words = problem.words(manage: gate.summary?.manage == true, summary: gate.summary,
                                      failure: RequestWords.titleFailed, now: now())
            if detail == nil { state = .failed(words) } else { stale = words }
        }
    }

    /// Reads the page after it failed.
    public func retry() {
        app.requests.refresh()
        Task { await load() }
    }

    public func signInAgain() { app.signInAgain() }

    /// The page's own action: it reads now, and the change it sent, heard
    /// here too, doesn't read it again.
    private func loadActed() async {
        soon?.cancel()
        soon = nil
        ownChangeUntil = Date().addingTimeInterval(ServerEvents.window + 1)
        await load()
    }

    private func loadSoon() {
        soon?.cancel()
        let wait = debounce
        soon = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled else { return }
            await self?.load()
        }
    }

    private func show(_ detail: DiscoverDetail) {
        let gate = self.gate
        let summary = gate.summary
        let manage = summary?.manage == true
        let action = RequestRules.mainAction(detail, gate: gate)
        let allowance = Allowance(detail: detail, summary: summary)
        self.detail = detail
        page = DiscoverTitlePage.of(detail)
        self.action = action
        requestsHeading = manage ? RequestWords.manage : RequestWords.yourRequests
        let now = now()
        requests = detail.requests.map { RequestCard.of($0, manage: manage, mayAsk: false, now: now) }
        cast = detail.cast.enumerated().map {
            CastCard(index: $0.offset, name: $0.element.name, character: $0.element.character,
                     photoPath: $0.element.photoUrl)
        }
        let sorted = (detail.seasons ?? []).sorted { a, b in
            (a.number == 0) != (b.number == 0) ? b.number == 0 : a.number < b.number
        }
        seasons = sorted.map { season in
            let request = RequestRules.requestOf(season, in: detail)
            return SeasonRow(number: season.number, name: RequestWords.seasonName(season),
                             state: RequestWords.seasonState(season),
                             pill: request.map { PillWords.of($0, manage: manage, now: now) },
                             symbol: Self.symbol(season), posterPath: season.posterUrl)
        }
        var lines: [String] = []
        var limit = false
        switch action {
        case .open(itemId: nil):
            lines.append(RequestWords.readyElsewhere)
        case .restricted(let text):
            if let text, !text.isEmpty { lines.append(text) }
        case .declined(let reason, let againAt):
            lines.append(RequestWords.declined(reason: reason, againAt: againAt, now: now))
        case .tellMeToo(let ask):
            lines.append(ask == .noAccess ? RequestWords.noAccessLine : RequestWords.askedAlready)
        case .request(let ask), .partial(_, let ask?):
            if ask == .noAccess { lines.append(RequestWords.noAccessLine) }
            limit = ask == .limit
        default:
            break
        }
        if gate.mayAsk { lines += RequestWords.allowance(allowance, kind: detail.kind, now: now) }
        self.lines = lines
        limitReached = limit
        switch action {
        case .mine(let request?), .following(let request?): actionPill = PillWords.of(request, manage: manage, now: now)
        default: actionPill = nil
        }
        state = .ready
        if pickPending && detail.kind == .show {
            pickPending = false
            switch action {
            case .request(.yes), .partial(_, .yes?): picker = SeasonPicker.of(detail, left: allowance.seasonsLeft)
            default: break
            }
        }
        // The picker follows the page as it's read again: seasons asked for meanwhile can't be ticked.
        picker = picker?.rebuilt(from: detail, left: allowance.seasonsLeft)
    }

    private static func symbol(_ season: DiscoverSeason) -> String {
        if season.declined { return "xmark.circle" }
        switch season.state {
        case .library: return "checkmark.circle.fill"
        case .requested: return "clock"
        case .partial: return "circle.lefthalf.filled"
        case .unaired: return "calendar"
        default: return "circle"
        }
    }

    // MARK: Actions

    /// Request: a movie is asked for; a show's picker opens.
    public func request() {
        guard let detail, !busy else { return }
        if detail.kind == .show {
            openPicker()
        } else {
            ask(NewRequest(kind: detail.kind, tmdbId: detail.tmdbId), title: detail.title)
        }
    }

    /// Request more seasons: the picker.
    public func openPicker() {
        guard let detail else { return }
        picker = SeasonPicker.of(detail, left: Allowance(detail: detail, summary: gate.summary).seasonsLeft)
    }

    public func toggle(_ season: Int) { picker?.toggle(season) }

    public func closePicker() { picker = nil }

    /// The picker's allowance lines, red while more are ticked than may be asked for.
    public var pickerLines: [String] {
        guard let detail else { return [] }
        return RequestWords.allowance(Allowance(detail: detail, summary: gate.summary), kind: .show, now: now())
    }

    /// The picker's Request: the seasons ticked.
    public func askPicked() {
        guard let detail, let picker, picker.ready, !busy else { return }
        ask(NewRequest(kind: detail.kind, tmdbId: detail.tmdbId, seasons: picker.asked), title: detail.title,
            closesPicker: true)
    }

    /// Tell me too: following the requests others made, which costs nothing.
    public func tellMeToo() {
        guard let detail, !busy else { return }
        ask(RequestRules.tellMeToo(detail), title: detail.title)
    }

    /// Stop following the request this account follows for it.
    public func stopFollowing() {
        guard let detail, !busy else { return }
        let request: MediaRequest?
        if case .following(let followed?) = action {
            request = followed
        } else {
            request = RequestRules.activeRequest(detail).flatMap { $0.watching ? $0 : nil }
        }
        guard let request else { return }
        busy = true
        Task {
            await runner.run(.stopFollowing, on: RequestCard.of(request, manage: false, mayAsk: false, now: now()))
            busy = false
        }
    }

    /// Where Open goes: the library's title.
    public func open() -> Screen? {
        switch action {
        case .open(let itemId?), .partial(let itemId?, _): .item(itemId)
        default: nil
        }
    }

    private func ask(_ new: NewRequest, title: String, closesPicker: Bool = false) {
        busy = true
        Task {
            defer { busy = false }
            do {
                let asked = try await app.requestsAPI().ask(new)
                if closesPicker { picker = nil }
                app.requests.acted(asked.request)
                app.shell.show(RequestWords.asked(asked, title: title))
                await loadActed()
            } catch {
                failed(RequestProblem.of(error))
            }
        }
    }

    private func failed(_ problem: RequestProblem) {
        // Someone acted meanwhile: the page is read again, and what happened is told.
        switch problem {
        case .conflict, .notFound: Task { await load() }
        default: break
        }
        app.tell(problem)
    }
}
