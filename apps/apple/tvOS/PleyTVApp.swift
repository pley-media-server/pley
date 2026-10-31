import PleyCommon
import SwiftUI

/// Pley for Apple TV.
@main
struct PleyTVApp: App {
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
        VStack(spacing: 24) {
            Text("Pley")
                .font(.largeTitle.bold())
            Text("Version \(version)")
            Text(UserAgent.current)
                .font(.callout.monospaced())
                .foregroundStyle(.secondary)
        }
        .multilineTextAlignment(.center)
        .padding()
    }
}
