import PleyModel
import PleyPlayer

/// What the Mac's Play (⌘↩) plays: the tile with the focus, or the page
/// showing. A show or season plays its next episode; music isn't played
/// yet, so it has none.
@MainActor
public struct PlayTarget {
    /// The title, for the menu: Play "Arrival".
    public let title: String
    /// What to play, worked out when Play is chosen (a show's next episode is asked for then).
    public let request: @MainActor () async -> PlayRequest?

    public init(title: String, request: @escaping @MainActor () async -> PlayRequest?) {
        self.title = title
        self.request = request
    }

    /// The menu item's words.
    public var menuTitle: String { "Play “\(title)”" }

    /// `item`'s Play, through the item actions; `nil` for what doesn't play here.
    public static func item(_ item: ItemSummary, actions: ItemActionsModel) -> PlayTarget? {
        guard plays(item.kind) else { return nil }
        return PlayTarget(title: item.title) { [weak actions] in await actions?.playRequest(item) }
    }

    /// Whether Play plays something of `kind`: movies, shows, seasons, episodes and extras.
    public nonisolated static func plays(_ kind: ItemKind) -> Bool {
        switch kind {
        case .movie, .show, .season, .episode, .extra: true
        default: false
        }
    }
}
