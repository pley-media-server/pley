import PleyModel
import PleyScreens
import SwiftUI

/// An artist's page for the sofa (docs/APPLE.md, Music): their picture,
/// "4 albums · 40 tracks", what the server says of them, and their albums,
/// newest first. Music plays from a later version, so there is no Play.
struct ArtistView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ArtistModel

    init(app: AppModel, itemId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: ArtistModel(app: app, itemId: itemId))
    }

    var body: some View {
        content.task { await model.load() }
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
        ScrollView {
            VStack(alignment: .leading, spacing: 40) {
                HStack(alignment: .center, spacing: 48) {
                    ServerImage(app: app, paths: page.posters, width: 300, symbol: "music.mic")
                        .frame(width: 300, height: 300)
                        .clipShape(Circle())
                    VStack(alignment: .leading, spacing: 12) {
                        Text(page.title)
                            .font(.system(size: 64, weight: .bold))
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("artist.name")
                        if !page.counts.isEmpty {
                            Text(page.counts).foregroundStyle(.secondary)
                        }
                        if !page.genres.isEmpty {
                            Text(page.genres.joined(separator: ", ")).font(.callout).foregroundStyle(.secondary)
                        }
                        if let summary = page.summary {
                            Text(summary).font(.callout).lineLimit(4).frame(maxWidth: 1100, alignment: .leading)
                        }
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                if page.albumsLoading {
                    ProgressView().padding(.horizontal, Metrics.gutter)
                } else if page.albums.isEmpty {
                    Text("No albums yet.").foregroundStyle(.secondary).padding(.horizontal, Metrics.gutter)
                } else {
                    TileRow(app: app, key: "albums", title: "Albums", shape: .square, tiles: page.albums,
                            prefix: "artist.row") { tile in
                        app.shell.open(Screen.page(of: tile.item))
                    }
                }
            }
            .padding(.vertical, 60)
        }
        .scrollClipDisabled()
    }
}

/// An album's page for the sofa (docs/APPLE.md, Music): its cover beside
/// its title, its artist (a button to their page), the year, "10 tracks ·
/// 42m" and the profile's rating, then its tracks, with disc headings on
/// albums of several discs. Opened from a track, that track has the focus.
/// No Play: music plays from a later version.
struct AlbumView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: AlbumModel
    @FocusState private var focusedTrack: Int?

    init(app: AppModel, itemId: Int, track: Int?) {
        self.app = app
        _model = StateObject(wrappedValue: AlbumModel(app: app, itemId: itemId, track: track))
    }

    var body: some View {
        content.task { await model.load() }
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
        HStack(alignment: .top, spacing: 80) {
            VStack(alignment: .leading, spacing: 20) {
                ServerImage(app: app, paths: page.posters, width: 500, symbol: "music.note")
                    .frame(width: 500, height: 500)
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                Text(page.title)
                    .font(.title2.bold())
                    .lineLimit(3)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("album.title")
                let line = [page.year.map(String.init), page.meta.isEmpty ? nil : page.meta].compactMap { $0 }
                if !line.isEmpty {
                    Text(line.joined(separator: " · ")).font(.callout).foregroundStyle(.secondary)
                }
                HStack(spacing: 24) {
                    if let artist = page.artist, let id = page.artistId {
                        Button(artist) { app.shell.open(.artist(id)) }
                            .accessibilityHint("Opens the artist")
                            .accessibilityIdentifier("album.artist")
                    }
                    rating(page.userRating)
                }
                .focusSection()
            }
            .frame(width: 500)
            ScrollViewReader { reader in
                ScrollView {
                    tracks(page)
                        .padding(.vertical, 40)
                }
                .scrollClipDisabled()
                .task(id: page.rows.count) {
                    guard let highlight = page.highlight, !page.rows.isEmpty else { return }
                    reader.scrollTo("track-\(highlight)", anchor: .center)
                    focusedTrack = highlight
                }
            }
            .focusSection()
        }
        .padding(.horizontal, Metrics.gutter)
        .padding(.vertical, 60)
    }

    /// The profile's rating, picked from a list of half stars.
    private func rating(_ rating: Int?) -> some View {
        ChoiceButton(title: "Your rating", value: rating.map(StarWords.stars) ?? "Not rated",
                     symbol: rating == nil ? "star" : "star.fill", text: rating.map(StarWords.stars) ?? "Rate",
                     choices: [Choice(label: "Not rated", tag: nil)]
                         + (1...10).reversed().map { Choice(label: StarWords.stars($0), tag: $0) },
                     selected: rating) { value in Task { await model.rate(value) } }
            .accessibilityIdentifier("item.rating")
    }

    @ViewBuilder private func tracks(_ page: AlbumPage) -> some View {
        if page.tracksLoading {
            ProgressView()
        } else if page.rows.isEmpty {
            Text("No tracks yet.").foregroundStyle(.secondary)
        } else {
            LazyVStack(alignment: .leading, spacing: 8) {
                ForEach(page.rows) { row in
                    switch row {
                    case .disc(let disc):
                        Text("Disc \(disc)")
                            .font(.headline)
                            .padding(.top, 20)
                            .accessibilityAddTraits(.isHeader)
                            .id(row.id)
                    case .track(let track):
                        TrackLine(track: track, highlighted: track.id == page.highlight)
                            .focused($focusedTrack, equals: track.id)
                            .id(row.id)
                    }
                }
            }
            .accessibilityIdentifier("album.tracks")
        }
    }
}

/// A track of an album: number, title, its own artist and its length. It
/// takes the focus (so the remote scrolls the list) but does nothing yet.
private struct TrackLine: View {
    let track: TrackRow
    let highlighted: Bool

    var body: some View {
        TrackContent(track: track, highlighted: highlighted)
            .focusable()
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(track.spoken)
            .accessibilityAddTraits(highlighted ? .isSelected : [])
            .accessibilityIdentifier("album.track.\(track.id)")
    }
}

/// A track's line, lit while it has the focus.
private struct TrackContent: View {
    let track: TrackRow
    let highlighted: Bool
    @Environment(\.isFocused) private var focused

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 24) {
            Text(track.number).monospacedDigit().foregroundStyle(.secondary).frame(minWidth: 50, alignment: .trailing)
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                if let artist = track.artist {
                    Text(artist).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 20)
            Text(track.duration).monospacedDigit().foregroundStyle(.secondary)
        }
        .font(.callout)
        .padding(.vertical, 14)
        .padding(.horizontal, 24)
        .background(fill, in: RoundedRectangle(cornerRadius: 10))
    }

    private var fill: Color {
        if focused { return .white.opacity(0.25) }
        return highlighted ? .white.opacity(0.12) : .clear
    }
}
