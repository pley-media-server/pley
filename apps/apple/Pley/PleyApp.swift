import PleyCommon
import SwiftUI

/// Pley for iPhone, iPad and Mac.
@main
struct PleyApp: App {
    var body: some Scene {
        WindowGroup {
            AboutView()
        }
    }
}

/// Which app this is: its name, version and how its requests introduce themselves.
struct AboutView: View {
    private let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"

    var body: some View {
        VStack(spacing: 12) {
            Text("Pley")
                .font(.largeTitle.bold())
            Text("Version \(version)")
            Text(UserAgent.current)
                .font(.footnote.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
        .multilineTextAlignment(.center)
        .padding()
    }
}
