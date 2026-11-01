import PleyData
import PleyScreens
import SwiftUI

/// A picture from the server through the app's `ImageLoader`, asked for at
/// the width it shows (in pixels, rounded up to a multiple of 64 so near
/// sizes share one cached picture). Each of `paths` is tried in turn until
/// one comes. Until then, a plain box with `symbol` faintly in its middle.
/// It fades in, except under Reduce Motion, and its fetch is cancelled when
/// it goes off screen. The picture says nothing to VoiceOver: what holds it
/// carries the label.
struct ServerImage: View {
    let app: AppModel
    let paths: [String]
    /// The width it shows at, in points.
    let width: CGFloat
    /// The SF Symbol in the placeholder.
    var symbol: String?
    /// Whether a box shows until the picture comes.
    var placeholder = true
    /// The widest picture asked for, in pixels.
    var limit = 4096
    @State private var image: CGImage?
    @Environment(\.displayScale) private var displayScale
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(app: AppModel, paths: [String], width: CGFloat, symbol: String? = nil, placeholder: Bool = true,
         limit: Int = 4096) {
        self.app = app
        self.paths = paths
        self.width = width
        self.symbol = symbol
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
            if placeholder {
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
            for path in paths {
                if let found = try? await app.images.image(path: path, widthPx: pixels) {
                    withAnimation(reduceMotion ? nil : .easeOut(duration: 0.15)) { image = found }
                    return
                }
                if Task.isCancelled { return }
            }
        }
    }
}

/// A profile's round picture, or its initials on its colour.
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
