import CoreGraphics
import CoreImage
import CoreImage.CIFilterBuiltins
import Foundation

/// QR codes for signing in with a code: the address to allow the app at,
/// drawn black on white in whole pixels per module, with the 4-module
/// quiet zone readers need. Core Image's generator, no other library.
public enum QRCode {
    /// Modules of white around the code.
    public static let quietZone = 4

    /// The QR code of `text`, `scale` pixels per module, or `nil` when it
    /// can't be made (too long for a QR code).
    public static func make(_ text: String, scale: Int = 10) -> CGImage? {
        guard scale > 0, let modules = modules(text) else { return nil }
        let count = modules.count
        let side = (count + 2 * quietZone) * scale
        var pixels = [UInt8](repeating: 255, count: side * side)
        for (row, line) in modules.enumerated() {
            for (column, dark) in line.enumerated() where dark {
                let top = (row + quietZone) * scale
                let left = (column + quietZone) * scale
                for y in top..<(top + scale) {
                    pixels.replaceSubrange((y * side + left)..<(y * side + left + scale),
                                           with: repeatElement(0, count: scale))
                }
            }
        }
        guard let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
        return CGImage(width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: side,
                       space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGBitmapInfo(rawValue: 0),
                       provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    /// The code's modules, top row first, `true` for dark, without any
    /// border; `nil` when `text` doesn't fit in a QR code.
    public static func modules(_ text: String) -> [[Bool]]? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let extent = output.extent.integral
        let width = Int(extent.width)
        let height = Int(extent.height)
        guard width > 0, width == height, width <= 256 else { return nil }
        var rgba = [UInt8](repeating: 255, count: width * height * 4)
        let context = CIContext(options: [.workingColorSpace: NSNull(), .outputColorSpace: NSNull(),
                                          .useSoftwareRenderer: true])
        context.render(output, toBitmap: &rgba, rowBytes: width * 4, bounds: extent, format: .RGBA8, colorSpace: nil)
        var grid = (0..<height).map { y in (0..<width).map { x in rgba[(y * width + x) * 4] < 128 } }
        // The generator adds a border of its own: keep the dark modules' box.
        let rows = grid.indices.filter { grid[$0].contains(true) }
        let columns = (0..<width).filter { x in grid.contains { $0[x] } }
        guard let top = rows.first, let bottom = rows.last, let left = columns.first, let right = columns.last,
              bottom - top == right - left
        else { return nil }
        grid = grid[top...bottom].map { Array($0[left...right]) }
        // Finder patterns sit in three corners; the bottom right has none.
        // A bitmap that came out upside down is turned the right way.
        if !finder(grid, row: 0, column: grid.count - 7) && finder(grid, row: grid.count - 7, column: grid.count - 7) {
            grid.reverse()
        }
        guard finder(grid, row: 0, column: 0), finder(grid, row: 0, column: grid.count - 7),
              finder(grid, row: grid.count - 7, column: 0)
        else { return nil }
        return grid
    }

    /// Whether a finder pattern (a 7×7 dark ring around a 3×3 dark centre) starts at `row`, `column`.
    static func finder(_ grid: [[Bool]], row: Int, column: Int) -> Bool {
        guard row >= 0, column >= 0, row + 7 <= grid.count, column + 7 <= grid.count else { return false }
        for y in 0..<7 {
            for x in 0..<7 {
                let ring = y == 0 || y == 6 || x == 0 || x == 6
                let centre = (2...4).contains(y) && (2...4).contains(x)
                if grid[row + y][column + x] != (ring || centre) { return false }
            }
        }
        return true
    }
}
