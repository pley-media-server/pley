import PleyData
import PleyScreens
import SwiftUI

/// A grid of `count` tiles in as many columns as fit (two on iPhone at the
/// accessibility text sizes), sized by Settings' grid size. Only the cells near the screen are made; a
/// cell whose tile isn't loaded yet shows its shape, and each cell says
/// when it shows (`showing`), so its page can be loaded.
struct TileGrid: View {
    let app: AppModel
    let shape: RowShape
    let count: Int
    let tile: (Int) -> Tile?
    var showing: (Int) -> Void = { _ in }
    let open: (Tile) -> Void
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1
    @Environment(\.dynamicTypeSize) private var typeSize
    @AppStorage(AppSettings.gridDensityKey) private var density = GridDensity.medium.rawValue

    private var width: CGFloat {
        let scale = (GridDensity(rawValue: density) ?? .medium).scale
        return Metrics.tileWidth(shape, regular: layout.wide) * scale * min(max(typeScale, 1), Metrics.maxTileScale)
    }

    private var columns: [GridItem] {
        if !layout.wide && typeSize.isAccessibilitySize {
            return Array(repeating: GridItem(.flexible(), spacing: Metrics.tileSpacing, alignment: .top), count: 2)
        }
        return [GridItem(.adaptive(minimum: width, maximum: width * 1.4), spacing: Metrics.tileSpacing,
                         alignment: .top)]
    }

    var body: some View {
        LazyVGrid(columns: columns, alignment: .leading, spacing: 20) {
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
        .padding(.horizontal, Metrics.gutter(regular: layout.wide))
    }
}
