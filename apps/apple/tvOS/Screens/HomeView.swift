import PleyData
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
            // Behind the page, not beside it in a stack: the backdrop fills the
            // screen without moving the page out from under the tab bar.
            page(rows)
                // Out to the screen's sides, as Discover's page is: the words and the
                // first tiles line up with Discover's, and the rows scroll to the edges.
                .padding(.horizontal, Self.sideInset)
                .ignoresSafeArea(.container, edges: .horizontal)
                .background { HomeBackdrop(app: app, paths: model.info?.backdrops ?? []) }
        }
    }

    /// Inside the screen's sides, past which the margin (`Metrics.gutter`) starts.
    private static let sideInset: CGFloat = 40

    private func page(_ rows: [HomeRow]) -> some View {
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

/// The backdrop of the tile with the focus, behind the whole of Home,
/// darkened towards the info block and the rows. It changes once the
/// focus has rested on a tile, so moving along a row doesn't flash every
/// picture on the way, and fades from one to the next (at once under
/// Reduce Motion). Until a tile has the focus, and for one with no
/// backdrop, it is plain.
private struct HomeBackdrop: View {
    let app: AppModel
    let paths: [String]
    @State private var rested: [String] = []
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                if !rested.isEmpty {
                    ServerImage(app: app, paths: rested, width: geometry.size.width, symbol: nil, placeholder: false,
                                limit: ImageLoader.deviceBackdropLimit)
                        .id(rested)
                        .frame(width: geometry.size.width, height: geometry.size.height)
                        .clipped()
                        .transition(.opacity)
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            // Darkened as Discover's backdrop is, towards the words on the left
            // and the rows below; the words' shadow keeps them clear of bright parts.
            .overlay {
                LinearGradient(colors: [.black.opacity(0.85), .black.opacity(0.35), .clear],
                               startPoint: .leading, endPoint: .trailing)
            }
            .overlay {
                LinearGradient(colors: [.clear, .black.opacity(0.95)], startPoint: .center, endPoint: .bottom)
            }
        }
        .animation(Motion.animation(.easeInOut(duration: 0.4), reduced: reduceMotion), value: rested)
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .task(id: paths) {
            guard paths != rested else { return }
            try? await Task.sleep(for: .seconds(HomeModel.infoDelay))
            guard !Task.isCancelled else { return }
            rested = paths
        }
    }
}

/// What the tile with the focus is, above the rows, at a height that
/// doesn't change: the title (a show's, for an episode), the episode, the
/// facts in a line, the summary (three lines) and the time left. VoiceOver
/// reads the tile itself, so this is hidden from it.
private struct InfoBlock: View {
    let info: HomeInfo?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // One on top of the other while the next fades in.
        ZStack(alignment: .topLeading) {
            if let info {
                facts(info)
                    // Another tile's facts fade in, at once under Reduce Motion.
                    .id(info.itemId)
                    .transition(.opacity)
            }
        }
        .animation(Motion.animation(.easeInOut(duration: 0.2), reduced: reduceMotion), value: info?.itemId)
        .frame(maxWidth: .infinity, minHeight: 230, maxHeight: 230, alignment: .topLeading)
        .padding(.horizontal, Metrics.gutter)
        .padding(.top, 20)
        .accessibilityHidden(true)
        .accessibilityIdentifier("home.info")
    }

    private func facts(_ info: HomeInfo) -> some View {
        VStack(alignment: .leading, spacing: 8) {
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
        // Readable over the bright parts of a backdrop the scrim doesn't darken enough.
        .shadow(color: .black.opacity(0.6), radius: 8)
    }
}
