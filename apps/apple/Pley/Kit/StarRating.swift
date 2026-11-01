import SwiftUI

/// The profile's own rating: five stars in half steps (1 to 10, two points
/// a star). A tap or a drag across the stars sets it; tapping the rating
/// it has takes it away. For VoiceOver it is one adjustable element,
/// "Your rating, 3.5 stars", swiped up or down a half star at a time.
struct StarRating: View {
    /// 1 to 10; `nil`: unrated.
    let rating: Int?
    let rate: (Int?) -> Void
    @ScaledMetric(relativeTo: .title3) private var size: CGFloat = 26

    var body: some View {
        HStack(spacing: 4) {
            ForEach(1...5, id: \.self) { star in
                Image(systemName: symbol(star))
                    .font(.system(size: size * 0.85))
                    .frame(width: size, height: size)
                    .foregroundStyle(rating == nil ? Color.secondary : Color.yellow)
            }
        }
        .contentShape(Rectangle())
        .gesture(DragGesture(minimumDistance: 0).onEnded { drag in
            // Five stars and the four gaps between them.
            let width = size * 5 + 16
            let value = Int((max(0, min(drag.location.x, width)) / width * 10).rounded(.up))
            rate(value == rating ? nil : max(1, value))
        })
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Your rating")
        .accessibilityValue(spoken)
        .accessibilityAdjustableAction { direction in
            let now = rating ?? 0
            switch direction {
            case .increment: rate(min(10, now + 1))
            case .decrement: rate(now > 1 ? now - 1 : nil)
            @unknown default: break
            }
        }
        .accessibilityIdentifier("item.rating")
    }

    private func symbol(_ star: Int) -> String {
        let points = rating ?? 0
        if points >= star * 2 { return "star.fill" }
        if points == star * 2 - 1 { return "star.leadinghalf.filled" }
        return "star"
    }

    /// "3.5 stars", "Not rated".
    private var spoken: String {
        guard let rating else { return "Not rated" }
        let stars = Double(rating) / 2
        let number = stars == stars.rounded() ? String(Int(stars)) : String(stars)
        return "\(number) \(stars == 1 ? "star" : "stars")"
    }
}
