import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// A library's collections in grids under their categories, with the
/// Category filter: its Collections tab, and the page of one library's
/// collections.
struct CollectionsGrid: View {
    let app: AppModel
    @ObservedObject var model: CollectionsModel
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1
    @Environment(\.dynamicTypeSize) private var typeSize

    private var width: CGFloat {
        Metrics.tileWidth(.poster, regular: layout.wide) * min(max(typeScale, 1), Metrics.maxTileScale)
    }

    private var columns: [GridItem] {
        if !layout.wide && typeSize.isAccessibilitySize {
            return Array(repeating: GridItem(.flexible(), spacing: Metrics.tileSpacing, alignment: .top), count: 2)
        }
        return [GridItem(.adaptive(minimum: width, maximum: width * 1.4), spacing: Metrics.tileSpacing,
                         alignment: .top)]
    }

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
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    filters
                    let groups = model.groups
                    ForEach(groups) { group in
                        VStack(alignment: .leading, spacing: 10) {
                            if model.showsHeadings {
                                HStack(alignment: .firstTextBaseline, spacing: 8) {
                                    Text(group.title)
                                        .font(.title3.bold())
                                        .accessibilityAddTraits(.isHeader)
                                    Text(group.cards.count.formatted(.number.locale(ShownLanguage.locale)))
                                        .font(.subheadline.monospacedDigit())
                                        .foregroundStyle(.secondary)
                                        .accessibilityHidden(true)
                                }
                                .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                            }
                            LazyVGrid(columns: columns, alignment: .leading, spacing: 20) {
                                ForEach(group.cards) { card in
                                    CollectionTile(app: app, card: card, width: width) {
                                        app.shell.open(.collection(card.id))
                                    }
                                }
                            }
                            .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                        }
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("collections.group.\(group.id)")
                    }
                }
                .padding(.vertical)
            }
            .refreshable { await model.load() }
            .accessibilityIdentifier("library.collections")
        }
    }

    /// Category (when there is more than one to choose between), Clear, and how many show.
    private var filters: some View {
        FlowLayout(spacing: 8) {
            if model.offersCategories {
                let chosen = model.chosenCategory
                Menu {
                    Picker("Category", selection: Binding(get: { model.chosenCategory }, set: { model.category = $0 })) {
                        Text(model.title(of: .all)).tag(CategoryChoice.all)
                        ForEach(model.categoryChoices) { group in
                            Text(group.title).tag(group.choice ?? .all)
                        }
                    }
                } label: {
                    FilterLabel(text: chosen == .all ? localized("Category") : model.title(of: chosen), on: chosen != .all)
                }
                .accessibilityLabel("Category")
                .accessibilityValue(model.title(of: chosen))
                .accessibilityIdentifier("collections.category")
            }
            if model.filtered {
                Button("Clear filters") { model.clearFilters() }
                    .font(.subheadline)
                    .accessibilityIdentifier("collections.clear")
            }
            Text(model.countWords)
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(.secondary)
                .padding(.vertical, 6)
                .accessibilityIdentifier("collections.count")
        }
        .menuStyle(.button)
        .buttonStyle(.bordered)
        .padding(.horizontal, Metrics.gutter(regular: layout.wide))
    }
}

/// A filter's button: its name, or what it is set to, tinted when set.
private struct FilterLabel: View {
    let text: String
    let on: Bool

    var body: some View {
        HStack(spacing: 4) {
            Text(text)
            Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).accessibilityHidden(true)
        }
        .font(.subheadline)
        .foregroundStyle(on ? Color.accentColor : Color.primary)
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
            .navigationTitle("Collections")
    }
}

/// A collection's page: its backdrop, poster and title, the TMDB or Smart
/// badge, what it holds in words, Play and Shuffle, then its titles.
struct CollectionView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: CollectionModel
    private let layout = WideLayout()
    private let launcher = Launcher()
    @Environment(\.dynamicTypeSize) private var typeSize

    init(app: AppModel, collectionId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: CollectionModel(app: app, collectionId: collectionId))
    }

    var body: some View {
        content
            .navigationTitle(model.title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .task { await model.load() }
            .reloads(app) { await model.load() }
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
                VStack(alignment: .leading, spacing: 20) {
                    header
                    if model.tiles.isEmpty {
                        Text("Nothing in this collection yet")
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                    } else {
                        TileGrid(app: app, shape: .poster, count: model.tiles.count,
                                 tile: { model.tiles.indices.contains($0) ? model.tiles[$0] : nil }) { tile in
                            app.shell.open(item: tile.item)
                        }
                        .accessibilityIdentifier("collection.items")
                    }
                }
                .padding(.bottom)
            }
            .refreshable { await model.load() }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 14) {
            ServerImage(app: app, paths: model.backdrop, width: 1200, symbol: nil, limit: ImageLoader.deviceBackdropLimit)
                .frame(height: layout.wide ? 260 : 180)
                .frame(maxWidth: .infinity)
                .clipped()
            let stacked = typeSize.isAccessibilitySize
            let facts = VStack(alignment: .leading, spacing: 6) {
                if model.automatic || model.smart || model.category != nil {
                    HStack(spacing: 6) {
                        if let category = model.category {
                            Text(category)
                                .font(.caption.weight(.semibold))
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
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("collection.notes")
                if layout.wide && !stacked { buttons.padding(.top, 4) }
            }
            Group {
                if stacked {
                    VStack(alignment: .leading, spacing: 12) { poster; facts; buttons }
                } else if layout.wide {
                    HStack(alignment: .top, spacing: 16) { poster; facts }
                } else {
                    // On iPhone the buttons go under the poster, at the page's width.
                    VStack(alignment: .leading, spacing: 14) {
                        HStack(alignment: .top, spacing: 16) { poster; facts }
                        buttons
                    }
                }
            }
            .padding(.horizontal, Metrics.gutter(regular: layout.wide))
            if let summary = model.summary {
                Text(summary)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 760, alignment: .leading)
                    .padding(.horizontal, Metrics.gutter(regular: layout.wide))
            }
        }
    }

    private var buttons: some View {
        PlayShuffleButtons(prefix: "collection", canPlay: model.canPlay, canShuffle: model.canShuffle,
                           play: { Task { launcher.play(app, await model.play(shuffle: false)) } },
                           shuffle: { Task { launcher.play(app, await model.play(shuffle: true)) } })
    }

    private var poster: some View {
        ServerImage(app: app, paths: model.poster, width: 110, symbol: "square.stack")
            .frame(width: 110, height: 165)
            .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
    }
}

/// The page of a Home row made of several libraries' collections of one
/// title: its title, the count and the libraries they come from (each
/// opens its own collection), Play and Shuffle, then every title.
struct MergedCollectionView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: MergedCollectionModel
    private let layout = WideLayout()
    private let launcher = Launcher()

    init(app: AppModel, ids: [Int]) {
        self.app = app
        _model = StateObject(wrappedValue: MergedCollectionModel(app: app, ids: ids))
    }

    var body: some View {
        content
            .navigationTitle(model.title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .task { await model.load() }
            .reloads(app) { await model.load() }
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
                VStack(alignment: .leading, spacing: 20) {
                    header
                    TileGrid(app: app, shape: .poster, count: model.tiles.count,
                             tile: { model.tiles.indices.contains($0) ? model.tiles[$0] : nil }) { tile in
                        app.shell.open(item: tile.item)
                    }
                    .accessibilityIdentifier("collection.items")
                }
                .padding(.vertical)
            }
            .refreshable { await model.load() }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Collection", systemImage: "square.stack")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.secondary)
            Text(model.title)
                .font(.title2.bold())
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("collection.title")
            FlowLayout(spacing: 6) {
                Text(model.count)
                    .foregroundStyle(.secondary)
                ForEach(model.sources) { source in
                    Text(verbatim: "·").foregroundStyle(.secondary).accessibilityHidden(true)
                    Button(source.library) { app.shell.open(.collection(source.id)) }
                        .buttonStyle(.borderless)
                        .accessibilityHint("Opens this library's collection")
                        .accessibilityIdentifier("collection.source.\(source.id)")
                }
            }
            .font(.subheadline)
            PlayShuffleButtons(prefix: "collection", canPlay: model.canPlay, canShuffle: model.canShuffle,
                               play: { Task { launcher.play(app, await model.play(shuffle: false)) } },
                               shuffle: { Task { launcher.play(app, await model.play(shuffle: true)) } })
                .padding(.top, 4)
        }
        .padding(.horizontal, Metrics.gutter(regular: layout.wide))
    }
}
