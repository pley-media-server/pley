import PleyModel
import PleyScreens
import SwiftUI

// The pieces Apple TV's request screens share, sized for a television:
// TMDB's posters with their badges, requests as tiles, stage pills in
// their colours, the download's bar, the states a page can't show in,
// Sign in (which asks first), the count on the ways in, TMDB's
// credit, the panel down the right of the screen, rows the focus comes
// back to, and the grid of TMDB's titles.

/// A TMDB title's poster as a card that lifts when focused, its status
/// badge on it ("In library", "Requested"), and its title and its year and
/// kind under it. One element for VoiceOver ("Dune: Part Two, 2024, Movie,
/// Requested"); it opens the title's page.
struct DiscoverCardView: View {
    let app: AppModel
    let card: DiscoverCard
    let width: CGFloat
    /// Its accessibility identifier: `discover.card.trending.movie:693134`.
    let identifier: String
    /// The focus came to it.
    let focused: (() -> Void)?
    /// Its row's focus, by card: the row puts the focus back on the card it last had.
    let rowFocus: FocusState<String?>.Binding?
    @FocusState private var hasFocus: Bool

    init(app: AppModel, card: DiscoverCard, width: CGFloat = Metrics.tileWidth(.poster), identifier: String,
         focused: (() -> Void)? = nil, rowFocus: FocusState<String?>.Binding? = nil) {
        self.app = app
        self.card = card
        self.width = width
        self.identifier = identifier
        self.focused = focused
        self.rowFocus = rowFocus
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button {
                app.shell.open(card.screen)
            } label: {
                picture
            }
            .buttonStyle(.card)
            .focused($hasFocus)
            .modifier(CardFocus(focus: rowFocus, id: card.key))
            .onChange(of: hasFocus) { _, now in if now { focused?() } }
            .accessibilityLabel(card.spoken)
            .accessibilityHint("Opens its page")
            .accessibilityIdentifier(identifier)
            TileCaption(title: card.title, subtitle: card.subtitle)
                .accessibilityHidden(true)
        }
        .frame(width: width, alignment: .leading)
    }

    private var picture: some View {
        ServerImage(app: app, path: card.posterPath, width: width, symbol: card.source.kind == .show ? "tv" : "film")
            .frame(width: width, height: Metrics.pictureHeight(.poster, width: width))
            .overlay(alignment: .topLeading) {
                if let badge = card.badge { TileBadge(text: badge) }
            }
            .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
    }
}

/// A request as a tile: its poster, the download's progress along the
/// picture, and its title and stage under it, the stage in its colour. One
/// element for VoiceOver ("Arrival, Downloading 40%"). It opens what the
/// request leads to.
struct RequestTileView: View {
    let app: AppModel
    let card: RequestCard
    let identifier: String
    let focused: (() -> Void)?
    let rowFocus: FocusState<Int?>.Binding?
    private let width = Metrics.tileWidth(.poster)
    @FocusState private var hasFocus: Bool

    init(app: AppModel, card: RequestCard, identifier: String, focused: (() -> Void)? = nil,
         rowFocus: FocusState<Int?>.Binding? = nil) {
        self.app = app
        self.card = card
        self.identifier = identifier
        self.focused = focused
        self.rowFocus = rowFocus
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // A tile with nowhere to go still takes the focus, so the row
            // reads on past it.
            Button {
                if let screen = card.opens { app.shell.open(screen) }
            } label: {
                ServerImage(app: app, path: card.request.posterUrl, width: width,
                            symbol: card.request.kind == .show ? "tv" : "film")
                    .frame(width: width, height: Metrics.pictureHeight(.poster, width: width))
                    .overlay(alignment: .bottom) {
                        if let progress = card.pill.progress {
                            DownloadBar(progress: progress, paused: card.pill.paused, warning: card.pill.warning,
                                        rounded: false)
                                .accessibilityHidden(true)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            }
            .buttonStyle(.card)
            .focused($hasFocus)
            .modifier(CardFocus(focus: rowFocus, id: card.id))
            .onChange(of: hasFocus) { _, now in if now { focused?() } }
            .accessibilityLabel(card.spoken)
            .accessibilityIdentifier(identifier)
            VStack(alignment: .leading, spacing: 2) {
                Text(card.title)
                    .font(.callout)
                    .lineLimit(1)
                Text(card.pill.text)
                    .font(.caption)
                    .foregroundStyle(PillColors.of(card.pill.tone))
                    .lineLimit(2)
            }
            .accessibilityHidden(true)
        }
        .frame(width: width, alignment: .leading)
    }
}

/// A tile's caption: its title, and a second line when it has one.
private struct TileCaption: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.callout)
                .lineLimit(1)
            if !subtitle.isEmpty {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}

/// White on a dark fill over a picture's corner: the accent colour is white on Apple TV.
struct TileBadge: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.caption.weight(.semibold))
            .foregroundStyle(.white)
            .lineLimit(1)
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
            .background(.black.opacity(0.75), in: Capsule())
            .padding(10)
    }
}

/// Ties a card's focus to its row's, when it is in one.
private struct CardFocus<ID: Hashable>: ViewModifier {
    let focus: FocusState<ID?>.Binding?
    let id: ID

    func body(content: Content) -> some View {
        if let focus {
            content.focused(focus, equals: id)
        } else {
            content
        }
    }
}

/// A row of cards: its title (a heading for VoiceOver), the cards in a
/// line the focus moves along, and See all at its end when it has one.
/// The row is a focus section, so moving up or down goes to the row above
/// or below; coming back to it puts the focus on the card it had last.
/// `cell` makes a card from an item and the row's focus, which the card
/// passes on as its `rowFocus`.
struct CardRow<Item: Identifiable, Cell: View>: View where Item.ID: Hashable {
    /// The row's identifier: `discover.row.trending`.
    let identifier: String
    let title: String
    let items: [Item]
    var seeAll: (() -> Void)?
    let cell: (Item, FocusState<Item.ID?>.Binding) -> Cell
    @FocusState private var focusedItem: Item.ID?
    @State private var lastFocused: Item.ID?

    init(identifier: String, title: String, items: [Item], seeAll: (() -> Void)? = nil,
         @ViewBuilder cell: @escaping (Item, FocusState<Item.ID?>.Binding) -> Cell) {
        self.identifier = identifier
        self.title = title
        self.items = items
        self.seeAll = seeAll
        self.cell = cell
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                    ForEach(items) { item in
                        cell(item, $focusedItem)
                    }
                    if let seeAll {
                        Button(RequestWords.seeAll, action: seeAll)
                            .accessibilityLabel("\(RequestWords.seeAll), \(title)")
                            .accessibilityIdentifier("\(identifier).seeAll")
                    }
                }
                // Room for the focused card to lift.
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 32)
            }
            .scrollClipDisabled()
        }
        .defaultFocus($focusedItem, lastFocused, priority: .userInitiated)
        .onChange(of: focusedItem) { _, now in
            if let now { lastFocused = now }
        }
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }
}

/// A stage pill's colours: waiting for approval amber; on its way blue;
/// ready green; declined and failed red; gone gray. Blue is named, as the
/// accent colour is white on Apple TV.
enum PillColors {
    static func of(_ tone: PillTone) -> Color {
        switch tone {
        case .amber: .orange
        case .blue: .blue
        case .green: .green
        case .red: .red
        case .gray: .secondary
        }
    }

    static func fill(_ tone: PillTone) -> Color {
        switch tone {
        case .amber: .orange.opacity(0.2)
        case .blue: .blue.opacity(0.22)
        case .green: .green.opacity(0.22)
        case .red: .red.opacity(0.22)
        case .gray: .secondary.opacity(0.2)
        }
    }
}

/// A request's stage: its words beside a dot in the stage's colour, the
/// download's bar under it while one runs (gray while paused, red with a
/// problem), and the line a failed request has.
struct StagePillView: View {
    let pill: PillWords
    /// The bar's width at most.
    var barWidth: CGFloat = 400

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Circle().fill(PillColors.of(pill.tone)).frame(width: 12, height: 12)
                Text(pill.text)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(PillColors.of(pill.tone))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 6)
            .background(PillColors.fill(pill.tone), in: RoundedRectangle(cornerRadius: 10))
            .accessibilityElement(children: .combine)
            if let progress = pill.progress {
                DownloadBar(progress: progress, paused: pill.paused, warning: pill.warning)
                    .frame(maxWidth: barWidth)
            }
            if let line = pill.line {
                Text(line).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

/// A download's progress, 8 points tall; VoiceOver hears its percentage.
struct DownloadBar: View {
    let progress: Double
    var paused = false
    var warning = false
    /// Its ends rounded; along a picture's edge, square.
    var rounded = true

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Rectangle().fill(rounded ? AnyShapeStyle(.quaternary) : AnyShapeStyle(.black.opacity(0.45)))
                Rectangle().fill(warning ? Color.red : paused ? Color.gray : Color.blue)
                    .frame(width: geometry.size.width * min(1, max(0, progress)))
            }
        }
        .frame(height: 8)
        .clipShape(RoundedRectangle(cornerRadius: rounded ? 4 : 0))
        .accessibilityElement()
        .accessibilityLabel("Download")
        .accessibilityValue("\(Int((min(1, max(0, progress)) * 100).rounded())) percent")
    }
}

/// Sign in again, which asks first: "Sign in again?", and that this Apple
/// TV shows a code to link it to the server again. Yes signs out of the
/// server and opens signing in to it.
struct SignInAgainButton: View {
    let app: AppModel
    let identifier: String
    @State private var asking = false

    var body: some View {
        Button(RequestWords.signIn) { asking = true }
            .accessibilityIdentifier(identifier)
            .confirmationDialog(RequestWords.signInAsk, isPresented: $asking, titleVisibility: .visible) {
                Button(RequestWords.signIn) { app.signInAgain() }
                    .accessibilityIdentifier("\(identifier).confirm")
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(RequestWords.signInAskBody(app.server?.name ?? "the server"))
            }
    }
}

/// Why a page can't show: requests off, no access, sign in again (with
/// Sign in), TMDB's key refused, TMDB away or the server out of
/// reach (with Retry), a title not found. Its button takes the focus, and
/// VoiceOver reads it out when it shows. Its identifiers start with
/// `prefix`: `discover.problem`, `discover.retry`.
struct ProblemView: View {
    let app: AppModel
    let words: ProblemWords
    let prefix: String
    var retry: (() -> Void)?

    var body: some View {
        VStack(spacing: 24) {
            Image(systemName: words.symbol)
                .font(.system(size: 60))
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(words.title).font(.title3.bold())
            if let body = words.body {
                Text(body).foregroundStyle(.secondary)
            }
            if words.signInAgain {
                SignInAgainButton(app: app, identifier: "\(prefix).signInAgain")
            } else if words.retry, let retry {
                Button(action: retry) {
                    Label(RequestWords.retry, systemImage: "arrow.clockwise")
                }
                .accessibilityIdentifier("\(prefix).retry")
            }
        }
        .multilineTextAlignment(.center)
        .frame(maxWidth: 1000)
        .frame(maxWidth: .infinity, minHeight: 400)
        .padding(Metrics.gutter)
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(prefix).problem")
        .task(id: words) { announce([words.title, words.body].compactMap { $0 }.joined(separator: " ")) }
    }
}

/// Has VoiceOver read `text` out.
func announce(_ text: String) {
    AccessibilityNotification.Announcement(text).post()
}

/// The number on the ways into requests: blue, red while failed requests
/// wait for a manager. Its words are on what holds it.
struct CountBadge: View {
    let text: String
    var alarm = false

    var body: some View {
        Text(text)
            .font(.caption.bold().monospacedDigit())
            .foregroundStyle(.white)
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
            .frame(minWidth: 40)
            .background(alarm ? Color.red : Color.blue, in: Capsule())
            .accessibilityHidden(true)
    }
}

/// "Details and pictures from TMDB.", at the foot of Discover and of every title page.
struct TmdbCredit: View {
    var body: some View {
        Text(RequestWords.credit)
            .font(.caption)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Metrics.gutter)
            .accessibilityIdentifier("tmdb.credit")
    }
}

/// A panel down the right of the screen over what was showing, as Add to
/// playlist is: what was showing dims, the panel scrolls and holds the
/// focus. Menu closes it, unless its content takes Menu itself (back a
/// step inside the panel).
struct SidePanel<Content: View>: View {
    var width: CGFloat = 760
    let identifier: String
    @ViewBuilder let content: () -> Content

    init(width: CGFloat = 760, identifier: String, @ViewBuilder content: @escaping () -> Content) {
        self.width = width
        self.identifier = identifier
        self.content = content
    }

    var body: some View {
        HStack(spacing: 0) {
            Color.black.opacity(0.45)
                .ignoresSafeArea()
                .accessibilityHidden(true)
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    content()
                }
                .padding(60)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(width: width)
            .background(.regularMaterial, ignoresSafeAreaEdges: .all)
            .focusSection()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(identifier)
        }
        .presentationBackground(.clear)
    }
}

extension View {
    /// Shows a `SidePanel` while `item` isn't `nil`; closing it sets `item` to `nil`.
    func sidePanel<Item: Identifiable, Content: View>(
        item: Binding<Item?>, identifier: String, @ViewBuilder content: @escaping (Item) -> Content
    ) -> some View {
        fullScreenCover(item: item) { shown in
            SidePanel(identifier: identifier) { content(shown) }
        }
    }

    /// Shows a `SidePanel` while `isPresented`.
    func sidePanel<Content: View>(
        isPresented: Binding<Bool>, identifier: String, @ViewBuilder content: @escaping () -> Content
    ) -> some View {
        fullScreenCover(isPresented: isPresented) {
            SidePanel(identifier: identifier, content: content)
        }
    }
}

/// A grid of TMDB's titles on Apple TV: a Discover row's See all
/// (`.list`) or Search's (`.words`). Its heading, then posters in rows of
/// `columns`; as the focus comes within two rows of the end, it wants
/// three rows more. When the reading stops short, or a page fails, Show
/// more (or Retry) is at its end, beside why when nothing came yet or a
/// page failed. Menu goes back.
struct DiscoverGridView: View {
    /// Posters across the screen: 250 points wide with room to lift, inside the margins.
    static let columns = 6

    @ObservedObject private var app: AppModel
    @StateObject private var model: DiscoverResultsModel
    /// What its identifiers start with: `discoverList` or `discoverResults`.
    private let prefix: String
    private let width = Metrics.tileWidth(.poster)

    init(app: AppModel, source: DiscoverSource) {
        self.app = app
        _model = StateObject(wrappedValue: DiscoverResultsModel(app: app, source: source,
                                                                screenful: Self.columns * 4, tv: true))
        if case .list = source {
            prefix = "discoverList"
        } else {
            prefix = "discoverResults"
        }
    }

    var body: some View {
        content
            // Back from a title, the grid stays as it was.
            .task { if model.state == .loading { await model.load() } }
            .onChange(of: app.change) { _, change in
                if change?.everything == true { Task { await model.load() } }
            }
            .accessibilityIdentifier("\(prefix).page")
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .empty:
            VStack(spacing: 24) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 60))
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(model.emptyTitle).font(.title3.bold())
                Text(model.emptyBody).foregroundStyle(.secondary)
            }
            .multilineTextAlignment(.center)
            .frame(maxWidth: 1000)
            .frame(maxWidth: .infinity, minHeight: 400)
            .padding(Metrics.gutter)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("\(prefix).empty")
            .task { announce("\(model.emptyTitle) \(model.emptyBody)") }
        case .failed(let words):
            ProblemView(app: app, words: words, prefix: prefix, retry: model.retry)
        case .ready:
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    Text(model.heading)
                        .font(.title3.bold())
                        .accessibilityAddTraits(.isHeader)
                        .padding(.horizontal, Metrics.gutter)
                    grid
                    end
                }
                .padding(.vertical, 24)
            }
        }
    }

    private var grid: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.fixed(width), spacing: Metrics.tileSpacing, alignment: .top),
                                 count: Self.columns),
                  alignment: .leading, spacing: Metrics.rowSpacing) {
            ForEach(Array(model.cards.enumerated()), id: \.element.id) { index, card in
                DiscoverCardView(app: app, card: card, width: width, identifier: "\(prefix).card.\(card.key)") {
                    focused(index)
                }
            }
        }
        .padding(.horizontal, Metrics.gutter)
        .focusSection()
    }

    /// The focus on the card at `index`: within two rows of the end, three rows more.
    private func focused(_ index: Int) {
        let count = model.cards.count
        if index >= count - 2 * Self.columns { model.want(count + 3 * Self.columns) }
    }

    /// The grid's end: reading on, or Show more (Retry when a page failed)
    /// beside why when nothing came yet or a page failed.
    @ViewBuilder private var end: some View {
        if model.loadingMore {
            ProgressView()
                .frame(maxWidth: .infinity)
                .accessibilityLabel("Loading")
        } else if model.stopped {
            HStack(alignment: .center, spacing: 40) {
                Button(model.failedMore ? RequestWords.retry : RequestWords.showMore, action: model.showMore)
                    .accessibilityIdentifier("discoverGrid.more")
                if model.cards.isEmpty || model.failedMore {
                    let aside = model.noneYet
                    VStack(alignment: .leading, spacing: 6) {
                        Text(aside.title).font(.headline)
                        if let body = aside.body {
                            Text(body).font(.callout).foregroundStyle(.secondary)
                        }
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("discoverGrid.noneYet")
                }
            }
            .padding(.horizontal, Metrics.gutter)
            .focusSection()
        }
    }
}
