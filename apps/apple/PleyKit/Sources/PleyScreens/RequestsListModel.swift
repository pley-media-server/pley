import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// A filter chip of the requests' list.
public struct RequestChip: Hashable, Sendable, Identifiable {
    /// The accessibility identifier's last part: `attention`, `pending`, `movie`.
    public var id: String
    /// "Needs attention · 3".
    public var title: String
    public var selected: Bool
}

/// What an empty list says.
public struct EmptyWords: Hashable, Sendable {
    public var title: String
    public var body: String
    /// Discover is offered: asking works.
    public var discover: Bool
}

/// Your requests, or for managers everyone's (Android's
/// `RequestsViewModel`), opening on Needs attention
/// (`attention=1&order=oldest`), with filters (movies or shows, a person,
/// a title) and their actions. Each card offers exactly the server's
/// actions. The list follows the changes to requests (one request read
/// again when only it changed), every 10 s while something in it
/// downloads, and every 30 s while events can't come. Once its first page
/// shows, the person's news is seen.
@MainActor
public final class RequestsListModel: ObservableObject {
    public enum State: Hashable {
        case loading
        case ready
        case empty(EmptyWords)
        case failed(ProblemWords)
    }

    public static let page = 30
    /// The most requests the server lists at once.
    public static let maxPage = 200
    /// How long typing in the title filter pauses before it applies.
    public static let typing: Double = 0.3
    /// The list this near its end reads the next page.
    public static let near = 5

    @Published public private(set) var state: State = .loading
    @Published public private(set) var cards: [RequestCard] = []
    @Published public private(set) var loadingMore = false
    /// Reading again failed: what shows is from before.
    @Published public private(set) var stale: ProblemWords?
    /// Why the list can't show at all: requests off, signed in before requests.
    @Published public private(set) var gateProblem: ProblemWords?
    /// An administrator's app without approvals: "Sign in again to approve and decline here."
    @Published public private(set) var approvalsBanner = false
    @Published public private(set) var filter: RequestFilter?
    /// What is typed in the title filter (managers), applied after a pause.
    @Published public private(set) var query = ""
    @Published public private(set) var refreshing = false
    /// Managers: the people with requests, for the person filter.
    @Published public private(set) var people: [RequestUser] = []

    public let runner: RequestActionRunner
    public var now: () -> Int = { Int(Date().timeIntervalSince1970) }
    public var movingEvery = RequestTimings.movingSeconds
    public var pollEvery = RequestTimings.listPollSeconds
    public var debounce = RequestTimings.changeDebounce
    public var typingPause = RequestsListModel.typing

    private let app: AppModel
    private var total = 0
    private var counts = RequestCounts()
    private var requests: [MediaRequest] = []
    private var seen = false
    private var reads = 0
    private var listeners: Set<AnyCancellable> = []
    private var soon: Task<Void, Never>?
    private var typed: Task<Void, Never>?

    public init(app: AppModel) {
        self.app = app
        runner = RequestActionRunner(app: app)
        runner.onOutcome = { [weak self] card, _, outcome in self?.acted(card.request, outcome) }
        app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, !change.requests.isEmpty || change.everything else { return }
            Task { await self.changed(change.everything ? .everything : change.requests) }
        }.store(in: &listeners)
        app.requests.$summary.dropFirst().sink { [weak self] _ in
            Task { @MainActor in self?.gateChanged() }
        }.store(in: &listeners)
    }

    private var gate: RequestsGate { app.requests.gate }
    private var summary: RequestSummary? { gate.summary }

    /// Everyone's, with Needs attention, the filters and the managers' actions.
    public var manage: Bool { summary?.manage == true }
    /// "Requests" for managers, else "Your requests".
    public var title: String { manage ? RequestWords.manage : RequestWords.yourRequests }
    /// Asking works: titles open their Discover page.
    public var mayAsk: Bool { gate.mayAsk }

    /// The view chips: Needs attention (managers), All, and the statuses with any or chosen.
    public var views: [RequestChip] {
        guard let filter else { return [] }
        var views: [RequestView] = manage ? [.attention, .all] : [.all]
        views += RequestView.statuses.filter { counts[$0.status!] > 0 || filter.view == $0 }
        return views.map { view in
            let count: Int? = switch view {
            case .attention: summary.map { $0.pending + $0.failed }
            case .all: nil
            default: counts[view.status!]
            }
            let title = count.map { RequestWords.filterCount(view.title, $0) } ?? view.title
            return RequestChip(id: view.key, title: title, selected: filter.view == view)
        }
    }

    /// Movies and Shows.
    public var kindChips: [RequestChip] {
        [(TitleKind.movie, RequestWords.filterMovies), (.show, RequestWords.filterShows)].map {
            RequestChip(id: $0.0.rawValue, title: $0.1, selected: filter?.kind == $0.0)
        }
    }

    /// The person chip's words: "Everyone", or the name chosen.
    public var personTitle: String {
        guard let id = filter?.userId else { return RequestWords.filterEveryone }
        return people.first { $0.id == id }?.name ?? RequestWords.filterEveryone
    }

    /// The list's first page, once the gate opens.
    public func load() async {
        if gate == .unknown { await app.requests.refreshNow() }
        gateChanged(reads: false)
        guard let filter else { return }
        await read(filter)
    }

    /// Reads the list, then again every 10 s while something in it moves
    /// and every 30 s while events can't come, until the view goes.
    public func run() async {
        // Back on the page (from a title it opened): what shows is read again quietly, where it was.
        if state == .ready { await reload(quiet: true) } else { await load() }
        var waited: Double = 0
        let tick = min(movingEvery, pollEvery)
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(tick))
            guard !Task.isCancelled else { return }
            waited += tick
            let moving = requests.contains { $0.moving }
            if (moving && waited >= movingEvery) || (!app.serverEvents.connected && waited >= pollEvery) {
                waited = 0
                await reload(quiet: true)
            }
        }
    }

    /// The summary changed: the gate's problem, the banner, and the list's first filter once it opens.
    private func gateChanged(reads: Bool = true) {
        let gate = self.gate
        approvalsBanner = gate.summary?.signInAgain == RequestSummary.approvalsScope
        switch gate {
        case .signInAgain: gateProblem = RequestProblem.signInAgain(.requests).words(manage: false, summary: summary)
        case .hidden: gateProblem = RequestProblem.off.words(manage: false, summary: nil)
        case .noAccess: gateProblem = RequestProblem.noAccess.words(manage: false, summary: nil)
        case .unknown, .open: gateProblem = nil
        }
        // Managers open on Needs attention; everyone else on all of theirs.
        if filter == nil, case .open(let summary) = gate {
            filter = RequestFilter(view: summary.manage ? .attention : .all)
            if reads, let filter { Task { await read(filter) } }
        }
        rebuild()
    }

    // MARK: Filters

    public func select(view: RequestView) { set { $0.view = view } }

    /// Movies or shows; the one chosen again lets go.
    public func select(kind: TitleKind?) { set { $0.kind = $0.kind == kind ? nil : kind } }

    public func select(person userId: Int?) { set { $0.userId = userId } }

    public func setMine(_ mine: Bool) { set { $0.mine = mine } }

    /// The title filter's field changed: it applies once typing pauses.
    public func setQuery(_ text: String) {
        query = text
        typed?.cancel()
        let wait = typingPause
        typed = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled, let self else { return }
            let q = text.trimmingCharacters(in: .whitespacesAndNewlines)
            self.set { $0.q = q }
        }
    }

    private func set(_ change: (inout RequestFilter) -> Void) {
        guard var filter else { return }
        change(&filter)
        guard filter != self.filter else { return }
        self.filter = filter
        Task { await read(filter) }
    }

    // MARK: Reading

    private func read(_ filter: RequestFilter) async {
        reads += 1
        let read = reads
        state = .loading
        do {
            let answer = try await app.requestsAPI().requests(filter.query(offset: 0, limit: Self.page))
            guard read == reads else { return }
            show(answer, filter)
            markSeen()
        } catch {
            guard read == reads else { return }
            state = .failed(words(RequestProblem.of(error)))
        }
    }

    private func words(_ problem: RequestProblem) -> ProblemWords {
        if case .offline = problem { app.wentOffline() }
        return problem.words(manage: manage, summary: summary, failure: RequestWords.listFailed, now: now())
    }

    private func show(_ answer: RequestList, _ filter: RequestFilter) {
        var ids: Set<Int> = []
        requests = answer.requests.filter { ids.insert($0.id).inserted }
        total = answer.total
        counts = answer.counts
        if manage { people = answer.people }
        stale = nil
        state = requests.isEmpty && total <= 0 ? .empty(empty(filter)) : .ready
        rebuild()
    }

    /// The cards, as the requests and the summary say them.
    private func rebuild() {
        let now = now()
        let manage = self.manage && summary?.signInAgain != RequestSummary.approvalsScope
        cards = requests.map { RequestCard.of($0, manage: manage, mayAsk: mayAsk, now: now) }
    }

    private func empty(_ filter: RequestFilter) -> EmptyWords {
        if filter.narrowed || filter.view.status != nil {
            return EmptyWords(title: RequestWords.filteredNone, body: RequestWords.filteredNoneBody, discover: false)
        }
        if filter.view == .attention {
            return EmptyWords(title: RequestWords.attentionNone, body: RequestWords.attentionNoneBody, discover: false)
        }
        return mayAsk
            ? EmptyWords(title: RequestWords.noneTitle, body: RequestWords.noneBody, discover: true)
            : EmptyWords(title: RequestWords.noneTitle, body: RequestWords.noneBodyOff, discover: false)
    }

    /// Once the first page has shown: the person's news is seen, and the badge clears.
    private func markSeen() {
        guard !seen, state == .ready else { return }
        seen = true
        guard (summary?.unseen ?? 0) > 0 else { return }
        app.requests.zeroUnseen()
        Task { try? await app.requestsAPI().markRequestsSeen() }
    }

    /// Pull to refresh, ⌘R or Retry: the summary and the list read again
    /// (once, however often it's asked meanwhile).
    public func refresh() async {
        guard !refreshing else { return }
        app.requests.refresh()
        guard let filter else { return await load() }
        guard state == .ready else { return await read(filter) }
        refreshing = true
        await reload(quiet: false)
        refreshing = false
    }

    /// Reads what shows again, as long as it is now. `quiet`: a failure keeps what shows.
    private func reload(quiet: Bool) async {
        guard let filter else { return }
        let count = max(Self.page, requests.count)
        do {
            let answer = try await first(filter, count: count)
            guard filter == self.filter else { return }
            show(answer, filter)
            markSeen()
        } catch {
            let words = words(RequestProblem.of(error))
            if state == .ready { stale = words } else if !quiet { state = .failed(words) }
        }
    }

    /// The first `count` requests `filter` keeps, read `maxPage` at a time.
    private func first(_ filter: RequestFilter, count: Int) async throws -> RequestList {
        let api = try await app.requestsAPI()
        var answer = try await api.requests(filter.query(offset: 0, limit: min(count, Self.maxPage)))
        while answer.requests.count < min(count, answer.total) {
            let read = answer.requests.count
            let next = try await api.requests(filter.query(offset: read, limit: min(count - read, Self.maxPage)))
            if next.requests.isEmpty { break }
            answer.requests += next.requests
        }
        return answer
    }

    /// The list shows the card at `index`: near its end, the next page.
    public func cardAppeared(_ index: Int) {
        guard let filter, state == .ready, index >= cards.count - Self.near, requests.count < total, !loadingMore else {
            return
        }
        loadingMore = true
        stale = nil
        let read = reads
        Task {
            defer { loadingMore = false }
            do {
                let answer = try await app.requestsAPI().requests(filter.query(offset: requests.count, limit: Self.page))
                guard read == reads, filter == self.filter else { return }
                var ids = Set(requests.map(\.id))
                requests += answer.requests.filter { ids.insert($0.id).inserted }
                total = answer.total
                rebuild()
            } catch {
                stale = words(RequestProblem.of(error))
            }
        }
    }

    // MARK: What changed

    private func changed(_ change: RequestChange) async {
        let shown = Set(requests.map(\.id))
        let only = change.ids.intersection(shown)
        guard !change.all, change.ids.count == 1, let id = only.first, state == .ready else {
            reloadSoon()
            return
        }
        // Only that request: it alone is read again. Not found (`nil`), it
        // was deleted or isn't this account's any more, and its card goes;
        // a read that fails keeps the card as it was.
        guard let api = try? await app.requestsAPI() else { return }
        do {
            replace(id, with: try await api.request(id))
        } catch {}
    }

    private func reloadSoon() {
        soon?.cancel()
        let wait = debounce
        soon = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled else { return }
            await self?.reload(quiet: true)
        }
    }

    /// `id` as it is now (`request`), kept where it still belongs; `nil`: gone.
    private func replace(_ id: Int, with request: MediaRequest?) {
        guard let filter else { return }
        let before = requests.count
        if let request, filter.matches(request) {
            requests = requests.map { $0.id == id ? request : $0 }
        } else {
            requests.removeAll { $0.id == id }
        }
        total -= before - requests.count
        if requests.isEmpty && total <= 0 {
            cards = []
            state = .empty(empty(filter))
        } else {
            rebuild()
        }
    }

    private func acted(_ request: MediaRequest, _ outcome: RequestOutcome) {
        switch outcome {
        case .changed(let now): replace(request.id, with: now)
        case .gone(let id): replace(id, with: nil)
        case .askedAgain: Task { await reload(quiet: true) }
        case .failed(let problem):
            // A race: the server sends the request as it is now.
            if case .conflict(_, let now?) = problem { replace(request.id, with: now) }
            if problem == .notFound { replace(request.id, with: nil) }
        }
    }

    public func signInAgain() { app.signInAgain() }
}
