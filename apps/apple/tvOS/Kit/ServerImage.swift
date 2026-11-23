import PleyData
import PleyScreens
import SwiftUI

/// A picture from the server through the app's `ImageLoader`, asked for at
/// the width it shows (in pixels, rounded up to a multiple of 64 so near
/// sizes share one cached picture). Each of `paths` is tried in turn until
/// one comes. Until then, a plain box with `symbol` faintly in its middle,
/// or, given a `title`, the title's gradient, with Pley's mark on it once no
/// picture will come (`PlaceholderArt`). It fades in, except under Reduce
/// Motion, and its fetch is canceled when it goes off screen. The picture
/// says nothing to VoiceOver: what holds it carries the label.
struct ServerImage: View {
    let app: AppModel
    let paths: [String]
    /// The width it shows at, in points.
    let width: CGFloat
    /// The SF Symbol in the placeholder.
    var symbol: String?
    /// The title whose gradient is the placeholder, in place of the box and its symbol.
    var title: String?
    /// Whether a box shows until the picture comes.
    var placeholder = true
    /// The widest picture asked for, in pixels.
    var limit = 4096
    @State private var image: CGImage?
    /// None of `paths` gave a picture.
    @State private var failed = false
    @Environment(\.displayScale) private var displayScale
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(app: AppModel, paths: [String], width: CGFloat, symbol: String? = nil, title: String? = nil,
         placeholder: Bool = true, limit: Int = 4096) {
        self.app = app
        self.paths = paths
        self.width = width
        self.symbol = symbol
        self.title = title
        self.placeholder = placeholder
        self.limit = limit
    }

    init(app: AppModel, path: String?, width: CGFloat, symbol: String? = nil, placeholder: Bool = true) {
        self.init(app: app, paths: path.map { [$0] } ?? [], width: width, symbol: symbol, placeholder: placeholder)
    }

    private var pixels: Int {
        ImageLoader.pixels(points: Double(width), scale: Double(displayScale), limit: limit)
    }

    var body: some View {
        ZStack {
            if placeholder, let title {
                PlaceholderArt(title: title, mark: image == nil && (paths.isEmpty || failed))
            } else if placeholder {
                Rectangle().fill(.quaternary)
                if image == nil, let symbol {
                    Image(systemName: symbol)
                        .font(.system(size: max(12, width * 0.22)))
                        .foregroundStyle(.secondary)
                        .opacity(0.3)
                }
            }
            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .scaledToFill()
                    .transition(.opacity)
            }
        }
        .accessibilityHidden(true)
        .task(id: "\(pixels) \(paths.joined(separator: " "))") {
            image = nil
            failed = false
            for path in paths {
                if let found = try? await app.images.image(path: path, widthPx: pixels) {
                    withAnimation(reduceMotion ? nil : .easeOut(duration: 0.15)) { image = found }
                    return
                }
                if Task.isCancelled { return }
            }
            failed = true
        }
    }
}

/// The picture of a title without one, as on the web and the other apps
/// (`Placeholder`): its 135° gradient, and Pley's mark on it when `mark`,
/// once no picture will come. Not while one loads: a page of tiles
/// scrolling in would flash marks before their pictures.
struct PlaceholderArt: View {
    let title: String
    let mark: Bool

    var body: some View {
        let colors = Placeholder.colors(title)
        let gradient = Gradient(colors: [color(colors.from), color(colors.to)])
        return Canvas { context, size in
            // CSS's 135deg: along the diagonal, as long as the box's sides projected on it.
            let reach = (size.width + size.height) / 4
            let center = CGPoint(x: size.width / 2, y: size.height / 2)
            context.fill(Path(CGRect(origin: .zero, size: size)),
                         with: .linearGradient(gradient, startPoint: CGPoint(x: center.x - reach, y: center.y - reach),
                                               endPoint: CGPoint(x: center.x + reach, y: center.y + reach)))
            guard mark else { return }
            let side = min(min(size.width, size.height) * Placeholder.markShare, Placeholder.markMax)
            var layer = context
            layer.translateBy(x: (size.width - side) / 2, y: (size.height - side) / 2)
            layer.scaleBy(x: side / Placeholder.markGrid, y: side / Placeholder.markGrid)
            for poster in Placeholder.markLayers {
                layer.fill(Path(poster.path), with: .color(.white.opacity(poster.opacity)),
                           style: FillStyle(eoFill: poster.evenOdd))
            }
        }
    }

    private func color(_ rgb: (red: Double, green: Double, blue: Double)) -> Color {
        Color(red: rgb.red, green: rgb.green, blue: rgb.blue)
    }
}

/// A profile's round picture, or its initials on its color.
struct AvatarView: View {
    let app: AppModel
    let name: String
    let path: String?
    let color: String?
    let size: CGFloat

    var body: some View {
        ZStack {
            Circle().fill(fill)
            Text(Avatar.initials(name))
                .font(.system(size: size * 0.4, weight: .semibold))
                .foregroundStyle(.white)
            if path != nil {
                ServerImage(app: app, path: path, width: size, placeholder: false)
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }

    private var fill: Color {
        guard let rgb = Avatar.rgb(color) else { return .gray }
        return Color(red: rgb.red, green: rgb.green, blue: rgb.blue)
    }
}
