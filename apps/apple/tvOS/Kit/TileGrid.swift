import PleyData
import PleyScreens
import SwiftUI

/// A grid of `count` tiles in as many columns as fit, sized by Settings' grid size. Only the cells near
/// the screen are made; a cell whose tile isn't loaded yet shows its
/// shape, and each cell says when it shows (`showing`), so its page can
/// be loaded.
struct TileGrid: View {
    let app: AppModel
    let shape: RowShape
    let count: Int
    let tile: (Int) -> Tile?
    var showing: (Int) -> Void = { _ in }
    let open: (Tile) -> Void
    @AppStorage(AppSettings.gridDensityKey) private var density = GridDensity.medium.rawValue

    var body: some View {
        let width = Metrics.tileWidth(shape) * (GridDensity(rawValue: density) ?? .medium).scale
        LazyVGrid(columns: [GridItem(.adaptive(minimum: width, maximum: width), spacing: Metrics.tileSpacing,
                                     alignment: .top)],
                  alignment: .leading, spacing: Metrics.rowSpacing) {
            ForEach(0..<count, id: \.self) { index in
                Group {
                    if let tile = tile(index) {
                        ArtworkTile(app: app, tile: tile, width: width) { open(tile) }
                    } else {
                        PlaceholderTile(shape: shape, width: width)
                    }
                }
                .onAppear { showing(index) }
            }
        }
        .padding(.horizontal, Metrics.gutter)
    }
}
