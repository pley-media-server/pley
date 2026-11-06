import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// A play in the history, as its row shows it (Android's `historyRow`).
public struct HistoryRow: Identifiable, Hashable, Sendable {
    public var id: Int { entryId }
    public let entryId: Int
    /// The show for an episode, else the item's own title.
    public let title: String
    /// "S2 · E5 · Glasshouse", "Artist · Album", or the year.
    public let subtitle: String
    /// "3 hours ago".
    public let time: String
    /// "Chrome on Windows"; `nil` when the server doesn't know.
    public let device: String?
    /// The picture's paths to try in order: its own, its parent's, its grandparent's.
    public let pictures: [String]
    /// Albums, artists and tracks have square art; the rest posters.
    public let square: Bool
    /// The page the row opens: a track's album, with the track.
    public let opens: Screen
    /// What the row's picture shows until it comes.
    public let kind: ItemKind
    /// What VoiceOver reads: "Severance, S1 · E1 · Good News About Hell, 3 hours ago, on Pixel 9".
    public let spoken: String

    public init(_ entry: HistoryEntry, now: Int) {
        let item = entry.item
        entryId = entry.id
        kind = item.kind
        switch item.kind {
        case .episode:
            title = item.grandparentTitle.flatMap { $0.isEmpty ? nil : $0 } ?? item.title
            subtitle = [Words.episodeLabel(season: item.parentIndex, episode: item.index), item.title]
                .filter { !$0.isEmpty }.joined(separator: " · ")
        case .track:
            title = item.title
            subtitle = [item.grandparentTitle, item.parentTitle].compactMap { $0 }.filter { !$0.isEmpty }
                .joined(separator: " · ")
        default:
            title = item.title
            subtitle = item.year.map(String.init) ?? ""
        }
        time = Words.relative(entry.viewedAt, now: now)
        device = entry.device.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
        pictures = Tile.posters(item)
        square = [.album, .artist, .track].contains(item.kind)
        opens = Screen.page(of: item)
        spoken = [title, subtitle, time, device.map { localized("on \($0)") }].compactMap { $0 }.filter { !$0.isEmpty }
            .joined(separator: ", ")
    }
}

/// History (the web's HistoryList on the account page): this profile's
/// plays, newest first, 15 at a time with Show more, each with when and
/// where it played. Removing one takes it away at once and puts it back,
/// with a toast, when the server refuses.
@MainActor
public final class HistoryModel: ObservableObject {
    public enum State: Equatable, Sendable {
        case loading
        case loaded
        case failed(String)
    }

    /// How many plays a page holds.
    public static let pageSize = 15
    /// When there are none.
    public static var emptyText: String { localized("Nothing played yet. Titles you watch to the end show here.") }

    @Published public private(set) var state: State = .loading
    @Published public private(set) var rows: [HistoryRow] = []
    /// Every play there is, however many are loaded.
    @Published public private(set) var total = 0
    @Published public private(set) var loadingMore = false

    public let app: AppModel
    private let now: () -> Int
    private var entries: [HistoryEntry] = []
    /// Removals on their way, so a reload doesn't bring their rows back.
    private var removing: Set<Int> = []

    public init(app: AppModel, now: @escaping () -> Int = { Int(Date().timeIntervalSince1970) }) {
        self.app = app
        self.now = now
    }

    /// Whether Show more shows.
    public var canLoadMore: Bool { entries.count < total }

    /// The first page again; what shows stays while it loads, and on a failure.
    public func load() async {
        if rows.isEmpty { state = .loading }
        do {
            let page = try await fetch(offset: 0, limit: max(Self.pageSize, entries.count))
            entries = page.entries.filter { !removing.contains($0.id) }
            total = page.total
            state = .loaded
            remake()
        } catch is CancellationError {
        } catch {
            if rows.isEmpty {
                state = .failed(asSentence(describe(error)))
            } else {
                app.shell.show(asSentence(localized("Couldn't load your history: \(describe(error))")))
            }
        }
    }

    /// The next 15 plays.
    public func loadMore() async {
        guard !loadingMore, canLoadMore else { return }
        loadingMore = true
        defer { loadingMore = false }
        do {
            let page = try await fetch(offset: entries.count, limit: Self.pageSize)
            let known = Set(entries.map(\.id))
            entries += page.entries.filter { !known.contains($0.id) && !removing.contains($0.id) }
            total = page.total
            remake()
        } catch is CancellationError {
        } catch {
            app.shell.show(asSentence(localized("Couldn't load more: \(describe(error))")))
        }
    }

    /// Removes the play `entryId`: its row goes at once, and comes back if the server refuses.
    public func remove(_ entryId: Int) async {
        guard let at = entries.firstIndex(where: { $0.id == entryId }), !removing.contains(entryId) else { return }
        let entry = entries.remove(at: at)
        total = max(total - 1, 0)
        removing.insert(entryId)
        remake()
        defer { removing.remove(entryId) }
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            try await api.deleteHistory(entryId)
        } catch {
            entries.insert(entry, at: min(at, entries.count))
            total += 1
            remake()
            if !(error is CancellationError) {
                app.shell.show(asSentence(localized("Couldn't remove it: \(describe(error))")))
            }
        }
    }

    private func fetch(offset: Int, limit: Int) async throws -> HistoryPage {
        // The first page is saved, to show while the server is out of reach.
        let limit = min(limit, 200)
        guard offset == 0 else {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            return try await api.history(offset: offset, limit: limit)
        }
        return try await app.cached(SnapshotKeys.history) { try await $0.history(offset: 0, limit: limit) }
    }

    private func remake() {
        let now = now()
        rows = entries.map { HistoryRow($0, now: now) }
    }
}
