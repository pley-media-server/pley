import PleyModel
import PleyScreens
import SwiftUI

/// A library's page: Recommended (its own rows, then its collections),
/// Library (every title, sorted and filtered, in a grid read 100 at a time)
/// and Collections (not for music). Sort is a menu with the order beside
/// it; the filters are a sheet on iPhone and a menu on iPad and Mac; More
/// has Save as Smart Playlist. Sorted by title, a strip of letters down the
/// side jumps through the grid. The page opens as it was left, or as See
/// all asks.
struct LibraryView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: LibraryModel
    @State private var filtering = false
    private let layout = WideLayout()

    init(app: AppModel, libraryId: Int, view: String? = nil) {
        self.app = app
        _model = StateObject(wrappedValue: LibraryModel(app: app, libraryId: libraryId, view: view))
    }

    var body: some View {
        content
            .navigationTitle(model.title)
            .toolbar { toolbar }
            .sheet(isPresented: $filtering) {
                NavigationStack {
                    LibraryFiltersForm(model: model)
                        .navigationTitle("Filters")
                        #if os(iOS)
                        .navigationBarTitleDisplayMode(.inline)
                        #endif
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button("Done") { filtering = false }
                                    .accessibilityIdentifier("library.filter.done")
                            }
                        }
                }
                .presentationDetents([.medium, .large])
            }
            .task { await model.load() }
            .reloads(app) { await model.reload() }
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
            VStack(spacing: 0) {
                if model.tabs.count > 1 {
                    Picker("View", selection: Binding(get: { model.params.tab }, set: { model.select($0) })) {
                        ForEach(model.tabs, id: \.self) { tab in
                            Text(tab.title).tag(tab)
                        }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .frame(maxWidth: 420)
                    .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                    .padding(.vertical, 8)
                    .accessibilityIdentifier("library.tabs")
                }
                if let notice = model.notice {
                    Text(asSentence(notice))
                        .font(.footnote)
                        .foregroundStyle(.red)
                        .padding(.horizontal)
                }
                switch model.params.tab {
                case .recommended:
                    recommended
                case .collections:
                    CollectionsGrid(app: app, model: model.collections)
                case .library:
                    Text(listing)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                        .padding(.bottom, 4)
                        .accessibilityIdentifier("library.listing")
                    if let grid = model.grid {
                        LibraryGridView(app: app, model: model, grid: grid)
                            // Another listing starts at its top.
                            .id(ObjectIdentifier(grid))
                    } else {
                        StateView(kind: .loading)
                    }
                }
            }
        }
    }

    @ViewBuilder private var recommended: some View {
        if let rows = model.rows {
            if rows.isEmpty && model.collections.cards.isEmpty {
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
                        CollectionsRow(app: app, model: model.collections)
                    }
                    .padding(.vertical)
                }
                .refreshable { await model.reload() }
            }
        } else {
            StateView(kind: .loading)
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        if model.state == .loaded, model.params.tab == .library {
            ToolbarItemGroup(placement: .primaryAction) {
                Menu {
                    Picker("Sort by", selection: Binding(get: { model.params.sort }, set: { model.sort(by: $0) })) {
                        ForEach(model.sorts, id: \.self) { sort in
                            Text(sort.title).tag(sort)
                        }
                    }
                    Divider()
                    Picker("Order", selection: Binding(get: { model.params.effectiveOrder },
                                                       set: { if $0 != model.params.effectiveOrder { model.flipOrder() } })) {
                        Text("Ascending").tag(SortOrder.ascending)
                        Text("Descending").tag(SortOrder.descending)
                    }
                } label: {
                    Label("Sort", systemImage: "arrow.up.arrow.down")
                }
                .accessibilityLabel("Sort")
                .accessibilityValue("\(model.params.sort.title), "
                    + (model.params.effectiveOrder == .ascending ? "ascending" : "descending"))
                .accessibilityIdentifier("library.sort")

                if model.filterable {
                    if layout.wide {
                        Menu {
                            LibraryFiltersMenu(model: model)
                        } label: {
                            filterLabel
                        }
                        .accessibilityLabel("Filter")
                        .accessibilityValue(model.filterSummary)
                        .accessibilityIdentifier("library.filter")
                    } else {
                        Button { filtering = true } label: { filterLabel }
                            .accessibilityLabel("Filter")
                            .accessibilityValue(model.filterSummary)
                            .accessibilityIdentifier("library.filter")
                    }
                }
                Menu {
                    Button { model.saveAs() } label: {
                        Label("Save as Smart Playlist…", systemImage: "text.badge.plus")
                    }
                    .accessibilityIdentifier("library.saveAs")
                } label: {
                    Label("More", systemImage: "ellipsis.circle")
                }
                .accessibilityIdentifier("library.more")
                #if os(macOS)
                Button {
                    Task { await model.reload() }
                } label: {
                    Label("Reload", systemImage: "arrow.clockwise")
                }
                .accessibilityIdentifier("library.reload")
                #endif
            }
        }
    }

    /// How the titles are listed: "Year, descending · Unwatched".
    private var listing: String {
        let order = model.params.effectiveOrder == .ascending ? "ascending" : "descending"
        return (["\(model.params.sort.title), \(order)", model.filterSummary]).filter { !$0.isEmpty }
            .joined(separator: " · ")
    }

    private var filterLabel: some View {
        Label("Filter", systemImage: model.params.filtered
            ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
    }
}

/// The Library tab's grid, with its A to Z when it has one, and what it
/// says when there is nothing (Clear filters, when filters hide everything).
private struct LibraryGridView: View {
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
                    ScrollView {
                        TileGrid(app: app, shape: grid.shape, count: total, tile: { grid.tile(at: $0) },
                                 showing: { grid.showing($0) }) { tile in
                            app.shell.open(item: tile.item)
                        }
                        .padding(.vertical)
                        // Room for the letters.
                        .padding(.trailing, model.alpha == nil ? 0 : 18)
                    }
                    .refreshable { await model.reload() }
                    .overlay(alignment: .trailing) {
                        if let alpha = model.alpha {
                            AlphaStrip(alpha: alpha) { letter in
                                reader.scrollTo(alpha.jumpTarget(letter), anchor: .top)
                            }
                        }
                    }
                }
                .accessibilityIdentifier("library.grid")
            }
        } else if let failure = grid.failure {
            StateView(kind: .failed(title: LibraryModel.failedTitle, why: failure) { Task { await grid.reload() } })
        } else {
            StateView(kind: .loading)
        }
    }
}

/// "#" and A to Z down the side: a tap or a drag along it jumps to the
/// letter, or the next that has titles; letters without titles are dim.
/// VoiceOver reads it as one control, "Jump to letter", swiped up and down.
private struct AlphaStrip: View {
    let alpha: AlphaIndex
    let jump: (Int) -> Void
    @State private var spoken = 0
    @State private var height: CGFloat = 1

    var body: some View {
        VStack(spacing: 0) {
            ForEach(AlphaIndex.letters.indices, id: \.self) { letter in
                Text(AlphaIndex.letters[letter])
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(alpha.has(letter) ? Color.accentColor : Color.secondary.opacity(0.4))
                    .frame(width: 18)
                    .frame(maxHeight: 16)
                    .accessibilityIdentifier("library.alpha.\(AlphaIndex.letters[letter])")
            }
        }
        .padding(.vertical, 6)
        .background(GeometryReader { geometry in
            Color.clear.onAppear { height = geometry.size.height }
                .onValueChange(of: geometry.size.height) { height = $0 }
        })
        .contentShape(Rectangle())
        .gesture(DragGesture(minimumDistance: 0).onChanged { value in
            let count = AlphaIndex.letters.count
            let letter = min(count - 1, max(0, Int(value.location.y / max(height, 1) * CGFloat(count))))
            jump(letter)
        })
        .padding(.trailing, 2)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Jump to letter")
        .accessibilityValue(AlphaIndex.letters[spoken])
        .accessibilityAdjustableAction { direction in
            let present = alpha.present
            switch direction {
            case .increment: spoken = present.first { $0 > spoken } ?? spoken
            case .decrement: spoken = present.last { $0 < spoken } ?? spoken
            @unknown default: return
            }
            jump(spoken)
        }
        .accessibilityIdentifier("library.alpha")
    }
}

/// The filters as a form: the iPhone's sheet.
private struct LibraryFiltersForm: View {
    @ObservedObject var model: LibraryModel

    var body: some View {
        Form {
            Section {
                Picker("Show", selection: Binding(get: { model.params.watched }, set: { model.filter(watched: $0) })) {
                    Text("All").tag(Bool?.none)
                    Text("Unwatched").tag(Bool?.some(false))
                    Text("Watched").tag(Bool?.some(true))
                }
                .pickerStyle(.inline)
                .labelsHidden()
                .accessibilityIdentifier("library.filter.watched")
            }
            if !model.filters.genres.isEmpty {
                Picker("Genre", selection: Binding(get: { model.params.genre }, set: { model.filter(genre: $0) })) {
                    Text("Any").tag(String?.none)
                    ForEach(model.filters.genres, id: \.self) { Text($0).tag(String?.some($0)) }
                }
                .accessibilityIdentifier("library.filter.genre")
            }
            if !model.filters.years.isEmpty {
                Picker("Year", selection: Binding(get: { model.params.year }, set: { model.filter(year: $0) })) {
                    Text("Any").tag(Int?.none)
                    ForEach(model.filters.years, id: \.self) { Text(String($0)).tag(Int?.some($0)) }
                }
                .accessibilityIdentifier("library.filter.year")
            }
            if model.params.filtered {
                Button("Clear Filters", role: .destructive) { model.clearFilters() }
                    .accessibilityIdentifier("library.filter.clear")
            }
        }
    }
}

/// The filters as a menu: iPad and Mac.
private struct LibraryFiltersMenu: View {
    @ObservedObject var model: LibraryModel

    var body: some View {
        Picker("Show", selection: Binding(get: { model.params.watched }, set: { model.filter(watched: $0) })) {
            Text("All").tag(Bool?.none)
            Text("Unwatched").tag(Bool?.some(false))
            Text("Watched").tag(Bool?.some(true))
        }
        if !model.filters.genres.isEmpty {
            Picker("Genre", selection: Binding(get: { model.params.genre }, set: { model.filter(genre: $0) })) {
                Text("Any").tag(String?.none)
                ForEach(model.filters.genres, id: \.self) { Text($0).tag(String?.some($0)) }
            }
            .pickerStyle(.menu)
        }
        if !model.filters.years.isEmpty {
            Picker("Year", selection: Binding(get: { model.params.year }, set: { model.filter(year: $0) })) {
                Text("Any").tag(Int?.none)
                ForEach(model.filters.years, id: \.self) { Text(String($0)).tag(Int?.some($0)) }
            }
            .pickerStyle(.menu)
        }
        if model.params.filtered {
            Divider()
            Button("Clear Filters", role: .destructive) { model.clearFilters() }
        }
    }
}
