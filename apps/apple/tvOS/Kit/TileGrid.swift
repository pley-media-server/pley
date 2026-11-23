import PleyData
import PleyScreens
import SwiftUI

/// A grid of `count` tiles across the page: as many columns as fit at
/// Settings' grid size, the tiles widened a little so the columns fill the
/// width between the margins, with nothing left over on the right. Only
/// the cells near the screen are made; a cell whose tile isn't loaded yet
/// shows its shape, and each cell says when it shows (`showing`), so its
/// page can be loaded.
struct TileGrid: View {
    let app: AppModel
    let shape: RowShape
    let count: Int
    let tile: (Int) -> Tile?
    var showing: (Int) -> Void = { _ in }
    /// Room kept on the right besides the margin: for a library's letters.
    var trailingRoom: CGFloat = 0
    let open: (Tile) -> Void
    @AppStorage(AppSettings.gridDensityKey) private var density = GridDensity.medium.rawValue
    /// The width between the margins, once laid out.
    @State private var room: CGFloat = 0

    var body: some View {
        let layout = Self.layout(room: room, base: Metrics.tileWidth(shape) * (GridDensity(rawValue: density) ?? .medium).scale)
        LazyVGrid(columns: Array(repeating: GridItem(.fixed(layout.width), spacing: Metrics.tileSpacing, alignment: .top),
                                 count: layout.columns),
                  alignment: .leading, spacing: Metrics.rowSpacing) {
            ForEach(0..<count, id: \.self) { index in
                Group {
                    if let tile = tile(index) {
                        ArtworkTile(app: app, tile: tile, width: layout.width) { open(tile) }
                    } else {
                        PlaceholderTile(shape: shape, width: layout.width)
                    }
                }
                .onAppear { showing(index) }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { room = $0 }
        .padding(.leading, Metrics.gutter)
        .padding(.trailing, Metrics.gutter + trailingRoom)
    }

    /// The columns that fit in `room` at `base` wide, and their width once
    /// they share all of it. Until laid out, one column at `base`.
    static func layout(room: CGFloat, base: CGFloat) -> (columns: Int, width: CGFloat) {
        guard room > base else { return (1, base) }
        let spacing = Metrics.tileSpacing
        let columns = max(1, Int((room + spacing) / (base + spacing)))
        return (columns, ((room - spacing * CGFloat(columns - 1)) / CGFloat(columns)).rounded(.down))
    }
}
