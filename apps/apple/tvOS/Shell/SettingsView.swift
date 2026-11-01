import PleyScreens
import SwiftUI

/// Settings: the profile signed in and its server, Switch profile, Servers
/// and Sign out (what Me holds on the other devices).
struct SettingsView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: MeModel
    @State private var confirmSignOut = false

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: MeModel(app: app))
    }

    var body: some View {
        HStack(alignment: .top, spacing: 80) {
            VStack(spacing: 24) {
                if let user = app.user {
                    AvatarView(app: app, name: model.name, path: user.avatarUrl, color: user.avatarColor, size: 240)
                }
                Text(model.name)
                    .font(.title2.bold())
                    .accessibilityIdentifier("me.name")
                if let kind = model.kind {
                    Text(kind).foregroundStyle(.secondary)
                }
            }
            .frame(width: 480)
            VStack(alignment: .leading, spacing: 40) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Server").font(.headline).foregroundStyle(.secondary)
                    Text(model.serverName).font(.title3)
                    Text(model.serverAddress).foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("me.server")
                // Buttons of their own, in a column the focus moves down.
                VStack(alignment: .leading, spacing: 24) {
                    if model.canSwitch {
                        Button("Switch profile") { model.switchProfile() }
                            .accessibilityIdentifier("me.switchProfile")
                    }
                    Button("Servers") { model.showServers() }
                        .accessibilityIdentifier("me.servers")
                    Button("Sign out", role: .destructive) { confirmSignOut = true }
                        .accessibilityIdentifier("me.signOut")
                }
                .focusSection()
                Text(MeModel.webNote)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, Metrics.gutter)
        .padding(.top, 40)
        .task(id: app.user?.id) { await model.load() }
        .alert("Sign out of \(app.server?.name ?? "this server")?", isPresented: $confirmSignOut) {
            Button("Sign out", role: .destructive) { Task { await model.signOut() } }
            Button("Cancel", role: .cancel) {}
        }
    }
}
