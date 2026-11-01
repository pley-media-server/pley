import PleyScreens
import SwiftUI

/// A screen with nothing to show yet, or nothing at all:
///
/// - Loading: still skeleton rows (they don't pulse), and a spinner once a
///   second has gone by.
/// - Failed: what went wrong in a sentence, and Try again.
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
        switch kind {
        case .loading:
            ZStack {
                skeleton
                if slow {
                    ProgressView().controlSize(.large)
                }
            }
            .task {
                try? await Task.sleep(for: .seconds(1))
                slow = true
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Loading")
        case .failed(let title, let why, let retry):
            VStack(spacing: 12) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.largeTitle)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(title).font(.headline)
                Text(asSentence(why))
                    .foregroundStyle(.secondary)
                Button("Try again", action: retry)
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("state.retry")
            }
            .multilineTextAlignment(.center)
            .padding()
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .empty(let symbol, let text, let action):
            VStack(spacing: 12) {
                Image(systemName: symbol)
                    .font(.largeTitle)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(text)
                    .foregroundStyle(.secondary)
                if let action {
                    Button(action.label, action: action.run)
                }
            }
            .multilineTextAlignment(.center)
            .padding()
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .accessibilityIdentifier("state.empty")
        }
    }

    /// Grey rows of grey tiles where the content will be.
    private var skeleton: some View {
        VStack(alignment: .leading, spacing: Metrics.rowSpacing) {
            ForEach(0..<3, id: \.self) { _ in
                VStack(alignment: .leading, spacing: 10) {
                    RoundedRectangle(cornerRadius: 4).fill(.quaternary).frame(width: 160, height: 18)
                    HStack(spacing: Metrics.tileSpacing) {
                        ForEach(0..<6, id: \.self) { _ in
                            PlaceholderTile(shape: .poster, width: 110)
                        }
                    }
                }
            }
            Spacer(minLength: 0)
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
    }
}
