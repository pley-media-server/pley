import Combine
import Foundation
import PleyModel
import PleyNetwork

/// Live TV through the open server (docs/API.md, "Live TV"), shared by
/// its pages, Search and the live player: whether it shows, its groups,
/// a category's channels a page at a time, the guide of the channels in
/// view, favourites, and watching a channel. A provider lists tens of
/// thousands of channels: nothing here asks for them all.
///
/// A favourite shows at once on every page and in the player
/// (`marked`), and is put back when the server refuses it. `version` goes
/// up when what the lists show may have changed (what's on moved on, the
/// lineup changed, a favourite was saved), and the screens ask again.
@MainActor
public final class LiveTvRepository: ObservableObject {
    /// Channels asked for at once, as the web asks.
    public nonisolated static let page = 100
    /// The guide is fetched in chunks of six hours, a page of channels at a time.
    public nonisolated static let guideChunk: Int64 = 6 * 3600

    /// Favourites set on this device, by channel: shown at once, before
    /// the server's own lists say so.
    @Published public private(set) var favorites: [Int: Bool] = [:]
    /// Goes up whenever the lists may have changed: their pages are asked again.
    @Published public private(set) var version = 0

    private let api: @Sendable () async throws -> PleyAPI
    /// Saves the lists' answers, and gives them back while the server is out of reach (the app's).
    public weak var cache: (any SnapshotCache)?
    /// Favourites on their way to the server: the counts it gives don't have them yet.
    private var unsaved: Set<Int> = []

    /// Live TV on the server `api` gives (the open one).
    public init(api: @escaping @Sendable () async throws -> PleyAPI) {
        self.api = api
    }

    /// Live TV on one server's API.
    public convenience init(api: PleyAPI) {
        self.init(api: { api })
    }

    public func summary() async throws -> LiveSummary {
        try await load(SnapshotKeys.live) { try await $0.liveSummary() }
    }

    /// The groups and the counts, the favourites' as set here.
    public func groups() async throws -> LiveGroups {
        var groups = try await load(SnapshotKeys.liveGroups) { try await $0.liveGroups() }
        let pending = unsaved.reduce(0) { sum, id in sum + (favorites[id] == true ? 1 : -1) }
        groups.favorites = max(groups.favorites + pending, 0)
        return groups
    }

    /// Page `page` (`LiveTvRepository.page` channels) of `category`'s
    /// channels matching `query`, as the server has them (`marked` shows
    /// the favourites set here).
    public func page(_ category: LiveCategory, query: String = "", page: Int) async throws -> LiveChannelPage {
        try await load(SnapshotKeys.livePage(category.key, query, page)) {
            try await $0.liveBrowse(category, query: query, offset: page * Self.page, limit: Self.page)
        }
    }

    /// The page of `category` that holds `channelId`: where a list opens
    /// again, and the channels either side of it.
    public func around(_ category: LiveCategory, channelId: Int) async throws -> LiveChannelPage {
        try await load(SnapshotKeys.liveAround(category.key, channelId)) {
            try await $0.liveBrowse(category, limit: Self.page, around: channelId)
        }
    }

    /// One channel, with what's on now and next, as the server has it.
    public func channel(_ id: Int) async throws -> LiveChannel {
        try await load(SnapshotKeys.liveChannel(id)) { try await $0.liveChannel(id) }
    }

    /// The guide of the channels `ids` for `guideChunk` from `from` (a multiple of it, unix seconds).
    public func guide(from: Int64, ids: [Int]) async throws -> LiveGuide {
        try await api().liveGuide(from: from, to: from + Self.guideChunk, ids: ids)
    }

    /// The channel numbered `number` exactly, of every channel; `nil` when none is.
    public func numbered(_ number: String) async throws -> LiveChannel? {
        try await api().liveBrowse(.all, query: number, limit: Self.page).channels.first { $0.number == number }
    }

    /// `fetch`'s answer, through the cache when there is one.
    private func load<T: Codable & Sendable>(_ key: String,
                                             _ fetch: @escaping @Sendable (PleyAPI) async throws -> T) async throws -> T {
        if let cache { return try await cache.cached(key, fetch) }
        return try await fetch(api())
    }

    /// `channel` with its favourite as set on this device.
    public func marked(_ channel: LiveChannel) -> LiveChannel {
        guard let favorite = favorites[channel.id], favorite != channel.favorite else { return channel }
        var channel = channel
        channel.favorite = favorite
        return channel
    }

    /// What's on moved on, or the lineup changed: the lists ask again.
    /// Favourites set here are the server's by now.
    public func refresh() {
        favorites = favorites.filter { unsaved.contains($0.key) }
        version += 1
    }

    /// A favourite channel or not: at once on screen, put back if the server refuses.
    public func setFavorite(_ channelId: Int, _ favorite: Bool) async throws {
        let before = favorites[channelId]
        favorites[channelId] = favorite
        unsaved.insert(channelId)
        defer { unsaved.remove(channelId) }
        do {
            try await api().setFavoriteChannel(channelId, favorite: favorite)
        } catch {
            favorites[channelId] = before
            throw error
        }
        // Favourites' own pages and counts come from the server.
        version += 1
    }

    /// Starts watching `channelId` on this device, `clientId` ending what
    /// it watched before: the session and the address its player opens.
    /// Throws the server's refusal (`PleyError.busy`: every connection of
    /// its source in use). `takeOver`: the channel whose connection to
    /// take, one the `409` said may be.
    public func watch(_ channelId: Int, clientId: String, takeOver: Int? = nil, controls: [Control]? = nil,
                      deviceName: String? = nil) async throws -> (LiveWatch, URL) {
        let api = try await api()
        let request = LiveWatchRequest(clientId: clientId, takeOver: takeOver, controls: controls,
                                       deviceName: deviceName)
        let watch = try await api.watchChannel(channelId, request)
        let url = try await api.liveMediaURL(watch.url)
        return (watch, url)
    }

    /// What `sessionId`'s channel is made into; `nil` when not known in time.
    public func plan(_ sessionId: String) async throws -> LivePlan? {
        try await api().livePlan(sessionId)
    }

    /// Stops watching `sessionId`, in the background: it outlives the screen that asks.
    public func leave(_ sessionId: String) {
        let api = api
        Task.detached {
            // Out of reach: the server lets the session go by itself after 45 s.
            try? await api().leaveChannel(sessionId)
        }
    }
}
