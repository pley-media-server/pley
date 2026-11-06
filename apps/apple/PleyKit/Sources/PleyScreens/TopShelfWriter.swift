import CryptoKit
import Foundation
import PleyModel
import PleyShelf
#if os(tvOS)
import TVServices
#endif

/// Writes what Apple TV's Top Shelf shows: Home's Continue Watching row,
/// ten titles at most, with their pictures at 800 px, for the server and
/// profile signed in (Android's Watch Next, which the app writes the same
/// way). The Top Shelf extension only reads it: it holds no sign-in.
///
/// Another profile or server replaces the row, and signing out empties
/// it. After each change the system is told (`changed`), and asks the
/// extension again.
@MainActor
public final class TopShelfWriter: ObservableObject {
    /// The picture width asked of the server.
    public static let imageWidth = 800

    public let store: ShelfStore
    /// The key the links are signed with, made the first time.
    public let linkKey: SymmetricKey?
    /// What the shelf holds as last written; `nil` when empty.
    @Published public private(set) var shown: ShelfSnapshot?

    private let images: @Sendable (String, Int) async throws -> Data
    private let changed: @Sendable () -> Void
    private var task: Task<Void, Never>?
    /// Counts the writes asked for: only the latest is saved.
    private var asked = 0

    /// `images` reads the server's picture at a path and width; `changed`
    /// tells the system the shelf changed.
    public init(store: ShelfStore, images: @escaping @Sendable (String, Int) async throws -> Data,
                changed: @escaping @Sendable () -> Void) {
        self.store = store
        self.images = images
        self.changed = changed
        linkKey = store.linkKey(create: true)
        shown = store.read()
    }

    /// The writer of the app group's shelf, telling tvOS of each change;
    /// `nil` without an app group (a build signed without App Groups).
    public static func appGroup(images: @escaping @Sendable (String, Int) async throws -> Data) -> TopShelfWriter? {
        guard let store = ShelfStore.appGroup() else { return nil }
        return TopShelfWriter(store: store, images: images, changed: {
            #if os(tvOS)
            TVTopShelfContentProvider.topShelfContentDidChange()
            #endif
        })
    }

    /// Home's rows were read for `owner`: its Continue Watching row (none:
    /// an empty row) goes to the shelf.
    public func show(_ hubs: [Hub], owner: String) {
        let row = hubs.first { $0.key.hasPrefix("continue") }?.items ?? []
        write(owner: owner, Self.entries(row))
    }

    /// The server and profile signed in are now `owner` (`nil`: none): the
    /// row of another one goes at once.
    public func owner(is owner: String?) {
        guard let shown, shown.owner != owner else { return }
        clear()
    }

    /// The server at `origin` was forgotten: its row goes.
    public func forget(origin: String) {
        guard let shown, shown.owner.hasPrefix(ShelfSnapshot.owner(origin: origin, profileId: 0).dropLast(1))
        else { return }
        clear()
    }

    /// Empties the shelf.
    public func clear() {
        // A write under way is dropped (`asked`), and finishes before the files go.
        asked += 1
        let previous = task
        previous?.cancel()
        guard shown != nil || previous != nil else { return }
        shown = nil
        let store = store
        let changed = changed
        task = Task {
            await previous?.value
            await Task.detached(priority: .utility) {
                store.clear()
                changed()
            }.value
        }
    }

    /// Waits for the write under way (tests).
    func idle() async {
        while let task {
            await task.value
            if self.task == task { self.task = nil }
        }
    }

    /// `link`'s signed address, `nil` without a key.
    public func url(_ link: ShelfLink) -> URL? {
        linkKey.map { link.url(key: $0) }
    }

    private func write(owner: String, _ entries: [(entry: ShelfEntry, path: String?)]) {
        asked += 1
        let mine = asked
        let previous = task
        let store = store
        let images = images
        task = Task { [weak self] in
            await previous?.value
            // Pictures saved already are kept; the others are read, ten at most.
            var made: [ShelfEntry] = []
            var fresh: [String: Data] = [:]
            for (entry, path) in entries {
                guard !Task.isCancelled else { return }
                var entry = entry
                if let path {
                    if let saved = store.savedImage(for: path) {
                        entry.image = saved
                    } else if let data = try? await images(path, Self.imageWidth),
                              let name = ShelfStore.imageName(path: path, data: data) {
                        fresh[name] = data
                        entry.image = name
                    }
                }
                made.append(entry)
            }
            guard let self, !Task.isCancelled, mine == self.asked else { return }
            let snapshot = ShelfSnapshot(owner: owner, entries: made, language: ShownLanguage.language.code)
            guard snapshot != self.shown || !fresh.isEmpty else { return }
            self.shown = snapshot
            let changed = self.changed
            let save = fresh
            await Task.detached(priority: .utility) {
                try? store.write(snapshot, images: save)
                changed()
            }.value
        }
    }

    /// The shelf's entries for Continue Watching's `items`, and the picture
    /// each asks the server for: an episode's still, a movie's backdrop,
    /// else the poster.
    nonisolated static func entries(_ items: [ItemSummary]) -> [(entry: ShelfEntry, path: String?)] {
        items.prefix(ShelfSnapshot.maxEntries).map { item in
            let episode = item.kind == .episode
            let show = episode ? item.grandparentTitle.flatMap { $0.isEmpty ? nil : $0 } : nil
            let subtitle = episode ? Words.episodeLabel(season: item.parentIndex, episode: item.index) : ""
            let position = item.userState.positionMs
            let progress: Double? = if position > 0, let duration = item.durationMs, duration > 0 {
                Double(position) / Double(duration)
            } else {
                nil
            }
            let wide = (episode ? [item.thumbUrl, item.backdropUrl] : [item.backdropUrl, item.thumbUrl])
                .compactMap { $0 }.first { !$0.isEmpty }
            let poster = item.posterUrl.flatMap { $0.isEmpty ? nil : $0 }
            let entry = ShelfEntry(itemId: item.id, title: show ?? item.title, subtitle: subtitle,
                                   progress: progress, kind: progress == nil ? .next : .continue,
                                   shape: wide == nil && poster != nil ? .poster : .wide)
            return (entry, wide ?? poster)
        }
    }
}
