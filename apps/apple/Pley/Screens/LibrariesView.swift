import PleyModel
import PleyScreens
import SwiftUI

/// The libraries this profile may open, in its order, each with what it
/// holds and whether the server is scanning it. Edit (or VoiceOver's Move
/// up and Move down) puts them in another order, which Home, the sidebar
/// and the profile's other devices follow. The iPhone's Libraries tab,
/// and the iPad's in a narrow window; the sidebar lists them otherwise.
struct LibrariesView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: LibrariesModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: LibrariesModel(app: app))
    }

    var body: some View {
        content
            .navigationTitle("Libraries")
            .toolbar {
                if model.reorderable {
                    #if os(iOS)
                    ToolbarItem(placement: .primaryAction) {
                        EditButton()
                            .accessibilityIdentifier("libraries.edit")
                    }
                    #endif
                    ToolbarItem(placement: .secondaryAction) {
                        Button("Use the Server's Order") { Task { await model.useServerOrder() } }
                            .accessibilityIdentifier("libraries.serverOrder")
                    }
                }
            }
            .task { await model.reload() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: LibrariesModel.failedTitle, why: why) { Task { await model.reload() } })
        case .loaded where model.entries.isEmpty:
            StateView(kind: .empty(symbol: "square.stack",
                                   text: "\(LibrariesModel.emptyTitle). Add a library on the server, "
                                       + "or ask its owner for one.",
                                   action: nil))
        case .loaded:
            List {
                if let notice = model.notice {
                    Text(asSentence(notice)).font(.footnote).foregroundStyle(.red)
                }
                ForEach(model.entries) { entry in
                    NavigationLink(value: Screen.library(entry.id)) {
                        LibraryRow(entry: entry)
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(entry.spoken)
                    .accessibilityAddTraits(.isButton)
                    .accessibilityActions {
                        if model.entries.first?.id != entry.id {
                            Button("Move up") { model.move(entry.id, by: -1) }
                        }
                        if model.entries.last?.id != entry.id {
                            Button("Move down") { model.move(entry.id, by: 1) }
                        }
                    }
                    .accessibilityIdentifier("libraries.row.\(entry.id)")
                }
                .onMove(perform: model.reorderable ? { model.move(fromOffsets: $0, toOffset: $1) } : nil)
            }
            .refreshable { await model.reload() }
            .accessibilityIdentifier("libraries.list")
        }
    }
}

/// A library's row: its kind's symbol, its name, what it holds, and a
/// spinning mark while the server scans it.
struct LibraryRow: View {
    let entry: LibraryEntry

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: entry.symbol)
                .font(.title3)
                .foregroundStyle(Color.accentColor)
                .frame(width: 32)
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.name)
                Text(entry.count)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if entry.scanning {
                ProgressView()
                    .controlSize(.small)
                    .accessibilityHidden(true)
            }
        }
        .padding(.vertical, 4)
    }
}
