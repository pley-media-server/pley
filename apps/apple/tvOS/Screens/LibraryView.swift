import PleyModel
import PleyScreens
import SwiftUI

/// A library's page for the sofa: Recommended and Library as buttons
/// across the top, then on Library a row of Sort, the order and Filter
/// (with Clear while filtered), a row of letters when sorted by title, and
/// the grid, read 100 titles at a time. Each row is a focus section, so up
/// and down go from one to the next.
struct LibraryView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: LibraryModel

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
            VStack(alignment: .leading, spacing: 24) {
                HStack(spacing: 24) {
                    Text(model.title)
                        .font(.title2.bold())
                        .accessibilityAddTraits(.isHeader)
                        .padding(.trailing, 24)
                    if model.tabs.count > 1 {
                        ForEach(model.tabs, id: \.self) { tab in
                            Button { model.select(tab) } label: {
                                // The tab showing has a check: a tint alone is too faint from the sofa.
                                if model.params.tab == tab {
                                    Label(tab.title, systemImage: "checkmark")
                                } else {
                                    Text(tab.title)
                                }
                            }
                                .buttonStyle(.bordered)
                                .accessibilityAddTraits(model.params.tab == tab ? .isSelected : [])
                                .accessibilityIdentifier("library.tab.\(tab.rawValue)")
                        }
                    }
                }
                .focusSection()
                .padding(.horizontal, Metrics.gutter)
                if let notice = model.notice {
                    Text(asSentence(notice))
                        .font(.callout)
                        .foregroundStyle(.red)
                        .padding(.horizontal, Metrics.gutter)
                }
                switch model.params.tab {
                case .recommended:
                    recommended
                case .library, .collections:
                    controls
                    if let grid = model.grid {
                        GridPage(app: app, model: model, grid: grid)
                            .id(ObjectIdentifier(grid))
                    } else {
                        StateView(kind: .loading)
                    }
                }
            }
            .padding(.top, 20)
        }
    }

    @ViewBuilder private var recommended: some View {
        if let rows = model.rows {
            if rows.isEmpty {
                StateView(kind: .empty(symbol: "sparkles", text: LibraryModel.emptyTitle, action: nil))
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                        ForEach(rows) { row in
                            TileRow(app: app, key: row.key, title: row.title, shape: row.shape, tiles: row.tiles,
                                    seeAll: row.seeAll.map { screen in { app.shell.open(screen) } }) { tile in
                                app.shell.open(item: tile.item)
                            }
                        }
                    }
                }
            }
        } else {
            StateView(kind: .loading)
        }
    }

    /// Sort, the order, Filter and Clear.
    private var controls: some View {
        HStack(spacing: 24) {
            Menu {
                Picker("Sort by", selection: Binding(get: { model.params.sort }, set: { model.sort(by: $0) })) {
                    ForEach(model.sorts, id: \.self) { sort in
                        Text(sort.title).tag(sort)
                    }
                }
            } label: {
                Label("Sort: \(model.params.sort.title)", systemImage: "arrow.up.arrow.down")
            }
            .accessibilityIdentifier("library.sort")
            Button {
                model.flipOrder()
            } label: {
                Label(model.params.effectiveOrder == .ascending ? "Ascending" : "Descending",
                      systemImage: model.params.effectiveOrder == .ascending ? "arrow.up" : "arrow.down")
            }
            .accessibilityIdentifier("library.order")
            if model.filterable {
                Menu {
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
                } label: {
                    Label(model.params.filtered ? "Filter: \(model.filterSummary)" : "Filter",
                          systemImage: "line.3.horizontal.decrease.circle")
                }
                .accessibilityIdentifier("library.filter")
                if model.params.filtered {
                    Button("Clear") { model.clearFilters() }
                        .accessibilityLabel("Clear filters")
                        .accessibilityIdentifier("library.filter.clear")
                }
            }
        }
        .focusSection()
        .padding(.horizontal, Metrics.gutter)
    }
}

/// The letters, when the listing has them, and the grid.
private struct GridPage: View {
    let app: AppModel
    @ObservedObject var model: LibraryModel
    @ObservedObject var grid: LibraryGrid

    var body: some View {
        if let total = grid.total {
            if total == 0 {
                if model.params.filtered {
                    StateView(kind: .empty(symbol: "line.3.horizontal.decrease.circle", text: LibraryModel.noMatchTitle,
                                           action: ("Clear filters", { model.clearFilters() })))
                } else {
                    StateView(kind: .empty(symbol: "film.stack", text: LibraryModel.emptyTitle, action: nil))
                }
            } else {
                ScrollViewReader { reader in
                    VStack(alignment: .leading, spacing: 16) {
                        if let alpha = model.alpha {
                            Letters(alpha: alpha) { reader.scrollTo(alpha.jumpTarget($0), anchor: .top) }
                        }
                        ScrollView {
                            TileGrid(app: app, shape: grid.shape, count: total, tile: { grid.tile(at: $0) },
                                     showing: { grid.showing($0) }) { tile in
                                app.shell.open(item: tile.item)
                            }
                            .padding(.vertical, 40)
                        }
                        .scrollClipDisabled()
                        .accessibilityIdentifier("library.grid")
                    }
                }
            }
        } else if let failure = grid.failure {
            StateView(kind: .failed(title: LibraryModel.failedTitle, why: failure) { Task { await grid.reload() } })
        } else {
            StateView(kind: .loading)
        }
    }
}

/// "#" and A to Z in a row: Select jumps the grid to the letter, or the
/// next that has titles; letters without titles are dim.
private struct Letters: View {
    let alpha: AlphaIndex
    let jump: (Int) -> Void

    var body: some View {
        HStack(spacing: 4) {
            ForEach(AlphaIndex.letters.indices, id: \.self) { letter in
                Button {
                    jump(letter)
                } label: {
                    Text(AlphaIndex.letters[letter])
                        .font(.callout.weight(.semibold))
                        .frame(minWidth: 28)
                        .opacity(alpha.has(letter) ? 1 : 0.35)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Jump to \(AlphaIndex.letters[letter])")
                .accessibilityIdentifier("library.alpha.\(AlphaIndex.letters[letter])")
            }
        }
        .focusSection()
        .padding(.horizontal, Metrics.gutter)
    }
}
