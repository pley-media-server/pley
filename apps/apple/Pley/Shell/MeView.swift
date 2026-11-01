import PleyScreens
import SwiftUI

/// Me: the profile signed in and its server, Switch profile, Servers and
/// Sign out.
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
            } footer: {
                Text(MeModel.webNote)
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
            }
            Section {
                if model.canSwitch {
                    Button("Switch profile") { model.switchProfile() }
                        .accessibilityIdentifier("me.switchProfile")
                }
                Button("Servers") { model.showServers() }
                    .accessibilityIdentifier("me.servers")
                Button("Sign out", role: .destructive) { confirmSignOut = true }
                    .accessibilityIdentifier("me.signOut")
            }
        }
        .navigationTitle("Me")
        .task(id: app.user?.id) { await model.load() }
        .confirmationDialog("Sign out of \(app.server?.name ?? "this server")?", isPresented: $confirmSignOut,
                            titleVisibility: .visible) {
            Button("Sign out", role: .destructive) { Task { await model.signOut() } }
                .accessibilityIdentifier("me.signOut.confirm")
        }
    }
}
