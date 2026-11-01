import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer

/// The rules and words playlists share across their pages and sheets
/// (Android's `Playlists`).
public enum Playlists {
    /// A title is at most this long, as on the web.
    public static let titleMax = 200
    /// What Play on a music playlist says until music plays (Phase 4).
    public static let musicLater = "Music plays from a later version"

    /// What is wrong with `title` as a playlist's title; `nil` when it will do.
    public static func check(_ title: String) -> String? {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return "Give it a title" }
        if trimmed.count > titleMax { return "At most \(titleMax) characters" }
        return nil
    }

    /// What a playlist of `kind`'s items plays: music for artists, albums and tracks, else video.
    public static func kind(of kind: ItemKind) -> PlaylistKind {
        [.artist, .album, .track].contains(kind) ? .audio : .video
    }

    /// `ids` with the one at `from` moved to `to` (unchanged when `from` is out of range).
    public static func moved<T>(_ ids: [T], from: Int, to: Int) -> [T] {
        guard ids.indices.contains(from) else { return ids }
        var next = ids
        let id = next.remove(at: from)
        next.insert(id, at: min(max(to, 0), next.count))
        return next
    }

    /// An entry's two lines, as the web's playlist page writes them: an
    /// episode's title over its show and number, a season's name over its
    /// show, else the title over what a tile says.
    public static func rowTitles(_ item: ItemSummary) -> (title: String, subtitle: String) {
        switch item.kind {
        case .episode:
            return (item.title, [item.grandparentTitle ?? "", Words.episodeLabel(season: item.parentIndex,
                                                                                episode: item.index)]
                .filter { !$0.isEmpty }.joined(separator: " · "))
        case .season:
            return (Words.seasonTitle(item.title, index: item.index), Words.cardSubtitle(item))
        default:
            return (item.title, Words.cardSubtitle(item))
        }
    }

    /// What is added, in words: one item's full title, else how many.
    public static func titleOf(_ items: [ItemSummary]) -> String {
        switch items.count {
        case 0: ""
        case 1: Words.fullTitle(items[0])
        default: Words.pluralize(items.count, "item")
        }
    }

    /// The toast after adding `added` entries to `playlist`.
    public static func added(_ added: Int, to playlist: String) -> String {
        switch added {
        case 0: "Nothing to add"
        case 1: "Added to \(playlist)"
        default: "Added \(added) items to \(playlist)"
        }
    }
}

/// A playlist as its card and the sheets list it: its title, "12 items ·
/// 3h 12m", whether it plays video or music, the Smart badge, and the
/// pictures of its first entries for the mosaic.
public struct PlaylistCard: Identifiable, Hashable, Sendable {
    public let id: Int
    public let title: String
    public let meta: String
    public let video: Bool
    public let smart: Bool
    /// Server paths, at most 4.
    public let artwork: [String]
    public let spokenLabel: String

    public init(_ playlist: Playlist) {
        id = playlist.id
        title = playlist.title
        meta = Words.playlistMeta(count: playlist.itemCount, durationMs: playlist.durationMs)
        video = playlist.kind != .audio
        smart = playlist.smart
        artwork = Array(playlist.artwork.prefix(4))
        spokenLabel = [title, smart ? "smart playlist" : video ? "playlist" : "music playlist", meta]
            .filter { !$0.isEmpty }.joined(separator: ", ")
    }
}

/// The profile's playlists by title, with New playlist.
@MainActor
public final class PlaylistsModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded
        case failed(String)
    }

    public static let failedTitle = "Couldn't load your playlists"
    public static let emptyTitle = "No playlists yet"

    @Published public private(set) var state: State = .loading
    @Published public private(set) var cards: [PlaylistCard] = []
    @Published public private(set) var notice: String?

    public let app: AppModel
    private var listener: AnyCancellable?

    public init(app: AppModel) {
        self.app = app
        listener = app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, change.playlists || change.everything else { return }
            Task { await self.load() }
        }
    }

    public func load() async {
        guard let api = await app.session.api else { return }
        do {
            cards = try await api.playlists().map(PlaylistCard.init)
            state = .loaded
            notice = nil
        } catch is CancellationError {
        } catch {
            if state == .loaded { notice = describe(error) } else { state = .failed(describe(error)) }
        }
    }

    /// New playlist: an empty one, of the kind chosen in its sheet.
    public func new() {
        app.shell.sheet = .newPlaylist([])
    }
}

/// An entry as the playlist page shows it: its position from 1, its two
/// lines, its length, and its picture (a 16:9 still for video, the square
/// cover for music, with how much was played).
public struct PlaylistRow: Identifiable, Hashable, Sendable {
    public var id: Int { entryId }
    public let entryId: Int
    public let item: ItemSummary
    public let number: Int
    public let title: String
    public let subtitle: String
    public let duration: String
    public let tile: Tile
    /// "3. Arrival, 2016, 1h 56m".
    public let spokenLabel: String
}

/// One playlist's page (Android's `PlaylistViewModel`), shared by every device.
///
/// - **Play and Shuffle.** Video plays as a queue of the playlist
///   (`PlayQueue.playlist`) from its first entry, or a random one. Music
///   says it plays from a later version; its rows open their album.
/// - **Rename and Delete** (the page asks first); deleted, the page goes back.
/// - **Moving entries**: the new order shows at once and goes to the
///   server; moves made while one is on its way are sent after it, the
///   latest order only. When the server refuses, its order comes back
///   with a toast saying why.
/// - **Removing entries**: the entry goes at once, with a toast "Removed X"
///   and Undo; it is sent once the undo's time has passed. A refusal brings
///   it back.
///
/// Smart playlists are read-only: their entries come from their filter.
@MainActor
public final class PlaylistModel: ObservableObject {
    public enum State: Equatable {
        case loading, loaded
        case failed(String)
        case notFound
    }

    public static let failedTitle = "Couldn't load this playlist"
    public static let notFoundTitle = "This playlist isn't available"
    public static let emptyTitle = "Nothing in this playlist yet"

    @Published public private(set) var state: State = .loading
    @Published public private(set) var detail: PlaylistDetail?
    /// The entries' order as moved here, until the server has it; `nil` for the server's.
    @Published public private(set) var order: [Int]?
    /// Entries removed here whose removal hasn't been sent or answered yet.
    @Published public private(set) var hidden: Set<Int> = []
    @Published public private(set) var notice: String?

    public let app: AppModel
    public let playlistId: Int
    /// Picks a random index below its argument (the tests set their own).
    var random: (Int) -> Int = { Int.random(in: 0..<$0) }
    /// Waits out the undo's time (the tests set their own clock).
    var waitForUndo: () async throws -> Void = { try await Task.sleep(for: .seconds(ShellModel.toastSeconds)) }
    /// Removals waiting for their undo's time, by entry.
    private var pending: [Int: Task<Void, Never>] = [:]
    /// An order is on its way.
    private var sending = false
    /// The latest order moved meanwhile, sent after it.
    private var queued: [Int]?
    private var listener: AnyCancellable?

    public init(app: AppModel, playlistId: Int) {
        self.app = app
        self.playlistId = playlistId
        listener = app.$change.dropFirst().compactMap { $0 }.sink { [weak self] change in
            guard let self, self.detail != nil else { return }
            let shown = Set(self.detail?.entries.map(\.item.id) ?? [])
            guard change.everything || change.playlists || !shown.isDisjoint(with: change.itemIds) else { return }
            Task { await self.load() }
        }
    }

    // MARK: What it shows

    public var title: String { detail?.playlist.title ?? "" }
    /// Plays movies and episodes; else music.
    public var video: Bool { detail?.playlist.kind != .audio }
    public var smart: Bool { detail?.playlist.smart ?? false }
    /// Entries can move and go: not in a smart playlist.
    public var editable: Bool { detail != nil && !smart }
    public var artwork: [String] { Array(detail?.playlist.artwork.prefix(4) ?? []) }

    /// A smart playlist's filter in words ("Genre: Comedy", "Newest first").
    public var notes: [String] {
        guard let filter = detail?.playlist.smartFilter, !filter.isEmpty else { return [] }
        return SmartFilter.describe(filter)
    }

    /// The entries as shown: removals and moves made here included.
    public var rows: [PlaylistRow] {
        guard let detail else { return [] }
        var entries = detail.entries.filter { !hidden.contains($0.entryId) }
        if let order {
            let at = Dictionary(order.enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a })
            entries = entries.enumerated().sorted { a, b in
                (at[a.element.entryId] ?? Int.max, a.offset) < (at[b.element.entryId] ?? Int.max, b.offset)
            }.map(\.element)
        }
        let shape: RowShape = video ? .landscape : .square
        return entries.enumerated().map { index, entry in
            let (title, subtitle) = Playlists.rowTitles(entry.item)
            let duration = Words.duration(ms: entry.item.durationMs)
            let spoken = ["\(index + 1). \(title)", subtitle, duration].filter { !$0.isEmpty }.joined(separator: ", ")
            return PlaylistRow(entryId: entry.entryId, item: entry.item, number: index + 1, title: title,
                               subtitle: subtitle, duration: duration, tile: Tile(entry.item, shape: shape),
                               spokenLabel: spoken)
        }
    }

    /// "12 items · 3h 12m", without the entries removed here.
    public var meta: String {
        guard let detail else { return "" }
        let gone = detail.entries.filter { hidden.contains($0.entryId) }
        return Words.playlistMeta(count: max(detail.playlist.itemCount - gone.count, 0),
                                  durationMs: max(detail.playlist.durationMs
                                      - gone.reduce(0) { $0 + ($1.item.durationMs ?? 0) }, 0))
    }

    public var canPlay: Bool { !rows.isEmpty }
    public var canShuffle: Bool { rows.count >= 2 }

    // MARK: Reading

    public func load() async {
        guard let api = await app.session.api else { return }
        do {
            detail = try await api.playlist(playlistId)
            state = .loaded
            notice = nil
        } catch is CancellationError {
        } catch PleyError.api(status: 404, _, _) {
            state = .notFound
        } catch {
            if detail == nil { state = .failed(describe(error)) } else { notice = describe(error) }
        }
    }

    // MARK: Playing

    /// What Play (from the entry at `from`) or Shuffle starts: video as a
    /// queue of the playlist; `nil` for music, which says it plays later.
    public func play(from: Int = 0, shuffle: Bool = false) -> PlayRequest? {
        let rows = rows
        guard !rows.isEmpty else { return nil }
        guard video else {
            app.shell.show(Playlists.musicLater)
            return nil
        }
        let videos = rows.map(\.item).filter { [.movie, .episode, .extra].contains($0.kind) }
        guard !videos.isEmpty else { return nil }
        let start = shuffle ? random(videos.count) : min(max(from, 0), videos.count - 1)
        return PlayRequest(itemId: videos[start].id, queue: PlayQueue(kind: .playlist, id: playlistId,
                                                                      itemIds: videos.map(\.id), position: start))
    }

    // MARK: Moving

    /// Moves the entry at `from` to `to`: shown at once, and sent unless
    /// `send` is false (Apple TV's Move sends once, when it ends).
    public func move(from: Int, to: Int, send: Bool = true) {
        guard let ids = shownOrder(), ids.indices.contains(from), ids.indices.contains(to), from != to else { return }
        let next = Playlists.moved(ids, from: from, to: to)
        order = next
        if send { self.send(next) }
    }

    /// Moves the entries at `offsets` before `destination`, as a list's drag does.
    public func move(fromOffsets offsets: IndexSet, toOffset destination: Int) {
        guard let ids = shownOrder() else { return }
        var next = ids
        let moving = offsets.map { next[$0] }
        for offset in offsets.sorted(by: >) { next.remove(at: offset) }
        let at = destination - offsets.filter { $0 < destination }.count
        next.insert(contentsOf: moving, at: min(max(at, 0), next.count))
        guard next != ids else { return }
        order = next
        send(next)
    }

    /// Moves `entryId` `by` places (-1 up, 1 down) from where it shows now.
    public func move(_ entryId: Int, by: Int, send: Bool = true) {
        guard let from = shownOrder()?.firstIndex(of: entryId) else { return }
        move(from: from, to: from + by, send: send)
    }

    /// Sends the order shown (after moves made with `send` false).
    public func sendOrder() {
        if let order { send(order) }
    }

    /// Forgets the moves not sent: back to the server's order.
    public func cancelMoves() {
        order = nil
    }

    // MARK: Removing

    /// Removes `entryId`: it goes at once, a toast offers Undo, and it is
    /// sent once that time has passed.
    public func remove(_ entryId: Int) {
        guard editable, pending[entryId] == nil, let row = rows.first(where: { $0.entryId == entryId }) else { return }
        hidden.insert(entryId)
        pending[entryId] = Task { [self] in
            do {
                try await waitForUndo()
            } catch {
                return
            }
            // Undone while it waited.
            guard !Task.isCancelled else { return }
            await sendRemoval(entryId)
        }
        app.shell.show("Removed \(row.title)") { [self] in undo(entryId) }
    }

    /// Puts back an entry `remove`d, while it hasn't been sent.
    public func undo(_ entryId: Int) {
        guard let task = pending.removeValue(forKey: entryId) else { return }
        task.cancel()
        hidden.remove(entryId)
    }

    /// Whether `entryId` can still be put back.
    public func undoable(_ entryId: Int) -> Bool { pending[entryId] != nil }

    /// The removals waiting for their undo's time.
    public var waiting: [Int] { Array(pending.keys) }

    // MARK: Rename and delete

    /// Renames the playlist; `false` (nothing sent) when `title` won't do.
    @discardableResult
    public func rename(_ title: String) async -> Bool {
        guard Playlists.check(title) == nil, editable else { return false }
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard name != self.title else { return true }
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            let answer = try await api.renamePlaylist(playlistId, title: name)
            if var detail {
                detail.playlist.title = answer.playlist.title
                self.detail = detail
            }
            app.note(ServerChange(playlists: true))
            return true
        } catch {
            app.shell.show("Couldn't rename it: \(describe(error))")
            return false
        }
    }

    /// Deletes the playlist (the page asked first), then goes back to the playlists.
    @discardableResult
    public func delete() async -> Bool {
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            try await api.deletePlaylist(playlistId)
            for task in pending.values { task.cancel() }
            pending = [:]
            if app.shell.path(for: app.shell.section).last == .playlist(playlistId) { app.shell.back() }
            app.shell.show("Deleted \(title)")
            app.note(ServerChange(playlists: true))
            return true
        } catch {
            app.shell.show("Couldn't delete it: \(describe(error))")
            return false
        }
    }

    // MARK: Inside

    /// The order shown, moves not drawn yet included; `nil` when entries can't move.
    private func shownOrder() -> [Int]? {
        guard editable, let detail else { return nil }
        return (order ?? detail.entries.map(\.entryId)).filter { !hidden.contains($0) }
    }

    /// Sends `ids`, or keeps them for after the order on its way.
    private func send(_ ids: [Int]) {
        if sending {
            queued = ids
            return
        }
        sending = true
        Task { [self] in
            var next: [Int]? = ids
            while let ids = next {
                next = nil
                do {
                    guard let api = await app.session.api else { throw PleyError.signedOut }
                    let answer = try await api.reorderPlaylist(playlistId, entryIds: ids)
                    detail = answer
                    // A newer move keeps its own order.
                    if order == ids { order = nil }
                    next = queued
                    queued = nil
                } catch {
                    // The server's order comes back; moves made meanwhile go with it.
                    queued = nil
                    order = nil
                    app.shell.show("Couldn't move it: \(describe(error))")
                }
            }
            sending = false
            app.note(ServerChange(playlists: true))
        }
    }

    private func sendRemoval(_ entryId: Int) async {
        pending[entryId] = nil
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            try await api.removeFromPlaylist(playlistId, entryId: entryId)
            if var detail, let entry = detail.entries.first(where: { $0.entryId == entryId }) {
                detail.entries.removeAll { $0.entryId == entryId }
                detail.playlist.itemCount = max(detail.playlist.itemCount - 1, 0)
                detail.playlist.durationMs = max(detail.playlist.durationMs - (entry.item.durationMs ?? 0), 0)
                self.detail = detail
            }
            order = order?.filter { $0 != entryId }
            hidden.remove(entryId)
            app.note(ServerChange(playlists: true))
        } catch {
            hidden.remove(entryId)
            app.shell.show("Couldn't remove it: \(describe(error))")
        }
    }
}

/// Add to playlist (a sheet; a side panel on Apple TV): the full title of
/// what is added, the profile's playlists that take it (of its kind, smart
/// ones left out), and a new playlist named on the spot. Shows, seasons,
/// albums and artists add all their episodes or tracks.
@MainActor
public final class AddToPlaylistModel: ObservableObject {
    /// "The Office — S2 · E3 — Office Olympics", "Heat (1995)", "3 items"; `nil` while it loads.
    @Published public private(set) var title: String?
    /// What a playlist must play to take it; `nil` while that isn't known.
    @Published public private(set) var kind: PlaylistKind?
    /// The playlists that take it; `nil` while they load.
    @Published public private(set) var playlists: [PlaylistCard]?
    /// A change is on its way: the buttons wait.
    @Published public private(set) var busy = false
    @Published public private(set) var failure: String?

    public let app: AppModel
    public let itemIds: [Int]

    public init(app: AppModel, itemIds: [Int]) {
        self.app = app
        self.itemIds = itemIds
    }

    public func load() async {
        guard let api = await app.session.api else { return }
        async let lists = api.playlists()
        let ids = itemIds
        let items = await withTaskGroup(of: (Int, ItemSummary?).self) { group in
            for (index, id) in ids.enumerated() {
                group.addTask { (index, try? await api.item(id).summary) }
            }
            var out: [(Int, ItemSummary)] = []
            for await (index, item) in group { if let item { out.append((index, item)) } }
            return out.sorted { $0.0 < $1.0 }.map(\.1)
        }
        title = Playlists.titleOf(items)
        let kind = items.first.map { Playlists.kind(of: $0.kind) } ?? .video
        self.kind = kind
        do {
            playlists = try await lists.filter { !$0.smart && $0.kind == kind }.map(PlaylistCard.init)
            failure = nil
        } catch {
            if playlists == nil { playlists = [] }
            failure = describe(error)
        }
    }

    /// Adds the items to `playlist`; `true` once added (the sheet closes, a toast says so).
    @discardableResult
    public func add(to playlist: PlaylistCard) async -> Bool {
        guard !busy else { return false }
        busy = true
        defer { busy = false }
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            let added = try await api.addToPlaylist(playlist.id, itemIds: itemIds)
            app.shell.show(Playlists.added(added, to: playlist.title))
            app.note(ServerChange(playlists: true))
            return true
        } catch {
            app.shell.show("Couldn't add it: \(describe(error))")
            return false
        }
    }

    /// Makes a playlist called `title` holding the items; `true` once made.
    @discardableResult
    public func create(_ title: String) async -> Bool {
        guard Playlists.check(title) == nil, !busy else { return false }
        busy = true
        defer { busy = false }
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            let made = try await api.createPlaylist(title: name, kind: kind, itemIds: itemIds)
            app.shell.show(Playlists.added(made.entries.count, to: name))
            app.note(ServerChange(playlists: true))
            return true
        } catch {
            app.shell.show("Couldn't make the playlist: \(describe(error))")
            return false
        }
    }
}

/// New playlist, and Save as… a library's filters as a smart playlist: a
/// title of 1 to 200 characters, and Movies & TV or Music for an empty
/// ordinary one. The playlist made opens.
@MainActor
public final class NewPlaylistModel: ObservableObject {
    public enum Mode: Hashable, Sendable {
        /// A playlist of these items (their kind decides what it plays), or an empty one of the kind chosen.
        case items([Int])
        /// A smart playlist of a library's filters, in Plex's syntax.
        case smart(libraryId: Int, query: String)
    }

    @Published public private(set) var busy = false

    public let app: AppModel
    public let mode: Mode

    public init(app: AppModel, mode: Mode) {
        self.app = app
        self.mode = mode
    }

    /// Whether the sheet asks what the playlist plays: only an empty, ordinary one.
    public var asksKind: Bool { mode == .items([]) }

    /// A smart playlist's filters in words: "Genre: Comedy · Unwatched · Newest first".
    public var filters: String {
        guard case .smart(_, let query) = mode else { return "" }
        return SmartFilter.describe(query).joined(separator: " · ")
    }

    /// Makes it; the new playlist's id, or `nil` (a toast says why) when it couldn't be made.
    public func create(_ title: String, kind: PlaylistKind = .video) async -> Int? {
        guard Playlists.check(title) == nil, !busy else { return nil }
        busy = true
        defer { busy = false }
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            let made: PlaylistDetail
            switch mode {
            case .items(let ids):
                made = try await api.createPlaylist(title: name, kind: ids.isEmpty ? kind : nil, itemIds: ids)
            case .smart(let library, let query):
                made = try await api.createSmartPlaylist(title: name, libraryId: library, filter: query)
            }
            app.note(ServerChange(playlists: true))
            return made.id
        } catch {
            app.shell.show("Couldn't make the playlist: \(describe(error))")
            return nil
        }
    }
}
