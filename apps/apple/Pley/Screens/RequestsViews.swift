import PleyModel
import PleyScreens
import SwiftUI

/// Your requests, and for managers everyone's (Requests), opening on Needs
/// attention: the views with how many each holds, movies or shows, and for
/// managers a title, a person and their own; then the cards, read on as
/// the list scrolls, and read again on a pull or ⌘R. An administrator's app
/// signed in before approvals says so, with Sign in again.
struct RequestsView: View {
    let app: AppModel
    @StateObject private var model: RequestsListModel
    private let layout = WideLayout()

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: RequestsListModel(app: app))
    }

    var body: some View {
        Group {
            if let problem = model.gateProblem {
                ProblemView(words: problem, prefix: "requests", retry: { Task { await model.refresh() } },
                            signInAgain: model.signInAgain)
            } else {
                VStack(spacing: 0) {
                    if model.approvalsBanner { approvalsBanner }
                    if model.filter != nil { filters }
                    content
                }
            }
        }
        .navigationTitle(model.title)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .task { await model.run() }
        .reloads(app) { await model.refresh() }
        .requestDialogs(model.runner)
        .accessibilityIdentifier("requests.page")
    }

    private var gutter: CGFloat { Metrics.gutter(regular: layout.wide) }

    /// "Sign in again to approve and decline here.", with Sign in again.
    private var approvalsBanner: some View {
        HStack(spacing: 12) {
            Image(systemName: "rectangle.portrait.and.arrow.right")
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(RequestWords.signInApprovals)
                .font(.subheadline)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button(RequestWords.signIn, action: model.signInAgain)
                .accessibilityIdentifier("requests.signInAgain")
        }
        .padding(12)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
        .padding(.horizontal, gutter)
        .padding(.top, 8)
        .accessibilityElement(children: .contain)
    }

    // MARK: Filters

    /// The title's field (managers); the views with their counts; movies or
    /// shows, a person and their own (managers).
    private var filters: some View {
        VStack(alignment: .leading, spacing: 8) {
            if model.manage {
                HStack(spacing: 6) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
                    TextField(RequestWords.filterTitle,
                              text: Binding(get: { model.query }, set: { model.setQuery($0) }))
                        .textFieldStyle(.plain)
                        .autocorrectionDisabled()
                        .submitLabel(.search)
                        .accessibilityIdentifier("requests.search")
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .background(.quaternary.opacity(0.6), in: RoundedRectangle(cornerRadius: 10))
                .frame(maxWidth: 520)
                .padding(.horizontal, gutter)
            }
            chips(id: "requests.views") {
                ForEach(model.views) { chip in
                    ChipButton(title: chip.title, selected: chip.selected, identifier: "requests.view.\(chip.id)") {
                        if let view = RequestView.allCases.first(where: { $0.key == chip.id }) {
                            model.select(view: view)
                        }
                    }
                }
            }
            chips(id: "requests.filters") {
                ForEach(model.kindChips) { chip in
                    ChipButton(title: chip.title, selected: chip.selected, identifier: "requests.kind.\(chip.id)") {
                        model.select(kind: TitleKind(rawValue: chip.id))
                    }
                }
                if model.manage {
                    if !model.people.isEmpty || model.filter?.userId != nil { personMenu }
                    ChipButton(title: RequestWords.filterMine, selected: model.filter?.mine == true,
                               identifier: "requests.yours") {
                        model.setMine(!(model.filter?.mine ?? false))
                    }
                }
            }
        }
        .padding(.vertical, 8)
    }

    private func chips<Content: View>(id: String, @ViewBuilder _ content: () -> Content) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) { content() }
                .padding(.horizontal, gutter)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(id)
    }

    /// Everyone, or one person's requests.
    private var personMenu: some View {
        Menu {
            Button {
                model.select(person: nil)
            } label: {
                if model.filter?.userId == nil {
                    Label(RequestWords.filterEveryone, systemImage: "checkmark")
                } else {
                    Text(RequestWords.filterEveryone)
                }
            }
            ForEach(model.people, id: \.id) { person in
                Button {
                    model.select(person: person.id)
                } label: {
                    if model.filter?.userId == person.id {
                        Label(person.name, systemImage: "checkmark")
                    } else {
                        Text(person.name)
                    }
                }
            }
        } label: {
            HStack(spacing: 4) {
                Text(model.personTitle)
                Image(systemName: "chevron.down").font(.caption2.weight(.semibold))
            }
            .font(.subheadline.weight(model.filter?.userId != nil ? .semibold : .regular))
            .foregroundStyle(model.filter?.userId != nil ? Color.white : Color.primary)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(model.filter?.userId != nil ? Color.accentColor : Color.secondary.opacity(0.15),
                        in: Capsule())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .fixedSize()
        .accessibilityIdentifier("requests.person")
    }

    // MARK: The list

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let words):
            ProblemView(words: words, prefix: "requests", retry: { Task { await model.refresh() } },
                        signInAgain: model.signInAgain)
        case .empty(let words):
            ScrollView {
                VStack(spacing: 12) {
                    Image(systemName: "list.bullet.rectangle")
                        .font(.largeTitle)
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    Text(words.title).font(.headline)
                    Text(words.body).foregroundStyle(.secondary)
                    if words.discover {
                        Button {
                            app.shell.open(.discover)
                        } label: {
                            Label(RequestWords.discover, systemImage: "safari")
                        }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("requests.discover")
                    }
                }
                .multilineTextAlignment(.center)
                .padding(32)
                .frame(maxWidth: 480)
                .frame(maxWidth: .infinity)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("requests.empty")
            }
            .refreshable { await model.refresh() }
        case .ready:
            ScrollView {
                LazyVStack(spacing: 10) {
                    if let stale = model.stale {
                        StaleLine(words: stale) { Task { await model.refresh() } }
                    }
                    ForEach(Array(model.cards.enumerated()), id: \.element.id) { index, card in
                        RequestCardView(app: app, card: card, runner: model.runner)
                            .onAppear { model.cardAppeared(index) }
                    }
                    if model.loadingMore {
                        ProgressView().padding()
                    }
                }
                .frame(maxWidth: 720)
                .padding(.horizontal, gutter)
                .padding(.vertical, 8)
                .frame(maxWidth: .infinity)
            }
            .refreshable { await model.refresh() }
            .accessibilityIdentifier("requests.list")
        }
    }
}

/// One request: its poster, title, year and seasons, its stage pill (with
/// the download's bar), who asked and when, the reason or the note, where
/// it went (managers), and its actions: the main ones as buttons, the rest
/// under More actions. The poster and title open the title's page.
struct RequestCardView: View {
    let app: AppModel
    let card: RequestCard
    @ObservedObject var runner: RequestActionRunner
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        let stack = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12))
            : AnyLayout(HStackLayout(alignment: .top, spacing: 12))
        stack {
            Button(action: open) {
                ServerImage(app: app, path: card.posterPath, width: 64,
                            symbol: card.request.kind == .show ? "tv" : "film")
                    .frame(width: 64, height: 96)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            .buttonStyle(.plain)
            .disabled(card.opens == nil)
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 6) {
                Button(action: open) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(card.title)
                            .font(.headline)
                            .lineLimit(typeSize.isAccessibilitySize ? nil : 2)
                            .multilineTextAlignment(.leading)
                        Text(card.line).font(.footnote).foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(card.opens == nil)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(card.spoken)
                .accessibilityHint(card.opens == nil ? "" : "Opens its page")
                .accessibilityAddTraits(card.opens == nil ? .isStaticText : .isButton)
                StagePillView(pill: card.pill)
                if let elsewhere = card.elsewhere {
                    Text(elsewhere).font(.footnote).foregroundStyle(.secondary)
                }
                if !card.asked.isEmpty {
                    Text(card.asked).font(.footnote).foregroundStyle(.secondary)
                }
                if let reason = card.reason {
                    Text(reason).font(.subheadline).italic().foregroundStyle(.secondary)
                }
                if let note = card.note {
                    Text(note).font(.footnote).foregroundStyle(.secondary)
                }
                if let sentTo = card.sentTo {
                    Text(sentTo).font(.footnote).foregroundStyle(.secondary)
                }
                actions
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("request.\(card.id)")
    }

    private func open() {
        if let screen = card.opens { app.shell.open(screen) }
    }

    private var busy: Bool { runner.busy.contains(card.id) }

    @ViewBuilder private var actions: some View {
        if !card.buttons.isEmpty || !card.menu.isEmpty {
            FlowLayout(spacing: 8, lineSpacing: 8) {
                ForEach(card.buttons, id: \.self) { action in
                    button(action)
                }
                if !card.menu.isEmpty {
                    Menu {
                        ForEach(card.menu, id: \.self) { action in
                            Button(role: action.destructive ? .destructive : nil) {
                                runner.tap(action, on: card)
                            } label: {
                                Label(action.title, systemImage: action.symbol)
                            }
                            .accessibilityIdentifier("request.\(card.id).\(action.rawValue)")
                        }
                    } label: {
                        Label(RequestWords.moreActions, systemImage: "ellipsis")
                            .labelStyle(.iconOnly)
                    }
                    .menuStyle(.button)
                    .buttonStyle(.bordered)
                    .fixedSize()
                    .accessibilityLabel(RequestWords.moreActions)
                    .accessibilityIdentifier("request.\(card.id).more")
                }
                if busy {
                    ProgressView().controlSize(.small)
                }
            }
            .disabled(busy)
            .padding(.top, 4)
        }
    }

    @ViewBuilder private func button(_ action: CardAction) -> some View {
        let button = Button(role: action.destructive ? .destructive : nil) {
            runner.tap(action, on: card)
        } label: {
            Label(action.title, systemImage: action.symbol)
        }
        .accessibilityIdentifier("request.\(card.id).\(action.rawValue)")
        if action.prominent {
            button.buttonStyle(.borderedProminent)
        } else {
            button.buttonStyle(.bordered)
        }
    }
}

extension View {
    /// The dialogs of `runner`'s actions: Withdraw, Remove and Delete ask
    /// first; Decline asks for an optional reason; Approve… for the
    /// profile, folder and seasons.
    func requestDialogs(_ runner: RequestActionRunner) -> some View {
        modifier(RequestDialogs(runner: runner))
    }
}

private struct RequestDialogs: ViewModifier {
    @ObservedObject var runner: RequestActionRunner

    func body(content: Content) -> some View {
        content
            .confirmationDialog(runner.confirming?.title ?? "",
                                isPresented: Binding(get: { runner.confirming != nil },
                                                     set: { if !$0 { runner.cancelConfirm() } }),
                                titleVisibility: .visible, presenting: runner.confirming) { confirming in
                Button(confirming.action.title, role: .destructive) { runner.confirm() }
                    .accessibilityIdentifier("request.\(confirming.action.rawValue).confirm")
                Button(RequestWords.cancel, role: .cancel) { runner.cancelConfirm() }
            }
            .sheet(item: Binding(get: { runner.declining }, set: { if $0 == nil { runner.cancelDecline() } })) { _ in
                DeclineSheet(runner: runner)
            }
            .sheet(item: Binding(get: { runner.approving },
                                 set: { if $0 == nil { runner.cancelApprove() } })) { model in
                ApproveSheet(model: model, runner: runner)
            }
    }
}

/// Decline: an optional reason, which the person who asked sees, cut to
/// what the server takes as it's typed.
private struct DeclineSheet: View {
    @ObservedObject var runner: RequestActionRunner
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(RequestWords.declineReason,
                              text: Binding(get: { runner.reason }, set: { runner.setReason($0) }), axis: .vertical)
                        .lineLimit(3...6)
                        .focused($focused)
                        .accessibilityIdentifier("decline.reason")
                } footer: {
                    Text(RequestWords.declineReasonHint)
                }
            }
            .navigationTitle(runner.declineTitle)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(RequestWords.cancel) { runner.cancelDecline() }
                        .accessibilityIdentifier("decline.cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(RequestWords.decline, role: .destructive) { runner.decline() }
                        .accessibilityIdentifier("decline.confirm")
                }
            }
            .onAppear { focused = true }
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        #else
        .frame(minWidth: 420, idealWidth: 460, minHeight: 240, idealHeight: 280)
        #endif
    }
}

/// Approve…: the quality profile and the folder (starting on the
/// settings' choices, anime's for anime), and for shows the seasons asked
/// for, all ticked. Without the service's lists it approves with the
/// settings' choices.
private struct ApproveSheet: View {
    @ObservedObject var model: ApproveModel
    let runner: RequestActionRunner

    var body: some View {
        NavigationStack {
            Form {
                if model.loading {
                    Section {
                        ProgressView().frame(maxWidth: .infinity)
                    }
                }
                if !model.profiles.isEmpty {
                    Section(RequestWords.approveProfile) {
                        ForEach(model.profiles) { profile in
                            choice(profile.name, selected: model.profile == profile.id,
                                   identifier: "approve.profile.\(profile.id.map(String.init) ?? "settings")") {
                                model.profile = profile.id
                            }
                        }
                    }
                }
                if !model.folders.isEmpty {
                    Section(RequestWords.approveFolder) {
                        ForEach(Array(model.folders.enumerated()), id: \.element.id) { index, folder in
                            choice(folder.name, detail: folder.free, selected: model.folder == folder.path,
                                   identifier: "approve.folder.\(index)") {
                                model.folder = folder.path
                            }
                        }
                    }
                }
                if model.request.kind == .show && !model.allSeasons.isEmpty {
                    Section(RequestWords.approveSeasons) {
                        ForEach(model.allSeasons, id: \.self) { season in
                            Toggle(RequestWords.seasons([season]),
                                   isOn: Binding(get: { model.seasons.contains(season) },
                                                 set: { _ in model.toggle(season: season) }))
                                .accessibilityIdentifier("approve.season.\(season)")
                        }
                    }
                }
                if let error = model.error {
                    Section {
                        Text(error).foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle(model.title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(RequestWords.cancel) { runner.cancelApprove() }
                        .accessibilityIdentifier("approve.cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(model.button) { model.approve() }
                        .disabled(!model.canApprove)
                        .accessibilityIdentifier("approve.confirm")
                }
            }
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        #else
        .frame(minWidth: 460, idealWidth: 500, minHeight: 380, idealHeight: 480)
        #endif
    }

    /// One of a list's choices, the chosen one with a check.
    private func choice(_ name: String, detail: String? = nil, selected: Bool, identifier: String,
                        choose: @escaping () -> Void) -> some View {
        Button(action: choose) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(name).foregroundStyle(.primary)
                    if let detail { Text(detail).font(.footnote).foregroundStyle(.secondary) }
                }
                Spacer()
                if selected {
                    Image(systemName: "checkmark").foregroundStyle(Color.accentColor)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(identifier)
    }
}
