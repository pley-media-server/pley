import PleyModel
import PleyScreens
import SwiftUI

/// The sizes the iPhone, iPad and Mac screens share: spacing, corners and
/// the tiles' widths. Text uses the system's text styles only.
enum Metrics {
    /// Corner radius of tiles and pictures.
    static let corner: CGFloat = 8
    /// Space between the tiles of a row or a grid.
    static let tileSpacing: CGFloat = 12
    /// Space between rows.
    static let rowSpacing: CGFloat = 28
    /// A tile's width grows with Dynamic Type up to this many times.
    static let maxTileScale: CGFloat = 1.6

    /// A heading and what goes with it (See all, a season's link): side by
    /// side, or one under the other at the accessibility text sizes, where
    /// side by side would break the heading's words.
    static func headingLayout(stacked: Bool) -> AnyLayout {
        stacked ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline))
    }

    /// The side margin: narrower on iPhone.
    static func gutter(regular: Bool) -> CGFloat { regular ? 20 : 16 }

    /// A tile's width before Dynamic Type: iPhone 110 (posters and squares)
    /// or 220 (stills); iPad and Mac 140 or 260.
    static func tileWidth(_ shape: RowShape, regular: Bool) -> CGFloat {
        switch shape {
        case .landscape: regular ? 260 : 220
        case .poster, .square: regular ? 140 : 110
        }
    }

    /// A tile's picture height for `width`.
    static func pictureHeight(_ shape: RowShape, width: CGFloat) -> CGFloat {
        switch shape {
        case .poster: width * 3 / 2
        case .landscape: width * 9 / 16
        case .square: width
        }
    }

    /// The SF Symbol a picture shows until it comes, by what it is of.
    static func symbol(_ kind: ItemKind) -> String {
        switch kind {
        case .show, .season, .episode: "tv"
        case .artist: "music.mic"
        case .album, .track: "music.note"
        default: "film"
        }
    }
}

/// Whether the screens use their wide layout: the iPad at regular width,
/// and the Mac always (it has no size classes).
struct WideLayout: DynamicProperty {
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    var wide: Bool { sizeClass == .regular }
    #else
    var wide: Bool { true }
    #endif
}
