import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// One row of Home: a hub of the server's, its tiles made.
public struct HomeRow: Identifiable, Hashable, Sendable {
    /// The hub's key (`continue_watching`, `recently_added:3`): stable, the row's id.
    public let key: String
    /// "Continue Watching", "Recently Added in Movies", as the server says it.
    public let title: String
    public let shape: RowShape
    public let tiles: [Tile]
    /// Where See all goes: the library, listed as the row is; `nil` for rows without one.
    public let seeAll: Screen?

    public var id: String { key }

    public init(_ hub: Hub) {
        key = hub.key
        title = hub.title
        let shape = RowShape.of(hub)
        self.shape = shape
        tiles = hub.items.map { Tile($0, shape: shape) }
        seeAll = ShellModel.seeAll(hub)
    }
}

/// Home: the server's rows for the profile signed in.
@MainActor
public final class HomeModel: ObservableObject {
    public enum State {
        case loading
        /// The rows with something in them, in the server's order; none: "Nothing here yet".
        case loaded([HomeRow])
        case failed(String)
    }

    /// What Home says when it couldn't load and has nothing to show.
    public static let failedTitle = "Couldn't load your home screen"
    /// What it says with no rows.
    public static let emptyTitle = "Nothing here yet"

    @Published public private(set) var state: State = .loading

    public let app: AppModel
    /// The `homeVersion` the rows shown are for.
    private var version: Int?

    public init(app: AppModel) {
        self.app = app
    }

    /// Loads the rows again. The rows shown stay until the new ones come,
    /// unless they belong to another sign-in or profile: those go at once,
    /// so a failed load never leaves one profile's rows on another's Home.
    public func reload() async {
        guard let api = await app.session.api else { return }
        if version != app.homeVersion {
            version = app.homeVersion
            state = .loading
        }
        if case .failed = state { state = .loading }
        do {
            state = .loaded(try await HomeRepository(api: api).load().map(HomeRow.init))
        } catch is CancellationError {
        } catch PleyError.signedOut {
            // The session says so, and sign-in shows.
        } catch {
            if case .loaded = state { return }
            state = .failed(describe(error))
        }
    }

    /// The picture for `item`'s tile: its poster, else its thumbnail (an episode's).
    public static func posterPath(_ item: ItemSummary) -> String? {
        [item.posterUrl, item.thumbUrl].compactMap { $0 }.first { !$0.isEmpty }
    }

    /// Pixels to ask for a tile `points` wide on a screen of `scale`.
    public static func pixels(_ points: Double, scale: Double) -> Int {
        max(1, Int((points * max(scale, 1)).rounded()))
    }
}
