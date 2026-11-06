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
    @ObservedObject private var shell: ShellModel
    @StateObject private var model: LibrariesModel

    init(app: AppModel) {
        self.app = app
        shell = app.shell
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
            .reloads(app) { await model.reload() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: LibrariesModel.failedTitle, why: why) { Task { await model.reload() } })
        case .loaded where model.entries.isEmpty:
            StateView(kind: .empty(symbol: "square.stack",
                                   text: localized("""
                                       \(LibrariesModel.emptyTitle). Add a library on the server, \
                                       or ask its owner for one.
                                       """),
                                   action: nil))
        case .loaded:
            List {
                if let notice = model.notice {
                    Text(asSentence(notice)).font(.footnote).foregroundStyle(.red)
                }
                if shell.showsLiveTv {
                    // Live TV first, where the iPhone has no tab for it.
                    NavigationLink(value: Screen.liveTv) {
                        HStack(spacing: 12) {
                            Image(systemName: "tv").frame(width: 28).accessibilityHidden(true)
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Live TV")
                                Text("\(shell.liveTv?.channels ?? 0) channels")
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("libraries.liveTv")
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
                    .contextMenu { LibraryMoveButtons(model: model, entry: entry) }
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
/// Move Up and Move Down: a library's row's context menu (right-click on the Mac).
struct LibraryMoveButtons: View {
    @ObservedObject var model: LibrariesModel
    let entry: LibraryEntry

    var body: some View {
        if model.reorderable {
            Button {
                model.move(entry.id, by: -1)
            } label: {
                Label("Move Up", systemImage: "arrow.up")
            }
            .disabled(model.entries.first?.id == entry.id)
            Button {
                model.move(entry.id, by: 1)
            } label: {
                Label("Move Down", systemImage: "arrow.down")
            }
            .disabled(model.entries.last?.id == entry.id)
        }
    }
}

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
