import PleyData
import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// A library's collections in grids under their categories, with the
/// Category filter: its Collections tab, and the page of one library's
/// collections.
struct CollectionsGrid: View {
    let app: AppModel
    @ObservedObject var model: CollectionsModel

    var body: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
                .task { await model.load() }
        case .failed(let why):
            StateView(kind: .failed(title: CollectionsModel.failedTitle, why: why) { Task { await model.load() } })
        case .loaded where model.collections.isEmpty:
            StateView(kind: .empty(symbol: "square.stack", text: CollectionsModel.emptyTitle, action: nil))
        case .loaded:
            let width = Metrics.tileWidth(.poster)
            let groups = model.groups
            ScrollView {
                VStack(alignment: .leading, spacing: 40) {
                    filters
                    ForEach(groups) { group in
                        VStack(alignment: .leading, spacing: 0) {
                            if model.showsHeadings {
                                Text(group.title)
                                    .font(.headline)
                                    .accessibilityAddTraits(.isHeader)
                                    .padding(.horizontal, Metrics.gutter)
                            }
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: width, maximum: width),
                                                         spacing: Metrics.tileSpacing, alignment: .top)],
                                      alignment: .leading, spacing: Metrics.rowSpacing) {
                                ForEach(group.cards) { card in
                                    CollectionTile(app: app, card: card, width: width) {
                                        app.shell.open(.collection(card.id))
                                    }
                                }
                            }
                            .padding(.horizontal, Metrics.gutter)
                            .padding(.vertical, 32)
                        }
                        .focusSection()
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("collections.group.\(group.id)")
                    }
                }
                .padding(.vertical, 40)
            }
            .scrollClipDisabled()
            // Down from any tab above reaches the grid, however few it holds.
            .focusSection()
            .accessibilityIdentifier("library.collections")
        }
    }

    /// Category (when there is more than one to choose between), Clear, and how many show.
    private var filters: some View {
        HStack(spacing: 24) {
            if model.offersCategories {
                // tvOS gives a menu's button no accessibility label of its own: it is set here.
                let chosen = model.chosenCategory
                Menu(chosen == .all ? localized("Category") : localized("Category: \(model.title(of: chosen))"),
                     systemImage: "square.stack") {
                    Picker("Category", selection: Binding(get: { model.chosenCategory }, set: { model.category = $0 })) {
                        Text(model.title(of: .all)).tag(CategoryChoice.all)
                        ForEach(model.categoryChoices) { group in
                            Text(group.title).tag(group.choice ?? .all)
                        }
                    }
                }
                .accessibilityLabel("Category")
                .accessibilityValue(model.title(of: chosen))
                .accessibilityIdentifier("collections.category")
            }
            if model.filtered {
                Button("Clear filters") { model.clearFilters() }
                    .accessibilityIdentifier("collections.clear")
            }
            Text(model.countWords)
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("collections.count")
        }
        .focusSection()
        .padding(.horizontal, Metrics.gutter)
    }
}

/// One library's collections, as a page of their own.
struct CollectionsPage: View {
    let app: AppModel
    @StateObject private var model: CollectionsModel

    init(app: AppModel, libraryId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: CollectionsModel(
            app: app, libraryId: libraryId, kind: app.shell.libraries.first { $0.id == libraryId }?.kind))
    }

    var body: some View {
        CollectionsGrid(app: app, model: model)
    }
}

/// A collection's page: its backdrop behind, its poster, title, badges and
/// what it holds in words, Play and Shuffle, then its titles in a grid.
struct CollectionView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: CollectionModel

    init(app: AppModel, collectionId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: CollectionModel(app: app, collectionId: collectionId))
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
            StateView(kind: .failed(title: CollectionModel.failedTitle, why: why) { Task { await model.load() } })
        case .notFound:
            StateView(kind: .empty(symbol: "square.stack", text: CollectionModel.notFoundTitle, action: nil))
        case .loaded:
            ScrollView {
                VStack(alignment: .leading, spacing: 40) {
                    header
                    if !model.tiles.isEmpty {
                        TileGrid(app: app, shape: .poster, count: model.tiles.count,
                                 tile: { model.tiles.indices.contains($0) ? model.tiles[$0] : nil }) { tile in
                            app.shell.open(item: tile.item)
                        }
                        .focusSection()
                        .accessibilityIdentifier("collection.items")
                    }
                }
                .padding(.vertical, 40)
            }
            .scrollClipDisabled()
            .background {
                ServerImage(app: app, paths: model.backdrop, width: 1920, symbol: nil, placeholder: false,
                            limit: ImageLoader.deviceBackdropLimit)
                    .overlay(Color.black.opacity(0.7))
                    .ignoresSafeArea()
            }
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 48) {
            ServerImage(app: app, paths: model.poster, width: 260, symbol: "square.stack")
                .frame(width: 260, height: 390)
                .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            VStack(alignment: .leading, spacing: 16) {
                if model.automatic || model.smart || model.category != nil {
                    HStack(spacing: 12) {
                        if let category = model.category {
                            Text(category)
                                .font(.callout.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .accessibilityLabel("Category: \(category)")
                                .accessibilityIdentifier("collection.category")
                        }
                        if model.automatic { ListBadge(text: "TMDB") }
                        if model.smart { ListBadge(text: localized("Smart")) }
                    }
                }
                Text(model.title)
                    .font(.title2.bold())
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("collection.title")
                Text(([model.count] + model.notes).joined(separator: " · "))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                if let summary = model.summary {
                    Text(summary).font(.callout).lineLimit(4)
                }
                PlayShuffleButtons(prefix: "collection", canPlay: model.canPlay, canShuffle: model.canShuffle,
                                   play: { Task { await play(model.play(shuffle: false)) } },
                                   shuffle: { Task { await play(model.play(shuffle: true)) } })
                    .focusSection()
                    .padding(.top, 8)
            }
        }
        .padding(.horizontal, Metrics.gutter)
    }

    private func play(_ request: PlayRequest?) async {
        guard let request else { return }
        await app.play(request)
    }
}

/// The page of a Home row made of several libraries' collections of one
/// title: its title, the count and the libraries they come from (each
/// opens its own collection), Play and Shuffle, then every title.
struct MergedCollectionView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: MergedCollectionModel

    init(app: AppModel, ids: [Int]) {
        self.app = app
        _model = StateObject(wrappedValue: MergedCollectionModel(app: app, ids: ids))
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
            StateView(kind: .failed(title: CollectionModel.failedTitle, why: why) { Task { await model.load() } })
        case .notFound:
            StateView(kind: .empty(symbol: "square.stack", text: CollectionModel.notFoundTitle, action: nil))
        case .loaded:
            ScrollView {
                VStack(alignment: .leading, spacing: 40) {
                    header
                    TileGrid(app: app, shape: .poster, count: model.tiles.count,
                             tile: { model.tiles.indices.contains($0) ? model.tiles[$0] : nil }) { tile in
                        app.shell.open(item: tile.item)
                    }
                    .focusSection()
                    .accessibilityIdentifier("collection.items")
                }
                .padding(.vertical, 40)
            }
            .scrollClipDisabled()
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label("Collection", systemImage: "square.stack")
                .font(.callout.weight(.medium))
                .foregroundStyle(.secondary)
            Text(model.title)
                .font(.title2.bold())
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("collection.title")
            Text(model.count)
                .font(.callout)
                .foregroundStyle(.secondary)
            HStack(spacing: 24) {
                PlayShuffleButtons(prefix: "collection", canPlay: model.canPlay, canShuffle: model.canShuffle,
                                   play: { Task { await play(model.play(shuffle: false)) } },
                                   shuffle: { Task { await play(model.play(shuffle: true)) } })
                // Each library's own collection, to open on its own.
                ForEach(model.sources) { source in
                    Button(source.library) { app.shell.open(.collection(source.id)) }
                        .accessibilityHint("Opens this library's collection")
                        .accessibilityIdentifier("collection.source.\(source.id)")
                }
            }
            .focusSection()
            .padding(.top, 8)
        }
        .padding(.horizontal, Metrics.gutter)
    }

    private func play(_ request: PlayRequest?) async {
        guard let request else { return }
        await app.play(request)
    }
}
