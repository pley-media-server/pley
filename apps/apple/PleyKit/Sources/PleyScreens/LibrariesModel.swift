import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// A library in the list of libraries, its words made.
public struct LibraryEntry: Identifiable, Hashable, Sendable {
    public let library: Library
    /// "412 movies", "38 shows", "120 artists".
    public let count: String
    /// The server is scanning it now.
    public let scanning: Bool

    public var id: Int { library.id }
    public var name: String { library.name }

    /// The SF Symbol of its kind.
    public var symbol: String { Self.symbol(library.kind) }

    public static func symbol(_ kind: LibraryKind) -> String {
        switch kind {
        case .shows: "tv"
        case .music: "music.note"
        default: "film"
        }
    }

    /// What VoiceOver says: "Movies, 412 movies, scanning".
    public var spoken: String { [name, count, scanning ? "scanning" : ""].filter { !$0.isEmpty }.joined(separator: ", ") }
}

/// The libraries this profile may open, in its order, each with what it
/// holds and whether it is being scanned. With two or more, they can be
/// put in another order: each move shows at once, and the orders are
/// saved one after the other, in the order they were made. A `409` (a
/// library added or removed meanwhile) reads the list again.
@MainActor
public final class LibrariesModel: ObservableObject {
    public enum State: Equatable {
        case loading
        case loaded
        case failed(String)
    }

    public static let failedTitle = "Couldn't load your libraries"
    public static let emptyTitle = "No libraries yet"

    @Published public private(set) var state: State = .loading
    @Published public private(set) var entries: [LibraryEntry] = []
    /// Why the last change couldn't be made while the list still shows.
    @Published public private(set) var notice: String?

    public let app: AppModel
    private var libraries: [Library] = []
    /// The saves, one after the other.
    private var saving: Task<Void, Never>?
    private var listeners: Set<AnyCancellable> = []

    public init(app: AppModel) {
        self.app = app
        app.$change.compactMap { $0 }.sink { [weak self] change in
            guard let self, change.libraries || change.everything else { return }
            Task { await self.reload() }
        }.store(in: &listeners)
        // The value sent, not `app.scans`: a publisher sends before the property changes.
        app.$scans.sink { [weak self] scans in self?.redraw(scans) }.store(in: &listeners)
    }

    /// Whether their order can be changed: two or more.
    public var reorderable: Bool { entries.count >= 2 }

    public func reload() async {
        guard let api = await app.session.api else { return }
        do {
            show(try await api.libraries())
            state = .loaded
            notice = nil
        } catch is CancellationError {
        } catch PleyError.signedOut {
        } catch {
            if state == .loaded { notice = describe(error) } else { state = .failed(describe(error)) }
        }
    }

    /// Moves the libraries at `offsets` before `destination` (SwiftUI's `onMove`).
    public func move(fromOffsets offsets: IndexSet, toOffset destination: Int) {
        var ids = libraries.map(\.id)
        let moving = offsets.sorted().map { ids[$0] }
        let before = offsets.filter { $0 < destination }.count
        ids.removeAll { moving.contains($0) }
        ids.insert(contentsOf: moving, at: max(0, min(destination - before, ids.count)))
        reorder(ids)
    }

    /// Moves library `id` one up (`-1`) or down (`+1`): Apple TV's Move.
    public func move(_ id: Int, by step: Int) {
        var ids = libraries.map(\.id)
        guard let at = ids.firstIndex(of: id) else { return }
        let to = at + step
        guard ids.indices.contains(to) else { return }
        ids.swapAt(at, to)
        reorder(ids)
    }

    /// Back to the server's order (Settings).
    public func useServerOrder() async {
        guard let api = await app.session.api else { return }
        await saving?.value
        do {
            show(try await api.resetLibraryOrder())
            app.note(ServerChange(home: true, libraries: true))
        } catch {
            app.shell.show("Couldn't change the order: \(describe(error))")
        }
    }

    /// Waits for the saves made so far (tests).
    public func saved() async {
        await saving?.value
    }

    // MARK: Inside

    private func reorder(_ ids: [Int]) {
        guard ids != libraries.map(\.id) else { return }
        let byId = Dictionary(uniqueKeysWithValues: libraries.map { ($0.id, $0) })
        show(ids.compactMap { byId[$0] })
        let previous = saving
        saving = Task { [weak self] in
            await previous?.value
            guard let self else { return }
            await self.save(ids)
        }
    }

    private func save(_ ids: [Int]) async {
        guard let api = await app.session.api else { return }
        do {
            let saved = try await api.setLibraryOrder(ids)
            // A later move shows already: the answer stands only for the last.
            if libraries.map(\.id) == ids { show(saved) }
            app.note(ServerChange(home: true, libraries: true))
        } catch PleyError.api(status: 409, _, _) {
            await reload()
        } catch is CancellationError {
        } catch {
            app.shell.show("Couldn't save the order: \(describe(error))")
            await reload()
        }
    }

    private func show(_ libraries: [Library]) {
        self.libraries = libraries
        app.shell.setLibraries(libraries)
        redraw(app.scans)
    }

    private func redraw(_ scans: [Int: ScanStatus]) {
        entries = libraries.map {
            LibraryEntry(library: $0, count: Words.libraryCount($0), scanning: $0.scanning || scans[$0.id] != nil)
        }
    }
}
