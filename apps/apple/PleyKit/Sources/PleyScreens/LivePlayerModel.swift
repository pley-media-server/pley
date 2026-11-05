import Combine
import Foundation
import PleyData
import PleyModel
import PleyPlayer

/// A live player to open: the channel, and the category it was opened in
/// (channel up and down move through it). Codable, so the Mac opens it in
/// a window of its own.
public struct LiveRequest: Hashable, Codable, Sendable {
    public var channelId: Int
    /// The category's key (`LiveCategory.key`).
    public var category: String

    public init(channelId: Int, category: LiveCategory) {
        self.channelId = channelId
        self.category = category.key
    }
}

/// The live player's screen (Android's `LivePlayerViewModel`), shared by
/// every device: the channel playing with what's on now and next, channel
/// up and down through the category it was opened in (wrapping round its
/// ends), a channel number typed on a remote or keyboard, the favourite,
/// and the trouble a channel can run into. `playback` does the watching.
/// The profile's remotes change channel the same way (`obey`).
@MainActor
public final class LivePlayerModel: ObservableObject, Identifiable {
    /// The remote's commands the live player obeys: channel up and down. It
    /// doesn't go back in a channel, so no pause or seek.
    public static let controls: [Control] = [.next, .previous]

    /// A typed number tunes this long after its last digit.
    public static let numberWait: Duration = .milliseconds(1_400)
    /// "No channel 123" shows this long.
    public static let notFoundShown: Duration = .seconds(2)
    /// What a cast from another device says shows this long.
    public static let bannerShown: Duration = .seconds(4)
    /// A channel number has this many digits at most.
    public static let maxDigits = 5

    public nonisolated let id = UUID()
    public let playback: LivePlayback
    public let live: LiveTvRepository

    /// The channel playing, with what's on now and next once known.
    @Published public private(set) var channel: LiveChannel?
    /// The category channel up and down go through; the Channels list can change it.
    @Published public private(set) var category: LiveCategory
    /// The digits typed so far, shown as they come.
    @Published public private(set) var digits = ""
    /// "No channel 123", for a moment.
    @Published public private(set) var notice: String?
    /// "Playing from Kitchen tablet", as a cast from another device starts, for a moment.
    @Published public private(set) var banner: String?
    /// The time now (unix seconds), moved every `LiveTvRules.playerTick`.
    @Published public private(set) var now: Int64

    private let clock: @Sendable () -> Int64
    private let numberWait: Duration
    private let toast: (String) -> Void
    /// Recently watched as it was at opening: watching reorders it, and channel up mustn't follow.
    private var recentOrder: [Int]?
    private var typing: Task<Void, Never>?
    private var noticeTask: Task<Void, Never>?
    private var bannerTask: Task<Void, Never>?
    private var ticking: Task<Void, Never>?
    private var zapping: Task<Void, Never>?
    private var lastRefresh: Int64 = 0
    private var watches: Set<AnyCancellable> = []

    public init(request: LiveRequest, live: LiveTvRepository, playback: LivePlayback, seen: LiveChannel? = nil,
                numberWait: Duration = LivePlayerModel.numberWait,
                clock: @escaping @Sendable () -> Int64 = { Int64(Date().timeIntervalSince1970) },
                toast: @escaping (String) -> Void = { _ in }) {
        self.live = live
        self.playback = playback
        self.category = LiveCategory.parse(request.category)
        self.channel = seen?.id == request.channelId ? seen : nil
        self.numberWait = numberWait
        self.clock = clock
        self.toast = toast
        now = clock()
        // The player's own changes show here (its phase, the picture).
        playback.objectWillChange.sink { [weak self] _ in self?.objectWillChange.send() }.store(in: &watches)
        live.$favorites.dropFirst().sink { [weak self] _ in
            Task { @MainActor in self?.remark() }
        }.store(in: &watches)
        start(request.channelId)
    }

    /// The channel tuned (or being tuned).
    public var channelId: Int? { playback.state.channelId }
    public var state: LiveState { playback.state }

    // MARK: Channels

    /// Tunes `channel` (from the Channels list, or a number).
    public func tune(_ channel: LiveChannel) {
        self.channel = live.marked(channel)
        playback.tune(channel.id)
        lastRefresh = 0
        refresh(channel.id)
    }

    /// Channel up (`by` 1) or down (-1) in the category, wrapping round its ends.
    public func zap(by: Int) {
        guard let current = channelId else { return }
        zapping?.cancel()
        zapping = Task { [weak self] in
            guard let self else { return }
            guard let next = try? await self.neighbour(of: current, by: by), !Task.isCancelled,
                  self.channelId == current
            else { return }
            self.tune(next)
        }
    }

    /// The channel `by` away from `current` in the category: across its
    /// pages, round from the last to the first. Recently watched goes in
    /// the order it had when the player opened.
    func neighbour(of current: Int, by: Int) async throws -> LiveChannel? {
        if category == .recent {
            if recentOrder == nil {
                recentOrder = try await live.page(.recent, page: 0).channels.map(\.id)
            }
            let order = recentOrder ?? []
            guard !order.isEmpty else { return nil }
            let index = order.firstIndex(of: current) ?? (by > 0 ? -1 : 0)
            let id = order[((index + by) % order.count + order.count) % order.count]
            return id == current ? nil : try await live.channel(id)
        }
        let page = try await live.around(category, channelId: current)
        guard !page.channels.isEmpty else { return nil }
        let index = page.channels.firstIndex { $0.id == current }
        if let index, page.channels.indices.contains(index + by) { return page.channels[index + by] }
        let size = LiveTvRepository.page
        let pages = max((page.total + size - 1) / size, 1)
        if pages == 1 {
            // Round the ends of a category of one page.
            guard let index else { return by > 0 ? page.channels.first : page.channels.last }
            let wrapped = page.channels[((index + by) % page.channels.count + page.channels.count) % page.channels.count]
            return wrapped.id == current ? nil : wrapped
        }
        let at = ((page.offset / size + (by > 0 ? 1 : -1)) % pages + pages) % pages
        let next = try await live.page(category, page: at).channels
        return by > 0 ? next.first : next.last
    }

    /// A command from the profile's remote on another device, for session
    /// `sessionId` (`player_command`): next and previous are channel up and
    /// down, as here, when it is the session watching now. The new
    /// channel's watch tells the server, and so the remote.
    public func obey(_ command: PlayerCommand, for sessionId: String) {
        guard playback.sessionId == sessionId else { return }
        switch command.command {
        case .next: zap(by: 1)
        case .previous: zap(by: -1)
        default: break
        }
    }

    /// The Channels list moved to another category: channel up and down follow it.
    public func show(_ category: LiveCategory) {
        guard category != self.category else { return }
        self.category = category
        recentOrder = nil
    }

    // MARK: A number

    /// A digit typed: it tunes `numberWait` after the last, or at `enterNumber`.
    /// `waits` false (Apple TV's on-screen pad, where moving to the next key
    /// takes longer than the wait) tunes only at `enterNumber`.
    public func type(_ digit: Int, waits: Bool = true) {
        guard (0...9).contains(digit), digits.count < Self.maxDigits else { return }
        digits += String(digit)
        typing?.cancel()
        guard waits else { return }
        let wait = numberWait
        typing = Task { [weak self] in
            try? await Task.sleep(for: wait)
            guard !Task.isCancelled else { return }
            self?.enterNumber()
        }
    }

    /// The last digit off (the number pad's ⌫).
    public func deleteDigit() {
        guard !digits.isEmpty else { return }
        digits.removeLast()
        if digits.isEmpty { typing?.cancel() }
    }

    /// Tunes the number typed (OK), or says no channel has it.
    public func enterNumber() {
        typing?.cancel()
        let number = digits
        digits = ""
        guard !number.isEmpty else { return }
        Task { [weak self] in
            guard let self else { return }
            let found = try? await self.live.numbered(number)
            if let found {
                // Typed on purpose: it plays in All channels, where every number is.
                if self.category != .all, await !self.inCategory(found.id) { self.show(.all) }
                self.tune(found)
            } else {
                self.say("No channel \(number)")
            }
        }
    }

    private func inCategory(_ id: Int) async -> Bool {
        let page = try? await live.around(category, channelId: id)
        return page?.channels.contains { $0.id == id } == true
    }

    private func say(_ text: String) {
        notice = text
        noticeTask?.cancel()
        noticeTask = Task { [weak self] in
            try? await Task.sleep(for: LivePlayerModel.notFoundShown)
            guard !Task.isCancelled else { return }
            self?.notice = nil
        }
    }

    /// Shows `text` over the player for a moment (`bannerShown`).
    public func showBanner(_ text: String) {
        banner = text
        bannerTask?.cancel()
        bannerTask = Task { [weak self] in
            try? await Task.sleep(for: LivePlayerModel.bannerShown)
            guard !Task.isCancelled else { return }
            self?.banner = nil
        }
    }

    // MARK: Favourite

    /// Stars the channel playing, or takes its star off: at once, put back with a toast if refused.
    public func toggleFavorite() {
        guard let channel else { return }
        let favorite = !channel.favorite
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.live.setFavorite(channel.id, favorite)
            } catch {
                self.toast("The favourite wasn't saved: \(describe(error))")
            }
        }
        remark()
    }

    private func remark() {
        if let channel { self.channel = live.marked(channel) }
    }

    // MARK: Trouble

    /// Tunes the channel again (Try again).
    public func retry() { playback.retry() }

    /// Watch here instead: takes the connection watching `channelId`.
    public func takeOver(_ channelId: Int) { playback.retry(takeOver: channelId) }

    // MARK: Life

    /// The app left the foreground (`false`): the channel and its
    /// connection are let go, and tuned again from the live edge on return,
    /// unless it had failed (stopped at the server, every connection in use).
    public func sceneActive(_ active: Bool) {
        if active {
            playback.resume()
            startTicking()
        } else {
            ticking?.cancel()
            playback.stop()
        }
    }

    /// Leaves: the session ends at the server, the player lets go.
    public func close() {
        typing?.cancel()
        noticeTask?.cancel()
        bannerTask?.cancel()
        ticking?.cancel()
        zapping?.cancel()
        playback.close()
    }

    // MARK: Inside

    private func start(_ channelId: Int) {
        playback.tune(channelId)
        refresh(channelId)
        startTicking()
    }

    /// What's on, every `playerTick`; asked for again once the programme
    /// on ended, at most every `refreshFloor`.
    private func startTicking() {
        ticking?.cancel()
        ticking = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: LiveTvRules.playerTick)
                guard let self, !Task.isCancelled else { return }
                self.now = self.clock()
                if let channel = self.channel, let on = channel.now, on.stop <= self.now,
                   self.now - self.lastRefresh >= Int64(LiveTvRules.refreshFloor.components.seconds) {
                    self.refresh(channel.id)
                }
            }
        }
    }

    private func refresh(_ channelId: Int) {
        lastRefresh = clock()
        Task { [weak self] in
            guard let self, let fresh = try? await self.live.channel(channelId), self.channelId == channelId else {
                return
            }
            self.channel = self.live.marked(fresh)
        }
    }
}
