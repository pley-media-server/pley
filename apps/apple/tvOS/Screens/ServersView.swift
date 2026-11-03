import PleyCommon
import PleyScreens
import SwiftUI

/// The servers found on this network, the ones opened before, and adding
/// one by its address.
struct ServersView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ServersModel
    @State private var adding = false

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: ServersModel(app: app))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 48) {
                Text("Servers")
                    .font(.largeTitle.bold())
                if let notice = app.notice {
                    Label(asSentence(notice), systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.orange)
                }
                if let error = model.error {
                    Text(asSentence(error)).foregroundStyle(.red)
                }
                if model.blocked {
                    VStack(alignment: .leading, spacing: 8) {
                        Label("Pley can't see your local network", systemImage: "wifi.exclamationmark")
                            .font(.headline)
                        Text("To find servers at home, allow Pley in Settings → Privacy → Local Network. "
                            + "Servers can still be added by their address.")
                            .foregroundStyle(.secondary)
                    }
                }
                section("On this network", searching: model.searching) {
                    ForEach(model.foundEntries) { entry in
                        row(entry)
                    }
                    if model.foundEntries.isEmpty && !model.searching && !model.blocked {
                        Text("No servers found on this network. Turn on Bonjour in the server's "
                             + "Settings → Network, or add it by its address.")
                            .foregroundStyle(.secondary)
                    }
                }
                if !model.savedEntries.isEmpty {
                    section("Your servers", searching: false) {
                        ForEach(model.savedEntries) { entry in
                            row(entry)
                                .contextMenu {
                                    Button("Forget", role: .destructive) { Task { await model.forget(entry) } }
                                }
                        }
                    }
                }
                Button {
                    model.error = nil
                    adding = true
                } label: {
                    Label("Add a server", systemImage: "plus")
                }
                .accessibilityIdentifier("servers.add")
                .focusSection()
            }
            .padding(.horizontal, 80)
            .padding(.vertical, 40)
        }
        .task { await model.load() }
        .task { await model.browse() }
        .sheet(isPresented: $adding) {
            AddServerView(model: model) { adding = false }
        }
    }

    private func section<Content: View>(_ title: String, searching: Bool,
                                        @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 16) {
                Text(title).font(.headline).foregroundStyle(.secondary)
                if searching { ProgressView() }
            }
            content()
        }
        .focusSection()
    }

    private func row(_ entry: ServerEntry) -> some View {
        Button {
            Task { await model.open(entry) }
        } label: {
            HStack(spacing: 24) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(entry.name).font(.title3.bold())
                    Text(Servers.displayAddress(entry.origin))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    if entry.lookalike {
                        Text("Has the name of one of your servers at another address: open it only if it's yours.")
                            .font(.callout)
                            .foregroundStyle(.orange)
                    }
                }
                Spacer()
                if model.opening == entry.origin {
                    ProgressView()
                } else if entry.saved && entry.found {
                    Image(systemName: "wifi").accessibilityLabel("On this network")
                }
            }
            .padding(.horizontal, 40)
            .padding(.vertical, 28)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.card)
        .disabled(model.opening != nil)
        .accessibilityIdentifier("servers.server.\(entry.name)")
    }
}

/// Typing a server's address with the system keyboard.
private struct AddServerView: View {
    @ObservedObject var model: ServersModel
    let done: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 40) {
            Text("Add a server").font(.title2.bold())
            TextField("Address, like 192.168.1.20", text: $model.address)
                .keyboardType(.URL)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .onSubmit { connect() }
                .accessibilityIdentifier("servers.address")
            if let error = model.error {
                Text(asSentence(error)).foregroundStyle(.red)
            }
            HStack(spacing: 40) {
                Button("Connect") { connect() }
                    .accessibilityIdentifier("servers.connect")
                    .disabled(model.adding || model.address.trimmingCharacters(in: .whitespaces).isEmpty)
                Button("Cancel", role: .cancel, action: done)
                if model.adding { ProgressView() }
            }
        }
        .padding(80)
        .frame(maxWidth: 1100)
    }

    private func connect() {
        Task {
            await model.add()
            if model.error == nil && model.address.isEmpty { done() }
        }
    }
}
