import PleyModel
import PleyScreens
import SwiftUI

/// An artist's page (docs/APPLE.md, Music): their picture, "4 albums · 40
/// tracks", what the server says of them, and their albums, newest first.
/// Music plays from a later version, so there is no Play.
struct ArtistView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ArtistModel
    @Environment(\.dynamicTypeSize) private var typeSize
    private let layout = WideLayout()

    init(app: AppModel, itemId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: ArtistModel(app: app, itemId: itemId))
    }

    var body: some View {
        content
            .navigationTitle(model.page?.title ?? "")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .task { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .notFound:
            StateView(kind: .empty(symbol: "music.mic", text: ItemModel.notFoundTitle, action: nil))
        case .failed(let why):
            StateView(kind: .failed(title: "Couldn't load this artist", why: why) { Task { await model.load() } })
        case .loaded:
            if let page = model.page {
                loaded(page)
            } else {
                StateView(kind: .loading)
            }
        }
    }

    private func loaded(_ page: ArtistPage) -> some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        let size: CGFloat = layout.wide ? 200 : 140
        let stack = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12))
            : AnyLayout(HStackLayout(alignment: .bottom, spacing: 20))
        return ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                stack {
                    ServerImage(app: app, paths: page.posters, width: size, symbol: "music.mic")
                        .frame(width: size, height: size)
                        .clipShape(Circle())
                    VStack(alignment: .leading, spacing: 6) {
                        Text(page.title)
                            .font(.largeTitle.bold())
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("artist.name")
                        if !page.counts.isEmpty {
                            Text(page.counts).foregroundStyle(.secondary)
                        }
                        if !page.genres.isEmpty {
                            Text(page.genres.joined(separator: ", ")).font(.subheadline).foregroundStyle(.secondary)
                        }
                    }
                }
                if let summary = page.summary {
                    Text(summary).frame(maxWidth: 760, alignment: .leading)
                }
                Text("Albums").font(.title3.bold()).accessibilityAddTraits(.isHeader)
                if page.albumsLoading {
                    ProgressView().frame(maxWidth: .infinity)
                } else if page.albums.isEmpty {
                    Text("No albums yet.").foregroundStyle(.secondary)
                } else {
                    TileGrid(app: app, shape: .square, count: page.albums.count,
                             tile: { page.albums.indices.contains($0) ? page.albums[$0] : nil }) { tile in
                        app.shell.open(Screen.page(of: tile.item))
                    }
                    .accessibilityIdentifier("artist.albums")
                }
            }
            .padding(.horizontal, gutter)
            .padding(.vertical, 16)
        }
    }
}

/// An album's page (docs/APPLE.md, Music): its cover, its artist (a link
/// to their page), the year, "10 tracks · 42m", the profile's rating, and
/// its tracks, with disc headings on albums of several discs. Opened from
/// a track, that track is marked and scrolled to. No Play: music plays
/// from a later version.
struct AlbumView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: AlbumModel
    @Environment(\.dynamicTypeSize) private var typeSize
    private let layout = WideLayout()

    init(app: AppModel, itemId: Int, track: Int?) {
        self.app = app
        _model = StateObject(wrappedValue: AlbumModel(app: app, itemId: itemId, track: track))
    }

    var body: some View {
        content
            .navigationTitle(model.page?.title ?? "")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .task { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .notFound:
            StateView(kind: .empty(symbol: "music.note", text: ItemModel.notFoundTitle, action: nil))
        case .failed(let why):
            StateView(kind: .failed(title: "Couldn't load this album", why: why) { Task { await model.load() } })
        case .loaded:
            if let page = model.page {
                loaded(page)
            } else {
                StateView(kind: .loading)
            }
        }
    }

    private func loaded(_ page: AlbumPage) -> some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        let size: CGFloat = layout.wide ? 240 : 160
        let stack = typeSize.isAccessibilitySize || !layout.wide
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 14))
            : AnyLayout(HStackLayout(alignment: .bottom, spacing: 24))
        return ScrollViewReader { reader in
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    stack {
                        ServerImage(app: app, paths: page.posters, width: size, symbol: "music.note")
                            .frame(width: size, height: size)
                            .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                            .shadow(radius: 6, y: 2)
                        heading(page)
                    }
                    if let summary = page.summary {
                        Text(summary).frame(maxWidth: 760, alignment: .leading)
                    }
                    tracks(page)
                }
                .padding(.horizontal, gutter)
                .padding(.vertical, 16)
                .frame(maxWidth: 900, alignment: .leading)
            }
            .task(id: page.rows.count) {
                guard let highlight = page.highlight, !page.rows.isEmpty else { return }
                try? await Task.sleep(for: .milliseconds(150))
                reader.scrollTo("track-\(highlight)", anchor: .center)
            }
        }
    }

    private func heading(_ page: AlbumPage) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(page.title)
                .font(.title.bold())
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("album.title")
            if let artist = page.artist {
                if let id = page.artistId {
                    Button(artist) { app.shell.open(.artist(id)) }
                        .buttonStyle(.plain)
                        .font(.title3)
                        .foregroundStyle(Color.accentColor)
                        .accessibilityHint("Opens the artist")
                        .accessibilityIdentifier("album.artist")
                } else {
                    Text(artist).font(.title3)
                }
            }
            let line = [page.year.map(String.init), page.meta.isEmpty ? nil : page.meta].compactMap { $0 }
            if !line.isEmpty {
                Text(line.joined(separator: " · ")).foregroundStyle(.secondary)
            }
            if !page.genres.isEmpty {
                Text(page.genres.joined(separator: ", ")).font(.subheadline).foregroundStyle(.secondary)
            }
            StarRating(rating: page.userRating) { rating in Task { await model.rate(rating) } }
                .padding(.top, 4)
        }
    }

    @ViewBuilder private func tracks(_ page: AlbumPage) -> some View {
        if page.tracksLoading {
            ProgressView().frame(maxWidth: .infinity)
        } else if page.rows.isEmpty {
            Text("No tracks yet.").foregroundStyle(.secondary)
        } else {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(page.rows) { row in
                    switch row {
                    case .disc(let disc):
                        Text("Disc \(disc)")
                            .font(.headline)
                            .padding(.top, 14)
                            .padding(.bottom, 4)
                            .accessibilityAddTraits(.isHeader)
                            .id(row.id)
                    case .track(let track):
                        trackRow(track, highlighted: track.id == page.highlight)
                            .id(row.id)
                    }
                }
            }
            .accessibilityIdentifier("album.tracks")
        }
    }

    private func trackRow(_ track: TrackRow, highlighted: Bool) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(track.number)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(minWidth: 24, alignment: .trailing)
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                if let artist = track.artist {
                    Text(artist).font(.subheadline).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            Text(track.duration).monospacedDigit().foregroundStyle(.secondary)
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 8)
        .background(highlighted ? Color.accentColor.opacity(0.15) : .clear,
                    in: RoundedRectangle(cornerRadius: 6))
        .overlay(alignment: .bottom) { Divider().padding(.leading, 44) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(track.spoken)
        .accessibilityAddTraits(highlighted ? .isSelected : [])
        .accessibilityIdentifier("album.track.\(track.id)")
    }
}
