import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// A title found on TMDB, for the sofa, laid out as a library's title page
/// is (docs/ANDROID.md, Requests, as on Android TV): the backdrop fills the
/// screen behind the title (the poster stands at the right without one),
/// its facts and genres, tagline and overview, and its one main action
/// with the request's stage and the lines that say what's left. Then the
/// requests for it, each opening its panel of actions; the seasons as
/// posters, each with its state or its request's stage, where Select on
/// one that may be asked for opens Which seasons? at it; the cast; TMDB's
/// credit. Menu goes back.
struct DiscoverTitleView: View {
    let app: AppModel
    @StateObject private var model: DiscoverTitleModel
    /// The season Which seasons? opens at, when it was opened from one.
    @State private var pickerStart: Int?
    /// The request whose panel shows.
    @State private var panel: PanelRequest?

    init(app: AppModel, kind: TitleKind, tmdbId: Int, pick: Bool = false) {
        self.app = app
        _model = StateObject(wrappedValue: DiscoverTitleModel(app: app, kind: kind, tmdbId: tmdbId, pick: pick))
    }

    var body: some View {
        content
            .task { await model.run() }
            .sidePanel(isPresented: Binding(get: { model.picker != nil }, set: { if !$0 { model.closePicker() } }),
                       identifier: "picker") {
                SeasonPickerPanel(model: model, start: pickerStart)
            }
            .sidePanel(item: $panel, identifier: "requests.panel") { shown in
                TitleRequestPanel(app: app, model: model, requestId: shown.id) { panel = nil }
            }
            // A request withdrawn or deleted from its panel leaves the page: its panel goes with it.
            .onChange(of: model.requests) { _, requests in
                if let shown = panel, !requests.contains(where: { $0.id == shown.id }) { panel = nil }
            }
            .onChange(of: model.picker == nil) { _, closed in if closed { pickerStart = nil } }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let words):
            ProblemView(app: app, words: words, prefix: "discoverTitle", retry: model.retry)
        case .ready:
            if let page = model.page {
                loaded(page)
            } else {
                StateView(kind: .loading)
            }
        }
    }

    private func loaded(_ page: DiscoverTitlePage) -> some View {
        ZStack(alignment: .top) {
            backdrop(page)
            ScrollView {
                VStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                    hero(page)
                    if !model.requests.isEmpty {
                        TitleRequestsSection(app: app, runner: model.runner, heading: model.requestsHeading,
                                             requests: model.requests) { card in panel = PanelRequest(id: card.id) }
                    }
                    if !model.seasons.isEmpty { seasons }
                    if !model.cast.isEmpty { cast }
                    // Focusable, so the remote scrolls down to it.
                    TmdbCredit()
                        .focusable()
                }
                .padding(.bottom, 80)
            }
            .scrollClipDisabled()
        }
        .accessibilityIdentifier("discoverTitle.page")
    }

    // MARK: Hero

    /// The backdrop behind the whole page, darkened towards the text.
    @ViewBuilder private func backdrop(_ page: DiscoverTitlePage) -> some View {
        if let path = page.backdropPath {
            ServerImage(app: app, paths: [path], width: 1920, symbol: nil, limit: ImageLoader.deviceBackdropLimit)
                .frame(width: 1920, height: 1080)
                .clipped()
                .overlay {
                    LinearGradient(colors: [.black.opacity(0.85), .black.opacity(0.35), .clear],
                                   startPoint: .leading, endPoint: .trailing)
                }
                .overlay {
                    LinearGradient(colors: [.clear, .black.opacity(0.9)], startPoint: .center, endPoint: .bottom)
                }
                .ignoresSafeArea()
                .accessibilityHidden(true)
        }
    }

    private func hero(_ page: DiscoverTitlePage) -> some View {
        HStack(alignment: .bottom, spacing: 60) {
            VStack(alignment: .leading, spacing: 20) {
                Spacer(minLength: 120)
                Text(page.title)
                    .font(.system(size: 64, weight: .bold))
                    .lineLimit(2)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("discoverTitle.title")
                if let original = page.originalTitle {
                    Text(original).font(.headline).foregroundStyle(.secondary)
                }
                meta(page)
                if let genres = page.genres {
                    Text(genres).font(.callout).foregroundStyle(.secondary)
                }
                if let tagline = page.tagline {
                    Text(tagline).font(.headline).italic()
                }
                if !page.overview.isEmpty {
                    Text(page.overview).font(.callout).lineLimit(4).frame(maxWidth: 1100, alignment: .leading)
                }
                actionBlock
                if let stale = model.stale {
                    staleLine(stale)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            // No backdrop on TMDB: the poster stands at the right instead.
            if page.backdropPath == nil, page.posterPath != nil {
                ServerImage(app: app, path: page.posterPath, width: 400, symbol: model.kind == .show ? "tv" : "film")
                    .frame(width: 400, height: 600)
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                    .accessibilityHidden(true)
            }
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(minHeight: 860, alignment: .bottomLeading)
    }

    /// "2024 · 1h 42m · ★ 8.4 · Movie", the age rating boxed.
    private func meta(_ page: DiscoverTitlePage) -> some View {
        HStack(spacing: 16) {
            ForEach(Array(page.meta.enumerated()), id: \.offset) { index, part in
                if index > 0 { Text(verbatim: "·") }
                Text(part)
            }
            if let certification = page.certification {
                Text(verbatim: "·")
                Text(certification)
                    .padding(.horizontal, 8)
                    .overlay(RoundedRectangle(cornerRadius: 4).stroke(.secondary, lineWidth: 2))
            }
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel((page.meta + [page.certification].compactMap { $0 }).joined(separator: ", "))
        .accessibilityIdentifier("discoverTitle.meta")
    }

    /// Reading the page again failed: what shows is from before, and why, with Retry.
    private func staleLine(_ words: ProblemWords) -> some View {
        HStack(spacing: 24) {
            Label(words.title, systemImage: words.symbol)
                .font(.callout)
                .foregroundStyle(.secondary)
            if words.retry {
                Button(RequestWords.retry, action: model.retry)
                    .font(.callout)
                    .accessibilityIdentifier("discoverTitle.stale.retry")
            }
        }
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discoverTitle.stale")
    }

    // MARK: The main action

    /// The one main action, the first that applies: Sign in; Open;
    /// Requested or Following with the request's stage (and Stop
    /// following); Tell me too; Open and Request more seasons; Request.
    /// The lines under it say why there's no button and what's left, red
    /// when nothing is left to ask for.
    private var actionBlock: some View {
        VStack(alignment: .leading, spacing: 16) {
            ScrollView(.horizontal) {
                HStack(spacing: 24) {
                    buttons
                }
                .padding(.vertical, 24)
            }
            .scrollClipDisabled()
            .focusSection()
            if let pill = model.actionPill {
                StagePillView(pill: pill)
                    .accessibilityIdentifier("discoverTitle.pill")
            }
            if !model.lines.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(model.lines, id: \.self) { line in
                        Text(line)
                            .font(.callout)
                            .foregroundStyle(model.limitReached ? Color.red : Color.secondary)
                    }
                }
                .frame(maxWidth: 1100, alignment: .leading)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("discoverTitle.lines")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discoverTitle.action")
    }

    // The buttons don't wait while an ask is on its way (a disabled button
    // on Apple TV lets go of the focus); the model ignores them meanwhile.
    @ViewBuilder private var buttons: some View {
        switch model.action {
        case .signInAgain:
            SignInAgainButton(app: app, identifier: "discoverTitle.signInAgain")
        case .open(let itemId?):
            openButton(itemId)
        case .mine:
            // Inert, but it takes the focus, so the remote can read the page from it.
            Button {} label: { Label(RequestWords.requested, systemImage: "checkmark") }
                .accessibilityIdentifier("discoverTitle.requested")
        case .following:
            Button {} label: { Label(RequestWords.following, systemImage: "checkmark") }
                .accessibilityIdentifier("discoverTitle.following")
            Button(action: model.stopFollowing) { Label(RequestWords.stopFollowing, systemImage: "bookmark.slash") }
                .accessibilityIdentifier("discoverTitle.stopFollowing")
        case .tellMeToo(.yes):
            Button(action: model.tellMeToo) { Label(RequestWords.tellMeToo, systemImage: "bookmark") }
                .accessibilityIdentifier("discoverTitle.tellMeToo")
        case .partial(let itemId, let more):
            if let itemId { openButton(itemId) }
            if more == .yes || more == .limit {
                Button(action: openPicker) { Label(RequestWords.moreSeasons, systemImage: "plus") }
                    .disabled(more != .yes)
                    .accessibilityIdentifier("discoverTitle.requestMore")
            }
        case .request(let ask) where ask != .noAccess:
            Button {
                pickerStart = nil
                model.request()
            } label: {
                Label(RequestWords.request, systemImage: "plus")
            }
            .disabled(ask != .yes)
            .accessibilityIdentifier("discoverTitle.request")
        default:
            EmptyView()
        }
    }

    private func openButton(_ itemId: Int) -> some View {
        Button {
            app.shell.open(.item(itemId))
        } label: {
            Label(RequestWords.open, systemImage: "play.fill")
        }
        .accessibilityIdentifier("discoverTitle.open")
    }

    private func openPicker() {
        pickerStart = nil
        model.openPicker()
    }

    /// Whether a show's seasons may be picked to ask for now: Request, or
    /// Request more seasons, even at the limit (the picker says so).
    private var canPick: Bool {
        guard let detail = model.detail, detail.kind == .show,
              (detail.seasons ?? []).contains(where: { RequestRules.askable($0, in: detail) }) else { return false }
        switch model.action {
        case .request(let ask): return ask != .noAccess
        case .partial(_, let more): return more == .yes || more == .limit
        default: return false
        }
    }

    // MARK: Rows

    /// The seasons, specials last, as posters: each with its badge (In
    /// library, Partly in library, Requested, Declined) and under it its
    /// request's stage, else its state ("10 episodes", "Airs Nov 4, 2026").
    private var seasons: some View {
        let detail = model.detail
        let byNumber = Dictionary((detail?.seasons ?? []).map { ($0.number, $0) }, uniquingKeysWith: { a, _ in a })
        let picks = canPick
        return CardRow(identifier: "discoverTitle.seasons", title: RequestWords.seasonsHeading,
                       items: model.seasons) { row, focus in
            let season = byNumber[row.number]
            SeasonTile(app: app, row: row, season: season, showPoster: model.page?.posterPath,
                       badge: season.flatMap { Self.badge($0, itemId: detail?.itemId) }, focus: focus) {
                guard picks, let season, let detail, RequestRules.askable(season, in: detail) else { return }
                pickerStart = season.number
                model.openPicker()
            }
        }
    }

    /// A season's badge: what it is for this account; none where it's still to ask for.
    private static func badge(_ season: DiscoverSeason, itemId: Int?) -> String? {
        if season.state == .library { return RequestWords.badge(.library, itemId: itemId) }
        if season.state == .partial { return RequestWords.badge(.partial, itemId: itemId) }
        if season.state == .requested { return RequestWords.badge(.requested, itemId: itemId) }
        if season.declined { return RequestWords.badge(.declined, itemId: itemId) }
        return nil
    }

    /// The cast with their photos, round; TMDB's people have no page here,
    /// so the row is there to read.
    private var cast: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(RequestWords.cast)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: 40) {
                    ForEach(model.cast) { person in
                        CastPhoto(app: app, person: person)
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 32)
            }
            .scrollClipDisabled()
        }
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discoverTitle.cast")
    }
}

/// The request whose panel shows, by its id: the panel shows the request as the page reads it now.
private struct PanelRequest: Identifiable, Hashable {
    let id: Int
}

/// A season as a poster that lifts when focused (the show's poster
/// without one of its own), its badge on it, the download's progress along
/// it, and its name and its request's stage (in the stage's colour) or its
/// state under it. One element for VoiceOver.
private struct SeasonTile: View {
    let app: AppModel
    let row: SeasonRow
    let season: DiscoverSeason?
    let showPoster: String?
    let badge: String?
    /// Its row's focus, by season: the row puts the focus back on the season it last had.
    let focus: FocusState<Int?>.Binding
    /// Select: Which seasons? at it, where it may be asked for.
    let select: () -> Void
    private let width = Metrics.tileWidth(.poster)

    /// The request's stage stands for the season while the season is asked for.
    private var pill: PillWords? { season?.state == .requested ? row.pill : nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button(action: select) {
                ServerImage(app: app, paths: [row.posterPath, showPoster].compactMap { $0 }, width: width, symbol: "tv")
                    .frame(width: width, height: Metrics.pictureHeight(.poster, width: width))
                    .overlay(alignment: .topLeading) {
                        if let badge { TileBadge(text: badge) }
                    }
                    .overlay(alignment: .bottom) {
                        if let pill, let progress = pill.progress {
                            DownloadBar(progress: progress, paused: pill.paused, warning: pill.warning, rounded: false)
                                .accessibilityHidden(true)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            }
            .buttonStyle(.card)
            .focused(focus, equals: row.id)
            .accessibilityLabel([row.name, badge, pill?.text ?? row.state].compactMap { $0 }.joined(separator: ", "))
            .accessibilityIdentifier("discoverTitle.season.\(row.number)")
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name)
                    .font(.callout)
                    .lineLimit(1)
                Text(pill?.text ?? row.state)
                    .font(.caption)
                    .foregroundStyle(pill.map { PillColors.of($0.tone) } ?? Color.secondary)
                    .lineLimit(2)
            }
            .accessibilityHidden(true)
        }
        .frame(width: width, alignment: .leading)
    }
}

/// The requests for the title ("Your requests"; everyone's for managers),
/// one under another; Select on one opens its panel of actions.
private struct TitleRequestsSection: View {
    let app: AppModel
    @ObservedObject var runner: RequestActionRunner
    let heading: String
    let requests: [RequestCard]
    let select: (RequestCard) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(heading)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            ForEach(requests) { card in
                RequestCardRow(app: app, card: card, busy: runner.busy.contains(card.id)) { select(card) }
            }
        }
        .frame(maxWidth: 1300, alignment: .leading)
        .padding(.horizontal, Metrics.gutter)
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discoverTitle.requests")
    }
}

/// A request's panel on the title's page, showing the request as the page
/// has it now, without Details; it closes when the request leaves the page.
private struct TitleRequestPanel: View {
    let app: AppModel
    @ObservedObject var model: DiscoverTitleModel
    let requestId: Int
    let close: () -> Void

    var body: some View {
        if let card = model.requests.first(where: { $0.id == requestId }) {
            // No Details: this is its title's page.
            RequestPanel(app: app, card: card, runner: model.runner, manage: model.gate.summary?.manage == true,
                         details: false, onClose: close)
        }
    }
}

/// Which seasons?, down the right of the screen: the title under its
/// name, every season (specials last) with its state, those that may be
/// asked for with a tick; the allowance, red while more are ticked than
/// may be asked for; Request N seasons, and Cancel. The focus starts on the
/// season it was opened at, else the first that may be asked for. Menu
/// closes it.
private struct SeasonPickerPanel: View {
    @ObservedObject var model: DiscoverTitleModel
    let start: Int?
    @FocusState private var focused: Int?

    var body: some View {
        if let picker = model.picker {
            VStack(alignment: .leading, spacing: 28) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(RequestWords.pickerTitle)
                        .font(.title3.bold())
                        .accessibilityAddTraits(.isHeader)
                    if let title = model.page?.title {
                        Text(title).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(picker.rows) { row in
                        seasonButton(row, picked: picker.picked.contains(row.season.number))
                    }
                }
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(model.pickerLines, id: \.self) { line in
                        Text(line)
                            .font(.callout)
                            .foregroundStyle(picker.overLimit ? Color.red : Color.secondary)
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("picker.lines")
                HStack(spacing: 24) {
                    Button(action: model.askPicked) {
                        Label(picker.button, systemImage: "plus")
                    }
                    .disabled(!picker.ready || model.busy)
                    .accessibilityIdentifier("picker.ask")
                    Button(RequestWords.cancel, action: model.closePicker)
                        .accessibilityIdentifier("picker.cancel")
                }
            }
            .defaultFocus($focused, first(picker))
            .onAppear { focused = first(picker) }
        }
    }

    /// The season it was opened at, when that may be asked for, else the first that may.
    private func first(_ picker: SeasonPicker) -> Int? {
        if let start, picker.rows.contains(where: { $0.season.number == start && $0.askable }) { return start }
        return picker.rows.first(where: \.askable)?.season.number
    }

    private func seasonButton(_ row: PickerSeason, picked: Bool) -> some View {
        Button {
            model.toggle(row.season.number)
        } label: {
            HStack(spacing: 24) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(RequestWords.seasonName(row.season))
                    Text(RequestWords.seasonState(row.season))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                if row.askable {
                    Image(systemName: picked ? "checkmark.circle.fill" : "circle")
                        .font(.title3)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .disabled(!row.askable)
        .focused($focused, equals: row.season.number)
        .accessibilityAddTraits(picked ? .isSelected : [])
        .accessibilityIdentifier("picker.season.\(row.season.number)")
    }
}

/// A person of the cast: their round photo (or initials), their name and
/// their part. It takes the focus so the row can be read along, and goes
/// nowhere: TMDB's people have no page here.
private struct CastPhoto: View {
    let app: AppModel
    let person: CastCard
    private let size: CGFloat = 180

    var body: some View {
        VStack(spacing: 14) {
            Button {} label: {
                AvatarView(app: app, name: person.name, path: person.photoPath, color: nil, size: size)
            }
            .buttonStyle(.borderless)
            .clipShape(Circle())
            .accessibilityLabel([person.name, person.character.map { localized("as \($0)") }].compactMap { $0 }
                .joined(separator: ", "))
            .accessibilityIdentifier("discoverTitle.cast.\(person.index)")
            VStack(spacing: 2) {
                Text(person.name).font(.caption).lineLimit(2)
                if let character = person.character, !character.isEmpty {
                    Text(character).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            .multilineTextAlignment(.center)
            .accessibilityHidden(true)
        }
        .frame(width: size + 40)
    }
}
