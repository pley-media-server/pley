import PleyModel
import PleyScreens
import SwiftUI

/// The shell's sheet, as a panel down the right of the screen over what
/// was showing: Add to playlist, New playlist, or Save as… a library's
/// filters as a smart playlist. Menu closes it.
struct ShellSheet: View {
    let app: AppModel
    let sheet: Sheet

    var body: some View {
        HStack(spacing: 0) {
            Color.black.opacity(0.45)
                .ignoresSafeArea()
                .accessibilityHidden(true)
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    switch sheet {
                    case .addToPlaylist(let ids):
                        AddToPlaylistPanel(app: app, itemIds: ids)
                    case .newPlaylist(let ids):
                        NewPlaylistPanel(app: app, mode: .items(ids))
                    case .saveSmartPlaylist(let library, let query):
                        NewPlaylistPanel(app: app, mode: .smart(libraryId: library, query: query))
                    }
                }
                .padding(60)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(width: 760)
            .background(.regularMaterial, ignoresSafeAreaEdges: .all)
            .focusSection()
        }
        .presentationBackground(.clear)
    }
}

/// Add to playlist: what is added in full, New Playlist named on the spot,
/// and the playlists of its kind (smart ones fill themselves).
private struct AddToPlaylistPanel: View {
    let app: AppModel
    @StateObject private var model: AddToPlaylistModel
    @State private var title = ""

    init(app: AppModel, itemIds: [Int]) {
        self.app = app
        _model = StateObject(wrappedValue: AddToPlaylistModel(app: app, itemIds: itemIds))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            Text("Add to Playlist")
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
            Text(model.title ?? " ")
                .font(.callout)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("addToPlaylist.what")
            TextField("New playlist's title", text: $title)
                .onSubmit(create)
                .accessibilityIdentifier("addToPlaylist.new.title")
            Button("Create Playlist", action: create)
                .disabled(Playlists.check(title) != nil || model.busy)
                .accessibilityIdentifier("addToPlaylist.new.create")
            if let playlists = model.playlists {
                if !playlists.isEmpty {
                    Text("Playlists").font(.headline).accessibilityAddTraits(.isHeader)
                }
                ForEach(playlists) { card in
                    Button {
                        Task { if await model.add(to: card) { app.shell.sheet = nil } }
                    } label: {
                        HStack(spacing: 24) {
                            PlaylistMosaic(app: app, artwork: card.artwork, video: card.video, size: 80)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(card.title)
                                Text(card.meta).font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 0)
                        }
                    }
                    .disabled(model.busy)
                    .accessibilityLabel(card.spokenLabel)
                    .accessibilityIdentifier("addToPlaylist.playlist.\(card.id)")
                }
            } else {
                ProgressView()
            }
            if let failure = model.failure {
                Text(asSentence(failure)).font(.callout).foregroundStyle(.red)
            }
        }
        .task { await model.load() }
    }

    private func create() {
        guard Playlists.check(title) == nil else { return }
        Task { if await model.create(title) { app.shell.sheet = nil } }
    }
}

/// New playlist, and Save as… a smart playlist: its title, what an empty
/// one plays, and a smart one's filters in words. The playlist made opens.
private struct NewPlaylistPanel: View {
    let app: AppModel
    @StateObject private var model: NewPlaylistModel
    @State private var title = ""
    @State private var kind = PlaylistKind.video
    @State private var tried = false

    init(app: AppModel, mode: NewPlaylistModel.Mode) {
        self.app = app
        _model = StateObject(wrappedValue: NewPlaylistModel(app: app, mode: mode))
    }

    private var smart: Bool {
        if case .smart = model.mode { return true }
        return false
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            Text(smart ? "Save as Smart Playlist" : "New Playlist")
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
            TextField("Title", text: $title)
                .onSubmit(create)
                .accessibilityIdentifier("newPlaylist.title")
            if tried, let problem = Playlists.check(title) {
                Text(problem).font(.callout).foregroundStyle(.red)
            }
            if model.asksKind {
                Button {
                    kind = kind == .video ? .audio : .video
                } label: {
                    Label(kind == .video ? "Plays: Movies & TV" : "Plays: Music",
                          systemImage: kind == .video ? "film" : "music.note")
                }
                .accessibilityLabel("Plays")
                .accessibilityValue(kind == .video ? "Movies & TV" : "Music")
                .accessibilityIdentifier("newPlaylist.kind")
            }
            if smart {
                Text("Filters").font(.headline).accessibilityAddTraits(.isHeader)
                Text(model.filters)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("newPlaylist.filters")
                Text("It plays what these filters find, as the library changes.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Button(smart ? "Save" : "Create", action: create)
                .disabled(model.busy)
                .accessibilityIdentifier("newPlaylist.create")
        }
    }

    private func create() {
        tried = true
        guard Playlists.check(title) == nil else { return }
        Task {
            guard let id = await model.create(title, kind: kind) else { return }
            app.shell.sheet = nil
            app.shell.open(.playlist(id))
        }
    }
}
