import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer

/// What can be done with an item from its context menu, its long press or
/// its page's More (Android's `ItemAction`, ItemActions.tsx without the
/// administrator's part): playing, watched marks, leaving Continue
/// Watching, My List, playlists, and going to its show or season. Editing,
/// refreshing and analysing are the web's, for administrators: never
/// offered here, whatever the server says of the profile.
public enum ItemAction: String, Hashable, Sendable, CaseIterable {
    case play, resume, fromStart, markWatched, markUnwatched
    /// Opened from a Continue Watching tile: it (an episode: its show) leaves the row.
    case removeFromContinueWatching
    case addToMyList, removeFromMyList, addToPlaylist, goToShow, goToSeason
    /// Artists, albums and tracks: their music page.
    case open

    public var title: String {
        switch self {
        case .play: "Play"
        case .resume: "Resume"
        case .fromStart: "Play from start"
        case .markWatched: "Mark as watched"
        case .markUnwatched: "Mark as unwatched"
        case .removeFromContinueWatching: "Remove from Continue Watching"
        case .addToMyList: "Add to My List"
        case .removeFromMyList: "Remove from My List"
        case .addToPlaylist: "Add to playlist…"
        case .goToShow: "Go to show"
        case .goToSeason: "Go to season"
        case .open: "Open"
        }
    }

    /// Its SF Symbol.
    public var symbol: String {
        switch self {
        case .play, .resume: "play.fill"
        case .fromStart: "arrow.counterclockwise"
        case .markWatched: "checkmark.circle"
        case .markUnwatched: "circle"
        case .removeFromContinueWatching: "minus.circle"
        case .addToMyList: "plus"
        case .removeFromMyList: "minus"
        case .addToPlaylist: "text.badge.plus"
        case .goToShow: "tv"
        case .goToSeason: "square.stack"
        case .open: "music.note"
        }
    }

    /// Its accessibility identifier's last part: `actions.markWatched`.
    public var key: String { rawValue }
}

/// The pure rules of item actions (Android's `itemActions` and `destination`).
public enum ItemActions {
    /// The actions for `item`, in the order they are listed. My List is
    /// offered for movies and shows when it is known whether the item is in
    /// it (`inWatchlist`); leaving Continue Watching when the actions were
    /// opened from that row.
    public static func actions(for item: ItemSummary, inWatchlist: Bool? = nil,
                               continueWatching: Bool = false) -> [ItemAction] {
        var out: [ItemAction] = []
        let kind = item.kind
        let music = kind == .artist || kind == .album || kind == .track
        if music {
            out.append(.open)
        } else if ItemKind.known.contains(kind) {
            if Words.isInProgress(item) {
                out += [.resume, .fromStart]
            } else {
                out.append(.play)
            }
        }
        if [.movie, .episode, .show, .season].contains(kind) {
            out.append(watched(item) ? .markUnwatched : .markWatched)
        }
        if continueWatching { out.append(.removeFromContinueWatching) }
        if let inWatchlist, kind == .movie || kind == .show {
            out.append(inWatchlist ? .removeFromMyList : .addToMyList)
        }
        if kind != .extra && ItemKind.known.contains(kind) { out.append(.addToPlaylist) }
        if kind == .episode && item.grandparentId != nil { out.append(.goToShow) }
        if kind == .episode && item.parentId != nil { out.append(.goToSeason) }
        if kind == .season && item.parentId != nil { out.append(.goToShow) }
        return out
    }

    /// Watched: a video marked so, or every episode of a show or season.
    public static func watched(_ item: ItemSummary) -> Bool {
        switch item.kind {
        case .show, .season:
            let leaves = item.leafCount ?? 0
            return leaves > 0 && (item.viewedLeafCount ?? 0) >= leaves
        default:
            return item.userState.watched
        }
    }

    /// Where `action` on `item` goes, for the actions that open a page; `nil` for the others.
    public static func destination(_ action: ItemAction, _ item: ItemSummary) -> Screen? {
        switch action {
        case .goToShow: (item.kind == .season ? item.parentId : item.grandparentId).map { .item($0) }
        case .goToSeason: item.parentId.map { .item($0) }
        case .open:
            switch item.kind {
            case .artist: .artist(item.id)
            case .album: .album(item.id)
            default: item.parentId.map { .album($0, track: item.id) }
            }
        default: nil
        }
    }
}

/// The item actions of the whole app (one, `AppModel.actions`): it does
/// the marks and keeps what the screens show of them until the server's
/// own answer comes back. A mark shows at once (`shown`), and goes back,
/// with a toast saying why, when the server refuses it. Whether a title is
/// in My List is read with the list itself (`GET /api/watchlist`), so the
/// menu knows it the moment it opens.
@MainActor
public final class ItemActionsModel: ObservableObject {
    /// What an action left for the screen to do.
    public enum Outcome: Equatable {
        /// Done here (or refused, with a toast).
        case done
        /// The screen plays it: the Mac opens a player window, the others the player.
        case play(PlayRequest)
        /// The screen opens this page.
        case open(Screen)
    }

    /// The actions offered.
    public static let offered: Set<ItemAction> = Set(ItemAction.allCases)

    /// Watched marks made here and not yet read back from the server.
    @Published public private(set) var watched: [Int: Bool] = [:]
    /// The ids in My List; `nil` until read.
    @Published public private(set) var listed: Set<Int>?

    private unowned let app: AppModel
    /// Items with a write on its way: a change naming them isn't the server's last word yet.
    private var inFlight: Set<Int> = []
    /// When each item was last read ahead.
    private var prefetched: [Int: Date] = [:]
    /// The time now (the tests set their own).
    var now: () -> Date = Date.init
    /// How often an item is read ahead at most.
    public static let prefetchSeconds: Double = 120

    init(app: AppModel) {
        self.app = app
    }

    /// The actions for `item`, as offered now.
    public func actions(for item: ItemSummary, continueWatching: Bool = false) -> [ItemAction] {
        let inList: Bool? = item.kind == .movie || item.kind == .show ? listed.map { $0.contains(item.id) } : nil
        return ItemActions.actions(for: shown(item), inWatchlist: inList, continueWatching: continueWatching)
            .filter(Self.offered.contains)
    }

    /// `item` with the marks made here.
    public func shown(_ item: ItemSummary) -> ItemSummary {
        guard let mark = watched[item.id] else { return item }
        return Tile(item, shape: .poster).marked(watched: mark).item
    }

    /// `tile` with the marks made here.
    public func shown(_ tile: Tile) -> Tile {
        guard let mark = watched[tile.id], ItemActions.watched(tile.item) != mark else { return tile }
        return tile.marked(watched: mark)
    }

    /// Does `action` on `item`; what is left for the screen to do comes back.
    @discardableResult
    public func perform(_ action: ItemAction, on item: ItemSummary) async -> Outcome {
        switch action {
        case .play, .resume, .fromStart:
            guard let request = await playRequest(item, fromStart: action == .fromStart) else {
                app.shell.show("Nothing to play")
                return .done
            }
            return .play(request)
        case .markWatched, .markUnwatched:
            await setWatched(item, action == .markWatched)
        case .addToMyList, .removeFromMyList:
            await setListed(item, action == .addToMyList)
        case .removeFromContinueWatching:
            _ = await removeFromContinueWatching(item)
        case .goToShow, .goToSeason, .open:
            if let screen = ItemActions.destination(action, item) { return .open(screen) }
        case .addToPlaylist:
            app.shell.sheet = .addToPlaylist([item.id])
        }
        return .done
    }

    /// What Play starts for `item`: itself, or a show's or season's on-deck episode.
    public func playRequest(_ item: ItemSummary, fromStart: Bool = false) async -> PlayRequest? {
        switch item.kind {
        case .show, .season:
            guard let api = await app.session.api, let next = try? await api.onDeck(item.id) else { return nil }
            return PlayRequest(itemId: next.id, fromStart: fromStart)
        case .movie, .episode, .extra:
            return PlayRequest(itemId: item.id, fromStart: fromStart)
        default:
            return nil
        }
    }

    /// Marks `item` watched or not: shown at once, undone if refused.
    public func setWatched(_ item: ItemSummary, _ mark: Bool) async {
        let before = watched[item.id]
        watched[item.id] = mark
        await write(item.id, undo: { $0.watched[item.id] = before }, why: "Couldn't mark it") { api in
            try await api.setWatched(item.id, mark)
        }
        var change = ServerChange(home: true, itemIds: [item.id])
        if item.kind == .show || item.kind == .season { change.parentIds = [item.id] }
        [item.parentId, item.grandparentId].compactMap { $0 }.forEach { change.itemIds.insert($0) }
        change.libraryIds = [item.libraryId]
        app.note(change)
    }

    /// Adds `item` to My List or takes it out: shown at once, undone if refused.
    public func setListed(_ item: ItemSummary, _ add: Bool) async {
        let before = listed
        var after = listed ?? []
        if add { after.insert(item.id) } else { after.remove(item.id) }
        listed = after
        await write(item.id, undo: { $0.listed = before }, why: "Couldn't change My List") { api in
            try await api.setInWatchlist(item.id, add)
        }
        app.note(ServerChange(home: true, watchlist: true))
    }

    /// Takes `item` out of Continue Watching; false when the server refused (a toast says why).
    public func removeFromContinueWatching(_ item: ItemSummary) async -> Bool {
        var ok = true
        await write(item.id, undo: { _ in ok = false }, why: "Couldn't remove it") { api in
            try await api.removeFromContinueWatching(item.id)
        }
        if ok { app.note(ServerChange(home: true, itemIds: [item.id])) }
        return ok
    }

    /// Play of `itemId` is about to be pressed: the server reads ahead what
    /// it plays first. At most once in 2 minutes per item; nothing is said
    /// when it fails, since Play works without it.
    public func prefetch(_ itemId: Int) {
        let time = now()
        if let last = prefetched[itemId], time.timeIntervalSince(last) < Self.prefetchSeconds { return }
        prefetched[itemId] = time
        Task { [app] in
            guard let api = await app.session.api else { return }
            try? await api.prefetch(itemId)
        }
    }

    /// Reads My List's ids.
    public func loadWatchlist() async {
        guard let items = try? await app.cached(SnapshotKeys.watchlist, { try await $0.watchlist() }) else { return }
        listed = Set(items.map(\.id))
    }

    /// The server's events: marks it now tells are its own; My List read again.
    func followed(_ change: ServerChange) {
        for id in watched.keys where !inFlight.contains(id) && change.names(id) {
            watched[id] = nil
        }
        if change.watchlist || change.everything { Task { await loadWatchlist() } }
    }

    /// Forgets everything: another profile or server.
    func reset() {
        watched = [:]
        listed = nil
        inFlight = []
        prefetched = [:]
    }

    private func write(_ id: Int, undo: (ItemActionsModel) -> Void, why: String,
                       _ body: (PleyAPI) async throws -> Void) async {
        inFlight.insert(id)
        defer { inFlight.remove(id) }
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            try await body(api)
        } catch is CancellationError {
            undo(self)
        } catch {
            undo(self)
            app.shell.show("\(why): \(describe(error))")
        }
    }
}
