import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// One of Discover's rows: its titles as read so far.
public struct DiscoverRowModel: Hashable, Sendable, Identifiable {
    public var list: DiscoverList
    public var pages = TitlePages()
    /// Its cards, words ready.
    public var cards: [DiscoverCard] = []
    /// The row was asked for: it came into view, or every row is read for a hero.
    public var asked = false

    public var id: DiscoverList { list }
    /// "Trending this week".
    public var title: String { list.title }
    /// It shows: it has titles, or is reading them without a problem.
    public var visible: Bool { !cards.isEmpty || (!pages.settled && pages.problem == nil) }
}

/// The hero's main button, for what it does from Discover; the rest is on the title's page.
public enum HeroButton: Hashable, Sendable {
    /// Request; disabled when nothing more may be asked for.
    case request(enabled: Bool)
    case tellMeToo
    case open
    case requestMore

    public var title: String {
        switch self {
        case .request: RequestWords.request
        case .tellMeToo: RequestWords.tellMeToo
        case .open: RequestWords.open
        case .requestMore: RequestWords.moreSeasons
        }
    }
}

/// Discover's top: the first title of its rows (trending's, unless it has
/// none for this account), with its main action, and its page once read.
public struct DiscoverHero: Hashable, Sendable {
    public var card: DiscoverCard
    public var overview: String
    public var backdropPath: String?
    public var action: MainAction
    public var primary: HeroButton?
    /// Under the buttons: the declined line, or the allowance lines when nothing more may be asked for.
    public var lines: [String]
    /// Its request's pill, when its page says it.
    public var pill: PillWords?
    /// The title's page, once read.
    public var detail: DiscoverDetail?
}

/// Discover (Android's `DiscoverViewModel`): a hero for the first trending
/// title (the first title of the next row with any, when trending has none
/// for this account: then every row is read at once, to find one), then
/// the rows read as they come into view (`rowAppeared`), page by page until
/// a screenful and then as they scroll (`cardAppeared`), each title once;
/// then the person's requests (read again as requests change, and every
/// 10 s while one downloads). Badges follow the changes in rows already
/// shown, without reading the rows again.
@MainActor
public final class DiscoverModel: ObservableObject {
    /// A row's screenful: what fills a phone's or a tablet's width, and a little more.
    public static let rowScreenful = 12
    /// Pages read on their own, one after another, before waiting for a scroll.
    public static let maxAutoPages = 4
    /// "Your requests" on Discover: the newest.
    public static let yours = 12
    /// A row this near its end reads its next page.
    public static let near = 4

    @Published public private(set) var rows: [DiscoverRowModel] = DiscoverList.allCases.map { DiscoverRowModel(list: $0) }
    @Published public private(set) var hero: DiscoverHero?
    /// "Your requests", for those who have any; `nil`: none to show.
    @Published public private(set) var yourRequests: [RequestCard]?
    /// Why the page can't show: the gate first, then trending's failure,
    /// then, with no title in any row, the first row's that failed.
    @Published public private(set) var problem: ProblemWords?
    /// Every row was read, and none has anything this account can ask for: "Nothing here right now".
    @Published public private(set) var nothing = false
    /// Shown from what was saved: the server didn't answer.
    @Published public private(set) var stale = false
    /// The hero's ask is on its way: its button waits.
    @Published public private(set) var asking = false

    public let credit = RequestWords.credit
    public let yourRequestsTitle = RequestWords.yourRequests
    public var now: () -> Int = { Int(Date().timeIntervalSince1970) }
    public var movingEvery = RequestTimings.movingSeconds
    public var debounce = RequestTimings.changeDebounce

    private let app: AppModel
    private var listeners: Set<AnyCancellable> = []
    private var yoursSoon: Task<Void, Never>?
    private var started = false

    public init(app: AppModel) {
        self.app = app
        app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, !change.requests.isEmpty || change.everything else { return }
            Task { await self.patch(change.requests) }
            self.readYoursSoon()
        }.store(in: &listeners)
        // A count moving says the cards again; the gate opening or closing
        // reads them (a change to the person's requests reads them itself).
        var kind = app.requests.gate.kind
        app.requests.$summary.dropFirst().map { RequestsGate.of($0) }.removeDuplicates().sink { [weak self] gate in
            let same = gate.kind == kind
            kind = gate.kind
            Task { @MainActor in
                guard let self else { return }
                if same { self.rebuild() } else { await self.gateChanged() }
            }
        }.store(in: &listeners)
    }

    private var gate: RequestsGate { app.requests.gate }

    /// Reads the page, then "Your requests" every 10 s while one moves, in
    /// the foreground, until the view goes (its `.task` is cancelled).
    public func run() async {
        await load()
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(movingEvery))
            guard !Task.isCancelled else { return }
            if app.active, yourRequests?.contains(where: { $0.request.moving }) == true { await readYours() }
        }
    }

    /// The gate first; then the trending row at once, for the hero.
    public func load() async {
        if gate == .unknown { await app.requests.refreshNow() }
        await gateChanged()
    }

    private func gateChanged() async {
        rebuild()
        guard gate.mayAsk else { return }
        if !started {
            started = true
            await rowAppeared(.trending)
        }
        await readYours()
    }

    /// `list`'s row came into view: its first page is read (from what was
    /// saved first), then more until a screenful.
    public func rowAppeared(_ list: DiscoverList) async {
        guard gate.mayAsk, let index = index(list), !rows[index].asked else { return }
        rows[index].asked = true
        rows[index].pages.loading = true
        rebuild()
        do {
            let first = try await app.cached(SnapshotKeys.discover(list)) { try await $0.discover(list) }
            guard let i = self.index(list) else { return }
            rows[i].pages = rows[i].pages.with(first)
            if list == .trending { stale = app.offline }
            rebuild()
            if !app.offline { await fill(list) }
        } catch {
            guard let i = self.index(list) else { return }
            rows[i].pages.loading = false
            rows[i].pages.problem = RequestProblem.of(error)
            rebuild()
        }
        // Trending has nothing this account can ask for: every row is read at once, for a hero.
        if list == .trending, let row = rows.first, row.pages.titles.isEmpty, row.pages.settled, row.pages.problem == nil {
            for other in DiscoverList.allCases.dropFirst() { await rowAppeared(other) }
        }
    }

    /// The row shows the card at `index`: near its end, its next page.
    public func cardAppeared(_ list: DiscoverList, index card: Int) {
        guard let i = index(list) else { return }
        let pages = rows[i].pages
        guard card >= rows[i].cards.count - Self.near, pages.more, !pages.loading, pages.page > 0 else { return }
        Task { _ = await next(list) }
    }

    /// Tries again what failed: the summary, and the rows that couldn't be read.
    public func retry() {
        Task {
            await app.requests.refreshNow()
            for row in rows where row.pages.problem != nil {
                guard let i = index(row.list) else { continue }
                rows[i].pages.problem = nil
                rows[i].asked = false
                await rowAppeared(row.list)
            }
            if !started { await gateChanged() }
            rebuild()
        }
    }

    public func signInAgain() { app.signInAgain() }

    private func index(_ list: DiscoverList) -> Int? { rows.firstIndex { $0.list == list } }

    /// Reads on while `list`'s row is short of a screenful; still short, it waits for a scroll.
    private func fill(_ list: DiscoverList) async {
        var auto = 0
        while let i = index(list), rows[i].pages.short(Self.rowScreenful), auto < Self.maxAutoPages {
            auto += 1
            if !(await next(list)) { break }
        }
        if let i = index(list), rows[i].pages.short(Self.rowScreenful) {
            rows[i].pages.stopped = true
            rebuild()
        }
    }

    /// Reads `list`'s next page; whether it came.
    private func next(_ list: DiscoverList) async -> Bool {
        guard let i = index(list), !rows[i].pages.loading else { return false }
        let page = rows[i].pages.page + 1
        rows[i].pages.loading = true
        rows[i].pages.problem = nil
        do {
            let answer = try await app.requestsAPI().discover(list, page: page)
            guard let i = index(list) else { return false }
            rows[i].pages = rows[i].pages.with(answer)
            rebuild()
            return true
        } catch {
            guard let i = index(list) else { return false }
            rows[i].pages.loading = false
            rows[i].pages.problem = RequestProblem.of(error)
            rebuild()
            return false
        }
    }

    /// The page from the rows: cards, the hero, the problem.
    private func rebuild() {
        for i in rows.indices { rows[i].cards = rows[i].pages.titles.map(DiscoverCard.init) }
        let gate = self.gate
        let summary = gate.summary
        let manage = summary?.manage == true
        let trending = rows[0].pages
        let allSettledEmpty = rows.allSatisfy { $0.pages.titles.isEmpty && $0.pages.settled }
        let failed = allSettledEmpty ? rows.lazy.compactMap(\.pages.problem).first : nil
        let failure = RequestProblem.of(gate) ?? (trending.titles.isEmpty ? trending.problem : nil) ?? failed
        problem = failure?.words(manage: manage, summary: summary, failure: RequestWords.discoverFailed, now: now())
        nothing = failure == nil && rows.allSatisfy { $0.pages.titles.isEmpty && $0.pages.settled && $0.pages.problem == nil }
        hero = heroTitle().map { hero(for: $0, detail: hero?.detail) }
        if let title = heroTitle(), hero?.detail?.tmdbId != title.tmdbId || hero?.detail?.kind != title.kind {
            readHero(title)
        }
    }

    /// The hero's title: the first of the first row with any, once the
    /// rows before it have come with none, or failed.
    private func heroTitle() -> DiscoverTitle? {
        for row in rows {
            if let first = row.pages.titles.first { return first }
            if !row.pages.settled { return nil }
        }
        return nil
    }

    private var heroRead: String?
    /// Counts the hero's reads: a read that a later one replaced keeps what it found to itself.
    private var heroReads = 0

    /// The hero's page, read for its exact main action.
    private func readHero(_ title: DiscoverTitle) {
        guard heroRead != title.key else { return }
        heroRead = title.key
        heroReads += 1
        let read = heroReads
        Task {
            guard let detail = try? await app.requestsAPI().discoverDetail(title.kind, tmdbId: title.tmdbId),
                  read == heroReads, let now = heroTitle(), now.key == title.key else { return }
            hero = hero(for: now, detail: detail)
        }
    }

    private func hero(for title: DiscoverTitle, detail: DiscoverDetail?) -> DiscoverHero {
        let gate = self.gate
        let summary = gate.summary
        let page = detail.flatMap { $0.kind == title.kind && $0.tmdbId == title.tmdbId ? $0 : nil }
        let allowance = page.map { Allowance(detail: $0, summary: summary) } ?? summary.map(Allowance.init(summary:))
        let action = page.map { RequestRules.mainAction($0, gate: gate) }
            ?? RequestRules.titleAction(title, gate: gate, allowance: allowance)
        // Only what the hero does from here gets a button: Requested and
        // Following show the request's stage, and Stop following, the
        // seasons and why asking doesn't work are on the title's page.
        var primary: HeroButton?
        var lines: [String] = []
        var pill: PillWords?
        let now = now()
        switch action {
        case .signInAgain, .restricted:
            break
        case .open(let itemId):
            primary = itemId != nil ? .open : nil
        case .declined(let reason, let againAt):
            lines.append(RequestWords.declined(reason: reason, againAt: againAt, now: now))
        case .mine(let request), .following(let request):
            pill = request.map { PillWords.of($0, manage: summary?.manage == true, now: now) }
        case .tellMeToo(let ask):
            primary = ask == .yes ? .tellMeToo : nil
        case .partial(let itemId, let more):
            primary = itemId != nil ? .open : more == .yes ? .requestMore : nil
        case .request(let ask):
            primary = ask == .noAccess ? nil : .request(enabled: ask == .yes)
            if ask == .limit, let allowance { lines += RequestWords.allowance(allowance, kind: title.kind, now: now) }
        }
        return DiscoverHero(card: DiscoverCard(title), overview: page?.overview ?? title.overview,
                            backdropPath: page?.backdropUrl ?? title.backdropUrl, action: action, primary: primary,
                            lines: lines, pill: pill, detail: page)
    }

    // MARK: The hero's actions

    /// The hero's main button: a movie is asked for here; a show opens
    /// its page with the picker; what a library has opens there; the rest
    /// open the title.
    public func heroPrimary() {
        guard let hero else { return }
        let title = hero.card.source
        let page = Screen.discoverTitle(title.kind, tmdbId: title.tmdbId)
        switch hero.action {
        case .open(let itemId):
            app.shell.open(itemId.map { .item($0) } ?? page)
        case .request(let can):
            if can != .yes {
                app.shell.open(page)
            } else if title.kind == .movie {
                ask(NewRequest(kind: .movie, tmdbId: title.tmdbId), title: title.title)
            } else {
                app.shell.open(.discoverTitle(title.kind, tmdbId: title.tmdbId, pick: true))
            }
        case .partial(let itemId, let more):
            app.shell.open(itemId.map { .item($0) } ?? .discoverTitle(title.kind, tmdbId: title.tmdbId, pick: more == .yes))
        case .tellMeToo(let can):
            if can == .yes, let detail = hero.detail {
                ask(RequestRules.tellMeToo(detail), title: title.title)
            } else if can == .yes && title.kind == .movie {
                ask(NewRequest(kind: .movie, tmdbId: title.tmdbId), title: title.title)
            } else {
                app.shell.open(page)
            }
        default:
            app.shell.open(page)
        }
    }

    /// The hero's Details: its Discover page, even for a title a library
    /// has whole (its main action, Open, goes to the library's).
    public func heroDetails() {
        guard let hero else { return }
        let title = hero.card.source
        app.shell.open(.discoverTitle(title.kind, tmdbId: title.tmdbId))
    }

    /// Asks for `new`, once at a time: a second press while it's on its way does nothing.
    private func ask(_ new: NewRequest, title: String) {
        guard !asking else { return }
        asking = true
        Task {
            defer { asking = false }
            do {
                let asked = try await app.requestsAPI().ask(new)
                app.requests.acted(asked.request)
                app.shell.show(RequestWords.asked(asked, title: title))
            } catch {
                app.tell(RequestProblem.of(error))
            }
        }
    }

    // MARK: What changed

    /// Badges in rows already shown follow the change; a change to the
    /// hero's title lets its page go, to be read again for its main action.
    private func patch(_ change: RequestChange) async {
        let before = heroTitle()
        let shown = rows.flatMap(\.pages.titles)
        if let patch = await app.patchFor(change, shown: shown) {
            for i in rows.indices { rows[i].pages.titles = rows[i].pages.titles.map(patch) }
        }
        if [before, heroTitle()].contains(where: { $0.map { RequestRules.touches($0, change) } == true }) {
            hero?.detail = nil
            heroRead = nil
        }
        rebuild()
    }

    private func readYoursSoon() {
        yoursSoon?.cancel()
        let wait = debounce
        yoursSoon = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled else { return }
            await self?.readYours()
        }
    }

    /// "Your requests" for those who have any.
    private func readYours() async {
        guard let summary = gate.summary, summary.own > 0, summary.yourRequests else {
            yourRequests = nil
            return
        }
        guard let list = try? await app.requestsAPI().requests(RequestQuery(mine: true, limit: Self.yours)) else { return }
        let now = now()
        let cards = list.requests.map { RequestCard.of($0, manage: false, mayAsk: summary.mayAsk, now: now) }
        yourRequests = cards.isEmpty ? nil : cards
    }
}
