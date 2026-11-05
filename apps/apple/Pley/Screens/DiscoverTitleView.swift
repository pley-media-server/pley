import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// A title found on TMDB, laid out as a library's title page is: the
/// backdrop fading into the page with the poster, the title, the line of
/// facts and the genres; its one main action; the tagline and overview.
/// Then the requests for it with their actions, the seasons with each
/// one's state (or its request's stage), the cast, and TMDB's credit.
/// Request on a show opens Which seasons?. On iPad and the Mac, from 700
/// points wide, the poster and the words sit in two columns.
struct DiscoverTitleView: View {
    let app: AppModel
    @StateObject private var model: DiscoverTitleModel
    @State private var width: CGFloat = 0
    @Environment(\.dynamicTypeSize) private var typeSize
    private let layout = WideLayout()

    init(app: AppModel, kind: TitleKind, tmdbId: Int, pick: Bool) {
        self.app = app
        _model = StateObject(wrappedValue: DiscoverTitleModel(app: app, kind: kind, tmdbId: tmdbId, pick: pick))
    }

    var body: some View {
        content
            .navigationTitle(model.page?.title ?? "")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .task { await model.run() }
            .reloads(app) { await model.load() }
            .requestDialogs(model.runner)
            .sheet(isPresented: Binding(get: { model.picker != nil }, set: { if !$0 { model.closePicker() } })) {
                SeasonPickerSheet(model: model)
            }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let words):
            ProblemView(words: words, prefix: "discoverTitle", retry: model.retry, signInAgain: model.signInAgain)
        case .ready:
            if let page = model.page {
                loaded(page)
            } else {
                StateView(kind: .loading)
            }
        }
    }

    private var twoColumns: Bool { width >= 700 && !typeSize.isAccessibilitySize }
    private var gutter: CGFloat { Metrics.gutter(regular: layout.wide) }

    private func loaded(_ page: DiscoverTitlePage) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                VStack(alignment: .leading, spacing: 0) {
                    backdrop(page)
                    if twoColumns {
                        HStack(alignment: .top, spacing: 24) {
                            poster(page, width: 200)
                            VStack(alignment: .leading, spacing: 16) {
                                heading(page).padding(.top, 70)
                                column(page)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .padding(.horizontal, gutter)
                        .padding(.top, -60)
                    } else {
                        VStack(alignment: .leading, spacing: 16) {
                            let stack = typeSize.isAccessibilitySize
                                ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12))
                                : AnyLayout(HStackLayout(alignment: .bottom, spacing: 16))
                            stack {
                                poster(page, width: 110)
                                heading(page)
                            }
                            .padding(.top, -50)
                            column(page)
                        }
                        .padding(.horizontal, gutter)
                    }
                }
                if !model.requests.isEmpty { requests }
                if !model.seasons.isEmpty { seasons }
                if !model.cast.isEmpty { cast }
                TmdbCredit().padding(.horizontal, gutter)
            }
            .padding(.bottom, 32)
            .frame(maxWidth: 1400, alignment: .leading)
            .frame(maxWidth: .infinity)
            .background(GeometryReader { geometry in
                Color.clear.preference(key: TitleWidthKey.self, value: geometry.size.width)
            })
        }
        .onPreferenceChange(TitleWidthKey.self) { width = $0 }
        .accessibilityIdentifier("discoverTitle.page")
    }

    // MARK: Header

    private func backdrop(_ page: DiscoverTitlePage) -> some View {
        ServerImage(app: app, path: page.backdropPath, width: max(width, 320), symbol: nil)
            .frame(height: twoColumns ? 320 : 220)
            .frame(maxWidth: .infinity)
            .clipped()
            .overlay {
                LinearGradient(colors: [.clear, Color.pageBackground], startPoint: .center, endPoint: .bottom)
            }
            .accessibilityHidden(true)
    }

    private func poster(_ page: DiscoverTitlePage, width: CGFloat) -> some View {
        ServerImage(app: app, path: page.posterPath, width: width, symbol: model.kind == .show ? "tv" : "film")
            .frame(width: width, height: width * 3 / 2)
            .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            .shadow(radius: 6, y: 2)
            .accessibilityHidden(true)
    }

    /// The title, its original one where it differs, the facts and the genres.
    private func heading(_ page: DiscoverTitlePage) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(page.title)
                .font(.title.bold())
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("discoverTitle.title")
            if let original = page.originalTitle {
                Text(original).font(.subheadline).foregroundStyle(.secondary)
            }
            // Each dot goes with the part after it, so a line never ends on one.
            FlowLayout(spacing: 8, lineSpacing: 4) {
                ForEach(Array(page.meta.enumerated()), id: \.offset) { index, part in
                    HStack(spacing: 8) {
                        if index > 0 { Text("·") }
                        Text(part)
                    }
                }
                if let certification = page.certification {
                    HStack(spacing: 8) {
                        Text("·")
                        Text(certification)
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 4)
                            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.secondary))
                    }
                }
            }
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel((page.meta + [page.certification].compactMap { $0 }).joined(separator: ", "))
            if let genres = page.genres {
                Text(genres).font(.subheadline).foregroundStyle(.secondary)
            }
        }
    }

    /// The main action, a line when the page couldn't be read again, the tagline and the overview.
    private func column(_ page: DiscoverTitlePage) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            actionBlock
            if let stale = model.stale {
                StaleLine(words: stale, retry: model.retry)
            }
            if let tagline = page.tagline {
                Text(tagline).font(.headline).italic()
            }
            if !page.overview.isEmpty {
                Text(page.overview).fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: 760, alignment: .leading)
    }

    // MARK: The main action

    /// The one main action, the first that applies: Sign in; Open;
    /// Requested or Following with the request's stage (and Stop
    /// following); Tell me too; Open and Request more seasons; Request.
    /// The lines under it say why there's no button and what's left.
    private var actionBlock: some View {
        VStack(alignment: .leading, spacing: 10) {
            FlowLayout(spacing: 8, lineSpacing: 8) {
                buttons
            }
            .controlSize(.large)
            .disabled(model.busy)
            if let pill = model.actionPill {
                StagePillView(pill: pill)
            }
            if !model.lines.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(model.lines, id: \.self) { line in
                        Text(line)
                            .font(.footnote)
                            .foregroundStyle(model.limitReached ? Color.red : Color.secondary)
                    }
                }
                .accessibilityElement(children: .combine)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discoverTitle.action")
    }

    @ViewBuilder private var buttons: some View {
        switch model.action {
        case .signInAgain:
            Button(RequestWords.signIn, action: model.signInAgain)
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("discoverTitle.signInAgain")
        case .open(let itemId?):
            openButton(itemId)
        case .mine:
            Button {} label: { Label(RequestWords.requested, systemImage: "checkmark") }
                .buttonStyle(.bordered)
                .disabled(true)
                .accessibilityIdentifier("discoverTitle.requested")
        case .following:
            Button {} label: { Label(RequestWords.following, systemImage: "checkmark") }
                .buttonStyle(.bordered)
                .disabled(true)
                .accessibilityIdentifier("discoverTitle.following")
            Button(RequestWords.stopFollowing, action: model.stopFollowing)
                .buttonStyle(.borderless)
                .accessibilityIdentifier("discoverTitle.stopFollowing")
        case .tellMeToo(.yes):
            Button(action: model.tellMeToo) { Label(RequestWords.tellMeToo, systemImage: "bookmark") }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("discoverTitle.tellMeToo")
        case .partial(let itemId, let more):
            if let itemId { openButton(itemId) }
            if more == .yes || more == .limit {
                Button(action: model.openPicker) { Label(RequestWords.moreSeasons, systemImage: "plus") }
                    .buttonStyle(.bordered)
                    .disabled(more != .yes)
                    .accessibilityIdentifier("discoverTitle.requestMore")
            }
        case .request(let ask) where ask != .noAccess:
            Button(action: model.request) { Label(RequestWords.request, systemImage: "plus") }
                .buttonStyle(.borderedProminent)
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
        .buttonStyle(.borderedProminent)
        .accessibilityIdentifier("discoverTitle.open")
    }

    // MARK: Rows

    /// The requests for it ("Your requests"; everyone's for managers), with their actions.
    private var requests: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(model.requestsHeading)
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
            ForEach(model.requests) { card in
                RequestCardView(app: app, card: card, runner: model.runner)
            }
        }
        .frame(maxWidth: 760, alignment: .leading)
        .padding(.horizontal, gutter)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discoverTitle.requests")
    }

    /// Each season: its poster, its name, and how it stands for this
    /// account, or the stage of the request asking for it.
    private var seasons: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(RequestWords.seasonsHeading)
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
            ForEach(model.seasons) { season in
                HStack(alignment: .top, spacing: 12) {
                    ServerImage(app: app, path: season.posterPath, width: 48, symbol: "tv")
                        .frame(width: 48, height: 72)
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                    VStack(alignment: .leading, spacing: 6) {
                        Text(season.name).font(.headline)
                        if let pill = season.pill {
                            StagePillView(pill: pill)
                        } else {
                            Label(season.state, systemImage: season.symbol)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Spacer(minLength: 0)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("discoverTitle.season.\(season.number)")
            }
        }
        .frame(maxWidth: 760, alignment: .leading)
        .padding(.horizontal, gutter)
    }

    /// The cast with their photos, round; TMDB's people have no page here.
    private var cast: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(RequestWords.cast)
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, gutter)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: 14) {
                    ForEach(model.cast) { person in
                        CastPhoto(app: app, person: person)
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("discoverTitle.cast")
    }
}

/// A person of the cast: their round photo (or initials), name and part.
private struct CastPhoto: View {
    let app: AppModel
    let person: CastCard
    @ScaledMetric(relativeTo: .body) private var size: CGFloat = 84

    var body: some View {
        VStack(spacing: 6) {
            AvatarView(app: app, name: person.name, path: person.photoPath, color: nil, size: min(size, 140))
            Text(person.name)
                .font(.footnote.weight(.medium))
                .lineLimit(2)
                .multilineTextAlignment(.center)
            if let character = person.character, !character.isEmpty {
                Text(character)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
            }
        }
        .frame(width: min(size, 140) + 16)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([person.name, person.character.map { "as \($0)" }].compactMap { $0 }
            .joined(separator: ", "))
        .accessibilityIdentifier("discoverTitle.cast.\(person.index)")
    }
}

/// Which seasons?: every season, specials last, those that may be asked
/// for ticked lowest first up to what's left; the rest show their state
/// and can't be ticked. Ticking more than may be asked for disables
/// Request and shows the allowance in red.
private struct SeasonPickerSheet: View {
    @ObservedObject var model: DiscoverTitleModel

    var body: some View {
        NavigationStack {
            if let picker = model.picker {
                Form {
                    Section {
                        ForEach(picker.rows) { row in
                            let picked = picker.picked.contains(row.season.number)
                            Button {
                                model.toggle(row.season.number)
                            } label: {
                                HStack(spacing: 12) {
                                    Image(systemName: picked ? "checkmark.circle.fill" : "circle")
                                        .foregroundStyle(row.askable ? Color.accentColor : Color.secondary)
                                        .font(.title3)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(RequestWords.seasonName(row.season)).foregroundStyle(.primary)
                                        Text(RequestWords.seasonState(row.season))
                                            .font(.footnote)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer(minLength: 0)
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .disabled(!row.askable)
                            .accessibilityAddTraits(picked ? .isSelected : [])
                            .accessibilityIdentifier("picker.season.\(row.season.number)")
                        }
                    }
                }
                .navigationTitle(RequestWords.pickerTitle)
                #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button(RequestWords.cancel, action: model.closePicker)
                            .accessibilityIdentifier("picker.cancel")
                    }
                }
                // The allowance sits over the button, where it shows however
                // many seasons there are.
                .footBar {
                    VStack(spacing: 10) {
                        ForEach(model.pickerLines, id: \.self) { line in
                            Text(line)
                                .font(.footnote)
                                .foregroundStyle(picker.overLimit ? Color.red : Color.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        Button(action: model.askPicked) {
                            Text(picker.button).frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .disabled(!picker.ready || model.busy)
                        .accessibilityIdentifier("picker.ask")
                    }
                    .padding()
                }
            }
        }
        #if os(iOS)
        .presentationDetents([.medium, .large])
        #else
        .frame(minWidth: 420, idealWidth: 460, minHeight: 420, idealHeight: 520)
        #endif
    }
}

private extension View {
    /// `content` at the foot, over what scrolls: on the system's glass
    /// sheets the list fades out under it, with no band of its own, and
    /// before them it sits on the bar's material.
    @ViewBuilder func footBar(@ViewBuilder _ content: () -> some View) -> some View {
        if #available(iOS 26, macOS 26, *) {
            safeAreaBar(edge: .bottom, content: content)
        } else {
            safeAreaInset(edge: .bottom) { content().background(.bar) }
        }
    }
}

/// The page's width, for its two columns.
private struct TitleWidthKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}
