import PleyModel
import PleyScreens
import SwiftUI

/// Home: the banner of featured titles (`HomeHero`), then the server's
/// rows for the profile signed in, each a line of tiles in its shape
/// (stills for Continue Watching, posters, squares for music), with See all
/// for the rows a library can list. On a phone the banner runs up under
/// the status bar, with no navigation bar over it. It reads its rows again
/// when the server says something on them changed.
struct HomeView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: HomeModel
    /// The banner's title showing, kept while the banner is scrolled away.
    @State private var heroIndex = 0
    private let layout = WideLayout()

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
                    .accessibilityIdentifier("home.reload")
                }
            }
            #endif
            #if os(iOS)
            // The banner is the top of a phone's Home, under the status bar.
            .toolbar(showsHero && !layout.wide ? .hidden : .automatic, for: .navigationBar)
            #endif
            .task(id: app.homeVersion) { await model.reload() }
            .reloads(app) { await model.reload() }
            .onValueChange(of: model.featured.map(\.id)) { _ in heroIndex = 0 }
    }

    private var showsHero: Bool {
        if case .loaded = model.state { return !model.featured.isEmpty }
        return false
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: HomeModel.failedTitle, why: why) { Task { await model.reload() } })
        case .noLibraries:
            StateView(kind: .empty(symbol: "square.stack",
                                   text: localized("""
                                       \(HomeModel.noLibrariesTitle). Add a library on the server, \
                                       or ask its owner for one.
                                       """),
                                   action: nil))
        case .empty(let scanning):
            StateView(kind: .empty(symbol: scanning ? "arrow.triangle.2.circlepath" : "film.stack",
                                   text: scanning ? HomeModel.scanningTitle
                                       : localized("""
                                           \(HomeModel.emptyTitle). Titles show here once your libraries have some.
                                           """),
                                   action: (localized("Reload"), { Task { await model.reload() } })))
        case .loaded(let rows):
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                    if !model.featured.isEmpty {
                        HomeHero(app: app, titles: model.featured, index: $heroIndex)
                            // Close up to the first row: the picture fades into the page.
                            .padding(.bottom, -Metrics.rowSpacing / 2)
                    }
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
                .padding(.vertical, showsHero && !layout.wide ? 0 : nil)
                .padding(.bottom)
            }
            #if os(iOS)
            .ignoresSafeArea(.container, edges: showsHero && !layout.wide ? .top : [])
            #endif
            .refreshable { await model.reload() }
            .accessibilityIdentifier("home.rows")
        }
    }
}
