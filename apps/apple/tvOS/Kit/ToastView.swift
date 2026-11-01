import PleyScreens
import SwiftUI

/// The shell's toast, top trailing, clear of the tabs; VoiceOver reads it
/// out when it shows. It takes no focus, so it never moves the focus away
/// from what the person is doing.
struct ToastView: View {
    @ObservedObject var shell: ShellModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack {
            if let toast = shell.toast {
                Text(toast.text)
                    .font(.callout)
                    .padding(.horizontal, 28)
                    .padding(.vertical, 16)
                    .background(.regularMaterial, in: Capsule())
                    .padding(.top, 40)
                    .padding(.trailing, Metrics.gutter)
                    .accessibilityIdentifier("toast")
                    .transition(reduceMotion ? .identity : .move(edge: .trailing).combined(with: .opacity))
                    .task(id: toast.id) { AccessibilityNotification.Announcement(toast.text).post() }
            }
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.2), value: shell.toast)
        .allowsHitTesting(false)
    }
}
