import CoreGraphics

/// The picture of a title without one, as the web's `placeholderGradient`
/// (apps/web/src/lib/images.ts) and Android's `Placeholder`: a 135° gradient
/// from hsl(h, 30 %, 22 %) to hsl(h + 40, 35 %, 12 %), where h comes from a
/// hash of the title, and Pley's mark on it once no picture will come.
public enum Placeholder {
    /// The web's hue for `seed`: `h = h * 31 + unit` over its UTF-16 units, unsigned 32-bit, mod 360.
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

    /// The side of the grid Pley's mark is drawn on.
    public static let markGrid: CGFloat = 64

    /// How much of the box's shorter side the mark takes, and its largest side (a backdrop's), in points.
    public static let markShare: CGFloat = 0.4
    public static let markMax: CGFloat = 120

    /// Pley's mark as the web's and Android's `PleyMark`: its three posters on
    /// its 64 by 64 grid, back to front, each with how much white it is filled
    /// with, lighter than the one behind. The front one is filled even-odd, so
    /// its play triangle is cut out of it.
    public static var markLayers: [(path: CGPath, opacity: Double, evenOdd: Bool)] {
        [
            (poster(CGRect(x: 46, y: 21.5, width: 6, height: 21), radius: 3), 0.16, false),
            (poster(CGRect(x: 36, y: 18.5, width: 8, height: 27), radius: 3.5), 0.24, false),
            (front, 0.36, true),
        ]
    }

    /// One of the posters behind: its left side square, under the poster in
    /// front of it, its right corners rounded by `radius`.
    private static func poster(_ rect: CGRect, radius: CGFloat) -> CGPath {
        let path = CGMutablePath()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY))
        path.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.minY), tangent2End: CGPoint(x: rect.maxX, y: rect.maxY),
                    radius: radius)
        path.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.maxY), tangent2End: CGPoint(x: rect.minX, y: rect.maxY),
                    radius: radius)
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.closeSubpath()
        return path
    }

    /// The front poster, its corners rounded by 4, and the play triangle in it,
    /// its corners rounded by 1: the web's path, its triangle by its corners.
    private static var front: CGPath {
        let path = CGMutablePath()
        path.addRoundedRect(in: CGRect(x: 12, y: 15.5, width: 22, height: 33), cornerWidth: 4, cornerHeight: 4)
        let top = CGPoint(x: 19, y: 25.25)
        let bottom = CGPoint(x: 19, y: 38.75)
        let tip = CGPoint(x: 30.47, y: 32)
        path.move(to: CGPoint(x: 19, y: 32))
        path.addArc(tangent1End: bottom, tangent2End: tip, radius: 1)
        path.addArc(tangent1End: tip, tangent2End: top, radius: 1)
        path.addArc(tangent1End: top, tangent2End: bottom, radius: 1)
        path.closeSubpath()
        return path
    }
}
