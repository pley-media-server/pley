import Combine
import Foundation
import PleyData
import PleyModel

/// A category's channels as far as they are loaded: `total` in the list
/// (`nil` until its first page is in), and the pages in, by number.
public struct ChannelPages: Equatable, Sendable {
    public var total: Int?
    public var pages: [Int: [LiveChannel]]
    /// The first page failed, with nothing to show: why.
    public var error: String?

    public init(total: Int? = nil, pages: [Int: [LiveChannel]] = [:], error: String? = nil) {
        self.total = total
        self.pages = pages
        self.error = error
    }

    /// The channel at `index`, once its page is in.
    public subscript(index: Int) -> LiveChannel? {
        guard let page = pages[index / LiveTvRepository.page] else { return nil }
        let at = index % LiveTvRepository.page
        return page.indices.contains(at) ? page[at] : nil
    }

    /// Every channel loaded, in order.
    public var loaded: [LiveChannel] { pages.keys.sorted().flatMap { pages[$0] ?? [] } }

    /// The ids of each loaded page's channels, for the guide.
    public var idPages: [[Int]] { pages.keys.sorted().compactMap { pages[$0].map { $0.map(\.id) } }.filter { !$0.isEmpty } }
}

/// A category's channels a page at a time, as a list scrolls
/// (`showingRows`): its first page (which says how many there are) and the
/// pages covering the rows in view, so a lineup of 50,000 channels costs
/// what is on screen. At most `maxPages` are kept (1,000 channels); the
/// pages furthest from the rows in view go first.
@MainActor
public final class ChannelPaging: ObservableObject {
    /// Pages kept at most: older iPads have 2 GB.
    public static let maxPages = 10
    /// Rows asked for either side of what shows.
    public static let margin = 20

    @Published public private(set) var pages = ChannelPages()

    private let live: LiveTvRepository
    private let maxPages: Int
    private var category: LiveCategory?
    private var query = ""
    private var rows = 0...0
    private var asking: [Int: Task<Void, Never>] = [:]
    /// Goes up with each new list: an answer for an older one is dropped.
    private var generation = 0
    private var watches: Set<AnyCancellable> = []
    /// The pages as the server gave them, before the favourites set here.
    private var raw: [Int: [LiveChannel]] = [:]

    public init(live: LiveTvRepository, maxPages: Int = ChannelPaging.maxPages) {
        self.live = live
        self.maxPages = maxPages
        // A favourite set shows at once; a change at the server asks for the pages in view again.
        live.$favorites.dropFirst().sink { [weak self] _ in
            Task { @MainActor in self?.remark() }
        }.store(in: &watches)
        live.$version.dropFirst().removeDuplicates().sink { [weak self] _ in
            Task { @MainActor in self?.reload() }
        }.store(in: &watches)
    }

    /// The category whose channels to list, searched for `query`: its pages start again unless it is the same.
    public func show(_ category: LiveCategory, query: String = "") {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard category != self.category || q != self.query else { return }
        self.category = category
        self.query = q
        rows = 0...0
        generation += 1
        for task in asking.values { task.cancel() }
        asking = [:]
        pages = ChannelPages()
        raw = [:]
        load(wanted())
    }

    /// Rows `first` to `last` are in view (a few either side are asked for with them). Cheap: call it as the list scrolls.
    public func showingRows(first: Int, last: Int) {
        let range = max(first, 0)...max(last, first, 0)
        guard range != rows else { return }
        rows = range
        load(wanted())
    }

    /// Asks again for the pages in view (what's on moved on, a favourite was saved), keeping what shows meanwhile.
    public func reload() {
        guard category != nil else { return }
        generation += 1
        for task in asking.values { task.cancel() }
        asking = [:]
        let keep = Set(wanted())
        pages.pages = pages.pages.filter { keep.contains($0.key) }
        raw = raw.filter { keep.contains($0.key) }
        load(Array(keep).sorted(), again: true)
    }

    /// The pages the rows in view need, and the first.
    func wanted() -> [Int] {
        let size = LiveTvRepository.page
        let from = max(rows.lowerBound - Self.margin, 0) / size
        var to = (rows.upperBound + Self.margin) / size
        if let total = pages.total { to = min(to, max((total - 1) / size, 0)) }
        return Array(Set([0] + Array(from...max(from, to)))).sorted()
    }

    private func load(_ wanted: [Int], again: Bool = false) {
        guard let category else { return }
        let query = query
        let generation = generation
        for page in wanted where (again || pages.pages[page] == nil) && asking[page] == nil {
            asking[page] = Task { [weak self, live] in
                do {
                    let answer = try await live.page(category, query: query, page: page)
                    guard let self, !Task.isCancelled, self.generation == generation else { return }
                    self.asking[page] = nil
                    self.pages.total = answer.total
                    self.pages.error = nil
                    self.raw[page] = answer.channels
                    self.pages.pages[page] = answer.channels.map(live.marked)
                    self.trim()
                } catch {
                    guard let self, !Task.isCancelled, self.generation == generation else { return }
                    self.asking[page] = nil
                    // A page past the end, or one that failed, shows nothing; only the first's failure is said.
                    if page == 0 && self.pages.total == nil { self.pages.error = describe(error) }
                }
            }
        }
    }

    /// At most `maxPages`: those furthest from the rows in view go.
    private func trim() {
        guard pages.pages.count > maxPages else { return }
        let center = (rows.lowerBound + rows.upperBound) / 2 / LiveTvRepository.page
        let keep = pages.pages.keys.sorted { abs($0 - center) < abs($1 - center) }.prefix(maxPages)
        pages.pages = pages.pages.filter { keep.contains($0.key) }
        raw = raw.filter { keep.contains($0.key) }
    }

    /// The pages as the server gave them, marked again with the favourites set here.
    private func remark() {
        pages.pages = raw.filter { pages.pages[$0.key] != nil }.mapValues { $0.map(live.marked) }
    }
}
