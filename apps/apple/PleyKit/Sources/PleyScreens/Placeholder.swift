import CoreGraphics
import PleyModel

/// The picture of a title without one, as every app draws it
/// (packages/placeholder/placeholder.json, PlaceholderTests): a 135° gradient
/// from hsl(h, 30 %, 22 %) to hsl(h + 40, 35 %, 12 %), where h comes from a
/// hash of the title, and Pley's mark on it once no picture will come.
public enum Placeholder {
    /// The hue for `seed`, as every app's: `h = h * 31 + unit` over its UTF-16 units, unsigned 32-bit, mod 360.
    public static func hue(_ seed: String) -> Int {
        var h: UInt32 = 0
        for unit in seed.utf16 { h = h &* 31 &+ UInt32(unit) }
        return Int(h % 360)
    }

    /// The gradient's two ends for `seed`, as red, green and blue from 0 to 1.
    public static func colors(_ seed: String)
        -> (from: (red: Double, green: Double, blue: Double), to: (red: Double, green: Double, blue: Double)) {
        let h = hue(seed)
        return (hsl(Double(h), 0.30, 0.22), hsl(Double((h + 40) % 360), 0.35, 0.12))
    }

    /// CSS's `hsl()`: `hue` in degrees, `saturation` and `lightness` from 0 to 1.
    public static func hsl(_ hue: Double, _ saturation: Double, _ lightness: Double)
        -> (red: Double, green: Double, blue: Double) {
        let c = (1 - abs(2 * lightness - 1)) * saturation
        let sector = hue.truncatingRemainder(dividingBy: 360) / 60
        let x = c * (1 - abs(sector.truncatingRemainder(dividingBy: 2) - 1))
        let rgb: (Double, Double, Double) = switch sector {
        case ..<1: (c, x, 0)
        case ..<2: (x, c, 0)
        case ..<3: (0, c, x)
        case ..<4: (0, x, c)
        case ..<5: (x, 0, c)
        default: (c, 0, x)
        }
        let m = lightness - c / 2
        return (rgb.0 + m, rgb.1 + m, rgb.2 + m)
    }

    // Pley's mark is a copy of packages/placeholder/placeholder.json, the one
    // every app draws it from; a test holds the copy to the file.

    /// The side of the grid Pley's mark is drawn on.
    public static let markGrid: CGFloat = 64

    /// How much of the box's shorter side the mark takes, and its largest side (a backdrop's), in points.
    public static let markShare: CGFloat = 0.4
    public static let markMax: CGFloat = 120

    /// Over a title (a playlist's cover), how much of the box's side the
    /// mark takes, and its gap above the title, in points.
    public static let markShareOverTitle: CGFloat = 0.26
    public static let markGap: CGFloat = 10

    /// The logo's three posters on the grid, back to front, each with how
    /// much white it is filled with, lighter than the one behind.
    static let markPosters: [(path: String, alpha: Double)] = [
        ("M46 21.5L49 21.5C50.66 21.5 52 22.84 52 24.5L52 39.5C52 41.16 50.66 42.5 49 42.5L46 42.5Z", 0.16),
        ("M36 18.5L40.5 18.5C42.43 18.5 44 20.07 44 22L44 42C44 43.93 42.43 45.5 40.5 45.5L36 45.5Z", 0.24),
        ("M16 15.5L30 15.5C32.21 15.5 34 17.29 34 19.5L34 44.5C34 46.71 32.21 48.5 30 48.5L16 48.5C13.79 48.5 12 46.71 12 44.5L12 19.5C12 17.29 13.79 15.5 16 15.5Z", 0.36),
    ]

    /// The glyph an item of each kind (the API's `kind`) takes: its library's.
    static let markKinds: [String: MarkGlyph] = [
        "movie": .movies, "show": .shows, "season": .shows, "episode": .shows, "artist": .music, "album": .music,
        "track": .music, "audiobook": .audiobooks, "part": .audiobooks,
    ]

    /// The glyph of any other kind (extras, collections) and of none.
    static let markDefault = MarkGlyph.play

    /// The glyph for an item of `kind`: its library's, or play.
    public static func glyph(kind: ItemKind?) -> MarkGlyph {
        kind.flatMap { markKinds[$0.rawValue] } ?? markDefault
    }

    /// The mark's posters back to front, `glyph` cut out of the front one.
    public static func markLayers(_ glyph: MarkGlyph) -> [MarkLayer] {
        builtLayers[glyph] ?? []
    }

    /// Each glyph's layers, built once rather than on each draw of each tile without a picture.
    private static let builtLayers: [MarkGlyph: [MarkLayer]] =
        Dictionary(uniqueKeysWithValues: MarkGlyph.allCases.map { ($0, layers(cutting: $0)) })

    /// The posters as paths, the glyph's path part of the front one's, so
    /// that filling it even-odd cuts the glyph out.
    private static func layers(cutting glyph: MarkGlyph) -> [MarkLayer] {
        let front = markPosters.count - 1
        return markPosters.enumerated().map { index, poster in
            index == front
                ? MarkLayer(path: path(poster.path + glyph.path), alpha: poster.alpha, evenOdd: true)
                : MarkLayer(path: path(poster.path), alpha: poster.alpha, evenOdd: false)
        }
    }

    /// `data`, a path of absolute M, L, C and Z as placeholder.json writes them,
    /// as a CGPath: placeholder.json keeps to those so that this takes a few lines
    /// in place of an SVG parser. A command without the numbers it takes is
    /// left out.
    static func path(_ data: String) -> CGPath {
        let path = CGMutablePath()
        var rest = Substring(data)
        while let command = rest.first {
            let operands = rest.dropFirst().prefix { !"MLCZ".contains($0) }
            rest = rest[operands.endIndex...]
            let numbers = operands.split { $0 == " " || $0 == "," }.compactMap { Double($0) }.map { CGFloat($0) }
            func point(_ index: Int) -> CGPoint { CGPoint(x: numbers[index], y: numbers[index + 1]) }
            switch command {
            case "M" where numbers.count == 2: path.move(to: point(0))
            case "L" where numbers.count == 2: path.addLine(to: point(0))
            case "C" where numbers.count == 6: path.addCurve(to: point(4), control1: point(0), control2: point(2))
            case "Z" where numbers.isEmpty: path.closeSubpath()
            default: break
            }
        }
        return path
    }
}

/// A glyph cut out of the mark's front poster, by the kind of library a
/// title is in: a clapperboard for movies, a TV for shows, notes for music,
/// an open book for audiobooks, and the logo's play for anything else.
public enum MarkGlyph: String, CaseIterable, Sendable {
    case play, movies, shows, music, audiobooks

    /// Its path on the mark's grid, inside the front poster.
    var path: String {
        switch self {
        case .play:
            "M19 27L19 37C19 37.36 19.19 37.69 19.51 37.87C19.82 38.05 20.2 38.04 20.51 37.86L29.01 32.86C29.31 32.68 29.5 32.35 29.5 32C29.5 31.65 29.31 31.32 29.01 31.14L20.51 26.14C20.2 25.96 19.82 25.95 19.51 26.13C19.19 26.31 19 26.64 19 27Z"
        case .movies:
            "M17.3 30.51C17.36 30.69 17.56 30.79 17.74 30.73L20.8 29.74C20.93 29.7 21.03 29.57 21.04 29.43L21.17 27.18C21.19 26.93 20.95 26.75 20.71 26.82L16.88 28.07C16.69 28.13 16.59 28.33 16.65 28.51ZM22 28.85C21.99 29.09 22.23 29.28 22.46 29.2L24.69 28.47C24.83 28.43 24.93 28.31 24.94 28.16L25.07 25.91C25.09 25.66 24.85 25.48 24.61 25.56L22.38 26.28C22.24 26.33 22.15 26.45 22.14 26.59ZM25.9 27.58C25.89 27.83 26.13 28.01 26.36 27.93L28.68 27.18C28.86 27.12 28.96 26.92 28.9 26.74L28.25 24.74C28.19 24.56 28 24.46 27.81 24.52L26.28 25.02C26.14 25.06 26.05 25.18 26.04 25.33ZM17.41 33.44C17.41 32.72 17.99 32.14 18.71 32.14L28.11 32.14C28.83 32.14 29.41 32.72 29.41 33.44L29.41 38.24C29.41 38.96 28.83 39.54 28.11 39.54L18.71 39.54C17.99 39.54 17.41 38.96 17.41 38.24Z"
        case .shows:
            "M19.59 24.83C19.31 25.07 19.29 25.49 19.53 25.77L22.62 29.32C22.82 29.55 23.18 29.55 23.38 29.32L26.47 25.77C26.71 25.49 26.69 25.07 26.41 24.83L26.39 24.82C26.11 24.58 25.7 24.6 25.46 24.88L23.23 27.44C23.11 27.58 22.89 27.58 22.77 27.44L20.54 24.88C20.3 24.6 19.89 24.58 19.61 24.82ZM16.25 31.72C16.25 30.68 17.1 29.82 18.15 29.82L27.85 29.82C28.9 29.82 29.75 30.68 29.75 31.72L29.75 37.52C29.75 38.57 28.9 39.42 27.85 39.42L18.15 39.42C17.1 39.42 16.25 38.57 16.25 37.52Z"
        case .music:
            "M20.16 26.47L29.61 24.47L29.61 34.87C29.61 35.97 28.88 36.93 27.82 37.23C26.77 37.53 25.65 37.09 25.07 36.16C24.49 35.23 24.61 34.02 25.35 33.22C26.09 32.41 27.28 32.19 28.26 32.68L28.26 27.26L21.51 28.69L21.51 36.87C21.51 37.97 20.78 38.93 19.72 39.23C18.67 39.53 17.55 39.09 16.97 38.16C16.39 37.23 16.51 36.02 17.25 35.22C17.99 34.41 19.18 34.19 20.16 34.68Z"
        case .audiobooks:
            "M16 27.58C16 27.01 16.52 26.58 17.08 26.7L21.95 27.69C22.18 27.74 22.35 27.95 22.35 28.18L22.35 36.86C22.35 37.18 22.06 37.42 21.75 37.35L16.72 36.32C16.3 36.24 16 35.87 16 35.44ZM23.65 28.18C23.65 27.95 23.82 27.74 24.05 27.69L28.92 26.7C29.48 26.58 30 27.01 30 27.58L30 35.44C30 35.87 29.7 36.24 29.28 36.32L24.25 37.35C23.94 37.42 23.65 37.18 23.65 36.86Z"
        }
    }
}

/// One layer of the mark on its grid, filled white at `alpha`: a poster,
/// the front one with a glyph cut out of it, filled even-odd.
public struct MarkLayer: @unchecked Sendable {
    // Unchecked Sendable: its path is built once and never changed.
    public let path: CGPath
    public let alpha: Double
    public let evenOdd: Bool
}
