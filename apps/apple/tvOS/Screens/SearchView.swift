import PleyModel
import PleyScreens
import SwiftUI

/// Search on Apple TV: the system's keyboard and field at the top; while
/// nothing is typed, the profile's recent searches and Browse by genre;
/// then the results in
/// rows the focus moves down through: Movies, Shows, Seasons, Episodes,
/// Artists, Albums, Tracks, Live TV and People, with their counts. Where
/// the account may ask for titles, TMDB's titles the libraries don't have
/// come last, in a Discover row with See all ("Nothing in your libraries
/// matches" above it when that is all there is). Before anything is typed
/// there is no way into Discover here: its tab is the way in.
struct SearchView: View {
    @ObservedObject private var app: AppModel
    @ObservedObject private var shell: ShellModel
    @StateObject private var model: SearchModel
    @StateObject private var genres: GenreBrowseModel

    init(app: AppModel) {
        self.app = app
        _shell = ObservedObject(wrappedValue: app.shell)
        _model = StateObject(wrappedValue: SearchModel(app: app))
        _genres = StateObject(wrappedValue: GenreBrowseModel(app: app))
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                content
            }
            .padding(.vertical, 24)
        }
        .searchable(text: $model.query, prompt: "Movies, shows, people and music")
        .onSubmit(of: .search) { model.submit() }
        .task(id: app.user?.id) { model.loadRecent() }
        .task(id: shell.libraries.map(\.id)) { await genres.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .idle:
            recent
            if !genres.genres.isEmpty { genreRow }
        case .loading:
            ProgressView()
                .frame(maxWidth: .infinity)
                .accessibilityLabel("Searching")
        case .ready(_, let groups, _):
            ForEach(groups) { group in
                if group.kind == .people {
                    peopleRow(group)
                } else if group.kind == .channels {
                    TVSearchChannels(app: app, group: group)
                } else {
                    TileRow(app: app, key: group.kind.rawValue, title: "\(group.kind.title) (\(group.count))",
                            shape: group.kind.shape, tiles: group.tiles, prefix: "search.group") { tile in
                        app.shell.open(model.open(tile))
                    }
                }
            }
            TVSearchDiscover(app: app, model: model)
        case .empty(let query):
            if model.nothingInLibraries {
                Text(RequestWords.nothingInLibraries)
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, Metrics.gutter)
                    .accessibilityIdentifier("search.empty")
                TVSearchDiscover(app: app, model: model)
            } else {
                message(SearchModel.noResults(query))
                    .accessibilityIdentifier("search.empty")
            }
        case .failed(_, let why):
            VStack(spacing: 24) {
                message(localized("Search failed. \(asSentence(why))"))
                Button("Try again") { model.retry() }
                    .accessibilityIdentifier("state.retry")
            }
            .frame(maxWidth: .infinity)
            .focusSection()
        }
    }

    /// The recent searches, newest first, then Clear.
    @ViewBuilder private var recent: some View {
        if model.recent.isEmpty && genres.genres.isEmpty {
            message(localized("Search for movies, shows, people and music."))
        } else if !model.recent.isEmpty {
            VStack(alignment: .leading, spacing: 20) {
                Text("Recent searches")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                ScrollView(.horizontal) {
                    HStack(spacing: 24) {
                        ForEach(Array(model.recent.enumerated()), id: \.element) { index, text in
                            Button {
                                model.searchAgain(text)
                            } label: {
                                Label(text, systemImage: "clock.arrow.circlepath")
                            }
                            .accessibilityHint("Searches again")
                            .accessibilityIdentifier("search.recent.\(index)")
                        }
                        Button("Clear") { model.clearRecent() }
                            .accessibilityLabel("Clear recent searches")
                            .accessibilityIdentifier("search.recent.clear")
                    }
                    .padding(.vertical, 20)
                }
                .scrollClipDisabled()
            }
            .padding(.horizontal, Metrics.gutter)
            .focusSection()
        }
    }

    /// A movie or show library's genres, as colored cards in a row, each
    /// opening the library filtered by it; with more than one such
    /// library, a menu beside the heading picks which.
    private var genreRow: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(alignment: .firstTextBaseline, spacing: 32) {
                Text("Browse by genre")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                if genres.libraries.count > 1, let name = genres.libraryName {
                    Menu {
                        Picker("Library", selection: Binding(get: { genres.libraryId ?? 0 },
                                                             set: { id in Task { await genres.choose(id) } })) {
                            ForEach(genres.libraries) { Text($0.name).tag($0.id) }
                        }
                    } label: {
                        Label(name, systemImage: "chevron.down")
                    }
                    .accessibilityLabel("Library")
                    .accessibilityValue(name)
                    .accessibilityIdentifier("search.genres.library")
                }
            }
            .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(spacing: Metrics.tileSpacing) {
                    ForEach(genres.genres, id: \.self) { genre in
                        Button {
                            if let screen = genres.screen(for: genre) { app.shell.open(screen) }
                        } label: {
                            GenreCard(name: genre, tint: GenreBrowseModel.tint(genre))
                        }
                        .buttonStyle(.card)
                        .accessibilityValue(genres.libraryName ?? "")
                        .accessibilityIdentifier("search.genre.\(genre)")
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 20)
            }
            .scrollClipDisabled()
        }
        .focusSection()
    }

    /// `text`, in the language shown.
    private func message(_ text: String) -> some View {
        Text(text)
            .font(.title3)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
            .padding(.top, 40)
    }

    /// People found: round photos (or initials) with their names.
    private func peopleRow(_ group: SearchModel.Group) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(verbatim: "\(group.kind.title) (\(group.count))")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(group.people) { person in
                        VStack(spacing: 14) {
                            Button {
                                app.shell.open(model.open(person))
                            } label: {
                                AvatarView(app: app, name: person.name, path: person.photoUrl, color: nil, size: 180)
                            }
                            .buttonStyle(.borderless)
                            .clipShape(Circle())
                            .accessibilityLabel(person.name)
                            .accessibilityHint("Opens their page")
                            .accessibilityIdentifier("search.person.\(person.id)")
                            Text(person.name)
                                .font(.caption)
                                .lineLimit(2)
                                .multilineTextAlignment(.center)
                                .accessibilityHidden(true)
                        }
                        .frame(width: 220)
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 32)
            }
            .scrollClipDisabled()
        }
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("search.group.people")
    }
}

/// TMDB's titles for the query the libraries don't have, in a row with
/// See all (the grid of every title found); empty tiles while they are
/// read, and why with Retry when TMDB couldn't be searched.
private struct TVSearchDiscover: View {
    let app: AppModel
    @ObservedObject var model: SearchModel

    var body: some View {
        switch model.discover {
        case .hidden:
            EmptyView()
        case .ready(_, let cards) where cards.isEmpty:
            EmptyView()
        case .ready(_, let cards):
            CardRow(identifier: "search.discover.row", title: RequestWords.discover, items: cards,
                    seeAll: model.discoverAll.map { all in { app.shell.open(all) } }) { card, focus in
                DiscoverCardView(app: app, card: card, identifier: "search.discover.card.\(card.key)",
                                 rowFocus: focus)
            }
        case .loading:
            VStack(alignment: .leading, spacing: 0) {
                heading
                ScrollView(.horizontal) {
                    LazyHStack(spacing: Metrics.tileSpacing) {
                        ForEach(0..<6, id: \.self) { _ in
                            PlaceholderTile(shape: .poster, width: Metrics.tileWidth(.poster))
                        }
                    }
                    .padding(.horizontal, Metrics.gutter)
                    .padding(.vertical, 32)
                }
                .scrollClipDisabled()
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("\(RequestWords.discover), loading")
            .accessibilityIdentifier("search.discover.row")
        case .failed(let words):
            VStack(alignment: .leading, spacing: 20) {
                heading
                HStack(spacing: 40) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(words.title).font(.headline)
                        if let body = words.body {
                            Text(body).font(.callout).foregroundStyle(.secondary)
                        }
                    }
                    if words.retry {
                        Button(RequestWords.retry, action: model.retryDiscover)
                            .accessibilityIdentifier("search.discover.retry")
                    }
                }
                .padding(.horizontal, Metrics.gutter)
            }
            .focusSection()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("search.discover.row")
            .task(id: words) { announce(words.title) }
        }
    }

    private var heading: some View {
        Text(RequestWords.discover)
            .font(.headline)
            .accessibilityAddTraits(.isHeader)
            .padding(.horizontal, Metrics.gutter)
    }
}

/// Live TV's channels found, in a row: each plays in All channels.
private struct TVSearchChannels: View {
    let app: AppModel
    let group: SearchModel.Group

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(verbatim: "\(group.kind.title) (\(group.count))")
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(spacing: 40) {
                    ForEach(group.channels) { channel in
                        Button {
                            Task { await app.watchLive(LiveRequest(channelId: channel.id, category: .all), seen: channel) }
                        } label: {
                            VStack(alignment: .leading, spacing: 10) {
                                TVChannelLogo(app: app, channel: channel, size: 160)
                                Text(channel.numberedName).font(.callout).lineLimit(1)
                                if let on = channel.now {
                                    Text(on.title).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                }
                            }
                            .frame(width: 260, alignment: .leading)
                        }
                        .buttonStyle(.borderless)
                        .accessibilityIdentifier("search.channel.\(channel.id)")
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 30)
            }
            .scrollClipDisabled()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("search.group.channels")
    }
}

/// A genre in Browse by genre: its name on one of `GenreBrowseModel.tints`
/// colors (the iPhone's), dark enough for white words.
private struct GenreCard: View {
    let name: String
    let tint: Int

    static let colors: [Color] = [
        Color(red: 0.60, green: 0.20, blue: 0.07), Color(red: 0.55, green: 0.38, blue: 0.03),
        Color(red: 0.43, green: 0.16, blue: 0.85), Color(red: 0.05, green: 0.45, blue: 0.56),
        Color(red: 0.09, green: 0.40, blue: 0.20), Color(red: 0.75, green: 0.09, blue: 0.36),
        Color(red: 0.12, green: 0.25, blue: 0.62), Color(red: 0.36, green: 0.27, blue: 0.20),
    ]

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            Self.colors[tint % Self.colors.count]
            Circle()
                .fill(.white.opacity(0.16))
                .frame(width: 110, height: 110)
                .offset(x: 24, y: -24)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                .accessibilityHidden(true)
            Text(name)
                .font(.headline)
                .foregroundStyle(.white)
                .lineLimit(2)
                .padding(20)
        }
        .frame(width: Metrics.tileWidth(.poster) * 1.4, height: 150)
        .clipShape(RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous))
    }
}
