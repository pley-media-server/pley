import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// Discover on Apple TV, the tab to ask for movies and shows (docs/ANDROID.md
/// says what Android TV's does, which this follows). At rest, the hero for
/// the first trending title over its backdrop: its title, facts, a line of
/// overview, its main action and Details. Above it, the way to the
/// person's requests with their number, for an account that has requests
/// to see. Down go TMDB's rows of posters with their badges, each read as
/// it comes into view, each with See all; then the person's requests; then
/// TMDB's credit. The backdrop follows the title the focus rests on in the
/// rows. Menu from the rows goes back to the hero, then to the tabs. When
/// requests can't be used, the page says why, and only that.
struct DiscoverView: View {
    let app: AppModel
    @ObservedObject private var shell: ShellModel
    @StateObject private var model: DiscoverModel

    init(app: AppModel) {
        self.app = app
        _shell = ObservedObject(wrappedValue: app.shell)
        _model = StateObject(wrappedValue: DiscoverModel(app: app))
    }

    var body: some View {
        content
            .task { await model.run() }
            .accessibilityIdentifier("discover.page")
    }

    @ViewBuilder private var content: some View {
        if let problem = model.problem {
            ProblemView(app: app, words: problem, prefix: "discover", retry: model.retry)
        } else if let hero = model.hero {
            DiscoverPage(app: app, model: model, hero: hero, entry: shell.requestsEntry)
        } else if model.nothing {
            DiscoverNothing()
        } else {
            StateView(kind: .loading)
        }
    }
}

/// Every row read, and none with anything this account can ask for.
private struct DiscoverNothing: View {
    var body: some View {
        VStack(spacing: 24) {
            Image(systemName: "safari")
                .font(.system(size: 60))
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(RequestWords.nothingHere).font(.title3.bold())
            Text(RequestWords.nothingBody).foregroundStyle(.secondary)
        }
        .multilineTextAlignment(.center)
        .frame(maxWidth: 1000)
        .frame(maxWidth: .infinity, minHeight: 400)
        .padding(Metrics.gutter)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("discover.nothing")
        .task { announce("\(RequestWords.nothingHere). \(RequestWords.nothingBody)") }
    }
}

/// The hero's buttons and the way to the requests, for the focus to go back to.
private enum HeroFocus: Hashable {
    case requests, primary, details
}

/// Discover with its hero: the backdrop behind it all, the hero, then the rows.
private struct DiscoverPage: View {
    let app: AppModel
    @ObservedObject var model: DiscoverModel
    let hero: DiscoverHero
    let entry: RequestsEntry
    @FocusState private var heroFocus: HeroFocus?
    /// The backdrop of the title or request the focus is on in the rows; `nil` on the hero.
    @State private var pending: [String]?
    /// `pending` once the focus has rested on it, so moving along a row doesn't flash every backdrop.
    @State private var rested: [String]?

    private static let heroID = "discover.hero"

    var body: some View {
        ZStack(alignment: .top) {
            backdrop
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        heroView
                            .id(Self.heroID)
                        rows
                            // Menu from the rows: back to the hero; from the hero, to the tabs.
                            .onExitCommand { backToHero(proxy) }
                    }
                    .padding(.bottom, 80)
                }
                .scrollClipDisabled()
            }
        }
        .defaultFocus($heroFocus, mainFocus)
        .onChange(of: heroFocus) { _, now in
            if now != nil { rowFocused(nil) }
        }
        .task(id: pending) {
            guard pending != rested else { return }
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            rested = pending
        }
    }

    /// The main action where it can be pressed, else Details.
    private var mainFocus: HeroFocus {
        if case .request(enabled: false) = hero.primary { return .details }
        if model.asking || hero.primary == nil { return .details }
        return .primary
    }

    private func backToHero(_ proxy: ScrollViewProxy) {
        withAnimation { proxy.scrollTo(Self.heroID, anchor: .top) }
        let target = mainFocus
        // Once the hero is back on screen.
        Task { @MainActor in
            await Task.yield()
            heroFocus = target
        }
    }

    private func rowFocused(_ paths: [String]?) {
        pending = paths
        if paths == nil { rested = nil }
    }

    // MARK: Backdrop

    private var backdropPaths: [String] {
        rested ?? [hero.backdropPath, hero.card.posterPath].compactMap { $0 }
    }

    /// The hero's backdrop (or, in the rows, the focused title's) behind
    /// the whole page, darkened towards the words and the rows.
    private var backdrop: some View {
        ServerImage(app: app, paths: backdropPaths, width: 1920, symbol: nil, limit: ImageLoader.deviceBackdropLimit)
            .id(backdropPaths)
            .frame(width: 1920, height: 1080)
            .clipped()
            .overlay {
                LinearGradient(colors: [.black.opacity(0.85), .black.opacity(0.35), .clear],
                               startPoint: .leading, endPoint: .trailing)
            }
            .overlay {
                LinearGradient(colors: [.clear, .black.opacity(rested == nil ? 0.9 : 0.95)],
                               startPoint: .center, endPoint: .bottom)
            }
            .ignoresSafeArea()
            .accessibilityHidden(true)
    }

    // MARK: Hero

    private var facts: String {
        let meta = hero.detail.map { DiscoverTitlePage.of($0).meta } ?? [hero.card.subtitle]
        return (meta + [hero.card.badge].compactMap { $0 }).filter { !$0.isEmpty }.joined(separator: " · ")
    }

    private var heroView: some View {
        VStack(alignment: .leading, spacing: 20) {
            requestsWay
            Spacer(minLength: 40)
            Text(hero.card.title)
                .font(.system(size: 64, weight: .bold))
                .lineLimit(2)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("discover.hero.title")
            if !facts.isEmpty {
                Text(facts)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if !hero.overview.isEmpty {
                Text(hero.overview)
                    .font(.callout)
                    .lineLimit(1)
                    .frame(maxWidth: 1100, alignment: .leading)
            }
            buttons
            if let pill = hero.pill {
                StagePillView(pill: pill)
            }
            ForEach(hero.lines, id: \.self) { line in
                Text(line).font(.callout).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, Metrics.gutter)
        // Room between the request's stage and the first row's name.
        .padding(.bottom, 40)
        .frame(maxWidth: .infinity, minHeight: 640, alignment: .bottomLeading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Self.heroID)
    }

    /// "Your requests" ("Requests" for managers) with the number waiting
    /// there, at the top right, for an account with requests to see. A
    /// focus section across the page, so up from the hero's buttons gets to it.
    @ViewBuilder private var requestsWay: some View {
        if entry.requests {
            HStack {
                Spacer()
                Button {
                    app.shell.open(.requests)
                } label: {
                    HStack(spacing: 16) {
                        Label(entry.requestsTitle, systemImage: "list.bullet.rectangle")
                        if let count = entry.badgeText {
                            CountBadge(text: count, alarm: entry.alarm)
                        }
                    }
                }
                .focused($heroFocus, equals: .requests)
                .accessibilityLabel(entry.requestsTitle)
                .accessibilityValue(entry.spoken ?? "")
                .accessibilityIdentifier("discover.requests")
            }
            .frame(maxWidth: .infinity)
            // Under the tab bar, not beside it.
            .padding(.top, 140)
            .focusSection()
        }
    }

    private var buttons: some View {
        HStack(spacing: 24) {
            if let button = hero.primary {
                let enabled: Bool = switch button {
                case .request(let enabled): enabled
                case .tellMeToo, .open, .requestMore: true
                }
                Button(action: model.heroPrimary) {
                    Label(button.title, systemImage: Self.symbol(button))
                }
                .disabled(!enabled || model.asking)
                .focused($heroFocus, equals: .primary)
                .accessibilityIdentifier("discover.hero.primary")
            }
            Button(action: model.heroDetails) {
                Label(RequestWords.details, systemImage: "info.circle")
            }
            .focused($heroFocus, equals: .details)
            .accessibilityHint("Opens its page")
            .accessibilityIdentifier("discover.hero.details")
        }
        .padding(.vertical, 12)
        // Across the page, so up from anywhere in the rows, and down from
        // the way to the requests at the right, lands on the buttons.
        .frame(maxWidth: .infinity, alignment: .leading)
        .focusSection()
    }

    private static func symbol(_ button: HeroButton) -> String {
        switch button {
        case .open: "play.fill"
        case .tellMeToo: "bookmark"
        case .request, .requestMore: "plus"
        }
    }

    // MARK: Rows

    private var rows: some View {
        LazyVStack(alignment: .leading, spacing: Metrics.rowSpacing / 2) {
            ForEach(model.rows.filter(\.visible)) { row in
                titleRow(row)
                    // A row is read once it comes near the screen.
                    .task { await model.rowAppeared(row.list) }
            }
            if let yours = model.yourRequests {
                CardRow(identifier: "discover.yourRequests", title: model.yourRequestsTitle, items: yours,
                        seeAll: { app.shell.open(.requests) }) { card, focus in
                    RequestTileView(app: app, card: card, identifier: "discover.yourRequests.\(card.id)",
                                    focused: { rowFocused([card.request.backdropUrl, card.request.posterUrl]
                                                   .compactMap { $0 }) },
                                    rowFocus: focus)
                }
            }
            TmdbCredit()
                .padding(.top, 24)
        }
    }

    @ViewBuilder private func titleRow(_ row: DiscoverRowModel) -> some View {
        let identifier = "discover.row.\(row.list.rawValue)"
        if row.cards.isEmpty {
            // Still being read: its place, with empty posters.
            VStack(alignment: .leading, spacing: 0) {
                Text(row.title)
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                    .padding(.horizontal, Metrics.gutter)
                HStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(0..<6, id: \.self) { _ in
                        PlaceholderTile(shape: .poster, width: Metrics.tileWidth(.poster))
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 32)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .clipped()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(identifier)
        } else {
            CardRow(identifier: identifier, title: row.title, items: row.cards,
                    seeAll: { app.shell.open(.discoverList(row.list)) }) { card, focus in
                DiscoverCardView(app: app, card: card, identifier: "discover.card.\(row.list.rawValue).\(card.key)",
                                 focused: {
                                     rowFocused([card.source.backdropUrl, card.posterPath].compactMap { $0 })
                                 },
                                 rowFocus: focus)
                    // Near the row's end, its next page.
                    .onAppear {
                        if let index = row.cards.firstIndex(where: { $0.key == card.key }) {
                            model.cardAppeared(row.list, index: index)
                        }
                    }
            }
        }
    }
}
