import PleyPlayer
import PleyScreens
import SwiftUI

/// Find Subtitles…, a sheet over the player on iPhone, iPad and Mac: the
/// language (the profile's at first), then what the server found online,
/// best first, each with its language, Hearing impaired, Forced, Made for
/// this file and its downloads. Choosing one adds it to the title and the
/// player shows it; the sheet closes. The playback goes on meanwhile.
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
                    .accessibilityIdentifier("subtitleSearch.language")
                }
                results
            }
            .navigationTitle("Find Subtitles")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .accessibilityIdentifier("subtitleSearch.cancel")
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 460, idealWidth: 520, minHeight: 420, idealHeight: 520)
        #endif
        .onAppear { model.search() }
        .onDisappear { model.close() }
        .onValueChange(of: model.added) { added in
            if added { dismiss() }
        }
    }

    @ViewBuilder private var results: some View {
        switch model.state {
        case .searching:
            Section {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Searching…").foregroundStyle(.secondary)
                }
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
                    Text(error)
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("subtitleSearch.addError")
                }
            }
        case .none(let message):
            Section { self.message(message) }
        case .notSetUp(let message, let hint):
            Section {
                VStack(alignment: .leading, spacing: 6) {
                    self.message(message)
                    if let hint { Text(hint).font(.callout).foregroundStyle(.secondary) }
                }
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

    private func message(_ text: String) -> some View {
        Text(text).accessibilityIdentifier("subtitleSearch.message")
    }

    private func label(_ row: FoundSubtitleRow) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(row.title)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                Text(row.detail)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                if let downloads = row.downloads {
                    Text(downloads)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
            Spacer(minLength: 0)
            if model.adding == row.id { ProgressView() }
        }
        .contentShape(Rectangle())
    }
}

#if os(macOS)
/// Find Subtitles… in the Playback menu: the key player window opens its search.
struct FindSubtitlesKey: FocusedValueKey {
    typealias Value = FindSubtitlesAction
}

struct FindSubtitlesAction {
    let run: @MainActor () -> Void
}

extension FocusedValues {
    var findSubtitles: FindSubtitlesAction? {
        get { self[FindSubtitlesKey.self] }
        set { self[FindSubtitlesKey.self] = newValue }
    }
}
#endif
