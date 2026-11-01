import PleyModel
import PleyScreens
import SwiftUI

/// Search on Apple TV: the system's keyboard and field at the top, the
/// profile's recent searches while nothing is typed, then the results in
/// rows the focus moves down through: People, Movies, Shows, Seasons,
/// Episodes, Artists, Albums, Tracks, with their counts.
struct SearchView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: SearchModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: SearchModel(app: app))
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                content
            }
            .padding(.vertical, 24)
        }
        .searchable(text: $model.query, prompt: "Movies, shows, people and music")
        .onSubmit(of: .search) { model.submit() }
        .task(id: app.user?.id) { model.loadRecent() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .idle:
            recent
        case .loading:
            ProgressView()
                .frame(maxWidth: .infinity)
                .accessibilityLabel("Searching")
        case .ready(_, let groups, _):
            ForEach(groups) { group in
                if group.kind == .people {
                    peopleRow(group)
                } else {
                    TileRow(app: app, key: group.kind.rawValue, title: "\(group.kind.title) (\(group.count))",
                            shape: group.kind.shape, tiles: group.tiles, prefix: "search.group") { tile in
                        app.shell.open(model.open(tile))
                    }
                }
            }
        case .empty(let query):
            message(SearchModel.noResults(query))
                .accessibilityIdentifier("search.empty")
        case .failed(_, let why):
            VStack(spacing: 24) {
                message("Search failed. \(asSentence(why))")
                Button("Try again") { model.retry() }
                    .accessibilityIdentifier("state.retry")
            }
            .frame(maxWidth: .infinity)
            .focusSection()
        }
    }

    /// The recent searches, newest first, then Clear.
    @ViewBuilder private var recent: some View {
        if model.recent.isEmpty {
            message("Search for movies, shows, people and music.")
        } else {
            VStack(alignment: .leading, spacing: 20) {
                Text("Recent searches")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                ScrollView(.horizontal) {
                    HStack(spacing: 24) {
                        ForEach(Array(model.recent.enumerated()), id: \.element) { index, text in
                            Button {
                                model.searchAgain(text)
                            } label: {
                                Label(text, systemImage: "clock.arrow.circlepath")
                            }
                            .accessibilityHint("Searches again")
                            .accessibilityIdentifier("search.recent.\(index)")
                        }
                        Button("Clear") { model.clearRecent() }
                            .accessibilityLabel("Clear recent searches")
                            .accessibilityIdentifier("search.recent.clear")
                    }
                    .padding(.vertical, 20)
                }
                .scrollClipDisabled()
            }
            .padding(.horizontal, Metrics.gutter)
            .focusSection()
        }
    }

    private func message(_ text: String) -> some View {
        Text(text)
            .font(.title3)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
            .padding(.top, 40)
    }

    /// People found: round photos (or initials) with their names.
    private func peopleRow(_ group: SearchModel.Group) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("\(group.kind.title) (\(group.count))")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(group.people) { person in
                        VStack(spacing: 14) {
                            Button {
                                app.shell.open(model.open(person))
                            } label: {
                                AvatarView(app: app, name: person.name, path: person.photoUrl, color: nil, size: 180)
                            }
                            .buttonStyle(.borderless)
                            .clipShape(Circle())
                            .accessibilityLabel(person.name)
                            .accessibilityHint("Opens their page")
                            .accessibilityIdentifier("search.person.\(person.id)")
                            Text(person.name)
                                .font(.caption)
                                .lineLimit(2)
                                .multilineTextAlignment(.center)
                                .accessibilityHidden(true)
                        }
                        .frame(width: 220)
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 32)
            }
            .scrollClipDisabled()
        }
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("search.group.people")
    }
}
