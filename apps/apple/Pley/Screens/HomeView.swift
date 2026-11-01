import PleyModel
import PleyScreens
import SwiftUI

/// Home: the server's rows for the profile signed in, each a line of tiles
/// in its shape (stills for Continue Watching, posters, squares for music),
/// with See all for the rows a library can list. It reads its rows again
/// when the server says something on them changed.
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
        case .noLibraries:
            StateView(kind: .empty(symbol: "square.stack",
                                   text: "\(HomeModel.noLibrariesTitle). Add a library on the server, "
                                       + "or ask its owner for one.",
                                   action: nil))
        case .empty(let scanning):
            StateView(kind: .empty(symbol: scanning ? "arrow.triangle.2.circlepath" : "film.stack",
                                   text: scanning ? HomeModel.scanningTitle
                                       : "\(HomeModel.emptyTitle). Titles show here once your libraries have some.",
                                   action: ("Reload", { Task { await model.reload() } })))
        case .loaded(let rows):
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                    ForEach(rows) { row in
                        TileRow(app: app, key: row.key, title: row.title, shape: row.shape, tiles: row.tiles,
                                seeAll: row.seeAll.map { screen in { app.shell.open(screen) } },
                                remove: row.continueWatching ? { tile in
                                    await model.removeFromContinueWatching(tile.item)
                                } : nil) { tile in
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
