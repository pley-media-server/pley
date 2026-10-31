import PleyScreens
import SwiftUI

/// A picture from the server, fetched at `widthPx` through the app's
/// `ImageLoader`; a plain placeholder until it comes, or if it doesn't.
struct ServerImage: View {
    let app: AppModel
    let path: String?
    let widthPx: Int
    /// Whether a grey box shows until the picture comes.
    var placeholder = true
    @State private var image: CGImage?

    var body: some View {
        ZStack {
            if placeholder { Rectangle().fill(.quaternary) }
            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .scaledToFill()
            }
        }
        .task(id: "\(widthPx) \(path ?? "")") {
            image = nil
            guard let path else { return }
            image = try? await app.images.image(path: path, widthPx: widthPx)
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
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        ZStack {
            Circle().fill(fill)
            Text(Avatar.initials(name))
                .font(.system(size: size * 0.4, weight: .semibold))
                .foregroundStyle(.white)
            if path != nil {
                ServerImage(app: app, path: path, widthPx: HomeModel.pixels(Double(size), scale: Double(displayScale)),
                            placeholder: false)
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
    }

    private var fill: Color {
        guard let rgb = Avatar.rgb(color) else { return .gray }
        return Color(red: rgb.red, green: rgb.green, blue: rgb.blue)
    }
}
