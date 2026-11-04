import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// What a grid of TMDB's titles shows.
public enum DiscoverSource: Hashable, Sendable {
    /// Every title TMDB finds for the words, those a library has too (Search's See all).
    case words(String)
    /// Every title of one of Discover's lists (a row's See all, on Apple TV).
    case list(DiscoverList)
}

/// A grid of TMDB's titles (Android's `DiscoverResultsViewModel`): the See
/// all of Search's Discover row, every title TMDB finds for the words, or
/// on Apple TV the See all of a Discover row (`DiscoverSource`); each with
/// its badge, those in a library too. It reads page by page until a
/// screenful, `fill` pages at most, then as the grid scrolls: a page at a
/// time near its end (`cardAppeared`), or on Apple TV until the grid has
/// what it wants (`want`), `fill` pages at most, stopping short then until
/// Show more (`showMore`): for an account with an age limit, page after
/// page can come empty. Badges follow the changes to requests without
/// reading the results again.
@MainActor
public final class DiscoverResultsModel: ObservableObject {
    public enum State: Hashable {
        case loading
        case ready
        /// TMDB found nothing: "Nothing on TMDB matches “q”".
        case empty
        case failed(ProblemWords)
    }

    /// A screenful: a phone's grid of posters.
    public static let screenful = 20
    /// Pages read on their own at most, before the grid scrolls.
    public static let fill = 4
    /// Pages Apple TV's grid reads at a time, at most, before it offers Show more.
    public static let tvFill = 5
    /// The grid this near its end reads the next page.
    public static let near = 9

    @Published public private(set) var state: State = .loading
    @Published public private(set) var cards: [DiscoverCard] = []
    @Published public private(set) var loadingMore = false
    /// Reading on failed: the titles before show, with why.
    @Published public private(set) var stale: ProblemWords?
    /// Apple TV: the reading stopped short of what the grid wants, or
    /// failed after some titles came: Show more (or Retry) at its end.
    @Published public private(set) var stopped = false

    public let source: DiscoverSource
    /// The words searched for, as the server takes them; empty for a list.
    public let query: String
    /// The list shown whole; `nil` for a search's.
    public var list: DiscoverList? {
        if case .list(let list) = source { return list }
        return nil
    }
    /// "Results for “q”", or the list's name.
    public var heading: String { list?.title ?? RequestWords.results(query) }
    public var emptyTitle: String { list == nil ? RequestWords.searchNone(query) : RequestWords.nothingHere }
    public var emptyBody: String { list == nil ? RequestWords.searchNoneBody : RequestWords.listNothingBody }
    /// What failing to read says, before why.
    public var failure: String { list == nil ? RequestWords.searchFailed : RequestWords.gridFailed }
    /// Beside a lone Show more: nothing came yet, and why when a page failed.
    public var noneYet: ProblemWords {
        if let problem = pages.problem {
            return problem.words(manage: false, summary: app.requests.summary, failure: failure)
        }
        let title = list == nil ? RequestWords.searchNoneYet(query) : RequestWords.noneYet
        return ProblemWords(title: title, body: RequestWords.noneYetBody, symbol: "safari")
    }
    /// Show more's place holds Retry: the last page failed.
    public var failedMore: Bool { pages.problem != nil }

    private let app: AppModel
    private let screenful: Int
    private let fillPages: Int
    /// Apple TV's grid: it stops short with Show more, even before any title came.
    private let tv: Bool
    private var pages = TitlePages()
    private var listeners: Set<AnyCancellable> = []
    private var reads = 0
    /// How many titles Apple TV's grid wants: it grows as the focus nears the end.
    private var wanted: Int

    public convenience init(app: AppModel, query: String) {
        self.init(app: app, source: .words(query))
    }

    /// `tv`: Apple TV's grid, which wants `screenful` titles first and
    /// reads `tvFill` pages at a time at most.
    public init(app: AppModel, source: DiscoverSource, screenful: Int = DiscoverResultsModel.screenful,
                tv: Bool = false) {
        self.app = app
        self.source = source
        self.screenful = screenful
        self.tv = tv
        fillPages = tv ? Self.tvFill : Self.fill
        wanted = screenful
        if case .words(let words) = source {
            query = PleyAPI.searchText(words)
        } else {
            query = ""
        }
        app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, !change.requests.isEmpty else { return }
            Task { await self.patch(change.requests) }
        }.store(in: &listeners)
    }

    /// The first pages, until a screenful.
    public func load() async {
        if app.requests.gate == .unknown { await app.requests.refreshNow() }
        let gate = app.requests.gate
        guard gate.mayAsk else {
            state = .failed((RequestProblem.of(gate) ?? .off).words(manage: false, summary: gate.summary))
            return
        }
        if case .words = source, query.isEmpty {
            state = .empty
            return
        }
        reads += 1
        let read = reads
        pages = TitlePages()
        wanted = screenful
        stopped = false
        state = .loading
        await fill(to: screenful, pages: fillPages, read: read)
    }

    /// The grid shows the card at `index`: near its end, the next page.
    public func cardAppeared(_ index: Int) {
        guard index >= cards.count - Self.near, pages.more, !pages.loading, state == .ready else { return }
        let read = reads
        pages.stopped = false
        pages.problem = nil
        Task { await fill(to: pages.titles.count + 1, pages: 1, read: read) }
    }

    /// Apple TV: the grid wants `count` titles (the focus came near its
    /// end). Pages are read until it has them, unless the reading stopped
    /// short or failed: then only Show more goes on.
    public func want(_ count: Int) {
        wanted = max(wanted, count)
        guard state == .ready, !pages.loading, !pages.stopped, pages.problem == nil,
              pages.more, pages.titles.count < wanted else { return }
        let read = reads
        Task { await fill(to: wanted, pages: fillPages, read: read) }
    }

    /// Apple TV's Show more, or Retry, at the grid's end: the reading goes
    /// on past where it stopped short or failed.
    public func showMore() {
        guard !pages.loading, pages.more else { return }
        let read = reads
        pages.stopped = false
        pages.problem = nil
        stopped = false
        stale = nil
        Task { await fill(to: max(wanted, pages.titles.count + 1), pages: fillPages, read: read) }
    }

    /// Tries again: the summary, and the search from its first page.
    public func retry() {
        app.requests.refresh()
        Task { await load() }
    }

    private func page(_ number: Int) async throws -> DiscoverPage {
        switch source {
        case .words: try await app.requestsAPI().discoverSearch(query, page: number)
        case .list(let list): try await app.requestsAPI().discover(list, page: number)
        }
    }

    /// Reads the pages after those read while fewer than `want` titles
    /// came, `pages` of them at most; short of `want` with more to read,
    /// it stops there.
    private func fill(to want: Int, pages most: Int, read: Int) async {
        var count = 0
        while pages.more && pages.titles.count < want {
            if count == most {
                pages.stopped = true
                break
            }
            pages.loading = true
            loadingMore = pages.page > 0
            do {
                let answer = try await page(pages.page + 1)
                guard read == reads else { return }
                pages = pages.with(answer)
                count += 1
                show()
            } catch {
                guard read == reads else { return }
                pages.loading = false
                loadingMore = false
                let problem = RequestProblem.of(error)
                let words = problem.words(manage: false, summary: app.requests.summary, failure: failure)
                pages.problem = problem
                // Apple TV's grid that read pages already keeps its place:
                // Retry beside why goes on from the page that failed.
                if pages.titles.isEmpty && !(tv && pages.page > 0) {
                    state = .failed(words)
                } else {
                    stale = words
                    stopped = true
                    state = .ready
                }
                return
            }
        }
        pages.loading = false
        loadingMore = false
        stopped = pages.stopped
        show(settled: true)
    }

    /// The cards as read; `settled`, the reading stopped, and nothing found is said so.
    private func show(settled: Bool = false) {
        cards = pages.titles.map(DiscoverCard.init)
        if pages.titles.isEmpty && (!pages.more || settled) && !(tv && pages.stopped) {
            state = .empty
        } else if !pages.titles.isEmpty || !pages.more || (tv && pages.stopped) {
            stale = nil
            state = .ready
        }
    }

    /// Badges in the results already shown follow the change.
    private func patch(_ change: RequestChange) async {
        guard let patch = await app.patchFor(change, shown: pages.titles) else { return }
        pages.titles = pages.titles.map(patch)
        cards = pages.titles.map(DiscoverCard.init)
    }
}
