import PleyModel
import PleyScreens
import SwiftUI

/// Listen, the iPhone's tab in place of Playlists (`ListenModel`): Recently
/// Played, the playlists with New Playlist first (a video playlist marked
/// Video), Artists, and the music libraries, each part left out while it
/// has nothing. The iPad's and the Mac's sidebar keep Playlists and the
/// music libraries as rows of their own.
struct ListenView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ListenModel
    @ObservedObject private var playlists: PlaylistsModel
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1

    init(app: AppModel) {
        self.app = app
        let model = ListenModel(app: app)
        _model = StateObject(wrappedValue: model)
        _playlists = ObservedObject(wrappedValue: model.playlists)
    }

    private var tileWidth: CGFloat { 108 * min(max(typeScale, 1), Metrics.maxTileScale) }

    var body: some View {
        content
            .navigationTitle("Listen")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button { playlists.new() } label: {
                        Label("New Playlist", systemImage: "plus")
                    }
                    .accessibilityIdentifier("playlists.new")
                }
            }
            // Read again each time it shows: plays and playlists change from other pages.
            .task(id: app.shell.libraries.map(\.id)) { await model.load() }
            .reloads(app) { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: ListenModel.failedTitle, why: why) { Task { await model.load() } })
        case .loaded where model.isEmpty:
            StateView(kind: .empty(symbol: "headphones", text: ListenModel.emptyTitle,
                                   action: (localized("New Playlist"), { playlists.new() })))
        case .loaded:
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                    if !model.recent.isEmpty {
                        TileRow(app: app, key: "recently_played", title: localized("Recently Played"), shape: .square,
                                tiles: model.recent, prefix: "listen.row") { app.shell.open(item: $0.item) }
                    }
                    playlistRow
                    if !model.artists.isEmpty {
                        TileRow(app: app, key: "artists", title: localized("Artists"), shape: .square,
                                tiles: model.artists,
                                seeAll: model.libraries.first.map { library in { app.shell.open(.library(library.id)) } },
                                prefix: "listen.row") { app.shell.open(item: $0.item) }
                    }
                    if !model.libraries.isEmpty { libraryList }
                }
                .padding(.vertical)
            }
            .refreshable { await model.load() }
            .accessibilityIdentifier("listen.page")
        }
    }

    /// The playlists, New Playlist first, in a row that scrolls sideways.
    private var playlistRow: some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        return VStack(alignment: .leading, spacing: 10) {
            Text("Playlists")
                .font(.title2.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, gutter)
            if let notice = playlists.notice {
                Text(asSentence(notice)).font(.footnote).foregroundStyle(.red).padding(.horizontal, gutter)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    Button { playlists.new() } label: {
                        VStack(spacing: 6) {
                            Image(systemName: "plus").font(.title2)
                            Text("New Playlist").font(.footnote.weight(.semibold)).multilineTextAlignment(.center)
                        }
                        .foregroundStyle(.secondary)
                        .frame(width: tileWidth, height: tileWidth)
                        .overlay {
                            RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous)
                                .strokeBorder(.secondary, style: StrokeStyle(lineWidth: 1.5, dash: [5, 4]))
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("listen.newPlaylist")
                    ForEach(playlists.cards) { card in
                        PlaylistTile(app: app, card: card, width: tileWidth, marksVideo: true) {
                            app.shell.open(.playlist(card.id))
                        }
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("listen.playlists")
    }

    /// The music libraries, each opening its own page.
    private var libraryList: some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        return VStack(alignment: .leading, spacing: 10) {
            Text("Music Libraries")
                .font(.title2.bold())
                .accessibilityAddTraits(.isHeader)
            ForEach(model.libraries) { library in
                Button {
                    app.shell.open(.library(library.id))
                } label: {
                    HStack(spacing: 14) {
                        Image(systemName: "music.note")
                            .font(.title3)
                            .foregroundStyle(.white)
                            .frame(width: 42, height: 42)
                            .background(Color.pink, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        VStack(alignment: .leading, spacing: 1) {
                            Text(library.name).font(.body.weight(.semibold)).foregroundStyle(.primary)
                            Text(Words.libraryCount(library)).font(.subheadline).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                        Image(systemName: "chevron.right").foregroundStyle(.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("listen.library.\(library.id)")
            }
        }
        .padding(.horizontal, gutter)
    }
}
