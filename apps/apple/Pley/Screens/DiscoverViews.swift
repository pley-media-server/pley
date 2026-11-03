import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// Discover: a hero for the first trending title, with its main action and
/// Details; rows of posters with their status badges (Trending this week,
/// Popular movies, Popular shows, Coming to cinemas, Popular anime), each
/// read as it comes into view and page by page as it scrolls; the
/// person's requests; and TMDB's credit. When requests can't be used, the
/// page says why, and only that.
struct DiscoverView: View {
    let app: AppModel
    @StateObject private var model: DiscoverModel
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: DiscoverModel(app: app))
    }

    private var gutter: CGFloat { Metrics.gutter(regular: layout.wide) }
    private var width: CGFloat {
        Metrics.tileWidth(.poster, regular: layout.wide) * min(max(typeScale, 1), Metrics.maxTileScale)
    }

    var body: some View {
        Group {
            if let problem = model.problem {
                ProblemView(words: problem, prefix: "discover", retry: model.retry, signInAgain: model.signInAgain)
            } else if model.nothing {
                StateView(kind: .empty(symbol: "safari", text: RequestWords.nothingHere, action: nil))
            } else {
                page
            }
        }
        .navigationTitle(RequestWords.discover)
        .task { await model.run() }
        .reloads(app) { model.retry() }
        .accessibilityIdentifier("discover.page")
    }

    private var page: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                if let hero = model.hero {
                    DiscoverHeroView(app: app, model: model, hero: hero)
                        .padding(.horizontal, gutter)
                } else {
                    RoundedRectangle(cornerRadius: 12)
                        .fill(.quaternary)
                        .aspectRatio(16 / 9, contentMode: .fit)
                        .frame(maxWidth: 900, maxHeight: 420)
                        .padding(.horizontal, gutter)
                        .accessibilityHidden(true)
                }
                ForEach(model.rows.filter(\.visible)) { row in
                    rowView(row)
                }
                if let yours = model.yourRequests {
                    yourRequests(yours)
                }
                TmdbCredit().padding(.horizontal, gutter)
            }
            .padding(.vertical, 16)
        }
    }

    private func rowView(_ row: DiscoverRowModel) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(row.title)
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, gutter)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    if row.cards.isEmpty {
                        ForEach(0..<6, id: \.self) { _ in PlaceholderTile(shape: .poster, width: width) }
                    }
                    ForEach(Array(row.cards.enumerated()), id: \.element.id) { index, card in
                        DiscoverCardView(app: app, card: card, width: width,
                                         identifier: "discover.card.\(row.list.rawValue).\(card.key)")
                            .onAppear { model.cardAppeared(row.list, index: index) }
                    }
                    if row.pages.loading && !row.cards.isEmpty {
                        ProgressView().frame(width: width / 2, height: Metrics.pictureHeight(.poster, width: width))
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discover.row.\(row.list.rawValue)")
        // A row is read once it comes near the screen.
        .task { await model.rowAppeared(row.list) }
    }

    /// The person's requests, with their stages; the heading's See all opens the list.
    private func yourRequests(_ cards: [RequestCard]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline) {
                Text(model.yourRequestsTitle)
                    .font(.title3.bold())
                    .accessibilityAddTraits(.isHeader)
                Spacer()
                Button(RequestWords.seeAll) { app.shell.open(.requests) }
                    .accessibilityLabel("\(RequestWords.seeAll), \(model.yourRequestsTitle)")
                    .accessibilityIdentifier("discover.yourRequests.seeAll")
            }
            .padding(.horizontal, gutter)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(cards) { card in
                        RequestTileView(app: app, card: card, width: width,
                                        identifier: "discover.yourRequests.\(card.id)")
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discover.yourRequests")
    }
}

/// Discover's hero: the title's backdrop fading into the page, its kind,
/// year and badge, its title and two lines of its overview over the fade
/// (under the picture at the accessibility text sizes); its main action
/// and Details; its request's stage, or the lines that say why there's no
/// button and what's left to ask for. The picture opens its page too.
private struct DiscoverHeroView: View {
    let app: AppModel
    @ObservedObject var model: DiscoverModel
    let hero: DiscoverHero
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Button(action: model.heroDetails) {
                picture
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(hero.card.spoken)
            .accessibilityHint("Opens its page")
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("discover.hero")
            if typeSize.isAccessibilitySize { words.accessibilityHidden(true) }
            FlowLayout(spacing: 8, lineSpacing: 8) {
                primary
                Button(action: model.heroDetails) {
                    Label(RequestWords.details, systemImage: "info.circle")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("discover.hero.details")
                if hero.primary == .following {
                    Button(RequestWords.stopFollowing, action: model.heroStopFollowing)
                        .buttonStyle(.borderless)
                        .disabled(model.asking)
                        .accessibilityIdentifier("discover.hero.stopFollowing")
                }
            }
            .controlSize(.large)
            if let pill = hero.pill {
                StagePillView(pill: pill)
            }
            ForEach(hero.lines, id: \.self) { line in
                Text(line).font(.footnote).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: 900, alignment: .leading)
    }

    private var picture: some View {
        ServerImage(app: app, paths: [hero.backdropPath, hero.card.posterPath].compactMap { $0 }, width: 900,
                    symbol: "film", limit: ImageLoader.deviceBackdropLimit)
            .aspectRatio(16 / 9, contentMode: .fill)
            .frame(maxWidth: .infinity, maxHeight: 420)
            .clipped()
            .overlay {
                // Steeper than Android's fade, which sits under light words on a
                // dark page: in light mode the words are dark, and a backdrop
                // left half showing under them hides the kind and year.
                if !typeSize.isAccessibilitySize {
                    LinearGradient(stops: [.init(color: .clear, location: 0.2),
                                           .init(color: Color.pageBackground.opacity(0.85), location: 0.6),
                                           .init(color: Color.pageBackground.opacity(0.95), location: 1)],
                                   startPoint: .top, endPoint: .bottom)
                }
            }
            .overlay(alignment: .bottomLeading) {
                if !typeSize.isAccessibilitySize { words.padding(16) }
            }
            .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    /// The kind and year with its badge, the title, two lines of overview.
    private var words: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text([hero.card.subtitle, hero.card.badge].compactMap { $0 }.joined(separator: " · "))
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
            Text(hero.card.title)
                .font(.title2.bold())
                .lineLimit(typeSize.isAccessibilitySize ? nil : 2)
                .multilineTextAlignment(.leading)
            if !hero.overview.isEmpty {
                Text(hero.overview)
                    .font(.subheadline)
                    .lineLimit(typeSize.isAccessibilitySize ? 4 : 2)
                    .multilineTextAlignment(.leading)
            }
        }
        .foregroundStyle(.primary)
    }

    @ViewBuilder private var primary: some View {
        if let button = hero.primary {
            let enabled: Bool = switch button {
            case .request(let enabled), .tellMeToo(let enabled): enabled
            case .open, .requestMore: true
            case .requested, .following: false
            }
            Button(action: model.heroPrimary) {
                Label(button.title, systemImage: Self.symbol(button))
            }
            .buttonStyle(.borderedProminent)
            .disabled(!enabled || model.asking)
            .accessibilityIdentifier("discover.hero.primary")
        }
    }

    private static func symbol(_ button: HeroButton) -> String {
        switch button {
        case .open: "play.fill"
        case .tellMeToo: "bookmark"
        case .requested, .following: "checkmark"
        case .request, .requestMore: "plus"
        }
    }
}

/// Search's See all: every title TMDB finds for the words, with its badge,
/// in a grid read on as it scrolls.
struct DiscoverResultsView: View {
    let app: AppModel
    @StateObject private var model: DiscoverResultsModel
    private let layout = WideLayout()
    @ScaledMetric(relativeTo: .body) private var typeScale: CGFloat = 1
    @Environment(\.dynamicTypeSize) private var typeSize

    init(app: AppModel, query: String) {
        self.app = app
        _model = StateObject(wrappedValue: DiscoverResultsModel(app: app, query: query))
    }

    private var width: CGFloat {
        Metrics.tileWidth(.poster, regular: layout.wide) * min(max(typeScale, 1), Metrics.maxTileScale)
    }

    private var columns: [GridItem] {
        if !layout.wide && typeSize.isAccessibilitySize {
            return Array(repeating: GridItem(.flexible(), spacing: Metrics.tileSpacing, alignment: .top), count: 2)
        }
        return [GridItem(.adaptive(minimum: width, maximum: width * 1.4), spacing: Metrics.tileSpacing,
                         alignment: .top)]
    }

    var body: some View {
        content
            .navigationTitle(model.heading)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            // Back from a title, the grid stays as it was.
            .task { if model.state == .loading { await model.load() } }
            .reloads(app) { await model.load() }
            .accessibilityIdentifier("discoverResults.page")
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .empty:
            VStack(spacing: 12) {
                Image(systemName: "magnifyingglass").font(.largeTitle).foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(model.emptyTitle).font(.headline)
                Text(model.emptyBody).foregroundStyle(.secondary)
            }
            .multilineTextAlignment(.center)
            .padding()
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("discoverResults.empty")
        case .failed(let words):
            ProblemView(words: words, prefix: "discoverResults", retry: model.retry)
        case .ready:
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if let stale = model.stale {
                        StaleLine(words: stale, retry: model.retry)
                            .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                    }
                    LazyVGrid(columns: columns, alignment: .leading, spacing: 20) {
                        ForEach(Array(model.cards.enumerated()), id: \.element.id) { index, card in
                            DiscoverCardView(app: app, card: card, width: width,
                                             identifier: "discoverResults.card.\(card.key)")
                                .onAppear { model.cardAppeared(index) }
                        }
                    }
                    .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                    if model.loadingMore {
                        ProgressView().frame(maxWidth: .infinity).padding()
                    }
                }
                .padding(.vertical, 16)
            }
        }
    }
}
