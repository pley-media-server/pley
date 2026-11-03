import PleyModel
import PleyScreens
import SwiftUI

/// The shell's sheet: Add to playlist, New playlist, or Save as… a
/// library's filters as a smart playlist.
struct ShellSheet: View {
    let app: AppModel
    let sheet: Sheet

    var body: some View {
        NavigationStack {
            switch sheet {
            case .addToPlaylist(let ids):
                AddToPlaylistSheet(app: app, itemIds: ids)
            case .newPlaylist(let ids):
                NewPlaylistSheet(app: app, mode: .items(ids))
            case .saveSmartPlaylist(let library, let query):
                NewPlaylistSheet(app: app, mode: .smart(libraryId: library, query: query))
            }
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        #else
        .frame(minWidth: 420, idealWidth: 460, minHeight: 380, idealHeight: 480)
        #endif
    }
}

/// Add to playlist: what is added in full, a new playlist named on the
/// spot, and the playlists of its kind (smart ones fill themselves).
private struct AddToPlaylistSheet: View {
    let app: AppModel
    @StateObject private var model: AddToPlaylistModel
    @State private var title = ""
    @FocusState private var naming: Bool

    init(app: AppModel, itemIds: [Int]) {
        self.app = app
        _model = StateObject(wrappedValue: AddToPlaylistModel(app: app, itemIds: itemIds))
    }

    var body: some View {
        Form {
            Section {
                Text(model.title ?? " ")
                    .font(.headline)
                    .redacted(reason: model.title == nil ? .placeholder : [])
                    .accessibilityIdentifier("addToPlaylist.what")
            }
            Section("New Playlist") {
                HStack {
                    TextField("Title", text: $title)
                        .focused($naming)
                        .submitLabel(.done)
                        .onSubmit(create)
                        .accessibilityIdentifier("addToPlaylist.new.title")
                    Button("Create", action: create)
                        .disabled(Playlists.check(title) != nil || model.busy)
                        .accessibilityIdentifier("addToPlaylist.new.create")
                }
            }
            Section("Playlists") {
                if let playlists = model.playlists {
                    if playlists.isEmpty {
                        Text(model.kind == .audio ? "No music playlists yet" : "No playlists yet")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(playlists) { card in
                        Button {
                            Task { if await model.add(to: card) { app.shell.sheet = nil } }
                        } label: {
                            HStack(spacing: 12) {
                                PlaylistMosaic(app: app, artwork: card.artwork, video: card.video, size: 44)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(card.title).foregroundStyle(.primary)
                                    Text(card.meta).font(.footnote).foregroundStyle(.secondary)
                                }
                                Spacer(minLength: 0)
                            }
                            // The whole row takes the tap, not only its words.
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(model.busy)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(card.spokenLabel)
                        .accessibilityAddTraits(.isButton)
                        .accessibilityIdentifier("addToPlaylist.playlist.\(card.id)")
                    }
                } else {
                    ProgressView()
                }
                if let failure = model.failure {
                    Text(asSentence(failure)).font(.footnote).foregroundStyle(.red)
                }
            }
        }
        #if os(macOS)
        .formStyle(.grouped)
        #endif
        .navigationTitle("Add to Playlist")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { app.shell.sheet = nil }
                    .accessibilityIdentifier("addToPlaylist.cancel")
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
private struct NewPlaylistSheet: View {
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
        Form {
            Section {
                TextField("Title", text: $title)
                    .onSubmit(create)
                    .accessibilityIdentifier("newPlaylist.title")
                if tried, let problem = Playlists.check(title) {
                    Text(problem).font(.footnote).foregroundStyle(.red)
                }
            } footer: {
                if smart {
                    Text("It plays what these filters find, as the library changes.")
                }
            }
            if model.asksKind {
                Picker("Plays", selection: $kind) {
                    Text("Movies & TV").tag(PlaylistKind.video)
                    Text("Music").tag(PlaylistKind.audio)
                }
                .accessibilityIdentifier("newPlaylist.kind")
            }
            if smart {
                Section("Filters") {
                    Text(model.filters)
                        .accessibilityIdentifier("newPlaylist.filters")
                }
            }
        }
        #if os(macOS)
        .formStyle(.grouped)
        #endif
        .navigationTitle(smart ? "Save as Smart Playlist" : "New Playlist")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { app.shell.sheet = nil }
                    .accessibilityIdentifier("newPlaylist.cancel")
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(smart ? "Save" : "Create", action: create)
                    .disabled(model.busy)
                    .accessibilityIdentifier("newPlaylist.create")
            }
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
