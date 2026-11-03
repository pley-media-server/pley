import PleyModel
import PleyScreens
import SwiftUI

/// The libraries, in the profile's order: the Libraries tab when there
/// are more than four. A long press on Select opens Move: up and down then
/// move the library, and Select puts it down; the order is saved as it
/// changes, and the tabs, Home and the profile's other devices follow.
struct LibrariesView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: LibrariesModel
    /// The library being moved.
    @State private var moving: Int?

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: LibrariesModel(app: app))
    }

    var body: some View {
        content
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
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 24) {
                    if let notice = model.notice {
                        Text(asSentence(notice)).font(.callout).foregroundStyle(.red)
                    }
                    ForEach(model.entries) { entry in
                        row(entry)
                    }
                }
                .frame(maxWidth: 1100)
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 40)
                .frame(maxWidth: .infinity)
            }
            .accessibilityIdentifier("libraries.list")
        }
    }

    private func row(_ entry: LibraryEntry) -> some View {
        let isMoving = moving == entry.id
        return Button {
            if isMoving { moving = nil } else { app.shell.open(.library(entry.id)) }
        } label: {
            HStack(spacing: 24) {
                Image(systemName: isMoving ? "arrow.up.arrow.down" : entry.symbol)
                    .font(.title2)
                    .frame(width: 60)
                VStack(alignment: .leading, spacing: 4) {
                    Text(entry.name).font(.headline)
                    Text(isMoving ? "Move up or down, then press Select" : entry.count)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                if entry.scanning {
                    ProgressView().accessibilityHidden(true)
                }
            }
            .padding(.vertical, 8)
        }
        // While one moves, the others can't take the focus: up and down move it.
        .disabled(moving != nil && !isMoving)
        .onMoveCommand { direction in
            guard isMoving else { return }
            switch direction {
            case .up: model.move(entry.id, by: -1)
            case .down: model.move(entry.id, by: 1)
            default: break
            }
        }
        .contextMenu {
            if model.reorderable {
                Button("Move") { moving = entry.id }
                    .accessibilityIdentifier("libraries.move")
                Button("Use the Server's Order") { Task { await model.useServerOrder() } }
                    .accessibilityIdentifier("libraries.serverOrder")
            }
        }
        .accessibilityLabel(entry.spoken)
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
}
