import PleyModel
import PleyScreens
import SwiftUI

/// Search: the field at the top (in the window's toolbar on the Mac); while
/// nothing is typed, the profile's recent searches as chips, Browse by
/// genre, and Discover as a card where requests are open; and then the
/// results in groups, each a row: People, Movies, Shows, Seasons,
/// Episodes, Artists, Albums, Tracks, with their counts. The results shown
/// stay while the next load. TMDB's titles no library has follow the
/// results, in the Discover row.
struct SearchView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: SearchModel
    @StateObject private var genres: GenreBrowseModel
    @ObservedObject private var shell: ShellModel
    /// The field has the keys: after ⌘F (the Mac's View menu, an iPad's keyboard).
    @FocusState private var fieldFocused: Bool
    private let layout = WideLayout()

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: SearchModel(app: app))
        _genres = StateObject(wrappedValue: GenreBrowseModel(app: app))
        _shell = ObservedObject(wrappedValue: app.shell)
    }

    var body: some View {
        content
            .navigationTitle("Search")
            #if os(iOS)
            .searchable(text: $model.query, placement: .navigationBarDrawer(displayMode: .always),
                        prompt: "Movies, shows, people and music")
            #else
            .searchable(text: $model.query, placement: .toolbar, prompt: "Movies, shows, people and music")
            #endif
            .modifier(SearchFieldFocus(focused: $fieldFocused))
            .task(id: shell.wantsSearchField) {
                guard shell.wantsSearchField else { return }
                fieldFocused = true
                shell.searchFieldShown()
            }
            .onSubmit(of: .search) { model.submit() }
            .task(id: app.user?.id) { model.loadRecent() }
            .task(id: shell.libraries.map(\.id)) { await genres.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .idle:
            recent
        case .loading:
            StateView(kind: .loading)
        case .ready(_, let groups, let loading):
            results(groups)
                .overlay(alignment: .top) {
                    if loading {
                        ProgressView()
                            .padding(8)
                            .accessibilityLabel("Searching")
                    }
                }
        case .empty(let query):
            if model.nothingInLibraries {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        Text(RequestWords.nothingInLibraries)
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                            .accessibilityIdentifier("search.empty")
                        SearchDiscoverRow(app: app, model: model)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 16)
                }
                .scrollDismissesKeyboard(.immediately)
            } else {
                StateView(kind: .empty(symbol: "magnifyingglass", text: SearchModel.noResults(query), action: nil))
                    .accessibilityIdentifier("search.empty")
            }
        case .failed(_, let why):
            StateView(kind: .failed(title: localized("Search failed"), why: why, retry: { model.retry() }))
        }
    }

    /// Before anything is typed: the profile's recent searches as chips,
    /// newest first, with Clear; Browse by genre; and Discover where
    /// requests are open.
    @ViewBuilder private var recent: some View {
        if model.recent.isEmpty && genres.genres.isEmpty && !shell.requestsEntry.discover {
            StateView(kind: .empty(symbol: "magnifyingglass",
                                   text: localized("Search for movies, shows, people and music."), action: nil))
        } else {
            let gutter = Metrics.gutter(regular: layout.wide)
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    if !model.recent.isEmpty { recentChips }
                    if !genres.genres.isEmpty { genreGrid }
                    if shell.requestsEntry.discover { SearchDiscoverCard() }
                }
                .padding(.horizontal, gutter)
                .padding(.vertical, 16)
            }
            .scrollDismissesKeyboard(.immediately)
        }
    }

    /// The recent searches as chips that wrap, newest first, with Clear.
    private var recentChips: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline) {
                Text("Recent searches")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                Spacer()
                Button("Clear") { model.clearRecent() }
                    .font(.subheadline)
                    .accessibilityLabel("Clear recent searches")
                    .accessibilityIdentifier("search.recent.clear")
            }
            FlowLayout(spacing: 8, lineSpacing: 8) {
                ForEach(Array(model.recent.enumerated()), id: \.element) { index, text in
                    Button {
                        model.searchAgain(text)
                    } label: {
                        Text(text)
                            .font(.subheadline)
                            .lineLimit(1)
                            .padding(.horizontal, 14)
                            .frame(minHeight: 34)
                            .background(.quaternary, in: Capsule())
                            .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Searches again")
                    .accessibilityIdentifier("search.recent.\(index)")
                }
            }
        }
    }

    /// A movie or show library's genres in colored tiles, each opening the
    /// library filtered by it; with more than one such library, a menu
    /// beside the heading picks which.
    private var genreGrid: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                Text("Browse by genre")
                    .font(.title2.bold())
                    .accessibilityAddTraits(.isHeader)
                Spacer()
                if genres.libraries.count > 1, let name = genres.libraryName {
                    Menu {
                        Picker("Library", selection: Binding(get: { genres.libraryId ?? 0 },
                                                             set: { id in Task { await genres.choose(id) } })) {
                            ForEach(genres.libraries) { Text($0.name).tag($0.id) }
                        }
                    } label: {
                        Label(name, systemImage: "chevron.down")
                            .labelStyle(TrailingIconLabelStyle())
                            .font(.subheadline.weight(.semibold))
                    }
                    .accessibilityLabel("Library")
                    .accessibilityValue(name)
                    .accessibilityIdentifier("search.genres.library")
                }
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: layout.wide ? 200 : 150), spacing: 10)], spacing: 10) {
                ForEach(genres.genres, id: \.self) { genre in
                    Button {
                        if let screen = genres.screen(for: genre) { app.shell.open(screen) }
                    } label: {
                        GenreTile(name: genre, tint: GenreBrowseModel.tint(genre))
                    }
                    .buttonStyle(.plain)
                    #if os(iOS)
                    .hoverEffect(.lift)
                    #endif
                    .accessibilityValue(genres.libraryName ?? "")
                    .accessibilityIdentifier("search.genre.\(genre)")
                }
            }
        }
    }

    private func results(_ groups: [SearchModel.Group]) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                ForEach(groups) { group in
                    if group.kind == .people {
                        PeopleRow(app: app, group: group) { person in app.shell.open(model.open(person)) }
                    } else if group.kind == .channels {
                        ChannelsRow(app: app, group: group)
                    } else {
                        TileRow(app: app, key: group.kind.rawValue, title: "\(group.kind.title) (\(group.count))",
                                shape: group.kind.shape, tiles: group.tiles, prefix: "search.group") { tile in
                            app.shell.open(model.open(tile))
                        }
                    }
                }
                SearchDiscoverRow(app: app, model: model)
            }
            .padding(.vertical, 16)
        }
        // Scrolling the results puts the keyboard away.
        .scrollDismissesKeyboard(.immediately)
    }
}

/// Live TV's channels found, in a row: each opens the live player in All channels.
private struct ChannelsRow: View {
    let app: AppModel
    let group: SearchModel.Group
    private let layout = WideLayout()
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    var body: some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        VStack(alignment: .leading, spacing: 10) {
            Text(verbatim: "\(group.kind.title) (\(group.count))")
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, gutter)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(group.channels) { channel in
                        Button {
                            watch(channel)
                        } label: {
                            VStack(alignment: .leading, spacing: 6) {
                                ChannelLogo(app: app, channel: channel, size: 72)
                                Text(channel.numberedName).font(.subheadline).lineLimit(2)
                                if let on = channel.now {
                                    Text(on.title).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                }
                            }
                            .frame(width: 140, alignment: .leading)
                        }
                        .buttonStyle(.plain)
                        .accessibilityElement(children: .combine)
                        .accessibilityHint("Watch")
                        .accessibilityIdentifier("search.channel.\(channel.id)")
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("search.group.channels")
    }

    private func watch(_ channel: LiveChannel) {
        let request = LiveRequest(channelId: channel.id, category: .all)
        #if os(macOS)
        openWindow(id: LiveWindow.id, value: request)
        #else
        Task { await app.watchLive(request, seen: channel) }
        #endif
    }
}

/// People found: their round photos (or initials) and names, in a row.
/// Ties the search field's focus to `focused` where the system can (iOS 18, the Mac).
private struct SearchFieldFocus: ViewModifier {
    let focused: FocusState<Bool>.Binding

    func body(content: Content) -> some View {
        #if os(iOS)
        if #available(iOS 18, *) {
            content.searchFocused(focused)
        } else {
            content
        }
        #else
        content.searchFocused(focused)
        #endif
    }
}

private struct PeopleRow: View {
    let app: AppModel
    let group: SearchModel.Group
    let open: (PersonSummary) -> Void
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var size: CGFloat = 84

    var body: some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        VStack(alignment: .leading, spacing: 10) {
            Text(verbatim: "\(group.kind.title) (\(group.count))")
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, gutter)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(group.people) { person in
                        Button {
                            open(person)
                        } label: {
                            VStack(spacing: 6) {
                                AvatarView(app: app, name: person.name, path: person.photoUrl, color: nil,
                                           size: min(size, 140))
                                Text(person.name)
                                    .font(.footnote.weight(.medium))
                                    .lineLimit(2)
                                    .multilineTextAlignment(.center)
                            }
                            .frame(width: min(size, 140) + 16)
                        }
                        .buttonStyle(.plain)
                        #if os(iOS)
                        .hoverEffect(.lift)
                        #endif
                        .accessibilityLabel(person.name)
                        .accessibilityHint("Opens their page")
                        .accessibilityIdentifier("search.person.\(person.id)")
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("search.group.people")
    }
}

/// A genre in Browse by genre: its name on one of `GenreBrowseModel.tints`
/// colors, dark enough for white words in light and dark mode alike.
private struct GenreTile: View {
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
                .frame(width: 64, height: 64)
                .offset(x: 14, y: -14)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                .accessibilityHidden(true)
            Text(name)
                .font(.headline)
                .foregroundStyle(.white)
                .lineLimit(2)
                .padding(12)
        }
        .frame(minHeight: 76)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

/// A label with its icon after its title: a menu's "Movies ▾".
private struct TrailingIconLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            configuration.title
            configuration.icon.imageScale(.small)
        }
    }
}
