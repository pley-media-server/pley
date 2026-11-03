import PleyModel
import PleyScreens
import SwiftUI

// What requests show on the app's other pages: the ways in on Me and in
// Search, Search's Discover row, and a library title's request.

/// Requests' rows on Me: Discover, and Your requests (Requests for
/// managers, with what it's for) carrying its number, red while failed
/// requests wait for a manager.
struct MeRequestsSection: View {
    @ObservedObject var shell: ShellModel

    var body: some View {
        let entry = shell.requestsEntry
        if entry.shows {
            Section {
                if entry.discover {
                    NavigationLink(value: Screen.discover) {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(RequestWords.discover)
                                Text(RequestWords.discoverHint).font(.footnote).foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "safari")
                        }
                    }
                    .accessibilityIdentifier("me.discover")
                }
                if entry.requests {
                    NavigationLink(value: Screen.requests) {
                        HStack {
                            Label {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(entry.requestsTitle)
                                    if entry.manage {
                                        Text(RequestWords.manageHint).font(.footnote).foregroundStyle(.secondary)
                                    }
                                }
                            } icon: {
                                Image(systemName: "list.bullet.rectangle")
                            }
                            Spacer(minLength: 8)
                            if let badge = entry.badgeText {
                                CountBadge(text: badge, alarm: entry.alarm)
                                    .accessibilityIdentifier("me.requests.badge")
                            }
                        }
                    }
                    .accessibilityValue(entry.spoken ?? "")
                    .accessibilityIdentifier("me.requests")
                }
            }
        }
    }
}

/// Discover's way in, at the top of Search before anything is typed.
struct SearchDiscoverEntry: View {
    var body: some View {
        NavigationLink(value: Screen.discover) {
            Label {
                VStack(alignment: .leading, spacing: 2) {
                    Text(RequestWords.discover).foregroundStyle(.primary)
                    Text(RequestWords.discoverHint).font(.footnote).foregroundStyle(.secondary)
                }
            } icon: {
                Image(systemName: "safari")
            }
        }
        .accessibilityIdentifier("search.discover")
    }
}

/// The Discover row after the libraries' results: TMDB's first titles for
/// the words that no library has, each opening its Discover page, with See
/// all; why TMDB couldn't be searched, with Retry; nothing when it found
/// none.
struct SearchDiscoverRow: View {
    let app: AppModel
    @ObservedObject var model: SearchModel
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1

    private var width: CGFloat {
        Metrics.tileWidth(.poster, regular: layout.wide) * min(max(typeScale, 1), Metrics.maxTileScale)
    }

    var body: some View {
        let gutter = Metrics.gutter(regular: layout.wide)
        switch model.discover {
        case .hidden:
            EmptyView()
        case .ready(_, let cards) where cards.isEmpty:
            EmptyView()
        default:
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .firstTextBaseline) {
                    Text(RequestWords.discover)
                        .font(.title3.bold())
                        .accessibilityAddTraits(.isHeader)
                    Spacer()
                    if let all = model.discoverAll {
                        Button(RequestWords.seeAll) { app.shell.open(all) }
                            .accessibilityLabel("\(RequestWords.seeAll), \(RequestWords.discover)")
                            .accessibilityIdentifier("search.discover.seeAll")
                    }
                }
                .padding(.horizontal, gutter)
                switch model.discover {
                case .failed(let words):
                    HStack(spacing: 12) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(words.title).font(.subheadline.weight(.semibold))
                            if let body = words.body {
                                Text(body).font(.footnote).foregroundStyle(.secondary)
                            }
                        }
                        Spacer(minLength: 0)
                        if words.retry {
                            Button(RequestWords.retry, action: model.retryDiscover)
                                .accessibilityIdentifier("search.discover.retry")
                        }
                    }
                    .padding(.horizontal, gutter)
                    .task(id: words) { announce(words.title) }
                default:
                    ScrollView(.horizontal, showsIndicators: false) {
                        LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                            if case .ready(_, let cards) = model.discover {
                                ForEach(cards) { card in
                                    DiscoverCardView(app: app, card: card, width: width,
                                                     identifier: "search.discover.card.\(card.key)")
                                }
                            } else {
                                ForEach(0..<6, id: \.self) { _ in PlaceholderTile(shape: .poster, width: width) }
                            }
                        }
                        .padding(.horizontal, gutter)
                    }
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("search.discover.row")
        }
    }
}

/// What requests say of a library's movie or show, under its buttons: the
/// stage of its request (with the download's bar), and Request more
/// seasons, which opens the title's Discover page with Which seasons?.
struct ItemRequestBlockView: View {
    let app: AppModel
    let block: ItemRequestBlock

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let pill = block.pill {
                StagePillView(pill: pill)
            }
            if block.more {
                Button {
                    app.shell.open(block.picker)
                } label: {
                    Label(RequestWords.moreSeasons, systemImage: "plus")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("item.requestMore")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("item.request")
    }
}
