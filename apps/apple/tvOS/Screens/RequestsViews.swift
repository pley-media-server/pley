import PleyModel
import PleyScreens
import SwiftUI

/// Your requests on Apple TV, and for managers everyone's (Requests), as
/// Android TV has them: the views as buttons with how many each holds
/// (managers open on Needs attention), movies or shows, and for managers
/// Asked by, a title's words and their own; then a card per request, read
/// on as the focus nears the end. Select on a card opens its panel down
/// the right: Open where a library has it, the server's actions in its
/// order, and Details. An administrator's app signed in before approvals
/// says so, with Sign in.
struct RequestsView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: RequestsListModel
    /// The request whose panel is open.
    @State private var panelFor: Int?
    @State private var people = false
    /// The card the focus is on, or whose panel is open, and where it is in the list.
    @State private var held: (id: Int, index: Int)?
    @FocusState private var focusedCard: Int?

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: RequestsListModel(app: app))
    }

    var body: some View {
        Group {
            if let problem = model.gateProblem {
                ProblemView(app: app, words: problem, prefix: "requests") { Task { await model.refresh() } }
            } else {
                page
            }
        }
        .task { await model.run() }
        .sidePanel(item: panelCard, identifier: "requests.panel") { card in
            RequestPanel(app: app, card: card, runner: model.runner, manage: model.manage) { panelFor = nil }
        }
        .sidePanel(isPresented: $people, identifier: "requests.people") {
            PeoplePanel(model: model) { people = false }
        }
        // The panel closed: the focus goes back to its card. Should the
        // card leave the list, as it does when approved off Needs
        // attention or withdrawn, the focus goes to the card now where it
        // was, rather than being left on nothing.
        .onChange(of: panelFor) { before, now in
            if let now {
                held = (now, model.cards.firstIndex { $0.id == now } ?? 0)
            } else if before != nil {
                refocus()
            }
        }
        .onChange(of: focusedCard) { _, now in
            if let now {
                held = (now, model.cards.firstIndex { $0.id == now } ?? 0)
            } else if panelFor == nil, let held, model.cards.contains(where: { $0.id == held.id }) {
                // The focus moved on from the cards, to the filters or the tabs.
                self.held = nil
            } else {
                refocus()
            }
        }
        .onChange(of: model.cards.map(\.id)) { _, _ in
            if focusedCard == nil || focusedCard == held?.id { refocus() }
        }
        .accessibilityIdentifier("requests.page")
    }

    /// The focus back on the held card, or, when it has left the list,
    /// on the card now where it was; nothing while a panel is open.
    private func refocus() {
        guard panelFor == nil, let held else { return }
        let ids = model.cards.map(\.id)
        if ids.contains(held.id) {
            if focusedCard != held.id { focusedCard = held.id }
        } else if !ids.isEmpty {
            focusedCard = ids[min(held.index, ids.count - 1)]
        }
    }

    /// The card whose panel is open, as it is now: should it leave the
    /// list (withdrawn, or approved off Needs attention), the panel goes too.
    private var panelCard: Binding<RequestCard?> {
        Binding(get: { panelFor.flatMap { id in model.cards.first { $0.id == id } } },
                set: { if $0 == nil { panelFor = nil } })
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 32) {
                Text(model.title)
                    .font(.title2.bold())
                    .accessibilityAddTraits(.isHeader)
                    .padding(.horizontal, Metrics.gutter)
                if model.approvalsBanner { approvalsBanner }
                if model.filter != nil {
                    views
                    filters
                }
                content
            }
            .padding(.vertical, 24)
        }
    }

    /// "Sign in again to approve and decline here.", with Sign in again.
    private var approvalsBanner: some View {
        HStack(spacing: 32) {
            Image(systemName: "rectangle.portrait.and.arrow.right")
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(RequestWords.signInApprovals)
                .font(.callout)
            SignInAgainButton(app: app, identifier: "requests.signInAgain")
        }
        .padding(.horizontal, Metrics.gutter)
        .focusSection()
        .accessibilityElement(children: .contain)
    }

    // MARK: Filters

    /// The views, each with its number: the one showing has a check.
    private var views: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 24) {
                ForEach(model.views) { chip in
                    chipButton(chip.title, selected: chip.selected, identifier: "requests.view.\(chip.id)") {
                        if let view = RequestView.allCases.first(where: { $0.key == chip.id }) {
                            model.select(view: view)
                        }
                    }
                }
            }
            .padding(.horizontal, Metrics.gutter)
            .padding(.vertical, 20)
        }
        .scrollClipDisabled()
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("requests.views")
    }

    /// Movies and shows (each press the next: movies, shows, both again);
    /// for managers Asked by, a title's words, and their own.
    private var filters: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 24) {
                let kind = model.filter?.kind
                chipButton(kindTitle(kind), selected: kind != nil, identifier: "requests.kind") {
                    model.select(kind: nextKind(kind))
                }
                if model.manage {
                    if !model.people.isEmpty || model.filter?.userId != nil {
                        let someone = model.filter?.userId != nil
                        chipButton("\(RequestWords.askedBy): \(model.personTitle)", selected: someone,
                                   identifier: "requests.person") {
                            people = true
                        }
                    }
                    TextField(RequestWords.filterTitle,
                              text: Binding(get: { model.query }, set: { model.setQuery($0) }))
                        .autocorrectionDisabled()
                        .frame(width: 520)
                        .accessibilityIdentifier("requests.search")
                    chipButton(RequestWords.filterMine, selected: model.filter?.mine == true,
                               identifier: "requests.yours") {
                        model.setMine(!(model.filter?.mine ?? false))
                    }
                }
            }
            .padding(.horizontal, Metrics.gutter)
            .padding(.vertical, 20)
        }
        .scrollClipDisabled()
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("requests.filters")
    }

    private func kindTitle(_ kind: TitleKind?) -> String {
        if kind == .movie { return RequestWords.filterMovies }
        if kind == .show { return RequestWords.filterShows }
        return RequestWords.kindAll
    }

    /// Movies and shows, then movies, then shows.
    private func nextKind(_ kind: TitleKind?) -> TitleKind? {
        if kind == nil { return .movie }
        if kind == .movie { return .show }
        return nil
    }

    /// A filter's button: the one chosen has a check, as a tint alone is too faint from the sofa.
    private func chipButton(_ title: String, selected: Bool, identifier: String,
                            action: @escaping () -> Void) -> some View {
        Button(action: action) {
            if selected {
                Label(title, systemImage: "checkmark")
            } else {
                Text(title)
            }
        }
        .buttonStyle(.bordered)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(identifier)
    }

    // MARK: The list

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let words):
            ProblemView(app: app, words: words, prefix: "requests") { Task { await model.refresh() } }
        case .empty(let words):
            VStack(spacing: 24) {
                Image(systemName: "list.bullet.rectangle")
                    .font(.system(size: 60))
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(words.title).font(.title3.bold())
                Text(words.body).foregroundStyle(.secondary)
                if words.discover {
                    Button {
                        app.shell.open(.discover)
                    } label: {
                        Label(RequestWords.find, systemImage: "safari")
                    }
                    .accessibilityIdentifier("requests.discover")
                }
            }
            .multilineTextAlignment(.center)
            .frame(maxWidth: 1000)
            .frame(maxWidth: .infinity, minHeight: 400)
            .padding(Metrics.gutter)
            .focusSection()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("requests.empty")
            .task(id: words) { announce("\(words.title) \(words.body)") }
        case .ready:
            RequestCards(app: app, model: model, runner: model.runner, focusedCard: $focusedCard) { card in
                panelFor = card.id
            }
        }
    }
}

/// The cards, one under the other, the next page read as the focus comes
/// near the end; above them, when reading again failed, why, with Retry.
private struct RequestCards: View {
    let app: AppModel
    @ObservedObject var model: RequestsListModel
    @ObservedObject var runner: RequestActionRunner
    let focusedCard: FocusState<Int?>.Binding
    let select: (RequestCard) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 32) {
            if let stale = model.stale {
                HStack(spacing: 32) {
                    Image(systemName: stale.symbol)
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    Text(stale.title).font(.callout).foregroundStyle(.secondary)
                    if stale.signInAgain {
                        SignInAgainButton(app: app, identifier: "requests.stale.signInAgain")
                    } else {
                        Button(RequestWords.retry) { Task { await model.refresh() } }
                            .accessibilityIdentifier("requests.stale.retry")
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .focusSection()
            }
            LazyVStack(alignment: .leading, spacing: 32) {
                ForEach(Array(model.cards.enumerated()), id: \.element.id) { index, card in
                    RequestCardRow(app: app, card: card, busy: runner.busy.contains(card.id), focus: focusedCard) {
                        select(card)
                    }
                    .onAppear { model.cardAppeared(index) }
                }
                if model.loadingMore {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                        .accessibilityLabel("Loading")
                }
            }
            .padding(.horizontal, Metrics.gutter)
            .focusSection()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("requests.list")
        }
    }
}

/// One request as a card that lifts when focused: its poster, its title
/// and what was asked for, its stage with the download's bar, who asked
/// and when, the decline's reason, and for managers the note and where it
/// went. Select runs `select` (on the list, its panel). One element for
/// VoiceOver.
struct RequestCardRow: View {
    let app: AppModel
    let card: RequestCard
    /// An action on it is on its way.
    let busy: Bool
    /// The list's focus, by request, for giving it back to the card.
    let focus: FocusState<Int?>.Binding?
    let select: () -> Void

    private static let poster: CGFloat = 120

    init(app: AppModel, card: RequestCard, busy: Bool, focus: FocusState<Int?>.Binding? = nil,
         select: @escaping () -> Void) {
        self.app = app
        self.card = card
        self.busy = busy
        self.focus = focus
        self.select = select
    }

    var body: some View {
        Button(action: select) {
            HStack(alignment: .top, spacing: 32) {
                ServerImage(app: app, path: card.posterPath, width: Self.poster,
                            symbol: card.request.kind == .show ? "tv" : "film")
                    .frame(width: Self.poster, height: Self.poster * 3 / 2)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                VStack(alignment: .leading, spacing: 10) {
                    HStack(alignment: .firstTextBaseline, spacing: 20) {
                        Text(card.title)
                            .font(.headline)
                            .lineLimit(1)
                        if busy {
                            ProgressView()
                        }
                    }
                    Text(card.line).font(.caption).foregroundStyle(.secondary)
                    StagePillView(pill: card.pill, barWidth: 480)
                    if let elsewhere = card.elsewhere {
                        Text(elsewhere).font(.caption).foregroundStyle(.secondary)
                    }
                    if !card.asked.isEmpty {
                        Text(card.asked).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                    if let reason = card.reason {
                        Text(reason).font(.caption).italic().foregroundStyle(.secondary).lineLimit(2)
                    }
                    if let note = card.note {
                        Text(note).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                    }
                    if let sentTo = card.sentTo {
                        Text(sentTo).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.card)
        .modifier(RequestFocus(focus: focus, id: card.id))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken)
        .accessibilityHint("Shows what can be done with it")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("request.\(card.id)")
    }

    private var spoken: String {
        [card.spoken, card.elsewhere, card.asked.isEmpty ? nil : card.asked, card.reason, card.note, card.sentTo]
            .compactMap { $0 }.joined(separator: ", ")
    }
}

/// Ties a card's focus to its list's, when it has one.
private struct RequestFocus: ViewModifier {
    let focus: FocusState<Int?>.Binding?
    let id: Int

    func body(content: Content) -> some View {
        if let focus {
            content.focused(focus, equals: id)
        } else {
            content
        }
    }
}

/// What may be done with a request, inside a `SidePanel` (Android TV's
/// request panel): at the top its title and year, its stage with the
/// download's bar, what was asked for, who asked and when, and its notes;
/// then Open where a library has it, the server's actions in its order
/// (Decline comes with Decline with a reason…; Approve is plain, the
/// settings' choices applying), and Details, its title's page.
///
/// Withdraw, Remove and Delete ask first, in the panel, Cancel first;
/// Decline with a reason… takes the words on the TV's keyboard, then
/// Decline. Menu from a question goes back to the actions; from the
/// actions it closes the panel (`onClose`). An action closes the panel
/// and runs on `runner`, which tells what went wrong.
struct RequestPanel: View {
    let app: AppModel
    let card: RequestCard
    @ObservedObject var runner: RequestActionRunner
    /// The account manages requests: the note and where it went show.
    let manage: Bool
    /// Details shows: not on the title's own page.
    let details: Bool
    let onClose: () -> Void

    private enum Step: Hashable {
        case actions
        case confirm(CardAction)
        case reason
    }

    private enum Spot: Hashable {
        case open, action(CardAction), details, cancel, confirm, reason, decline
    }

    @State private var step = Step.actions
    /// The action pressed last, for the focus to come back to from its question.
    @State private var used: CardAction?
    @State private var reason = ""
    @FocusState private var spot: Spot?

    init(app: AppModel, card: RequestCard, runner: RequestActionRunner, manage: Bool, details: Bool = true,
         onClose: @escaping () -> Void) {
        self.app = app
        self.card = card
        self.runner = runner
        self.manage = manage
        self.details = details
        self.onClose = onClose
    }

    private var request: MediaRequest { card.request }
    private var busy: Bool { runner.busy.contains(card.id) }
    private var actions: [CardAction] { CardAction.panel(request) }

    var body: some View {
        VStack(alignment: .leading, spacing: 28) {
            switch step {
            case .actions:
                head
                actionRows
            case .confirm(let action):
                Text(action.confirmation(card.title) ?? action.title)
                    .font(.title3.bold())
                    .accessibilityAddTraits(.isHeader)
                confirmRows(action)
            case .reason:
                Text(RequestWords.declineTitle(card.title))
                    .font(.title3.bold())
                    .accessibilityAddTraits(.isHeader)
                if !request.mine {
                    Text(RequestWords.declineReasonHint).font(.callout).foregroundStyle(.secondary)
                }
                reasonRows
            }
        }
        .onExitCommand(perform: back)
        .onChange(of: step) { _, step in
            let next = first(step)
            Task { spot = next }
        }
        .defaultFocus($spot, first(step))
    }

    /// Menu: from a question back to the actions, else the panel closes.
    private func back() {
        if step == .actions { onClose() } else { step = .actions }
    }

    /// Where the focus starts on `step`: the action it came back from, else
    /// the first row; Cancel on a question; the field for a reason.
    private func first(_ step: Step) -> Spot {
        switch step {
        case .actions:
            if let used, actions.contains(used) { return .action(used) }
            if request.itemId != nil { return .open }
            return actions.first.map { .action($0) } ?? .details
        case .confirm: return .cancel
        case .reason: return .reason
        }
    }

    // MARK: The request

    private var head: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(request.year.map { "\(card.title) (\($0))" } ?? card.title)
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
            StagePillView(pill: card.pill, barWidth: 520)
            Text([what, card.asked.isEmpty ? nil : card.asked].compactMap { $0 }.joined(separator: " · "))
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            if let elsewhere = card.elsewhere {
                Text(elsewhere).font(.caption).foregroundStyle(.secondary)
            }
            if let reason = card.reason {
                Text(reason).font(.caption).italic().foregroundStyle(.secondary).lineLimit(3)
            }
            if manage, let note = card.note {
                Text(note).font(.caption).foregroundStyle(.secondary).lineLimit(2)
            }
            if manage, let sentTo = card.sentTo {
                Text(sentTo).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            if busy {
                ProgressView()
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("request.panel.head")
    }

    /// What was asked for: a show's seasons, else movie or show.
    private var what: String {
        if request.kind == .show, let seasons = request.seasons, !seasons.isEmpty {
            return RequestWords.seasons(seasons)
        }
        return RequestWords.kind(request.kind)
    }

    // MARK: Rows

    @ViewBuilder private var actionRows: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let itemId = request.itemId {
                row(RequestWords.open, symbol: "play.fill", spot: .open, identifier: "request.panel.open") {
                    onClose()
                    app.shell.open(.item(itemId))
                }
            }
            ForEach(actions, id: \.self) { action in
                row(action.title, symbol: action.symbol, role: action == .delete ? .destructive : nil,
                    spot: .action(action), identifier: "request.panel.\(action.rawValue)") {
                    pressed(action)
                }
            }
            if details {
                row(RequestWords.details, symbol: "info.circle", spot: .details, identifier: "request.panel.details") {
                    onClose()
                    app.shell.open(.discoverTitle(request.kind, tmdbId: request.tmdbId))
                }
            }
        }
    }

    private func pressed(_ action: CardAction) {
        used = action
        if action == .declineWithReason {
            reason = ""
            step = .reason
        } else if action.confirms {
            step = .confirm(action)
        } else {
            run(action)
        }
    }

    /// Cancel first, as the app's other questions: Select pressed twice undoes nothing.
    @ViewBuilder private func confirmRows(_ action: CardAction) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            row(RequestWords.cancel, symbol: "arrow.uturn.backward", spot: .cancel,
                identifier: "request.panel.cancel") {
                step = .actions
            }
            row(action.title, symbol: action.symbol, role: .destructive, spot: .confirm,
                identifier: "request.panel.confirm") {
                run(action)
            }
        }
    }

    /// The reason's field (cut to what the server takes as it's typed),
    /// whose Done goes on to Decline; then Decline and Cancel.
    @ViewBuilder private var reasonRows: some View {
        VStack(alignment: .leading, spacing: 16) {
            TextField(RequestWords.declineReason,
                      text: Binding(get: { reason }, set: { reason = RequestRules.cutReason($0) }))
                .focused($spot, equals: .reason)
                .onSubmit { spot = .decline }
                .accessibilityIdentifier("request.panel.reason")
            row(RequestWords.decline, symbol: CardAction.decline.symbol, role: .destructive, spot: .decline,
                identifier: "request.panel.decline") {
                run(.declineWithReason, reason: RequestRules.declineReason(reason))
            }
            row(RequestWords.cancel, symbol: "arrow.uturn.backward", spot: .cancel,
                identifier: "request.panel.cancel") {
                step = .actions
            }
        }
    }

    private func row(_ title: String, symbol: String, role: ButtonRole? = nil, spot at: Spot, identifier: String,
                     action: @escaping () -> Void) -> some View {
        Button(role: role, action: action) {
            Label(title, systemImage: symbol)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .focused($spot, equals: at)
        .accessibilityIdentifier(identifier)
    }

    /// The panel closes and the action runs; the runner tells a failure.
    private func run(_ action: CardAction, reason: String? = nil) {
        guard !busy else { return }
        let card = card
        let runner = runner
        onClose()
        Task { await runner.run(action, on: card, reason: reason) }
    }
}

/// Asked by: everyone, or one person's requests, the one chosen with a check.
private struct PeoplePanel: View {
    @ObservedObject var model: RequestsListModel
    let close: () -> Void
    @Namespace private var namespace

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(RequestWords.askedBy)
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.bottom, 12)
            choice(RequestWords.filterEveryone, id: nil, identifier: "requests.person.everyone")
            ForEach(model.people, id: \.id) { person in
                choice(person.name, id: person.id, identifier: "requests.person.\(person.id)")
            }
        }
        .focusScope(namespace)
    }

    private func choice(_ name: String, id: Int?, identifier: String) -> some View {
        let selected = model.filter?.userId == id
        return Button {
            model.select(person: id)
            close()
        } label: {
            HStack {
                Text(name)
                Spacer(minLength: 0)
                if selected {
                    Image(systemName: "checkmark").accessibilityHidden(true)
                }
            }
        }
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(identifier)
        .prefersDefaultFocus(selected, in: namespace)
    }
}
