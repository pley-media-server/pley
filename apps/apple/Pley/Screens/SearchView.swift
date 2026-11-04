import PleyModel
import PleyScreens
import SwiftUI

/// Search: the field at the top (in the window's toolbar on the Mac), the
/// profile's recent searches while nothing is typed, and then the results
/// in groups, each a row: People, Movies, Shows, Seasons, Episodes,
/// Artists, Albums, Tracks, with their counts. The results shown stay
/// while the next load. Where requests are open, Discover heads the page
/// before anything is typed, and TMDB's titles no library has follow the
/// results, in the Discover row.
struct SearchView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: SearchModel
    @ObservedObject private var shell: ShellModel
    /// The field has the keys: after ⌘F (the Mac's View menu, an iPad's keyboard).
    @FocusState private var fieldFocused: Bool
    private let layout = WideLayout()

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: SearchModel(app: app))
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
            StateView(kind: .failed(title: "Search failed", why: why, retry: { model.retry() }))
        }
    }

    /// Before anything is typed: Discover where requests are open, then the
    /// profile's recent searches, newest first, with Clear.
    @ViewBuilder private var recent: some View {
        if model.recent.isEmpty && !shell.requestsEntry.discover {
            StateView(kind: .empty(symbol: "magnifyingglass",
                                   text: "Search for movies, shows, people and music.", action: nil))
        } else {
            List {
                if shell.requestsEntry.discover {
                    Section { SearchDiscoverEntry() }
                }
                if !model.recent.isEmpty { recentSection }
            }
            #if os(macOS)
            .listStyle(.inset)
            #endif
        }
    }

    /// The recent searches, newest first, with Clear.
    private var recentSection: some View {
        Section {
            ForEach(Array(model.recent.enumerated()), id: \.element) { index, text in
                Button {
                    model.searchAgain(text)
                } label: {
                    Label(text, systemImage: "clock.arrow.circlepath")
                        .foregroundStyle(.primary)
                }
                .accessibilityHint("Searches again")
                .accessibilityIdentifier("search.recent.\(index)")
            }
        } header: {
            HStack {
                Text("Recent searches")
                Spacer()
                Button("Clear") { model.clearRecent() }
                    .font(.footnote)
                    .accessibilityLabel("Clear recent searches")
                    .accessibilityIdentifier("search.recent.clear")
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
            Text("\(group.kind.title) (\(group.count))")
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
            Text("\(group.kind.title) (\(group.count))")
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
