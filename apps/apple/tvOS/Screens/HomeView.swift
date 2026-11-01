import PleyModel
import PleyScreens
import SwiftUI

/// Home: the server's rows of posters, moved through with the focus
/// engine, one row at a time.
struct HomeView: View {
    static let posterWidth: CGFloat = 250

    @ObservedObject private var app: AppModel
    @StateObject private var model: HomeModel
    @State private var confirmSignOut = false

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: HomeModel(app: app))
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 48) {
                header
                    .padding(.horizontal, 80)
                    .focusSection()
                content
            }
            .padding(.vertical, 40)
        }
        .alert("Sign out of \(app.server?.name ?? "this server")?", isPresented: $confirmSignOut) {
            Button("Sign out", role: .destructive) { Task { await app.signOut() } }
            Button("Cancel", role: .cancel) {}
        }
        .task(id: app.homeVersion) { await model.reload() }
    }

    private var header: some View {
        HStack(spacing: 40) {
            VStack(alignment: .leading, spacing: 4) {
                Text(app.server?.name ?? "Home").font(.title2.bold())
                if let user = app.user {
                    Text(user.displayName ?? user.name).foregroundStyle(.secondary)
                }
            }
            Spacer()
            Button("Switch profile") { app.showProfiles() }
            Button("Servers") { app.showServers() }
            Button("Sign out") { confirmSignOut = true }
        }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            ProgressView().frame(maxWidth: .infinity).padding(80)
        case .failed(let why):
            VStack(spacing: 24) {
                Text(asSentence(why)).multilineTextAlignment(.center)
                Button("Try again") { Task { await model.reload() } }
            }
            .frame(maxWidth: .infinity)
            .focusSection()
        case .loaded(let hubs):
            if hubs.isEmpty {
                Text("Nothing here yet. Add a library on the server, or ask its owner for one.")
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 80)
            }
            ForEach(hubs) { hub in
                row(hub)
            }
        }
    }

    private func row(_ hub: Hub) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(hub.title)
                .font(.headline)
                .padding(.horizontal, 80)
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: 48) {
                    ForEach(hub.items) { item in
                        PosterTile(app: app, item: item, width: Self.posterWidth)
                    }
                }
                // Room for the focused poster to lift.
                .padding(.horizontal, 80)
                .padding(.vertical, 32)
            }
            .scrollClipDisabled()
        }
        .focusSection()
    }
}

/// A poster, 2:3, that takes focus, with its title below on one line.
private struct PosterTile: View {
    let app: AppModel
    let item: ItemSummary
    let width: CGFloat
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button {
                app.showItem(item.id)
            } label: {
                ServerImage(app: app, path: HomeModel.posterPath(item),
                            widthPx: HomeModel.pixels(Double(width), scale: Double(displayScale)))
                    .frame(width: width, height: width * 3 / 2)
            }
            .buttonStyle(.card)
            .accessibilityLabel(item.title)
            Text(item.title)
                .font(.callout)
                .lineLimit(1)
                .frame(width: width, alignment: .leading)
                .accessibilityHidden(true)
        }
    }
}
