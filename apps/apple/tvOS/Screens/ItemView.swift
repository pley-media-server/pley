import PleyData
import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// A title's page for the sofa (docs/APPLE.md, Item pages): the backdrop
/// fills the screen behind the title, where it sits, its facts and summary,
/// and a row of buttons: Play or Resume, From start, Request more seasons
/// (a show some of whose seasons may still be asked for), Watched, My
/// List, the trailer, the profile's rating, the version, audio and
/// subtitles, and More. A movie or show someone asked for has its
/// request's stage under its facts, with the download's bar while it
/// comes. Below, rows: a show's seasons and episodes, extras, cast, More
/// Like This, details and Media info. An item that isn't a title (an
/// artist, an album, a track, an extra) shows the page it belongs on.
struct ItemView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ItemModel
    /// A player was open: once it closes, the page reads the item again
    /// for its new resume point.
    @State private var sawPlayer = false
    @State private var showMediaInfo = false
    @FocusState private var playFocused: Bool

    init(app: AppModel, itemId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: ItemModel(app: app, itemId: itemId))
    }

    var body: some View {
        Group {
            if let screen = model.elsewhere {
                ScreenView(app: app, screen: screen)
            } else {
                content
            }
        }
        .task { await model.load() }
        .onReceive(app.$player) { player in
            if player != nil {
                sawPlayer = true
            } else if sawPlayer {
                sawPlayer = false
                Task { await model.load() }
            }
        }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .notFound:
            StateView(kind: .empty(symbol: "questionmark.square.dashed", text: ItemModel.notFoundTitle, action: nil))
        case .failed(let why):
            StateView(kind: .failed(title: localized("Couldn't load this title"), why: why) {
                Task { await model.load() }
            })
        case .loaded:
            if let page = model.page {
                loaded(page)
            } else {
                StateView(kind: .loading)
            }
        }
    }

    private func loaded(_ page: ItemPage) -> some View {
        ZStack(alignment: .top) {
            backdrop(page)
            ScrollView {
                VStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                    hero(page)
                    rows(page)
                }
                .padding(.bottom, 80)
            }
            .scrollClipDisabled()
        }
        .accessibilityIdentifier("item.page")
    }

    // MARK: Hero

    /// The backdrop behind the whole page, darkened towards the text.
    private func backdrop(_ page: ItemPage) -> some View {
        ServerImage(app: app, paths: page.backdrops, width: 1920, symbol: nil, limit: ImageLoader.deviceBackdropLimit)
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

    private func hero(_ page: ItemPage) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            Spacer(minLength: 120)
            if let context = page.context {
                Text([context.showTitle, context.seasonTitle, context.episodeLongLabel].compactMap { $0 }
                    .joined(separator: " · "))
                    .font(.headline)
                    .foregroundStyle(.secondary)
            }
            Text(page.title)
                .font(.system(size: 64, weight: .bold))
                .lineLimit(2)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("item.title")
            meta(page)
            if let pill = model.requestBlock?.pill {
                StagePillView(pill: pill)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("item.request")
            }
            if let tagline = page.tagline {
                Text(tagline).font(.headline).italic()
            }
            if let summary = page.summary {
                Text(summary).font(.callout).lineLimit(4).frame(maxWidth: 1100, alignment: .leading)
            }
            if !page.genres.isEmpty {
                Text(page.genres.joined(separator: ", ")).font(.callout).foregroundStyle(.secondary)
            }
            if let notice = model.notice {
                Text(notice).font(.callout).foregroundStyle(.red)
            }
            buttons(page)
                .focusSection()
            if !page.timeLeftOrEmpty.isEmpty {
                Text(page.timeLeftOrEmpty).font(.callout).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(minHeight: 860, alignment: .bottomLeading)
    }

    @ViewBuilder private func meta(_ page: ItemPage) -> some View {
        if !page.meta.isEmpty {
            HStack(spacing: 16) {
                ForEach(Array(page.meta.enumerated()), id: \.offset) { index, part in
                    if index > 0 { Text(verbatim: "·") }
                    switch part {
                    case .text(let text):
                        Text(text)
                    case .contentRating(let rating):
                        Text(rating)
                            .padding(.horizontal, 8)
                            .overlay(RoundedRectangle(cornerRadius: 4).stroke(.secondary, lineWidth: 2))
                    case .rating(let rating):
                        Label(rating, systemImage: "star.fill")
                    case .badges(let badges):
                        ForEach(badges, id: \.self) { badge in
                            Text(badge)
                                .font(.caption.bold())
                                .padding(.horizontal, 8)
                                .padding(.vertical, 2)
                                .background(.white.opacity(0.2), in: RoundedRectangle(cornerRadius: 4))
                        }
                    }
                }
            }
            .font(.callout)
            .foregroundStyle(.secondary)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(page.spokenMeta)
            .accessibilityIdentifier("item.meta")
        }
    }

    // MARK: Buttons

    private func buttons(_ page: ItemPage) -> some View {
        ScrollView(.horizontal) {
            HStack(spacing: 24) {
                if page.primary.kind == .unavailable || model.request() == nil {
                    Text("Nothing to play here yet.").foregroundStyle(.secondary)
                } else {
                    Button {
                        play(model.request())
                    } label: {
                        Label(page.primary.label, systemImage: "play.fill")
                    }
                    .focused($playFocused)
                    .accessibilityValue(page.primary.timeLeft)
                    .accessibilityIdentifier("item.play")
                    if page.fromStart {
                        Button {
                            play(model.request(fromStart: true))
                        } label: {
                            Label("From start", systemImage: "arrow.counterclockwise")
                        }
                        .accessibilityLabel("Play from start")
                        .accessibilityIdentifier("item.playFromStart")
                    }
                }
                if let block = model.requestBlock, block.more {
                    Button {
                        app.shell.open(block.picker)
                    } label: {
                        Label(RequestWords.moreSeasons, systemImage: "plus")
                    }
                    .accessibilityIdentifier("item.requestMore")
                }
                if page.markable {
                    Button {
                        Task { await model.toggleWatched() }
                    } label: {
                        Image(systemName: page.watched ? "checkmark.circle.fill" : "checkmark.circle")
                    }
                    .accessibilityLabel("Watched")
                    .accessibilityValue(page.watched ? "Watched" : "Not watched")
                    .accessibilityIdentifier("item.watched")
                }
                if let listed = page.myList {
                    Button {
                        Task { await model.toggleMyList() }
                    } label: {
                        Image(systemName: listed ? "checkmark" : "plus")
                    }
                    .accessibilityLabel(listed ? "Remove from My List" : "Add to My List")
                    .accessibilityIdentifier("item.myList")
                }
                if let trailer = page.trailer {
                    Button {
                        play(model.request(for: trailer))
                    } label: {
                        Image(systemName: "film")
                    }
                    .accessibilityLabel("Trailer")
                    .accessibilityIdentifier("item.trailer")
                }
                if page.rateable {
                    rating(page.userRating)
                }
                if let tracks = page.tracks {
                    pickers(tracks)
                }
                more(page)
            }
            .padding(.vertical, 24)
        }
        .scrollClipDisabled()
        .onChange(of: playFocused) { _, now in if now { model.prefetch() } }
    }

    /// The profile's rating, picked from a list of half stars.
    private func rating(_ rating: Int?) -> some View {
        ChoiceButton(title: localized("Your rating"), value: rating.map(StarWords.stars) ?? localized("Not rated"),
                     symbol: rating == nil ? "star" : "star.fill",
                     text: rating.map(StarWords.stars) ?? localized("Rate"),
                     choices: [Choice(label: localized("Not rated"), tag: nil)]
                         + (1...10).reversed().map { Choice(label: StarWords.stars($0), tag: $0) },
                     selected: rating) { value in Task { await model.rate(value) } }
            .accessibilityIdentifier("item.rating")
    }

    @ViewBuilder private func pickers(_ tracks: PleyScreens.TrackChoices) -> some View {
        if tracks.versions.count > 1 {
            ChoiceButton(title: localized("Version"), value: tracks.versionLabel ?? "", symbol: "square.stack",
                         text: tracks.versionLabel, choices: tracks.versions.map { Choice(label: $0.label, tag: $0.id) },
                         selected: tracks.versionId) { id in if let id { model.chooseVersion(id) } }
                .accessibilityIdentifier("item.version")
        }
        if !tracks.audio.isEmpty {
            ChoiceButton(title: localized("Audio"), value: tracks.audioLabel ?? "", symbol: "speaker.wave.2",
                         choices: [Choice(label: tracks.automaticLabel, tag: nil)]
                             + tracks.audio.map { Choice(label: $0.label, tag: $0.id) },
                         selected: tracks.automaticAudio ? nil : tracks.audioIndex) { index in
                Task { await model.chooseAudio(index) }
            }
            .accessibilityIdentifier("item.audio")
        }
        if !tracks.subtitles.isEmpty {
            ChoiceButton(title: localized("Subtitles"), value: tracks.subtitleLabel, symbol: "captions.bubble",
                         // Plural in French and Spanish (the subtitles), unlike a setting's Off.
                         choices: [Choice(label: localized(keyed: LocalizedStringResource("subtitles.off",
                                                                                         defaultValue: "Off")),
                                          tag: nil)]
                             + tracks.subtitles.map { Choice(label: $0.label, tag: $0.id) },
                         selected: tracks.subtitleIndex) { index in Task { await model.chooseSubtitle(index) } }
                .accessibilityIdentifier("item.subtitles")
        }
    }

    /// The rest of the item's actions; Watched and My List have their buttons.
    @ViewBuilder private func more(_ page: ItemPage) -> some View {
        let shown: Set<ItemAction> = [.play, .resume, .fromStart, .markWatched, .markUnwatched, .addToMyList,
                                      .removeFromMyList]
        let list = app.actions.actions(for: page.item, continueWatching: false).filter { !shown.contains($0) }
        if !list.isEmpty {
            Menu {
                ForEach(list, id: \.self) { action in
                    Button {
                        ItemActionsMenu.run(app, action, on: page.item)
                    } label: {
                        Label(action.title, systemImage: action.symbol)
                    }
                }
            } label: {
                Image(systemName: "ellipsis")
            }
            .accessibilityLabel("More")
            .accessibilityIdentifier("item.more")
        }
    }

    // MARK: Rows

    private func rows(_ page: ItemPage) -> some View {
        VStack(alignment: .leading, spacing: Metrics.rowSpacing) {
            if let seasons = page.seasons {
                seasonsSection(seasons, onShow: page.kind == .show)
            }
            if !page.extras.isEmpty {
                TileRow(app: app, key: "extras", title: localized("Extras"), shape: .landscape,
                        tiles: page.extras.map { Tile($0, shape: .landscape, line: Words.extraSubtitle($0)) },
                        prefix: "item.row") { tile in play(model.request(for: tile.item)) }
            }
            if !page.cast.isEmpty {
                cast(page)
            }
            if !page.related.isEmpty {
                TileRow(app: app, key: "related", title: localized("More Like This"), shape: .poster,
                        tiles: page.related.map { Tile($0, shape: .poster) }, prefix: "item.row") { tile in
                    app.shell.open(Screen.page(of: tile.item))
                }
            }
            details(page)
        }
    }

    /// The episodes of a season; on a show's page, its seasons to choose
    /// from and a button to the season's own page.
    private func seasonsSection(_ section: SeasonsSection, onShow: Bool) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            if onShow {
                ScrollView(.horizontal) {
                    HStack(spacing: 20) {
                        ForEach(section.seasons) { tab in
                            Button(tab.title) { model.selectSeason(tab.id) }
                                .font(.callout)
                                .buttonStyle(.bordered)
                                .tint(tab.id == section.selected ? .white : nil)
                                .accessibilityAddTraits(tab.id == section.selected ? .isSelected : [])
                                .accessibilityIdentifier("item.season.\(tab.id)")
                        }
                        if let tab = section.selectedTab {
                            Button {
                                app.shell.open(.item(tab.id))
                            } label: {
                                Label("Open \(tab.title)", systemImage: "chevron.right")
                            }
                            .font(.callout)
                            .accessibilityIdentifier("item.seasonPage")
                        }
                    }
                    .padding(.horizontal, Metrics.gutter)
                    .padding(.vertical, 20)
                }
                .scrollClipDisabled()
                .focusSection()
            } else {
                Text(section.selectedTab?.title ?? localized("Episodes"))
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                    .padding(.horizontal, Metrics.gutter)
            }
            if let episodes = section.episodes {
                if episodes.isEmpty {
                    Text("No episodes yet.").foregroundStyle(.secondary).padding(.horizontal, Metrics.gutter)
                } else {
                    ScrollView(.horizontal) {
                        LazyHStack(alignment: .top, spacing: Metrics.tileSpacing) {
                            ForEach(episodes) { episode in
                                EpisodeCard(app: app, episode: episode) {
                                    app.shell.open(.item(episode.id))
                                } play: {
                                    play(model.request(for: episode.item))
                                }
                            }
                        }
                        .padding(.horizontal, Metrics.gutter)
                        .padding(.vertical, 32)
                    }
                    .scrollClipDisabled()
                    .focusSection()
                    .accessibilityIdentifier("item.row.episodes")
                }
            } else {
                ProgressView().padding(.horizontal, Metrics.gutter)
            }
        }
    }

    private func cast(_ page: ItemPage) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(page.castTitle)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, Metrics.gutter)
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: 40) {
                    ForEach(page.cast, id: \.self) { person in
                        CastCard(app: app, person: person) { app.shell.open(.person(person.id)) }
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 32)
            }
            .scrollClipDisabled()
        }
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("item.row.cast")
    }

    /// Crew, the facts, and Media info behind a button (tvOS has no
    /// disclosure groups).
    @ViewBuilder private func details(_ page: ItemPage) -> some View {
        let rows = page.details.rows
        VStack(alignment: .leading, spacing: 14) {
            if !page.details.crew.isEmpty || !rows.isEmpty || !page.partOf.isEmpty {
                Text("Details").font(.headline).accessibilityAddTraits(.isHeader)
                ForEach(page.details.crew, id: \.role) { line in
                    InfoLine(label: line.label, value: line.names)
                }
                if !page.partOf.isEmpty {
                    // Each collection is a button to its page.
                    HStack(alignment: .firstTextBaseline, spacing: 20) {
                        Text("Part of").foregroundStyle(.secondary).frame(width: 260, alignment: .leading)
                        ForEach(page.partOf, id: \.id) { collection in
                            Button(collection.title) { app.shell.open(.collection(collection.id)) }
                                .accessibilityLabel("Part of \(collection.title)")
                                .accessibilityHint("Opens the collection")
                                .accessibilityIdentifier("item.partOf.\(collection.id)")
                        }
                    }
                    .font(.callout)
                }
                ForEach(rows, id: \.label) { row in
                    InfoLine(label: row.label, value: row.value)
                }
            }
            if !page.mediaInfo.isEmpty {
                Button {
                    showMediaInfo.toggle()
                } label: {
                    Label("Media info", systemImage: showMediaInfo ? "chevron.down" : "chevron.right")
                }
                .accessibilityValue(showMediaInfo ? "Shown" : "Hidden")
                .accessibilityIdentifier("item.mediaInfo")
                .padding(.top, 10)
                if showMediaInfo {
                    mediaInfo(page.mediaInfo)
                }
            }
        }
        .padding(.horizontal, Metrics.gutter)
        .focusSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("item.details")
    }

    private func mediaInfo(_ files: [FileInfo]) -> some View {
        VStack(alignment: .leading, spacing: 24) {
            ForEach(files) { file in
                VStack(alignment: .leading, spacing: 8) {
                    if let title = file.title {
                        Text(title).font(.headline)
                    }
                    ForEach(file.rows, id: \.self) { row in
                        InfoLine(label: row.label, value: row.value)
                    }
                    ForEach(Array(file.streams.enumerated()), id: \.offset) { _, stream in
                        VStack(alignment: .leading, spacing: 2) {
                            Text([stream.kind, stream.codec, stream.language, stream.title]
                                .compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · "))
                                .font(.callout.bold())
                            if !stream.details.isEmpty {
                                Text(stream.details).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .accessibilityElement(children: .combine)
                    }
                }
            }
        }
        // Focusable, so the remote can scroll down through it.
        .focusable()
    }

    private func play(_ request: PlayRequest?) {
        guard let request else { return }
        Task { await app.play(request) }
    }
}

private extension ItemPage {
    /// "1h 33m left" under the buttons while resuming.
    var timeLeftOrEmpty: String { primary.kind == .resume ? primary.timeLeft : "" }
}

/// One of a `ChoiceButton`'s choices.
struct Choice: Hashable {
    let label: String
    let tag: Int?
}

/// A button of the row that offers a list to choose from (audio,
/// subtitles, a version, a rating), the current one ticked, in the
/// system's full-screen list. Its words come in the language shown.
struct ChoiceButton: View {
    let title: String
    /// The current choice, as VoiceOver reads it.
    let value: String
    let symbol: String
    /// Words beside the symbol, when it has some.
    var text: String?
    let choices: [Choice]
    let selected: Int?
    let pick: (Int?) -> Void
    @State private var open = false

    var body: some View {
        Button {
            open = true
        } label: {
            if let text {
                Label(text, systemImage: symbol)
            } else {
                Image(systemName: symbol)
            }
        }
        .accessibilityLabel(title)
        .accessibilityValue(value)
        .confirmationDialog(title, isPresented: $open, titleVisibility: .visible) {
            ForEach(choices, id: \.self) { choice in
                Button(choice.tag == selected ? "\(choice.label) ✓" : choice.label) { pick(choice.tag) }
            }
        }
    }
}

/// A rating in words.
enum StarWords {
    /// "3.5 stars", from 1 to 10, in the language shown: a whole number
    /// counts its stars, a half is written in the shown locale ("3,5 étoiles").
    static func stars(_ rating: Int) -> String {
        if rating % 2 == 0 { return localized("\(rating / 2) stars") }
        let number = (Double(rating) / 2).formatted(.number.locale(ShownLanguage.locale))
        return localized("\(number) stars")
    }
}

/// A label and its value, on one line.
private struct InfoLine: View {
    let label: String
    let value: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 20) {
            Text(label).foregroundStyle(.secondary).frame(width: 260, alignment: .leading)
            Text(value)
        }
        .font(.callout)
        .accessibilityElement(children: .combine)
    }
}

/// An episode as a card: its still with progress or a check, and under it
/// its number and title and its length and air date. Select opens its
/// page; Play/Pause plays it, and a long press offers its actions.
private struct EpisodeCard: View {
    let app: AppModel
    @ObservedObject private var actions: ItemActionsModel
    let episode: EpisodeLine
    let open: () -> Void
    let play: () -> Void
    private let width: CGFloat = 420

    init(app: AppModel, episode: EpisodeLine, open: @escaping () -> Void, play: @escaping () -> Void) {
        self.app = app
        _actions = ObservedObject(wrappedValue: app.actions)
        self.episode = episode
        self.open = open
        self.play = play
    }

    private var watched: Bool { actions.watched[episode.id] ?? episode.watched }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Button(action: open) {
                ServerImage(app: app, paths: episode.pictures, width: width, symbol: "tv")
                    .frame(width: width, height: width * 9 / 16)
                    .overlay(alignment: .bottom) {
                        if episode.progress > 0 { ProgressBar(fraction: episode.progress) }
                    }
                    .overlay(alignment: .topTrailing) {
                        if watched {
                            Image(systemName: "checkmark.circle.fill")
                                .symbolRenderingMode(.palette)
                                .foregroundStyle(.white, .black.opacity(0.75))
                                .font(.title3)
                                .padding(10)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            }
            .buttonStyle(.card)
            .accessibilityLabel(episode.spokenLabel)
            .accessibilityValue(episode.spokenValue)
            .accessibilityAction(named: "Play", play)
            .itemActions(app, episode.item)
            .onPlayPauseCommand(perform: play)
            .accessibilityIdentifier("item.episode.\(episode.id)")
            VStack(alignment: .leading, spacing: 4) {
                Text(episode.title).font(.callout).lineLimit(1)
                Text(episode.line).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                if let summary = episode.summary {
                    Text(summary).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                }
            }
            .accessibilityHidden(true)
        }
        .frame(width: width, alignment: .leading)
    }
}

/// A person of the cast: their round photo (or initials) that lifts when
/// focused, their name and their part.
private struct CastCard: View {
    let app: AppModel
    let person: Credit
    let open: () -> Void
    private let size: CGFloat = 180

    var body: some View {
        VStack(spacing: 14) {
            Button(action: open) {
                AvatarView(app: app, name: person.name, path: person.photoUrl, color: nil, size: size)
            }
            .buttonStyle(.borderless)
            .clipShape(Circle())
            .accessibilityLabel([person.name, person.character.map { localized("as \($0)") }].compactMap { $0 }
                .joined(separator: ", "))
            .accessibilityHint("Opens their page")
            .accessibilityIdentifier("item.cast.\(person.id)")
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
