import PleyModel
import PleyScreens
import SwiftUI

/// Home: the server's rows, each a line of posters.
struct HomeView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: HomeModel
    @State private var confirmSignOut = false
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: HomeModel(app: app))
    }

    /// Posters are 110 points wide on iPhone, 130 on iPad and Mac.
    private var posterWidth: CGFloat {
        #if os(iOS)
        sizeClass == .regular ? 130 : 110
        #else
        130
        #endif
    }

    var body: some View {
        content
            .navigationTitle(app.server?.name ?? "Home")
            .toolbar {
                ToolbarItem(placement: .primaryAction) { menu }
            }
            .confirmationDialog("Sign out of \(app.server?.name ?? "this server")?", isPresented: $confirmSignOut,
                                titleVisibility: .visible) {
                Button("Sign out", role: .destructive) { Task { await app.signOut() } }
            }
            .task(id: app.homeVersion) { await model.reload() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            ProgressView()
                .controlSize(.large)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .failed(let why):
            VStack(spacing: 12) {
                Text(asSentence(why)).multilineTextAlignment(.center)
                Button("Try again") { Task { await model.reload() } }
            }
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .loaded(let hubs):
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 28) {
                    if hubs.isEmpty {
                        Text("Nothing here yet. Add a library on the server, or ask its owner for one.")
                            .foregroundStyle(.secondary)
                            .padding()
                    }
                    ForEach(hubs) { hub in
                        row(hub)
                    }
                }
                .padding(.vertical)
            }
            .refreshable { await model.reload() }
        }
    }

    private func row(_ hub: Hub) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(hub.title)
                .font(.title3.bold())
                .padding(.horizontal)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: 12) {
                    ForEach(hub.items) { item in
                        PosterTile(app: app, item: item, width: posterWidth)
                    }
                }
                .padding(.horizontal)
            }
        }
    }

    private var menu: some View {
        Menu {
            if let user = app.user {
                Text(user.displayName ?? user.name)
            }
            Button("Switch profile") { app.showProfiles() }
            Button("Servers") { app.showServers() }
            #if os(macOS)
            Button("Reload") { Task { await model.reload() } }
            #endif
            Divider()
            Button("Sign out", role: .destructive) { confirmSignOut = true }
        } label: {
            Label("Account", systemImage: "person.crop.circle")
        }
    }
}

/// A poster, 2:3, with its title below on one line; it opens the item's page.
private struct PosterTile: View {
    let app: AppModel
    let item: ItemSummary
    let width: CGFloat
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        Button {
            app.showItem(item.id)
        } label: {
            tile
        }
        .buttonStyle(.plain)
        .accessibilityLabel(item.title)
    }

    private var tile: some View {
        VStack(alignment: .leading, spacing: 6) {
            ServerImage(app: app, path: HomeModel.posterPath(item),
                        widthPx: HomeModel.pixels(Double(width), scale: Double(displayScale)))
                .frame(width: width, height: width * 3 / 2)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            Text(item.title)
                .font(.footnote)
                .lineLimit(1)
                .frame(width: width, alignment: .leading)
        }
        .contentShape(Rectangle())
    }
}
