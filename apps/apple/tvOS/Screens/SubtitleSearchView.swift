import PleyPlayer
import PleyScreens
import SwiftUI

/// Find Subtitles… on Apple TV, over the player: the language (the
/// profile's at first), then what the server found online, best first,
/// each with its language, Hearing impaired, Forced, Made for this file and
/// its downloads. Choosing one adds it to the title and the player shows
/// it; the list closes. Menu goes back to the player.
struct SubtitleSearchView: View {
    @StateObject var model: SubtitleSearchModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Picker("Language", selection: Binding(get: { model.pickedLanguage },
                                                          set: { model.choose(language: $0) })) {
                        ForEach(model.languages, id: \.code) { language in
                            Text(language.name).tag(language.code)
                        }
                    }
                    .pickerStyle(.navigationLink)
                    .accessibilityIdentifier("subtitleSearch.language")
                }
                results
            }
            .navigationTitle("Find Subtitles")
        }
        // Over the video, which goes on behind: the list stays readable.
        .background(.thickMaterial, ignoresSafeAreaEdges: .all)
        .onAppear { model.search() }
        .onDisappear { model.close() }
        .onChange(of: model.added) { _, added in
            if added { dismiss() }
        }
    }

    @ViewBuilder private var results: some View {
        switch model.state {
        case .searching:
            Section {
                HStack(spacing: 20) {
                    ProgressView()
                    Text("Searching…").foregroundStyle(.secondary)
                }
                .focusable()
            }
        case .found(let rows):
            Section {
                ForEach(rows) { row in
                    Button { model.add(row) } label: { label(row) }
                        .disabled(model.adding != nil)
                        .accessibilityLabel(row.spokenLabel)
                        .accessibilityIdentifier("subtitleSearch.result")
                }
            } footer: {
                if let error = model.addError {
                    Text(error).accessibilityIdentifier("subtitleSearch.addError")
                }
            }
        case .none(let message):
            Section { self.message(message) }
        case .notSetUp(let message, let hint):
            Section {
                self.message(message, hint: hint)
            }
        case .failed(let text, let canRetry):
            Section {
                message(text)
                if canRetry {
                    Button("Try Again") { model.search() }
                        .accessibilityIdentifier("subtitleSearch.retry")
                }
            }
        }
    }

    /// A message the remote can reach, so VoiceOver reads it.
    private func message(_ text: String, hint: String? = nil) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(text).accessibilityIdentifier("subtitleSearch.message")
            if let hint { Text(hint).font(.callout).foregroundStyle(.secondary) }
        }
        .focusable()
    }

    private func label(_ row: FoundSubtitleRow) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 24) {
            VStack(alignment: .leading, spacing: 6) {
                Text(row.title).lineLimit(2)
                Text(row.detail)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                if let downloads = row.downloads {
                    Text(downloads)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
            Spacer(minLength: 0)
            if model.adding == row.id { ProgressView() }
        }
    }
}
