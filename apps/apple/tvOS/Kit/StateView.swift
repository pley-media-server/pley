import PleyScreens
import SwiftUI

/// A screen with nothing to show yet, or nothing at all:
///
/// - Loading: a spinner once a second has gone by.
/// - Failed: what went wrong in a sentence, and Try again, which takes focus.
/// - Empty: a symbol, a sentence and an action when there is one.
struct StateView: View {
    enum Kind {
        case loading
        case failed(title: String, why: String, retry: () -> Void)
        case empty(symbol: String, text: String, action: (label: String, run: () -> Void)?)
    }

    let kind: Kind
    @State private var slow = false

    var body: some View {
        Group {
            switch kind {
            case .loading:
                ProgressView()
                    .opacity(slow ? 1 : 0)
                    .task {
                        try? await Task.sleep(for: .seconds(1))
                        slow = true
                    }
                    .accessibilityLabel("Loading")
            case .failed(let title, let why, let retry):
                VStack(spacing: 24) {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.system(size: 60))
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    Text(title).font(.title3.bold())
                    Text(asSentence(why)).foregroundStyle(.secondary)
                    Button("Try again", action: retry)
                        .accessibilityIdentifier("state.retry")
                }
                .focusSection()
            case .empty(let symbol, let text, let action):
                VStack(spacing: 24) {
                    Image(systemName: symbol)
                        .font(.system(size: 60))
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    Text(text).foregroundStyle(.secondary)
                    if let action {
                        Button(action.label, action: action.run)
                    }
                }
                .accessibilityIdentifier("state.empty")
            }
        }
        .multilineTextAlignment(.center)
        .frame(maxWidth: 1000)
        .frame(maxWidth: .infinity, minHeight: 400)
        .padding(Metrics.gutter)
    }
}
