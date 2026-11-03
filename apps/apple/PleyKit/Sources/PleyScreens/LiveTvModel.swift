import Combine
import Foundation
import PleyData
import PleyModel

/// Live TV's page (Android's `LiveTvViewModel`), shared by every device:
/// the categories (Favourites, Recently watched, All channels, the
/// regions and the other groups), the category's channels a page at a
/// time (`paging`), the guide of the pages loaded, the search, and the
/// view (guide or channels). The category, the view and the channel
/// watched last are kept per server and profile, and the list opens again
/// on that channel (`landing`).
@MainActor
public final class LiveTvModel: ObservableObject {
    /// What the page shows in place of channels.
    public enum Status: Equatable, Sendable {
        case loading
        case ready
        /// Not open to this profile: what to say.
        case notAllowed
        /// No channels at all yet.
        case noChannels
        case failed(String)
    }

    @Published public private(set) var status: Status = .loading
    @Published public private(set) var groups = LiveGroups()
    @Published public private(set) var category: LiveCategory
    @Published public private(set) var view: LiveViewKind
    @Published public private(set) var query = ""
    /// The guide's programmes of the channels loaded, by channel.
    @Published public private(set) var programs: [Int: [LiveProgram]] = [:]
    /// The time now (unix seconds), moved every `LiveTvRules.pageTick`.
    @Published public private(set) var now: Int64
    /// Where the guide starts: the half hour before the one it is in.
    @Published public private(set) var guideStart: Int64
    /// Apple TV's guide shows two hours from here, moved half an hour at a time.
    @Published public private(set) var window: Int64
    /// The row the list opens on (the channel watched last), once known; the list scrolls to it once.
    @Published public private(set) var landing: Int?

    public let live: LiveTvRepository
    public let paging: ChannelPaging

    private let settings: AppSettings
    private let origin: String
    private let profileId: Int
    private let clock: @Sendable () -> Int64
    private let toast: (String) -> Void
    private var guides: [String: LiveGuide] = [:]
    private var asking: Set<String> = []
    /// The guide's times in view.
    private var shownTimes: (from: Int64, to: Int64)
    private var ticking: Task<Void, Never>?
    private var lastRefresh: Int64 = 0
    private var watches: Set<AnyCancellable> = []
    private var started = false

    /// Apple TV's guide window.
    public static let windowSpan: Int64 = 2 * 3600

    public init(live: LiveTvRepository, settings: AppSettings, origin: String, profileId: Int,
                clock: @escaping @Sendable () -> Int64 = { Int64(Date().timeIntervalSince1970) },
                toast: @escaping (String) -> Void = { _ in }) {
        self.live = live
        self.settings = settings
        self.origin = origin
        self.profileId = profileId
        self.clock = clock
        self.toast = toast
        paging = ChannelPaging(live: live)
        let kept = settings.liveView(origin: origin, profileId: profileId)
        category = LiveCategory.parse(kept.category ?? LiveCategory.all.key)
        view = kept.view.flatMap(LiveViewKind.init(rawValue:)) ?? .guide
        let now = clock()
        self.now = now
        let start = LiveTvRules.guideStart(now)
        guideStart = start
        window = start
        shownTimes = (start, start + Self.windowSpan)
        paging.$pages.sink { [weak self] pages in
            Task { @MainActor in self?.pagesChanged(pages) }
        }.store(in: &watches)
        live.$version.dropFirst().removeDuplicates().sink { [weak self] _ in
            Task { @MainActor in self?.forgetGuide() }
        }.store(in: &watches)
    }

    /// The channel watched last, as kept.
    public var keptChannel: Int? { settings.liveView(origin: origin, profileId: profileId).channel }

    /// Reads the groups and opens the category kept, on the channel watched last.
    public func start() {
        guard !started else { return }
        started = true
        reload()
        startTicking()
    }

    /// Everything read again (pull to refresh, Try again, the lineup changed).
    public func reload() {
        Task { [weak self] in
            guard let self else { return }
            do {
                let summary = try await self.live.summary()
                guard summary.allowed else {
                    self.status = .notAllowed
                    return
                }
                guard summary.available || summary.channels > 0 else {
                    self.status = .noChannels
                    return
                }
                self.groups = try await self.live.groups()
                // A group gone, or Favourites emptied: All channels.
                if !self.has(self.category) { self.category = .all }
                self.status = .ready
                await self.land()
                self.paging.show(self.category, query: self.query)
                self.live.refresh()
            } catch {
                if self.status != .ready { self.status = .failed(describe(error)) }
            }
        }
    }

    /// Whether the categories list `category`.
    func has(_ category: LiveCategory) -> Bool {
        switch category {
        case .all, .recent, .favorites: true
        case .group(let name): groups.groups.contains { $0.name == name }
        }
    }

    /// Where the kept channel is in the category: the row the list opens on.
    private func land() async {
        guard query.isEmpty, let channel = keptChannel,
              let page = try? await live.around(category, channelId: channel),
              let index = page.channels.firstIndex(where: { $0.id == channel })
        else {
            landing = nil
            return
        }
        landing = page.offset + index
    }

    /// The list scrolled to its landing row: it isn't asked for again.
    public func landed() { landing = nil }

    /// Shows `category`'s channels, and keeps it.
    public func show(_ category: LiveCategory) {
        guard category != self.category else { return }
        self.category = category
        settings.keepLiveView(origin: origin, profileId: profileId, category: category.key)
        landing = nil
        forgetGuide()
        paging.show(category, query: query)
    }

    /// The guide or the channels, kept.
    public func show(_ view: LiveViewKind) {
        self.view = view
        settings.keepLiveView(origin: origin, profileId: profileId, view: view.rawValue)
        if view == .guide { pagesChanged(paging.pages) }
    }

    /// Searches the category for `query` (a channel's name or number, or what's on).
    public func search(_ query: String) {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard q != self.query else { return }
        self.query = q
        forgetGuide()
        paging.show(category, query: q)
    }

    /// The groups whose names have the search in them, as quick links.
    public var foundGroups: [LiveGroup] { LiveTvRules.foundGroups(groups.groups, query: query) }

    /// The categories list's groups, by region.
    public var sections: GroupSections { LiveTvRules.groupSections(groups.groups) }

    /// The category's name, for its title.
    public var title: String { LiveTvRules.categoryName(category, groups: groups.groups) }

    /// What an empty list says: a title and a line.
    public var empty: (title: String, detail: String) {
        if !query.isEmpty {
            return ("No channels match", "No channel's name, number or programme on now has “\(query)” in it.")
        }
        switch category {
        case .favorites: return ("No favourites yet", "Star a channel to find it here.")
        case .recent: return ("Nothing watched yet", "The channels you watch show here, the last first.")
        default: return ("No channels", "No channels in this group.")
        }
    }

    /// A channel opened: kept as the one the list opens on next time.
    public func watched(_ channelId: Int) {
        settings.keepLiveView(origin: origin, profileId: profileId, channel: channelId)
    }

    /// Stars `channel` or takes its star off: at once, put back with a toast if refused.
    public func toggleFavorite(_ channel: LiveChannel) {
        let favorite = !channel.favorite
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.live.setFavorite(channel.id, favorite)
                if let groups = try? await self.live.groups() { self.groups = groups }
            } catch {
                self.toast("The favourite wasn't saved: \(describe(error))")
            }
        }
    }

    // MARK: The guide

    /// The guide shows `from` to `to` (unix seconds): their chunks are asked for.
    public func showingTimes(from: Int64, to: Int64) {
        shownTimes = (from, to)
        loadGuide()
    }

    /// Apple TV: the guide's window moves by `slots` half hours, within the guide.
    public func moveWindow(by slots: Int) {
        let latest = guideStart + LiveTvRules.guideSpan - Self.windowSpan
        let moved = min(max(window + Int64(slots) * LiveTvRules.slot, guideStart), latest)
        guard moved != window else { return }
        window = moved
        showingTimes(from: moved, to: moved + Self.windowSpan)
    }

    /// The programmes of `channel` the guide shows between `from` and `to`.
    public func programs(of channel: LiveChannel, from: Int64, to: Int64) -> [LiveProgram] {
        var list = LiveTvRules.between(programs[channel.id], from: from, to: to)
        // Before its guide is in, what the list said is on.
        if list.isEmpty, let on = channel.now, on.stop > from, on.start < to { list = [on] }
        return list
    }

    private func pagesChanged(_ pages: ChannelPages) {
        guard view == .guide else { return }
        loadGuide(pages)
    }

    private func loadGuide(_ pages: ChannelPages? = nil) {
        let pages = pages ?? paging.pages
        let chunks = LiveTvRules.chunks(start: guideStart, from: shownTimes.from, to: shownTimes.to)
        for (number, page) in pages.pages {
            let ids = page.filter(\.hasGuide).map(\.id)
            guard !ids.isEmpty else { continue }
            for chunk in chunks {
                let key = "\(number):\(chunk):\(ids.first ?? 0)"
                guard guides[key] == nil, !asking.contains(key) else { continue }
                asking.insert(key)
                Task { [weak self] in
                    guard let self else { return }
                    let guide = try? await self.live.guide(from: chunk, ids: ids)
                    guard self.asking.remove(key) != nil, let guide else { return }
                    self.guides[key] = guide
                    self.programs = LiveTvRules.merge(Array(self.guides.values))
                }
            }
        }
    }

    private func forgetGuide() {
        guides = [:]
        asking = []
        programs = [:]
        loadGuide()
    }

    // MARK: Time

    /// The page left (`false`) or came back: the clock stops while it's away.
    public func visible(_ shown: Bool) {
        if shown {
            startTicking()
        } else {
            ticking?.cancel()
        }
    }

    private func startTicking() {
        ticking?.cancel()
        ticking = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: LiveTvRules.pageTick)
                guard let self, !Task.isCancelled else { return }
                self.tick()
            }
        }
    }

    /// The time moves on: the now line and the bars; what's on is asked
    /// for again once a programme on ended, at most every `refreshFloor`.
    func tick() {
        now = clock()
        let start = LiveTvRules.guideStart(now)
        if start != guideStart {
            guideStart = start
            if window < start { window = start }
        }
        let ended = paging.pages.loaded.contains { $0.now.map { $0.stop <= now } ?? false }
        if ended, now - lastRefresh >= Int64(LiveTvRules.refreshFloor.components.seconds) {
            lastRefresh = now
            live.refresh()
        }
    }

    deinit {
        ticking?.cancel()
    }
}
