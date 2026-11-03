import PleyScreens
import SwiftUI

/// A row of tiles: its title (a heading for VoiceOver), See all when it
/// has one, and the tiles in a line that scrolls sideways and makes only
/// the tiles near the screen.
struct TileRow: View {
    let app: AppModel
    /// The row's key, for its identifier (`home.row.continue_watching`, `item.row.related`).
    let key: String
    let title: String
    let shape: RowShape
    let tiles: [Tile]
    var seeAll: (() -> Void)?
    /// Continue Watching: its tiles' actions offer to leave it, through this.
    var remove: ((Tile) async -> Void)?
    /// What the identifier starts with: `home.row`, `item.row`.
    var prefix = "home.row"
    let open: (Tile) -> Void
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1
    @Environment(\.dynamicTypeSize) private var typeSize

    init(app: AppModel, key: String, title: String, shape: RowShape, tiles: [Tile], seeAll: (() -> Void)? = nil,
         remove: ((Tile) async -> Void)? = nil, prefix: String = "home.row", open: @escaping (Tile) -> Void) {
        self.app = app
        self.key = key
        self.title = title
        self.shape = shape
        self.tiles = tiles
        self.seeAll = seeAll
        self.remove = remove
        self.prefix = prefix
        self.open = open
    }

    private var width: CGFloat {
        Metrics.tileWidth(shape, regular: layout.wide) * min(max(typeScale, 1), Metrics.maxTileScale)
    }

    var body: some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        let heading = Metrics.headingLayout(stacked: typeSize.isAccessibilitySize)
        VStack(alignment: .leading, spacing: 10) {
            heading {
                Text(title)
                    .font(.title3.bold())
                    .accessibilityAddTraits(.isHeader)
                if !typeSize.isAccessibilitySize { Spacer() }
                if let seeAll {
                    Button("See all", action: seeAll)
                        .accessibilityLabel("See all, \(title)")
                        .accessibilityIdentifier("\(identifier).seeAll")
                }
            }
            .padding(.horizontal, gutter)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(tiles) { tile in
                        ArtworkTile(app: app, tile: tile, width: width, continueWatching: remove != nil,
                                    remove: remove.map { remove in { await remove(tile) } }) { open(tile) }
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }

    private var identifier: String { "\(prefix).\(key)" }
}
