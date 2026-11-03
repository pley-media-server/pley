import PleyScreens
import SwiftUI

#if os(iOS)
import UIKit
#endif

/// The shell's toast, at the bottom of the window, with Undo when it has
/// one; VoiceOver reads it out when it shows.
struct ToastView: View {
    @ObservedObject var shell: ShellModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack {
            if let toast = shell.toast {
                HStack(spacing: 16) {
                    Text(toast.text)
                        .font(.callout)
                    if toast.undo != nil {
                        Button("Undo") { shell.undoToast() }
                            .font(.callout.bold())
                            .accessibilityIdentifier("toast.undo")
                    }
                }
                .padding(.horizontal, 18)
                .padding(.vertical, 12)
                .background(.regularMaterial, in: Capsule())
                .padding(.bottom, 16)
                .padding(.horizontal)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("toast")
                .transition(reduceMotion ? .identity : .move(edge: .bottom).combined(with: .opacity))
                .task(id: toast.id) { announce(toast.text) }
            }
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.2), value: shell.toast)
    }
}

/// VoiceOver reads `text` out (a toast, the offline banner, a channel's trouble).
@MainActor
func announce(_ text: String) {
    #if os(iOS)
    if #available(iOS 17, *) {
        AccessibilityNotification.Announcement(text).post()
    } else {
        UIAccessibility.post(notification: .announcement, argument: text)
    }
    #else
    AccessibilityNotification.Announcement(text).post()
    #endif
}
