import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// The profile's playlists by title, each with its mosaic, after New Playlist.
struct PlaylistsView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: PlaylistsModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: PlaylistsModel(app: app))
    }

    var body: some View {
        content
            // Read again each time it shows: playlists change from other pages.
            .task { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: PlaylistsModel.failedTitle, why: why) { Task { await model.load() } })
        case .loaded:
            let width: CGFloat = 300
            ScrollView {
                VStack(alignment: .leading, spacing: 32) {
                    HStack(spacing: 24) {
                        Text("Playlists")
                            .font(.title2.bold())
                            .accessibilityAddTraits(.isHeader)
                        Button { model.new() } label: {
                            Label("New Playlist", systemImage: "plus")
                        }
                        .accessibilityIdentifier("playlists.new")
                    }
                    .focusSection()
                    .padding(.horizontal, Metrics.gutter)
                    if let notice = model.notice {
                        Text(asSentence(notice)).font(.callout).foregroundStyle(.red)
                            .padding(.horizontal, Metrics.gutter)
                    }
                    if model.cards.isEmpty {
                        Text(PlaylistsModel.emptyTitle)
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, Metrics.gutter)
                    }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: width, maximum: width),
                                                 spacing: Metrics.tileSpacing, alignment: .top)],
                              alignment: .leading, spacing: Metrics.rowSpacing) {
                        ForEach(model.cards) { card in
                            PlaylistTile(app: app, card: card, width: width) { app.shell.open(.playlist(card.id)) }
                        }
                    }
                    .padding(.horizontal, Metrics.gutter)
                    .focusSection()
                    .accessibilityIdentifier("playlists.grid")
                }
                .padding(.vertical, 40)
            }
            .scrollClipDisabled()
        }
    }
}

/// One playlist: its mosaic, title, Smart badge, count and length and its
/// filter in words; Play, Shuffle, Rename and Delete; then its entries.
/// Select plays from an entry (music in Now Playing); a long press has
/// Move and Remove. In Move, up and down move the entry and Select puts it
/// down (Menu puts it back); a removal can be undone for 5 seconds with
/// Undo beside Play. Smart playlists are read-only.
struct PlaylistView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: PlaylistModel
    @State private var renaming = false
    @State private var newTitle = ""
    @State private var deleting = false
    /// The entry being moved.
    @State private var moving: Int?
    /// The entry removed last, while it can be put back.
    @State private var removed: Int?
    /// The entry with the focus; while one moves, the focus stays on it.
    @FocusState private var focused: Int?

    init(app: AppModel, playlistId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: PlaylistModel(app: app, playlistId: playlistId))
    }

    var body: some View {
        content
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
            ScrollView {
                VStack(alignment: .leading, spacing: 40) {
                    header
                    LazyVStack(alignment: .leading, spacing: 12) {
                        if model.rows.isEmpty {
                            Text(PlaylistModel.emptyTitle).foregroundStyle(.secondary)
                        }
                        ForEach(model.rows) { row in
                            entry(row)
                        }
                    }
                    .focusSection()
                    .onChange(of: focused) { _, now in
                        if let moving, now != moving { focused = moving }
                    }
                    .accessibilityIdentifier("playlist.entries")
                }
                .frame(maxWidth: 1400, alignment: .leading)
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 40)
            }
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 48) {
            PlaylistMosaic(app: app, artwork: model.artwork, video: model.video, size: 300)
            VStack(alignment: .leading, spacing: 14) {
                if model.smart { ListBadge(text: "Smart") }
                Text(model.title)
                    .font(.title2.bold())
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("playlist.title")
                Text(model.meta)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("playlist.meta")
                if !model.notes.isEmpty {
                    Text(model.notes.joined(separator: " · ")).font(.callout).foregroundStyle(.secondary)
                }
                HStack(spacing: 24) {
                    PlayShuffleButtons(prefix: "playlist", canPlay: model.canPlay, canShuffle: model.canShuffle,
                                       play: { play(model.play()) }, shuffle: { play(model.play(shuffle: true)) })
                    if let removed, model.undoable(removed) {
                        Button {
                            model.undo(removed)
                            app.shell.dismissToast()
                            self.removed = nil
                        } label: {
                            Label("Undo", systemImage: "arrow.uturn.backward")
                        }
                        .accessibilityLabel("Undo Remove")
                        .accessibilityIdentifier("playlist.undo")
                    }
                    if model.editable {
                        Button {
                            newTitle = model.title
                            renaming = true
                        } label: {
                            Label("Rename", systemImage: "pencil")
                        }
                        .accessibilityIdentifier("playlist.rename")
                        Button(role: .destructive) { deleting = true } label: {
                            Label("Delete", systemImage: "trash")
                        }
                        .accessibilityIdentifier("playlist.delete")
                    }
                }
                // While an entry moves, up and down are its own.
                .disabled(moving != nil)
                .focusSection()
                .padding(.top, 8)
            }
        }
    }

    private func entry(_ row: PlaylistRow) -> some View {
        let isMoving = moving == row.entryId
        return Button {
            if isMoving {
                moving = nil
                model.sendOrder()
                // The entry put down keeps the focus, once the others can take it again.
                let entry = row.entryId
                Task { @MainActor in focused = entry }
            } else {
                play(model.play(from: row.number - 1))
            }
        } label: {
            EntryRow(app: app, row: row, video: model.video, moving: isMoving)
        }
        // While one moves, the others can't take the focus: up and down move it.
        .disabled(moving != nil && !isMoving)
        .focused($focused, equals: row.entryId)
        .onMoveCommand { direction in
            guard isMoving else { return }
            switch direction {
            case .up: model.move(row.entryId, by: -1, send: false)
            case .down: model.move(row.entryId, by: 1, send: false)
            default: break
            }
        }
        .onExitCommand(perform: isMoving ? {
            model.cancelMoves()
            moving = nil
            let entry = row.entryId
            Task { @MainActor in focused = entry }
        } : nil)
        .contextMenu {
            Button("Play from Here") { play(model.play(from: row.number - 1)) }
            Button(model.video ? "Go to Page" : "Go to Album") { app.shell.open(item: row.item) }
            if model.editable {
                Button("Move") { moving = row.entryId }
                    .accessibilityIdentifier("playlist.entry.move")
                Button("Remove", role: .destructive) {
                    model.remove(row.entryId)
                    removed = row.entryId
                }
                .accessibilityIdentifier("playlist.entry.removeFromMenu")
            }
        }
        .accessibilityLabel(row.spokenLabel)
        .accessibilityHint(isMoving ? "Move up or down, then press Select" : "")
        .accessibilityActions {
            if model.editable, row.number > 1 {
                Button("Move up") { model.move(row.entryId, by: -1) }
            }
            if model.editable, row.number < model.rows.count {
                Button("Move down") { model.move(row.entryId, by: 1) }
            }
        }
        .accessibilityIdentifier("playlist.entry.\(row.entryId)")
    }

    private func play(_ request: PlayRequest?) {
        guard let request else { return }
        Task { await app.play(request) }
    }
}

/// An entry's row: its number, picture with progress, two lines and length.
private struct EntryRow: View {
    let app: AppModel
    let row: PlaylistRow
    let video: Bool
    let moving: Bool

    var body: some View {
        HStack(spacing: 28) {
            Text("\(row.number)")
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 44, alignment: .trailing)
            let width: CGFloat = video ? 220 : 110
            ServerImage(app: app, paths: row.tile.pictures, width: width, symbol: Metrics.symbol(row.item.kind))
                .frame(width: width, height: Metrics.pictureHeight(row.tile.shape, width: width))
                .overlay(alignment: .bottom) {
                    if row.tile.progress > 0 { ProgressBar(fraction: row.tile.progress) }
                }
                .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 4) {
                Text(row.title).font(.headline).lineLimit(1)
                Text(moving ? "Move up or down, then press Select" : row.subtitle)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if moving {
                Image(systemName: "arrow.up.arrow.down").font(.title3)
            } else if row.tile.watched {
                Image(systemName: "checkmark.circle.fill").font(.title3)
            }
            Text(row.duration)
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 6)
    }
}
