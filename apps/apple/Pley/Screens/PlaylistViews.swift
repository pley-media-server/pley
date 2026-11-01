import PleyModel
import PleyScreens
import SwiftUI

/// The profile's playlists by title, each with its mosaic, and New
/// Playlist in the toolbar.
struct PlaylistsView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: PlaylistsModel
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1
    @Environment(\.dynamicTypeSize) private var typeSize

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: PlaylistsModel(app: app))
    }

    private var width: CGFloat {
        (layout.wide ? 170 : 150) * min(max(typeScale, 1), Metrics.maxTileScale)
    }

    private var columns: [GridItem] {
        if !layout.wide && typeSize.isAccessibilitySize {
            return [GridItem(.flexible(), alignment: .top)]
        }
        return [GridItem(.adaptive(minimum: width, maximum: width * 1.3), spacing: Metrics.tileSpacing, alignment: .top)]
    }

    var body: some View {
        content
            .navigationTitle("Playlists")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button { model.new() } label: {
                        Label("New Playlist", systemImage: "plus")
                    }
                    .accessibilityIdentifier("playlists.new")
                }
            }
            // Read again each time it shows: playlists change from other pages.
            .task { await model.load() }
            .reloads(app) { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: PlaylistsModel.failedTitle, why: why) { Task { await model.load() } })
        case .loaded where model.cards.isEmpty:
            StateView(kind: .empty(symbol: "music.note.list", text: PlaylistsModel.emptyTitle,
                                   action: ("New Playlist", { model.new() })))
        case .loaded:
            ScrollView {
                if let notice = model.notice {
                    Text(asSentence(notice)).font(.footnote).foregroundStyle(.red).padding(.horizontal)
                }
                LazyVGrid(columns: columns, alignment: .leading, spacing: 20) {
                    ForEach(model.cards) { card in
                        PlaylistTile(app: app, card: card, width: width) { app.shell.open(.playlist(card.id)) }
                    }
                }
                .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                .padding(.vertical)
            }
            .refreshable { await model.load() }
            .accessibilityIdentifier("playlists.grid")
        }
    }
}

/// One playlist: its mosaic, title, Smart badge, count and length, its
/// filter in words, Play and Shuffle, then its entries. On iPhone and iPad,
/// Edit moves entries and a swipe removes one; on the Mac they are dragged,
/// and removed from their context menu. A removal can be undone for 5
/// seconds. Rename and Delete are in the More menu. Smart playlists are
/// read-only.
struct PlaylistView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: PlaylistModel
    @State private var renaming = false
    @State private var newTitle = ""
    @State private var deleting = false
    private let layout = WideLayout()
    private let launcher = Launcher()
    @Environment(\.dynamicTypeSize) private var typeSize

    init(app: AppModel, playlistId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: PlaylistModel(app: app, playlistId: playlistId))
    }

    var body: some View {
        content
            .navigationTitle(model.title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar { toolbar }
            .alert("Rename Playlist", isPresented: $renaming) {
                TextField("Title", text: $newTitle)
                    .accessibilityIdentifier("playlist.rename.title")
                Button("Rename") { Task { await model.rename(newTitle) } }
                    .accessibilityIdentifier("playlist.rename.save")
                Button("Cancel", role: .cancel) {}
            }
            .confirmationDialog("Delete \(model.title)?", isPresented: $deleting, titleVisibility: .visible) {
                Button("Delete Playlist", role: .destructive) { Task { await model.delete() } }
                    .accessibilityIdentifier("playlist.delete.confirm")
            } message: {
                Text("The titles in it stay in your libraries.")
            }
            .task { await model.load() }
            .reloads(app) { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: PlaylistModel.failedTitle, why: why) { Task { await model.load() } })
        case .notFound:
            StateView(kind: .empty(symbol: "music.note.list", text: PlaylistModel.notFoundTitle, action: nil))
        case .loaded:
            List {
                Section {
                    header
                        .listRowSeparator(.hidden)
                }
                Section {
                    if model.rows.isEmpty {
                        Text(PlaylistModel.emptyTitle).foregroundStyle(.secondary)
                    }
                    ForEach(model.rows) { row in
                        entry(row)
                    }
                    .onMove(perform: model.editable ? { model.move(fromOffsets: $0, toOffset: $1) } : nil)
                }
            }
            #if os(macOS)
            .listStyle(.inset)
            #else
            .listStyle(.plain)
            #endif
            .refreshable { await model.load() }
            .accessibilityIdentifier("playlist.entries")
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        if model.editable {
            #if os(iOS)
            ToolbarItem(placement: .primaryAction) {
                EditButton().accessibilityIdentifier("playlist.edit")
            }
            #endif
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button {
                        newTitle = model.title
                        renaming = true
                    } label: {
                        Label("Rename…", systemImage: "pencil")
                    }
                    .accessibilityIdentifier("playlist.rename")
                    Button(role: .destructive) { deleting = true } label: {
                        Label("Delete…", systemImage: "trash")
                    }
                    .accessibilityIdentifier("playlist.delete")
                } label: {
                    Label("More", systemImage: "ellipsis.circle")
                }
                .accessibilityIdentifier("playlist.more")
            }
        }
    }

    private var header: some View {
        let facts = VStack(alignment: .leading, spacing: 6) {
            if model.smart { ListBadge(text: "Smart") }
            Text(model.title)
                .font(.title2.bold())
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("playlist.title")
            Text(model.meta)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("playlist.meta")
            if !model.notes.isEmpty {
                Text(model.notes.joined(separator: " · "))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if layout.wide && !typeSize.isAccessibilitySize { buttons.padding(.top, 4) }
        }
        return Group {
            if typeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 12) {
                    PlaylistMosaic(app: app, artwork: model.artwork, video: model.video, size: 140)
                    facts
                    buttons
                }
            } else if layout.wide {
                HStack(alignment: .top, spacing: 16) {
                    PlaylistMosaic(app: app, artwork: model.artwork, video: model.video, size: 180)
                    facts
                }
            } else {
                // On iPhone the buttons go under the mosaic, at the page's width.
                VStack(alignment: .leading, spacing: 14) {
                    HStack(alignment: .top, spacing: 16) {
                        PlaylistMosaic(app: app, artwork: model.artwork, video: model.video, size: 120)
                        facts
                    }
                    buttons
                }
            }
        }
        .padding(.vertical, 8)
    }

    private var buttons: some View {
        PlayShuffleButtons(prefix: "playlist", canPlay: model.canPlay, canShuffle: model.canShuffle,
                           play: { launcher.play(app, model.play()) },
                           shuffle: { launcher.play(app, model.play(shuffle: true)) })
    }

    private func entry(_ row: PlaylistRow) -> some View {
        Button {
            open(row)
        } label: {
            EntryRow(app: app, row: row, video: model.video)
        }
        .buttonStyle(.plain)
        #if os(iOS)
        .hoverEffect(.highlight)
        .swipeActions(edge: .trailing) {
            if model.editable {
                Button(role: .destructive) { model.remove(row.entryId) } label: {
                    Label("Remove", systemImage: "minus.circle")
                }
                .accessibilityIdentifier("playlist.entry.remove")
            }
        }
        #endif
        .contextMenu { menu(row) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.spokenLabel)
        .accessibilityHint(model.video ? "Plays from here" : "Opens its album")
        .accessibilityAddTraits(.isButton)
        .accessibilityActions { menu(row) }
        .accessibilityIdentifier("playlist.entry.\(row.entryId)")
    }

    /// An entry's actions: its context menu, and VoiceOver's.
    @ViewBuilder private func menu(_ row: PlaylistRow) -> some View {
        if model.video {
            Button { launcher.play(app, model.play(from: row.number - 1)) } label: {
                Label("Play from Here", systemImage: "play.fill")
            }
        }
        Button { app.shell.open(item: row.item) } label: {
            Label(model.video ? "Go to Page" : "Go to Album", systemImage: "info.circle")
        }
        if model.editable {
            if row.number > 1 {
                Button { model.move(row.entryId, by: -1) } label: { Label("Move Up", systemImage: "arrow.up") }
                    .accessibilityIdentifier("playlist.entry.moveUp")
            }
            if row.number < model.rows.count {
                Button { model.move(row.entryId, by: 1) } label: { Label("Move Down", systemImage: "arrow.down") }
                    .accessibilityIdentifier("playlist.entry.moveDown")
            }
            Button(role: .destructive) { model.remove(row.entryId) } label: {
                Label("Remove", systemImage: "minus.circle")
            }
            .accessibilityIdentifier("playlist.entry.removeFromMenu")
        }
    }

    /// Video plays from the entry; music opens its album.
    private func open(_ row: PlaylistRow) {
        if model.video {
            launcher.play(app, model.play(from: row.number - 1))
        } else {
            app.shell.open(item: row.item)
        }
    }
}

/// An entry's row: its number, picture with progress, two lines and length.
private struct EntryRow: View {
    let app: AppModel
    let row: PlaylistRow
    let video: Bool
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        HStack(spacing: 12) {
            Text("\(row.number)")
                .font(.footnote.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 22, alignment: .trailing)
            if !typeSize.isAccessibilitySize {
                let width: CGFloat = video ? 112 : 56
                ServerImage(app: app, paths: row.tile.pictures, width: width, symbol: Metrics.symbol(row.item.kind))
                    .frame(width: width, height: Metrics.pictureHeight(row.tile.shape, width: width))
                    .overlay(alignment: .bottom) {
                        if row.tile.progress > 0 { ProgressBar(fraction: row.tile.progress) }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                if !row.subtitle.isEmpty {
                    Text(row.subtitle)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                }
            }
            Spacer(minLength: 0)
            if row.tile.watched {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(Color.accentColor)
                    .accessibilityHidden(true)
            }
            Text(row.duration)
                .font(.footnote.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .contentShape(Rectangle())
    }
}
