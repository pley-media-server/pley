import PleyScreens
import SwiftUI

/// A row of tiles: its title (a heading for VoiceOver), See all when it
/// has one, and the tiles in a line the focus moves along. The row is a
/// focus section, so moving up or down goes to the row above or below;
/// coming back to a row puts the focus on the tile it had last.
struct TileRow: View {
    let app: AppModel
    /// The row's key, for its identifier (`home.row.continue_watching`).
    let key: String
    let title: String
    let shape: RowShape
    let tiles: [Tile]
    var seeAll: (() -> Void)?
    /// Continue Watching: its tiles' actions offer to leave it, through this.
    var remove: ((Tile) async -> Void)?
    /// The focus came to a tile (Home's info block).
    var focused: ((Tile) -> Void)?
    /// What the identifier starts with: `home.row`, `item.row`.
    var prefix = "home.row"
    let open: (Tile) -> Void
    /// The tile with the focus in this row, and the one that had it last.
    @FocusState private var focusedTile: Int?
    @State private var lastFocused: Int?

    init(app: AppModel, key: String, title: String, shape: RowShape, tiles: [Tile], seeAll: (() -> Void)? = nil,
         remove: ((Tile) async -> Void)? = nil, focused: ((Tile) -> Void)? = nil, prefix: String = "home.row",
         open: @escaping (Tile) -> Void) {
        self.app = app
        self.key = key
        self.title = title
        self.shape = shape
        self.tiles = tiles
        self.seeAll = seeAll
        self.remove = remove
        self.focused = focused
        self.prefix = prefix
        self.open = open
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(tiles) { tile in
                        ArtworkTile(app: app, tile: tile, width: Metrics.tileWidth(shape), continueWatching: remove != nil,
                                    remove: remove.map { remove in { await remove(tile) } },
                                    focused: focused.map { focused in { focused(tile) } },
                                    rowFocus: $focusedTile) { open(tile) }
                    }
                    if let seeAll {
                        Button("See all", action: seeAll)
                            .accessibilityLabel("See all, \(title)")
                            .accessibilityIdentifier("\(prefix).\(key).seeAll")
                    }
                }
                // Room for the focused tile to lift.
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 32)
            }
            .scrollClipDisabled()
        }
        // Coming back up or down to the row, the focus goes to the tile it left.
        .defaultFocus($focusedTile, lastFocused, priority: .userInitiated)
        .onChange(of: focusedTile) { _, now in
            if let now { lastFocused = now }
        }
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(prefix).\(key)")
    }
}
