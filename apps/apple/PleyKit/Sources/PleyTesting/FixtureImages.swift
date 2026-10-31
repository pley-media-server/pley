import CoreGraphics
import Foundation
import ImageIO

/// Pictures for the tests, made in code.
public enum FixtureImages {
    /// A `width` × `height` picture of one colour, encoded as `type`
    /// (`public.png`, `public.jpeg`).
    public static func make(width: Int, height: Int, type: String = "public.png") -> Data {
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return Data() }
        context.setFillColor(red: 0.22, green: 0.74, blue: 0.97, alpha: 1)
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        guard let image = context.makeImage() else { return Data() }
        let out = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(out, type as CFString, 1, nil) else { return Data() }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { return Data() }
        return out as Data
    }
}
