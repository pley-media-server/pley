import Combine
import Foundation
import PleyModel
import PleyNetwork

/// A library's titles in a grid, read a page of 100 at a time as the grid
/// scrolls (for older iPads, where thousands of titles mustn't be held):
///
/// - The grid is as long as the listing's `total`, from the first page;
///   cells of pages not read yet show a placeholder.
/// - A cell that shows (`showing`) reads its page, and the next page once
///   past 70 % of it. Pages being read far from where the grid is are
///   cancelled.
/// - At most 20 pages (2,000 titles) are kept: the farthest go first.
/// - Read again (`reload`), each page kept is asked with its `ETag`, and a
///   `304` keeps it.
@MainActor
public final class LibraryGrid: ObservableObject {
    public static let pageSize = 100
    /// The most pages kept at once.
    public static let maxPages = 20
    /// How far into a page the next one is read.
    public static let ahead = 0.7

    /// What a page is read with: the offset, the limit and the tag of the
    /// page shown; `nil` back means it is unchanged.
    public typealias Fetch = @Sendable (_ offset: Int, _ limit: Int, _ tag: String?) async throws
        -> (page: LibraryItems, tag: String?)?

    /// How many titles the listing has; `nil` before the first page came.
    @Published public private(set) var total: Int?
    /// Why the first page couldn't be read; `nil` while it reads or once it came.
    @Published public private(set) var failure: String?
    /// The pages read, by number.
    @Published public private(set) var pages: [Int: [Tile]] = [:]

    public let shape: RowShape
    private let fetch: Fetch
    private let makeTile: @Sendable (ItemSummary) -> Tile
    private var tags: [Int: String] = [:]
    private var loading: [Int: Task<Void, Never>] = [:]
    /// The page the grid was last at.
    private var current = 0

    public init(shape: RowShape, makeTile: @escaping @Sendable (ItemSummary) -> Tile, fetch: @escaping Fetch) {
        self.shape = shape
        self.makeTile = makeTile
        self.fetch = fetch
    }

    /// The tile at `index`, when its page is read.
    public func tile(at index: Int) -> Tile? {
        let page = pages[index / Self.pageSize]
        let at = index % Self.pageSize
        guard let page, page.indices.contains(at) else { return nil }
        return page[at]
    }

    /// Every title read, in order, when every page is: for the A to Z.
    public var everything: [ItemSummary]? {
        guard let total else { return nil }
        let count = (total + Self.pageSize - 1) / Self.pageSize
        var out: [ItemSummary] = []
        out.reserveCapacity(total)
        for n in 0..<count {
            guard let page = pages[n] else { return nil }
            out += page.map(\.item)
        }
        return out
    }

    /// How many titles are kept now.
    public var kept: Int { pages.values.reduce(0) { $0 + $1.count } }

    /// The first page, and with it the total.
    public func start() async {
        failure = nil
        await load(0)?.value
    }

    /// The cell at `index` shows: its page, and the next one past 70 % of it.
    public func showing(_ index: Int) {
        guard let total, index >= 0, index < total else { return }
        let page = index / Self.pageSize
        current = page
        load(page)
        let into = Double(index % Self.pageSize) / Double(Self.pageSize)
        let last = (total - 1) / Self.pageSize
        if into >= Self.ahead, page < last { load(page + 1) }
        // Pages being read that the grid has left behind.
        for (n, task) in loading where abs(n - page) > 1 {
            task.cancel()
            loading[n] = nil
        }
    }

    /// Reads every page, as the A to Z needs: only for listings of 2,000
    /// titles at most, which the pages kept can hold.
    public func loadAll() async {
        guard let total, total <= Self.pageSize * Self.maxPages else { return }
        let count = (total + Self.pageSize - 1) / Self.pageSize
        for n in 0..<count where pages[n] == nil {
            await load(n, keep: false)?.value
            if Task.isCancelled { return }
        }
    }

    /// Reads the pages near where the grid is again, each with its tag;
    /// the others go, to be read when shown.
    public func reload() async {
        let near = Set([current - 1, current, current + 1]).filter { pages[$0] != nil || $0 == 0 }
        for n in pages.keys where !near.contains(n) {
            pages[n] = nil
            tags[n] = nil
        }
        for n in near.sorted() {
            loading[n]?.cancel()
            loading[n] = nil
            await load(n, force: true)?.value
        }
    }

    /// Reads page `n` unless it is read or being read; `force` reads it
    /// again (with its tag).
    @discardableResult
    private func load(_ n: Int, force: Bool = false, keep: Bool = true) -> Task<Void, Never>? {
        if let task = loading[n] { return task }
        if !force && pages[n] != nil { return nil }
        let tag = pages[n] != nil ? tags[n] : nil
        let fetch = fetch
        let makeTile = makeTile
        let task = Task { [weak self] in
            do {
                let answer = try await fetch(n * Self.pageSize, Self.pageSize, tag)
                guard !Task.isCancelled, let self else { return }
                self.loading[n] = nil
                guard let answer else { return }
                let tiles = answer.page.items.map(makeTile)
                self.total = answer.page.total
                self.pages[n] = tiles
                self.tags[n] = answer.tag
                self.failure = nil
                if keep { self.trim() }
            } catch {
                guard let self else { return }
                if !Task.isCancelled { self.loading[n] = nil }
                if n == 0 && self.pages[0] == nil && !(error is CancellationError) {
                    self.failure = describe(error)
                }
            }
        }
        loading[n] = task
        return task
    }

    /// At most `maxPages` pages, the ones farthest from where the grid is dropped.
    private func trim() {
        guard pages.count > Self.maxPages else { return }
        let far = pages.keys.sorted { abs($0 - current) > abs($1 - current) }
        for n in far.prefix(pages.count - Self.maxPages) {
            pages[n] = nil
            tags[n] = nil
        }
    }
}
