import PleyModel
import PleyScreens
import SwiftUI

/// Listen, Apple TV's tab in place of Playlists (`ListenModel`, as on
/// iPhone): Recently Played, the playlists after New Playlist (a video
/// playlist marked Video), Artists with See all, and the music libraries,
/// each a row the focus moves down through, and each left out while it
/// has nothing. Music libraries have no tabs of their own: they are here.
struct ListenView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ListenModel
    @ObservedObject private var playlists: PlaylistsModel

    init(app: AppModel) {
        self.app = app
        let model = ListenModel(app: app)
        _model = StateObject(wrappedValue: model)
        _playlists = ObservedObject(wrappedValue: model.playlists)
    }

    /// A playlist's and a music library's width: the square tiles'.
    private var width: CGFloat { Metrics.tileWidth(.square) }

    var body: some View {
        content
            // Read again each time it shows: plays and playlists change from other pages.
            .task(id: app.shell.libraries.map(\.id)) { await model.load() }
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
                    if !model.libraries.isEmpty { libraryRow }
                }
                .padding(.vertical, 40)
            }
            .scrollClipDisabled()
            .accessibilityIdentifier("listen.page")
        }
    }

    /// The playlists, New Playlist first, in a row.
    private var playlistRow: some View {
        VStack(alignment: .leading, spacing: 0) {
            heading("Playlists")
            if let notice = playlists.notice {
                Text(asSentence(notice)).font(.callout).foregroundStyle(.red)
                    .padding(.horizontal, Metrics.gutter)
            }
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    Button { playlists.new() } label: {
                        VStack(spacing: 12) {
                            Image(systemName: "plus").font(.title2)
                            Text("New Playlist").font(.callout).multilineTextAlignment(.center)
                        }
                        .frame(width: width, height: width)
                    }
                    .buttonStyle(.card)
                    .accessibilityIdentifier("listen.newPlaylist")
                    ForEach(playlists.cards) { card in
                        PlaylistTile(app: app, card: card, width: width, marksVideo: true) {
                            app.shell.open(.playlist(card.id))
                        }
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 20)
            }
            .scrollClipDisabled()
        }
        .focusSection()
        .accessibilityIdentifier("listen.playlists")
    }

    /// The music libraries, each a card opening its page.
    private var libraryRow: some View {
        VStack(alignment: .leading, spacing: 0) {
            heading("Music Libraries")
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(model.libraries) { library in
                        VStack(alignment: .leading, spacing: 16) {
                            Button {
                                app.shell.open(.library(library.id))
                            } label: {
                                Image(systemName: "music.note")
                                    .font(.system(size: 72))
                                    .foregroundStyle(.white)
                                    .frame(width: width, height: width)
                                    .background(Color.pink)
                            }
                            .buttonStyle(.card)
                            .accessibilityLabel(library.name)
                            .accessibilityValue(Words.libraryCount(library))
                            .accessibilityIdentifier("listen.library.\(library.id)")
                            VStack(alignment: .leading, spacing: 2) {
                                Text(library.name).font(.callout).lineLimit(1)
                                Text(Words.libraryCount(library)).font(.caption).foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                            .accessibilityHidden(true)
                        }
                        .frame(width: width, alignment: .leading)
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 20)
            }
            .scrollClipDisabled()
        }
        .focusSection()
    }

    private func heading(_ title: LocalizedStringKey) -> some View {
        Text(title)
            .font(.headline)
            .accessibilityAddTraits(.isHeader)
            .padding(.horizontal, Metrics.gutter)
    }
}
