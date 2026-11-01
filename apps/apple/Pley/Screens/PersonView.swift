import PleyModel
import PleyScreens
import SwiftUI

/// A person's page (docs/APPLE.md, People): their photo, name and "3
/// movies · 1 show", then a row of titles for each thing they did (Acting,
/// Directing, Writing…), each with the year and their part.
struct PersonView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: PersonModel
    @Environment(\.dynamicTypeSize) private var typeSize
    private let layout = WideLayout()

    init(app: AppModel, personId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: PersonModel(app: app, personId: personId))
    }

    var body: some View {
        content
            .navigationTitle(model.page?.name ?? "")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .task { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .notFound:
            StateView(kind: .empty(symbol: "person.crop.circle.badge.questionmark", text: PersonModel.notFoundTitle,
                                   action: nil))
        case .failed(let why):
            StateView(kind: .failed(title: "Couldn't load this person", why: why) { Task { await model.load() } })
        case .loaded:
            if let page = model.page {
                loaded(page)
            } else {
                StateView(kind: .loading)
            }
        }
    }

    private func loaded(_ page: PersonPage) -> some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        let stack = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12))
            : AnyLayout(HStackLayout(alignment: .center, spacing: 20))
        return ScrollView {
            VStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                stack {
                    AvatarView(app: app, name: page.name, path: page.photo, color: nil, size: layout.wide ? 160 : 112)
                    VStack(alignment: .leading, spacing: 6) {
                        Text(page.name)
                            .font(.largeTitle.bold())
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("person.name")
                        if !page.counts.isEmpty {
                            Text(page.counts).foregroundStyle(.secondary)
                        }
                    }
                }
                .padding(.horizontal, gutter)
                .padding(.top, 16)
                if page.roles.isEmpty {
                    Text("Nothing of theirs is in your libraries.")
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, gutter)
                }
                ForEach(page.roles) { row in
                    TileRow(app: app, key: row.role.rawValue, title: row.title, shape: .poster, tiles: row.tiles,
                            prefix: "person.row") { tile in
                        app.shell.open(Screen.page(of: tile.item))
                    }
                }
            }
            .padding(.bottom, 32)
        }
    }
}
