import PleyModel
import PleyScreens
import SwiftUI

/// Me: the profile signed in and its server, Switch profile, Discover and
/// Your requests (Requests for managers, with its number) where requests
/// are open, the profile's audio and subtitle defaults, History, Settings (on the Mac, the
/// Settings window, ⌘,), Diagnostics and About, Servers and Sign out.
struct MeView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: MeModel
    @State private var confirmSignOut = false

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: MeModel(app: app))
    }

    var body: some View {
        List {
            Section {
                HStack(spacing: 14) {
                    if let user = app.user {
                        AvatarView(app: app, name: model.name, path: user.avatarUrl, color: user.avatarColor,
                                   size: 56)
                    }
                    VStack(alignment: .leading, spacing: 2) {
                        Text(model.name)
                            .font(.title3.bold())
                            .accessibilityIdentifier("me.name")
                        if let kind = model.kind {
                            Text(kind).font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                }
                .accessibilityElement(children: .combine)
                .padding(.vertical, 4)
                if model.canSwitch {
                    Button("Switch profile") { model.switchProfile() }
                        .accessibilityIdentifier("me.switchProfile")
                }
            } footer: {
                Text(MeModel.webNote)
            }
            MeRequestsSection(shell: app.shell)
            TrackDefaultsSection(model: model)
            if app.downloads.available {
                Section {
                    NavigationLink(value: Screen.downloads) {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(DownloadWords.meEntry)
                                Text(DownloadWords.meHint).font(.footnote).foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "arrow.down.circle")
                        }
                    }
                    .accessibilityIdentifier("me.downloads")
                }
            }
            Section {
                NavigationLink(value: Screen.history) {
                    Label("History", systemImage: "clock")
                }
                .accessibilityIdentifier("me.history")
                #if os(macOS)
                SettingsLink {
                    Label("Settings…", systemImage: "gearshape")
                }
                .accessibilityIdentifier("me.settings")
                NavigationLink(value: Screen.diagnostics) {
                    Label("Diagnostics", systemImage: "stethoscope")
                }
                .accessibilityIdentifier("me.diagnostics")
                NavigationLink(value: Screen.about) {
                    Label("About Pley", systemImage: "info.circle")
                }
                .accessibilityIdentifier("me.about")
                #else
                NavigationLink(value: Screen.settings) {
                    Label("Settings", systemImage: "gearshape")
                }
                .accessibilityIdentifier("me.settings")
                #endif
            }
            Section("Server") {
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.serverName)
                    Text(model.serverAddress)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("me.server")
                Button("Servers") { model.showServers() }
                    .accessibilityIdentifier("me.servers")
                Button("Sign out", role: .destructive) { confirmSignOut = true }
                    .accessibilityIdentifier("me.signOut")
            }
        }
        #if os(macOS)
        .listStyle(.inset)
        #endif
        .navigationTitle("Me")
        .task(id: app.user?.id) { await model.load() }
        .confirmationDialog("Sign out of \(app.server?.name ?? "this server")?", isPresented: $confirmSignOut,
                            titleVisibility: .visible) {
            Button("Sign out", role: .destructive) { Task { await model.signOut() } }
                .accessibilityIdentifier("me.signOut.confirm")
        }
    }
}

/// The profile's audio and subtitle defaults, kept on the server: the
/// audio language, when subtitles come on, and their language when the
/// mode needs one. Read-only for a sign-in without the `play` scope.
private struct TrackDefaultsSection: View {
    @ObservedObject var model: MeModel

    var body: some View {
        Section {
            Picker(selection: Binding(get: { model.trackDefaults.audioLanguage },
                                      set: { code in Task { await model.setAudioLanguage(code) } })) {
                Text("The file's default").tag(String?.none)
                ForEach(MeModel.languages, id: \.code) { language in
                    Text(language.name).tag(Optional(language.code))
                }
            } label: {
                Text("Audio language")
            }
            .accessibilityIdentifier("me.audioLanguage")
            Picker(selection: Binding(get: { model.trackDefaults.subtitleMode },
                                      set: { mode in Task { await model.setSubtitleMode(mode) } })) {
                ForEach(MeModel.modes, id: \.self) { mode in
                    Text(MeModel.modeLabel(mode)).tag(mode)
                }
            } label: {
                Text("Subtitles")
            }
            .accessibilityIdentifier("me.subtitleMode")
            if model.showsSubtitleLanguage {
                Picker(selection: Binding(get: { model.trackDefaults.subtitleLanguage },
                                          set: { code in Task { await model.setSubtitleLanguage(code) } })) {
                    Text("Same as the audio language").tag(String?.none)
                    ForEach(MeModel.languages, id: \.code) { language in
                        Text(language.name).tag(Optional(language.code))
                    }
                } label: {
                    Text("Subtitle language")
                }
                .accessibilityIdentifier("me.subtitleLanguage")
            }
        } header: {
            Text("Audio and subtitles")
        } footer: {
            Text(model.editable ? MeModel.modeHint(model.trackDefaults.subtitleMode) : MeModel.readOnlyNote)
        }
        #if os(iOS)
        .pickerStyle(.navigationLink)
        #endif
        .disabled(!model.editable || model.saving)
    }
}
