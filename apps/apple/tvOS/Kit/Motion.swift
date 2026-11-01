import SwiftUI
import UIKit

/// Animations that follow Reduce Motion: none while it is on.
enum Motion {
    /// Whether the person turned on Reduce Motion.
    @MainActor static var reduced: Bool { UIAccessibility.isReduceMotionEnabled }

    /// `body`'s changes, animated unless Reduce Motion is on.
    @MainActor static func animate(_ animation: Animation = .easeInOut(duration: 0.2), _ body: () -> Void) {
        if reduced {
            body()
        } else {
            withAnimation(animation, body)
        }
    }

    /// `animation`, or none under Reduce Motion: for `.animation(_:value:)`.
    static func animation(_ animation: Animation, reduced: Bool) -> Animation? {
        reduced ? nil : animation
    }
}
