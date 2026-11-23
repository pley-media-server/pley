import PleyModel
import PleyScreens
import SwiftUI

/// A library's page for the sofa: its name (a menu of the other movie and
/// show libraries, which switches to the one chosen) and Recommended,
/// Library and Collections (not for music) across the top. Recommended has the rows of
/// collections administrators put there. On Library a row of Sort, the
/// order, Filter (with Clear while filtered) and, at the far end, Save as
/// Smart Playlist; then the grid, read 100 titles at a time, with the
/// letters down the right of the screen when sorted by title. The top of
/// the page scrolls away with the titles, so the grid has the screen once
/// the focus is in it.
/// Each row is a focus section, so up and down go from one to the next.
struct LibraryView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: LibraryModel
    @FocusState private var tabFocus: LibraryTab?

    init(app: AppModel, libraryId: Int, view: String? = nil) {
        self.app = app
        _model = StateObject(wrappedValue: LibraryModel(app: app, libraryId: libraryId, view: view))
    }

    var body: some View {
        content
            .task { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: LibraryModel.failedTitle, why: why) { Task { await model.load() } })
        case .notFound:
            StateView(kind: .empty(symbol: "questionmark.folder", text: LibraryModel.notFoundTitle, action: nil))
        case .loaded:
            switch model.params.tab {
            case .recommended:
                recommended
            case .collections:
                CollectionsGrid(app: app, model: model.collections, header: AnyView(header))
            case .library:
                GridPage(app: app, model: model, grid: model.grid, header: AnyView(VStack(alignment: .leading,
                                                                                          spacing: 32) {
                    header
                    controls
                }))
                .id(model.grid.map(ObjectIdentifier.init))
            }
        }
    }

    /// The library's name, Recommended, Library and Collections, and what
    /// went wrong last. It scrolls with the page: switching tabs makes the
    /// page anew, and the focus stays on the tab chosen.
    private var header: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .center, spacing: 20) {
                title
                    .padding(.trailing, 28)
                if model.tabs.count > 1 {
                    ForEach(model.tabs, id: \.self) { tab in
                        Button { choose(tab) } label: {
                            // The tab showing has a check: a tint alone is too faint from the sofa.
                            if model.params.tab == tab {
                                Label(tab.title, systemImage: "checkmark")
                            } else {
                                Text(tab.title)
                            }
                        }
                        .buttonStyle(.bordered)
                        .font(.callout)
                        .focused($tabFocus, equals: tab)
                        .accessibilityAddTraits(model.params.tab == tab ? .isSelected : [])
                        .accessibilityIdentifier("library.tab.\(tab.rawValue)")
                    }
                }
            }
            .focusSection()
            if let notice = model.notice {
                Text(asSentence(notice))
                    .font(.callout)
                    .foregroundStyle(.red)
            }
        }
        .padding(.horizontal, Metrics.gutter)
        .padding(.top, 20)
    }

    /// The movie and show libraries the title's menu switches between.
    private var others: [Library] {
        let video = app.shell.libraries.filter { $0.kind != .music }
        return video.contains { $0.id == model.libraryId } && video.count > 1 ? video : []
    }

    /// The library's name; with other movie and show libraries, a menu of
    /// them that switches to the one chosen (`ShellModel.switchLibrary`).
    @ViewBuilder private var title: some View {
        if others.isEmpty {
            Text(model.title)
                .font(.title2.bold())
                .lineLimit(1)
                .accessibilityAddTraits(.isHeader)
        } else {
            Menu {
                ForEach(others) { library in
                    Button(library.id == model.libraryId ? "\(library.name) ✓" : library.name) {
                        if library.id != model.libraryId { app.shell.switchLibrary(to: library.id) }
                    }
                }
            } label: {
                Label(model.title, systemImage: "chevron.down")
                    .font(.title2.bold())
                    .lineLimit(1)
            }
            .accessibilityLabel(model.title)
            .accessibilityHint("Switches to another library")
            .accessibilityAddTraits(.isHeader)
            .accessibilityIdentifier("library.switch")
        }
    }

    /// Shows `tab`, and keeps the focus on its button in the page made anew.
    private func choose(_ tab: LibraryTab) {
        model.select(tab)
        Task { @MainActor in
            await Task.yield()
            tabFocus = tab
        }
    }

    private var recommended: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 32) {
                header
                if let rows = model.rows {
                    if rows.isEmpty {
                        StateView(kind: .empty(symbol: "sparkles", text: LibraryModel.emptyTitle, action: nil))
                            .frame(minHeight: 500)
                    } else {
                        LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                            ForEach(rows) { row in
                                TileRow(app: app, key: row.key, title: row.title, shape: row.shape, tiles: row.tiles,
                                        seeAll: row.seeAll.map { screen in { app.shell.open(screen) } }) { tile in
                                    app.shell.open(item: tile.item)
                                }
                            }
                        }
                    }
                } else {
                    StateView(kind: .loading)
                        .frame(minHeight: 500)
                }
            }
            .padding(.bottom, 60)
        }
    }

    /// Sort, the order, Filter and Clear, and Save as Smart Playlist at the far end.
    private var controls: some View {
        HStack(spacing: 20) {
            // tvOS gives a menu's button no accessibility label of its own,
            // whatever its label view shows: it is set here.
            Menu("Sort: \(model.params.sort.title)", systemImage: "arrow.up.arrow.down") {
                Picker("Sort by", selection: Binding(get: { model.params.sort }, set: { model.sort(by: $0) })) {
                    ForEach(model.sorts, id: \.self) { sort in
                        Text(sort.title).tag(sort)
                    }
                }
            }
            .accessibilityLabel("Sort")
            .accessibilityValue(model.params.sort.title)
            .accessibilityIdentifier("library.sort")
            Button {
                model.flipOrder()
            } label: {
                Label(model.params.effectiveOrder == .ascending ? "Ascending" : "Descending",
                      systemImage: model.params.effectiveOrder == .ascending ? "arrow.up" : "arrow.down")
            }
            .accessibilityIdentifier("library.order")
            if model.filterable {
                Menu(model.params.filtered ? "Filter: \(model.filterSummary)" : "Filter",
                     systemImage: "line.3.horizontal.decrease.circle") {
                    Picker("Show", selection: Binding(get: { model.params.watched },
                                                      set: { model.filter(watched: $0) })) {
                        Text("All").tag(Bool?.none)
                        Text("Unwatched").tag(Bool?.some(false))
                        Text("Watched").tag(Bool?.some(true))
                    }
                    if !model.filters.genres.isEmpty {
                        Picker("Genre", selection: Binding(get: { model.params.genre },
                                                           set: { model.filter(genre: $0) })) {
                            Text("Any").tag(String?.none)
                            ForEach(model.filters.genres, id: \.self) { Text($0).tag(String?.some($0)) }
                        }
                        .pickerStyle(.menu)
                    }
                    if !model.filters.years.isEmpty {
                        Picker("Year", selection: Binding(get: { model.params.year },
                                                          set: { model.filter(year: $0) })) {
                            Text("Any").tag(Int?.none)
                            ForEach(model.filters.years, id: \.self) { Text(String($0)).tag(Int?.some($0)) }
                        }
                        .pickerStyle(.menu)
                    }
                }
                .accessibilityLabel("Filter")
                .accessibilityValue(model.filterSummary)
                .accessibilityIdentifier("library.filter")
                if model.params.filtered {
                    Button("Clear") { model.clearFilters() }
                        .accessibilityLabel("Clear filters")
                        .accessibilityIdentifier("library.filter.clear")
                }
            }
            Spacer(minLength: 20)
            Button {
                model.saveAs()
            } label: {
                // A word of its own: the panel's full title doesn't fit beside Sort and Filter in French or Spanish.
                let title = LocalizedStringResource("library.saveAs", defaultValue: "Save as Smart Playlist")
                Label(localized(keyed: title), systemImage: "text.badge.plus")
            }
            .accessibilityIdentifier("library.saveAs")
        }
        .font(.callout)
        .focusSection()
        .padding(.horizontal, Metrics.gutter)
    }
}

/// The top of the page and the grid, scrolling as one, and the letters
/// down the right when the listing has them.
private struct GridPage: View {
    let app: AppModel
    @ObservedObject var model: LibraryModel
    let grid: LibraryGrid?
    let header: AnyView

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    header
                    if let grid {
                        GridBody(app: app, model: model, grid: grid)
                    } else {
                        StateView(kind: .loading)
                            .frame(maxWidth: .infinity, minHeight: 500)
                    }
                }
                .padding(.bottom, 60)
            }
            // Down the right of the screen, staying while the titles scroll.
            .overlay(alignment: .trailing) {
                if let grid {
                    LetterStrip(model: model, grid: grid) { reader.scrollTo($0, anchor: .top) }
                }
            }
        }
    }
}

/// The letters, while the grid has titles to jump through.
private struct LetterStrip: View {
    @ObservedObject var model: LibraryModel
    @ObservedObject var grid: LibraryGrid
    let scrollTo: (Int) -> Void

    var body: some View {
        if let alpha = model.alpha, (grid.total ?? 0) > 0 {
            Letters(alpha: alpha) { scrollTo(alpha.jumpTarget($0)) }
        }
    }
}

/// The grid, or why there is none.
private struct GridBody: View {
    let app: AppModel
    @ObservedObject var model: LibraryModel
    @ObservedObject var grid: LibraryGrid

    var body: some View {
        if let total = grid.total {
            if total == 0 {
                Group {
                    if model.params.filtered {
                        StateView(kind: .empty(symbol: "line.3.horizontal.decrease.circle",
                                               text: LibraryModel.noMatchTitle,
                                               action: (localized("Clear filters"), { model.clearFilters() })))
                    } else {
                        StateView(kind: .empty(symbol: "film.stack", text: LibraryModel.emptyTitle, action: nil))
                    }
                }
                .frame(maxWidth: .infinity, minHeight: 500)
            } else {
                // Room on the right for the letters, when they show.
                TileGrid(app: app, shape: grid.shape, count: total, tile: { grid.tile(at: $0) },
                         showing: { grid.showing($0) }, trailingRoom: model.alpha == nil ? 0 : Letters.width) { tile in
                    app.shell.open(item: tile.item)
                }
                // Down from any button above reaches the grid, however few titles it has.
                .focusSection()
                .accessibilityIdentifier("library.grid")
            }
        } else if let failure = grid.failure {
            StateView(kind: .failed(title: LibraryModel.failedTitle, why: failure) { Task { await grid.reload() } })
                .frame(maxWidth: .infinity, minHeight: 500)
        } else {
            StateView(kind: .loading)
                .frame(maxWidth: .infinity, minHeight: 500)
        }
    }
}

/// "#" and A to Z down the right of the screen: Select jumps the grid to
/// the letter, or the next that has titles; letters without titles are
/// dim. Right from the grid's last column reaches them, and left goes back.
private struct Letters: View {
    let alpha: AlphaIndex
    let jump: (Int) -> Void
    /// How much of the screen's right the letters take.
    static let width: CGFloat = 60

    var body: some View {
        VStack(spacing: 0) {
            ForEach(AlphaIndex.letters.indices, id: \.self) { letter in
                Button {
                    jump(letter)
                } label: {
                    Text(AlphaIndex.letters[letter])
                        .font(.caption.weight(.semibold))
                        .frame(width: 40, height: 28)
                        .opacity(alpha.has(letter) ? 1 : 0.35)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Jump to \(AlphaIndex.letters[letter])")
                .accessibilityIdentifier("library.alpha.\(AlphaIndex.letters[letter])")
            }
        }
        .padding(.vertical, 12)
        .frame(width: Self.width)
        .background(.black.opacity(0.25), in: Capsule())
        .focusSection()
        .padding(.trailing, 12)
    }
}
