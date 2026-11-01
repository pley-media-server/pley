import PleyModel
import PleyScreens
import SwiftUI

/// The sizes Apple TV's screens share: the screen's margins, spacing,
/// corners and the tiles' widths, for a television seen from the sofa.
enum Metrics {
    /// The side margin, inside the screen's safe area.
    static let gutter: CGFloat = 80
    /// Corner radius of tiles and pictures.
    static let corner: CGFloat = 12
    /// Space between the tiles of a row or a grid: room for the focused
    /// one to lift.
    static let tileSpacing: CGFloat = 48
    /// Space between rows.
    static let rowSpacing: CGFloat = 48

    /// A tile's width: 250 for posters and squares, 420 for stills.
    static func tileWidth(_ shape: RowShape) -> CGFloat {
        switch shape {
        case .landscape: 420
        case .poster, .square: 250
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
