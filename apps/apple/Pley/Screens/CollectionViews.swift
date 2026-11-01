import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// A library's collections in a grid: its Collections tab, and the page
/// of one library's collections.
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
        case .loaded where model.cards.isEmpty:
            StateView(kind: .empty(symbol: "square.stack", text: CollectionsModel.emptyTitle, action: nil))
        case .loaded:
            ScrollView {
                LazyVGrid(columns: columns, alignment: .leading, spacing: 20) {
                    ForEach(model.cards) { card in
                        CollectionTile(app: app, card: card, width: width) { app.shell.open(.collection(card.id)) }
                    }
                }
                .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                .padding(.vertical)
            }
            .refreshable { await model.load() }
            .accessibilityIdentifier("library.collections")
        }
    }
}

/// The Recommended tab's last row: the library's collections.
struct CollectionsRow: View {
    let app: AppModel
    @ObservedObject var model: CollectionsModel
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1

    var body: some View {
        if !model.cards.isEmpty {
            let width = Metrics.tileWidth(.poster, regular: layout.wide) * min(max(typeScale, 1), Metrics.maxTileScale)
            VStack(alignment: .leading, spacing: 10) {
                Text("Collections")
                    .font(.title3.bold())
                    .accessibilityAddTraits(.isHeader)
                    .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                ScrollView(.horizontal, showsIndicators: false) {
                    LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                        ForEach(model.cards) { card in
                            CollectionTile(app: app, card: card, width: width) {
                                app.shell.open(.collection(card.id))
                            }
                        }
                    }
                    .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("library.row.collections")
        }
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
                if model.automatic || model.smart {
                    HStack(spacing: 6) {
                        if model.automatic { ListBadge(text: "TMDB") }
                        if model.smart { ListBadge(text: "Smart") }
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
