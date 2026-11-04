import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// Search's See all (Android's `DiscoverResultsViewModel`): every title
/// TMDB finds for the words, with its badge, those in a library too. It
/// reads page by page until a screenful, `fill` pages at most, then a page
/// at a time as the grid scrolls near its end. Badges follow the changes
/// to requests without reading the results again.
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
    /// The grid this near its end reads the next page.
    public static let near = 9

    @Published public private(set) var state: State = .loading
    @Published public private(set) var cards: [DiscoverCard] = []
    @Published public private(set) var loadingMore = false
    /// Reading on failed: the titles before show, with why.
    @Published public private(set) var stale: ProblemWords?

    /// The words searched for, as the server takes them.
    public let query: String
    /// "Results for “q”".
    public var heading: String { RequestWords.results(query) }
    public var emptyTitle: String { RequestWords.searchNone(query) }
    public let emptyBody = RequestWords.searchNoneBody

    private let app: AppModel
    private var pages = TitlePages()
    private var listeners: Set<AnyCancellable> = []
    private var reads = 0

    public init(app: AppModel, query: String) {
        self.app = app
        self.query = PleyAPI.searchText(query)
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
        guard !query.isEmpty else {
            state = .empty
            return
        }
        reads += 1
        let read = reads
        pages = TitlePages()
        state = .loading
        await fill(to: Self.screenful, pages: Self.fill, read: read)
    }

    /// The grid shows the card at `index`: near its end, the next page.
    public func cardAppeared(_ index: Int) {
        guard index >= cards.count - Self.near, pages.more, !pages.loading, state == .ready else { return }
        let read = reads
        pages.stopped = false
        pages.problem = nil
        Task { await fill(to: pages.titles.count + 1, pages: 1, read: read) }
    }

    /// Tries again: the summary, and the search from its first page.
    public func retry() {
        app.requests.refresh()
        Task { await load() }
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
                let answer = try await app.requestsAPI().discoverSearch(query, page: pages.page + 1)
                guard read == reads else { return }
                pages = pages.with(answer)
                count += 1
                show()
            } catch {
                guard read == reads else { return }
                pages.loading = false
                loadingMore = false
                let problem = RequestProblem.of(error)
                let words = problem.words(manage: false, summary: app.requests.summary,
                                          failure: RequestWords.searchFailed)
                pages.problem = problem
                if pages.titles.isEmpty { state = .failed(words) } else { stale = words }
                return
            }
        }
        pages.loading = false
        loadingMore = false
        show(settled: true)
    }

    /// The cards as read; `settled`, the reading stopped, and nothing found is said so.
    private func show(settled: Bool = false) {
        cards = pages.titles.map(DiscoverCard.init)
        if pages.titles.isEmpty && (!pages.more || settled) {
            state = .empty
        } else if !pages.titles.isEmpty || !pages.more {
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
