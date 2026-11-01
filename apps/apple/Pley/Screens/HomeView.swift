import PleyModel
import PleyScreens
import SwiftUI

/// Home: the server's rows for the profile signed in, each a line of tiles
/// in its shape (stills for Continue Watching, posters, squares for music).
struct HomeView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: HomeModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: HomeModel(app: app))
    }

    var body: some View {
        content
            .navigationTitle("Home")
            #if os(macOS)
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        Task { await model.reload() }
                    } label: {
                        Label("Reload", systemImage: "arrow.clockwise")
                    }
                    .keyboardShortcut("r")
                    .accessibilityIdentifier("home.reload")
                }
            }
            #endif
            .task(id: app.homeVersion) { await model.reload() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: HomeModel.failedTitle, why: why) { Task { await model.reload() } })
        case .loaded(let rows) where rows.isEmpty:
            StateView(kind: .empty(symbol: "film.stack",
                                   text: "\(HomeModel.emptyTitle). Add a library on the server, "
                                       + "or ask its owner for one.",
                                   action: nil))
        case .loaded(let rows):
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                    ForEach(rows) { row in
                        TileRow(app: app, key: row.key, title: row.title, shape: row.shape, tiles: row.tiles) { tile in
                            app.shell.open(item: tile.item)
                        }
                    }
                }
                .padding(.vertical)
            }
            .refreshable { await model.reload() }
            .accessibilityIdentifier("home.rows")
        }
    }
}
