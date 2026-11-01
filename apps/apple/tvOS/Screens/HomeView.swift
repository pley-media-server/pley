import PleyModel
import PleyScreens
import SwiftUI

/// Home: the server's rows for the profile signed in, each a line of tiles
/// in its shape, moved through with the focus engine one row at a time.
/// Above them, what the tile with the focus is: its title, its facts, and
/// once the focus rests, its summary. It reads its rows again when the
/// server says something on them changed.
struct HomeView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: HomeModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: HomeModel(app: app))
    }

    var body: some View {
        content
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
            VStack(alignment: .leading, spacing: 0) {
                InfoBlock(info: model.info)
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                        ForEach(rows) { row in
                            TileRow(app: app, key: row.key, title: row.title, shape: row.shape, tiles: row.tiles,
                                    seeAll: row.seeAll.map { screen in { app.shell.open(screen) } },
                                    remove: row.continueWatching ? { tile in
                                        await model.removeFromContinueWatching(tile.item)
                                    } : nil,
                                    focused: { model.focus($0) }) { tile in
                                app.shell.open(item: tile.item)
                            }
                        }
                    }
                    .padding(.bottom, 40)
                }
                .accessibilityIdentifier("home.rows")
            }
        }
    }
}

/// What the tile with the focus is, above the rows, at a height that
/// doesn't change: the title (a show's, for an episode), the episode, the
/// facts in a line, the summary (three lines) and the time left. VoiceOver
/// reads the tile itself, so this is hidden from it.
private struct InfoBlock: View {
    let info: HomeInfo?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let info {
                Text(info.title)
                    .font(.title2.bold())
                    .lineLimit(1)
                if !info.context.isEmpty {
                    Text(info.context)
                        .font(.headline)
                        .lineLimit(1)
                }
                Text(([info.timeLeft] + info.meta).filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Text(info.summary)
                    .font(.callout)
                    .lineLimit(3)
                    .frame(maxWidth: 1100, alignment: .leading)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 230, maxHeight: 230, alignment: .topLeading)
        .padding(.horizontal, Metrics.gutter)
        .padding(.top, 20)
        .accessibilityHidden(true)
        .accessibilityIdentifier("home.info")
    }
}
