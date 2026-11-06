import PleyCommon
import PleyScreens
import SwiftUI

#if os(iOS)
import UIKit
#endif

/// The servers found on this network, the ones opened before, and adding
/// one by its address.
struct ServersView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ServersModel
    @Environment(\.openURL) private var openURL
    /// The address field has the keys: after File → Add Server… on the Mac.
    @FocusState private var addressFocused: Bool

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: ServersModel(app: app))
    }

    var body: some View {
        List {
            if let notice = app.notice {
                Section {
                    Label(asSentence(notice), systemImage: "exclamationmark.triangle")
                }
            }
            if model.blocked {
                Section { blockedNotice }
            }
            Section {
                ForEach(model.foundEntries) { entry in
                    row(entry)
                }
                if model.foundEntries.isEmpty && !model.searching && !model.blocked {
                    Text("""
                         No servers found on this network. Turn on Bonjour in the server's \
                         Settings → Network, or add it by its address.
                         """)
                        .foregroundStyle(.secondary)
                }
            } header: {
                HStack(spacing: 8) {
                    Text("On this network")
                    if model.searching { ProgressView().controlSize(.small) }
                }
            }
            if !model.savedEntries.isEmpty {
                Section("Your servers") {
                    ForEach(model.savedEntries) { entry in
                        row(entry)
                            .swipeActions {
                                Button("Forget", role: .destructive) { Task { await model.forget(entry) } }
                            }
                            .contextMenu {
                                Button("Forget", role: .destructive) { Task { await model.forget(entry) } }
                            }
                    }
                }
            }
            Section {
                TextField("Address", text: $model.address, prompt: Text("192.168.1.20 or pley.example.com"))
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .textContentType(.URL)
                    #endif
                    .submitLabel(.go)
                    .onSubmit { Task { await model.add() } }
                    .focused($addressFocused)
                    .accessibilityIdentifier("servers.address")
                    .disabled(model.adding)
                HStack {
                    Button("Connect") { Task { await model.add() } }
                        .accessibilityIdentifier("servers.connect")
                        .disabled(model.adding || model.address.trimmingCharacters(in: .whitespaces).isEmpty)
                    if model.adding { ProgressView().controlSize(.small) }
                }
            } header: {
                Text("Add a server")
            } footer: {
                if let error = model.error {
                    Text(asSentence(error)).foregroundStyle(.red)
                }
            }
        }
        .navigationTitle("Servers")
        .task { await model.load() }
        .task(id: app.addingServer) {
            guard app.addingServer else { return }
            addressFocused = true
            app.addingServerShown()
        }
        .task { await model.browse() }
    }

    private func row(_ entry: ServerEntry) -> some View {
        Button {
            Task { await model.open(entry) }
        } label: {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(entry.name)
                        .foregroundStyle(.primary)
                    Text(Servers.displayAddress(entry.origin))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    if entry.lookalike {
                        Text("Has the name of one of your servers at another address: open it only if it's yours.")
                            .font(.footnote)
                            .foregroundStyle(.orange)
                    }
                }
                Spacer()
                if model.opening == entry.origin {
                    ProgressView().controlSize(.small)
                } else if entry.saved && entry.found {
                    Image(systemName: "wifi")
                        .foregroundStyle(.secondary)
                        .accessibilityLabel("On this network")
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(model.opening != nil)
        .accessibilityIdentifier("servers.server.\(entry.name)")
    }

    private var blockedNotice: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Pley can't see your local network", systemImage: "wifi.exclamationmark")
                .font(.headline)
            Text("""
                 To find servers at home, allow Pley in Settings → Privacy & Security → Local Network. \
                 Servers can still be added by their address.
                 """)
                .font(.footnote)
                .foregroundStyle(.secondary)
            #if os(iOS)
            Button("Open Settings") {
                if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) }
            }
            #endif
        }
        .padding(.vertical, 4)
    }
}
