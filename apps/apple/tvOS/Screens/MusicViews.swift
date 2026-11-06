import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// An artist's page for the sofa (docs/APPLE.md, Music): their picture,
/// "4 albums · 40 tracks", what the server says of them, Play and Shuffle
/// (every album's tracks), and their albums, newest first.
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
            StateView(kind: .failed(title: localized("Couldn't load this artist"), why: why) {
                Task { await model.load() }
            })
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
                        if model.canPlay {
                            PlayShuffleButtons(prefix: "artist", canPlay: !model.starting, canShuffle: !model.starting,
                                               play: { Task { await model.play() } },
                                               shuffle: { Task { await model.play(shuffle: true) } })
                                .padding(.top, 10)
                        }
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                if page.albumsLoading {
                    ProgressView().padding(.horizontal, Metrics.gutter)
                } else if page.albums.isEmpty {
                    Text("No albums yet.").foregroundStyle(.secondary).padding(.horizontal, Metrics.gutter)
                } else {
                    TileRow(app: app, key: "albums", title: localized("Albums"), shape: .square, tiles: page.albums,
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
/// Play and Shuffle play the album; Select on a track plays the album from
/// it, or pauses and plays on while it is the one playing, which a speaker
/// marks.
struct AlbumView: View {
    @ObservedObject private var app: AppModel
    @ObservedObject private var music: MusicPlayback
    @StateObject private var model: AlbumModel
    @FocusState private var focusedTrack: Int?
    /// Play has the focus when the album opens, unless it opened from a track.
    @FocusState private var playFocused: Bool

    init(app: AppModel, itemId: Int, track: Int?) {
        self.app = app
        music = app.music
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
            StateView(kind: .failed(title: localized("Couldn't load this album"), why: why) {
                Task { await model.load() }
            })
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
                if model.canPlay {
                    HStack(spacing: 24) {
                        Button { Task { await model.play() } } label: {
                            Label("Play", systemImage: "play.fill")
                        }
                        .focused($playFocused)
                        .accessibilityIdentifier("album.play")
                        Button { Task { await model.play(shuffle: true) } } label: {
                            Label("Shuffle", systemImage: "shuffle")
                        }
                        .disabled(!model.canShuffle)
                        .accessibilityIdentifier("album.shuffle")
                    }
                    .focusSection()
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
                    guard let highlight = page.highlight, !page.rows.isEmpty else {
                        if model.canPlay { playFocused = true }
                        return
                    }
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
        ChoiceButton(title: localized("Your rating"), value: rating.map(StarWords.stars) ?? localized("Not rated"),
                     symbol: rating == nil ? "star" : "star.fill",
                     text: rating.map(StarWords.stars) ?? localized("Rate"),
                     choices: [Choice(label: localized("Not rated"), tag: nil)]
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
                        let current = music.state.track?.id == track.id
                        TrackLine(track: track, highlighted: track.id == page.highlight,
                                  playing: current ? music.state.playing : nil) {
                            Task { await model.playOrToggle(track.id) }
                        }
                        .focused($focusedTrack, equals: track.id)
                            .id(row.id)
                    }
                }
            }
            .accessibilityIdentifier("album.tracks")
        }
    }
}

/// A track of an album: number, title, its own artist and its length.
/// Select plays the album from it; the track playing shows a speaker in
/// place of its number, and Select pauses or plays it on.
private struct TrackLine: View {
    let track: TrackRow
    let highlighted: Bool
    /// Whether it plays (`true`) or waits paused; `nil` while another track is current.
    let playing: Bool?
    let select: () -> Void

    var body: some View {
        Button(action: select) {
            TrackContent(track: track, highlighted: highlighted, playing: playing)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(playing.map {
            $0 ? localized("\(track.spoken), playing") : localized("\(track.spoken), paused")
        } ?? track.spoken)
        .accessibilityHint(playing.map { $0 ? "Pauses" : "Plays on" } ?? "Plays the album from this track")
        .accessibilityAddTraits(highlighted ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("album.track.\(track.id)")
    }
}

/// A track's line, lit while it has the focus.
private struct TrackContent: View {
    let track: TrackRow
    let highlighted: Bool
    let playing: Bool?
    @Environment(\.isFocused) private var focused

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 24) {
            Group {
                if let playing {
                    Image(systemName: playing ? "speaker.wave.2.fill" : "speaker.fill")
                } else {
                    Text(track.number).foregroundStyle(.secondary)
                }
            }
            .monospacedDigit()
            .frame(minWidth: 50, alignment: .trailing)
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
