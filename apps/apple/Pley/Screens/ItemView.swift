import PleyData
import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// A title's page (docs/APPLE.md, Item pages): the backdrop and poster,
/// the title and where it sits (show, season), the line of facts, Play or
/// Resume, From start, Play on…, Watched, My List, the trailer and More; the stage
/// of a request for it and Request more seasons; the profile's rating;
/// the version, audio and subtitle pickers; the summary; a show's seasons
/// and episodes; extras, cast, More Like This, crew and details, and
/// Media info.
///
/// One column on iPhone. On iPad and the Mac, from 700 points wide, the
/// poster and the title's own parts sit in two columns above the rows;
/// at accessibility text sizes it stays one column. An item that isn't a
/// title (an artist, an album, a track, an extra) shows the page it
/// belongs on instead.
struct ItemView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ItemModel
    /// A player was open: once it closes, the page reads the item again
    /// for its new resume point.
    @State private var sawPlayer = false
    @State private var width: CGFloat = 0
    @State private var showMediaInfo = false
    @Environment(\.dynamicTypeSize) private var typeSize
    private let layout = WideLayout()
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

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
                    .navigationTitle(model.page == nil ? "" : model.title)
                    #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                    #endif
            }
        }
        .task { await model.load() }
        // A request's download is followed only while the page shows.
        .task { await model.followRequest() }
        .reloads(app) { await model.load() }
        #if os(macOS)
        // ⌘↩ plays the page's title, as its Play does.
        .focusedSceneValue(\.playTarget, model.request() == nil ? nil : PlayTarget(title: model.title) {
            [model] in model.request()
        })
        #endif
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
            StateView(kind: .failed(title: localized("Couldn't load this title"), why: why) { Task { await model.load() } })
        case .loaded:
            if let page = model.page {
                loaded(page)
            } else {
                StateView(kind: .loading)
            }
        }
    }

    /// Two columns: wide enough, and the text not at an accessibility size.
    private var twoColumns: Bool { width >= 700 && !typeSize.isAccessibilitySize }
    private var gutter: CGFloat { Metrics.gutter(regular: layout.wide) }

    private func loaded(_ page: ItemPage) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Metrics.rowSpacing) {
                VStack(alignment: .leading, spacing: 0) {
                    backdrop(page)
                    if twoColumns {
                        HStack(alignment: .top, spacing: 24) {
                            poster(page, width: 200)
                            main(page)
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
                rows(page)
            }
            .padding(.bottom, 32)
            .frame(maxWidth: 1400, alignment: .leading)
            .frame(maxWidth: .infinity)
            .background(GeometryReader { geometry in
                Color.clear.preference(key: WidthKey.self, value: geometry.size.width)
            })
        }
        .onPreferenceChange(WidthKey.self) { width = $0 }
        .accessibilityIdentifier("item.page")
    }

    // MARK: Header

    private func backdrop(_ page: ItemPage) -> some View {
        ServerImage(app: app, paths: page.backdrops, width: max(width, 320), symbol: nil,
                    limit: ImageLoader.deviceBackdropLimit)
            .frame(height: twoColumns ? 320 : 220)
            .frame(maxWidth: .infinity)
            .clipped()
            .overlay {
                LinearGradient(colors: [.clear, Color.pageBackground], startPoint: .center, endPoint: .bottom)
            }
            .accessibilityHidden(true)
    }

    private func poster(_ page: ItemPage, width: CGFloat) -> some View {
        ServerImage(app: app, paths: page.posters, width: width, symbol: Metrics.symbol(page.kind))
            .frame(width: width, height: width * 3 / 2)
            .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            .shadow(radius: 6, y: 2)
            .accessibilityHidden(true)
    }

    /// The title, where it sits, and the facts.
    private func heading(_ page: ItemPage) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let context = page.context {
                breadcrumb(context)
            }
            Text(page.title)
                .font(.title.bold())
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("item.title")
            if let episode = page.context?.episodeLongLabel {
                Text(episode).font(.subheadline).foregroundStyle(.secondary)
            }
            meta(page)
        }
    }

    @ViewBuilder private func breadcrumb(_ context: ItemContext) -> some View {
        FlowLayout(spacing: 6) {
            if let show = context.showTitle {
                if let id = context.showId {
                    Button(show) { app.shell.open(.item(id)) }
                        .accessibilityHint("Opens the show")
                        .accessibilityIdentifier("item.show")
                } else {
                    Text(show)
                }
            }
            if let season = context.seasonTitle {
                Text(verbatim: "·").foregroundStyle(.secondary).accessibilityHidden(true).flowSeparator()
                if let id = context.seasonId {
                    Button(season) { app.shell.open(.item(id)) }
                        .accessibilityHint("Opens the season")
                        .accessibilityIdentifier("item.seasonLink")
                } else {
                    Text(season)
                }
            }
        }
        .font(.subheadline.weight(.semibold))
        .buttonStyle(.plain)
        .foregroundStyle(Color.accentColor)
    }

    @ViewBuilder private func meta(_ page: ItemPage) -> some View {
        let parts = facts(page)
        if !parts.isEmpty {
            FlowLayout(spacing: 8, lineSpacing: 4) {
                ForEach(Array(parts.enumerated()), id: \.offset) { index, part in
                    if index > 0 { Text(verbatim: "·").flowSeparator() }
                    switch part {
                    case .text(let text):
                        Text(text)
                    case .contentRating(let rating):
                        Text(rating)
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 4)
                            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.secondary))
                    case .rating(let rating):
                        Label(rating, systemImage: "star.fill").labelStyle(.titleAndIcon)
                    case .badges(let badges):
                        ForEach(badges, id: \.self) { badge in
                            Text(badge)
                                .font(.caption.weight(.bold))
                                .padding(.horizontal, 5)
                                .padding(.vertical, 1)
                                .background(.quaternary, in: RoundedRectangle(cornerRadius: 3))
                        }
                    }
                }
            }
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(parts.map(\.spoken).joined(separator: ", "))
            .accessibilityIdentifier("item.meta")
        }
    }

    /// The two-column page's right-hand column.
    private func main(_ page: ItemPage) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            heading(page).padding(.top, 70)
            column(page)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Everything under the heading in the column: buttons, rating,
    /// pickers, the words, crew.
    private func column(_ page: ItemPage) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            primary(page)
            buttons(page)
            if let block = model.requestBlock {
                ItemRequestBlockView(app: app, block: block)
            }
            if page.rateable {
                StarRating(rating: page.userRating) { rating in Task { await model.rate(rating) } }
            }
            if let tracks = page.tracks {
                pickers(page, tracks)
            }
            if let notice = model.notice {
                Text(notice).font(.footnote).foregroundStyle(.red)
            }
            if let tagline = page.tagline {
                Text(tagline).font(.headline).italic()
            }
            if let summary = page.summary {
                Text(summary).fixedSize(horizontal: false, vertical: true)
            }
            if !page.genres.isEmpty {
                Text(page.genres.joined(separator: ", "))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Genres: \(page.genres.joined(separator: ", "))")
            }
            if !page.partOf.isEmpty {
                FlowLayout(spacing: 6) {
                    Text("Part of")
                        .foregroundStyle(.secondary)
                    ForEach(page.partOf, id: \.id) { collection in
                        Button(collection.title) { app.shell.open(.collection(collection.id)) }
                            .buttonStyle(.borderless)
                            .accessibilityLabel("Part of \(collection.title)")
                            .accessibilityHint("Opens the collection")
                            .accessibilityIdentifier("item.partOf.\(collection.id)")
                    }
                }
                .font(.subheadline)
            }
            crew(page.details.crew)
        }
        .frame(maxWidth: 760, alignment: .leading)
    }

    // MARK: Buttons

    @ViewBuilder private func primary(_ page: ItemPage) -> some View {
        let action = page.primary
        if action.kind == .unavailable || model.request() == nil {
            Text("Nothing to play here yet.").foregroundStyle(.secondary)
        } else {
            // Play and From start share the row, their words on one line; at
            // accessibility sizes, or when they don't fit across, one above
            // the other.
            ViewThatFits(in: .horizontal) {
                if !typeSize.isAccessibilitySize {
                    HStack(spacing: 12) { playButtons(page) }
                        .fixedSize(horizontal: false, vertical: true)
                }
                VStack(alignment: .leading, spacing: 10) { playButtons(page) }
            }
            .frame(maxWidth: 480, alignment: .leading)
            .controlSize(.large)
        }
    }

    /// Play or Resume, and From start. Resume has how far it was played
    /// under its name, and the time left beside that.
    @ViewBuilder private func playButtons(_ page: ItemPage) -> some View {
        let action = page.primary
        Button {
            play(model.request())
        } label: {
            VStack(spacing: 4) {
                Label(action.label, systemImage: "play.fill")
                    .lineLimit(1)
                if action.kind == .resume {
                    HStack(spacing: 6) {
                        ProgressView(value: action.progress)
                            .tint(.white)
                            .frame(minWidth: 40)
                            .accessibilityHidden(true)
                        if !action.timeLeft.isEmpty {
                            Text(action.timeLeft)
                                .font(.caption)
                                .lineLimit(1)
                                .fixedSize()
                                .opacity(0.85)
                                .accessibilityHidden(true)
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .accessibilityValue(action.timeLeft)
        .accessibilityIdentifier("item.play")
        .onHover { if $0 { model.prefetch() } }
        if page.fromStart {
            Button {
                play(model.request(fromStart: true))
            } label: {
                Label("From start", systemImage: "arrow.counterclockwise")
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Play from start")
            .accessibilityIdentifier("item.playFromStart")
        }
    }

    private func buttons(_ page: ItemPage) -> some View {
        FlowLayout(spacing: 10) {
            if let target = model.castTarget {
                PlayOnMenu(remote: app.remote, target: target)
                    .menuStyle(.button)
            }
            if page.markable {
                Button {
                    Task { await model.toggleWatched() }
                } label: {
                    Label("Watched", systemImage: page.watched ? "checkmark.circle.fill" : "checkmark.circle")
                }
                .accessibilityValue(page.watched ? Text("Watched") : Text("Not watched"))
                .accessibilityAddTraits(page.watched ? .isSelected : [])
                .accessibilityIdentifier("item.watched")
            }
            if let listed = page.myList {
                Button {
                    Task { await model.toggleMyList() }
                } label: {
                    Label("My List", systemImage: listed ? "checkmark" : "plus")
                }
                .accessibilityLabel(listed ? Text("Remove from My List") : Text("Add to My List"))
                .accessibilityIdentifier("item.myList")
            }
            DownloadButton(model: model)
            if let trailer = page.trailer {
                Button {
                    play(model.request(for: trailer))
                } label: {
                    Label("Trailer", systemImage: "film")
                }
                .accessibilityIdentifier("item.trailer")
            }
            more(page)
        }
        .buttonStyle(.bordered)
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
                        run(action, on: page.item)
                    } label: {
                        Label(action.title, systemImage: action.symbol)
                    }
                }
            } label: {
                Label("More", systemImage: "ellipsis")
            }
            .menuStyle(.button)
            .accessibilityIdentifier("item.more")
        }
    }

    // MARK: Pickers

    private func pickers(_ page: ItemPage, _ tracks: PleyScreens.TrackChoices) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if tracks.versions.count > 1 {
                pickerRow("Version", chosen: versionName(page, tracks), id: "item.version") {
                    Picker("Version", selection: Binding(get: { tracks.versionId ?? 0 },
                                                         set: { model.chooseVersion($0) })) {
                        ForEach(tracks.versions) { Text($0.label).tag($0.id) }
                    }
                }
            }
            if !tracks.audio.isEmpty {
                let audio = Binding<Int?>(get: { tracks.automaticAudio ? nil : tracks.audioIndex },
                                          set: { index in Task { await model.chooseAudio(index) } })
                let chosen = tracks.automaticAudio ? tracks.automaticLabel
                    : tracks.audio.first { $0.id == tracks.audioIndex }?.label ?? tracks.automaticLabel
                pickerRow("Audio", chosen: chosen, id: "item.audio") {
                    Picker("Audio", selection: audio) {
                        Text(tracks.automaticLabel).tag(Int?.none)
                        ForEach(tracks.audio) { Text($0.label).tag(Int?.some($0.id)) }
                    }
                }
            }
            if !tracks.subtitles.isEmpty {
                let subtitles = Binding<Int?>(get: { tracks.subtitleIndex },
                                              set: { index in Task { await model.chooseSubtitle(index) } })
                let chosen = tracks.subtitles.first { $0.id == tracks.subtitleIndex }?.label ?? localized("Off")
                pickerRow("Subtitles", chosen: chosen, id: "item.subtitles") {
                    Picker("Subtitles", selection: subtitles) {
                        Text("Off").tag(Int?.none)
                        ForEach(tracks.subtitles) { Text($0.label).tag(Int?.some($0.id)) }
                    }
                }
            }
        }
        .frame(maxWidth: 420, alignment: .leading)
    }

    /// The version that plays, by its edition ("Open Matte") or "Version 2";
    /// its picture, codec and size are in the menu.
    private func versionName(_ page: ItemPage, _ tracks: PleyScreens.TrackChoices) -> String {
        page.mediaInfo.first { $0.id == tracks.versionId }?.title
            ?? tracks.versions.first { $0.id == tracks.versionId }?.label ?? ""
    }

    /// The facts under the title. With the Version picker on the page, the
    /// edition that plays is left out of them: the picker says it.
    private func facts(_ page: ItemPage) -> [MetaPart] {
        guard let tracks = page.tracks, tracks.versions.count > 1,
              let edition = page.mediaInfo.first(where: { $0.id == tracks.versionId })?.title,
              let at = page.meta.firstIndex(of: .text(edition)) else { return page.meta }
        var parts = page.meta
        parts.remove(at: at)
        return parts
    }

    /// A picker's name, and its choice on one line beside it: a long
    /// choice is cut short rather than wrapped, which a menu picker's own
    /// label does. The choices open in a menu.
    private func pickerRow(_ title: LocalizedStringKey, chosen: String, id: String,
                           @ViewBuilder picker: () -> some View) -> some View {
        HStack(spacing: 12) {
            Text(title).fixedSize()
            Spacer(minLength: 0)
            Menu {
                picker().pickerStyle(.inline).labelsHidden()
            } label: {
                HStack(spacing: 4) {
                    Text(chosen).lineLimit(1)
                    Image(systemName: "chevron.up.chevron.down").imageScale(.small)
                }
            }
            .menuStyle(.button)
            .buttonStyle(.borderless)
            .accessibilityLabel(Text(title))
            .accessibilityValue(chosen)
            .accessibilityIdentifier(id)
        }
    }

    // MARK: Crew

    @ViewBuilder private func crew(_ lines: [CrewLine]) -> some View {
        if !lines.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(lines, id: \.role) { line in
                    FlowLayout(spacing: 4, lineSpacing: 2) {
                        Text("\(line.label):").foregroundStyle(.secondary)
                        ForEach(Array(line.people.enumerated()), id: \.element.id) { index, person in
                            Button(person.name + (index < line.people.count - 1 || line.more > 0 ? "," : "")) {
                                app.shell.open(.person(person.id))
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(person.name)
                            .accessibilityHint("Opens their page")
                        }
                        if line.more > 0 {
                            Text("and \(line.more) more").foregroundStyle(.secondary)
                        }
                    }
                    .font(.subheadline)
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("item.crew")
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
            if !page.mediaInfo.isEmpty {
                mediaInfo(page.mediaInfo)
            }
        }
    }

    /// The episodes of a season; on a show's page, a picker of its seasons
    /// and a link to the season's own page.
    private func seasonsSection(_ section: SeasonsSection, onShow: Bool) -> some View {
        // At the accessibility text sizes the heading and its controls stack.
        let heading = Metrics.headingLayout(stacked: typeSize.isAccessibilitySize)
        return VStack(alignment: .leading, spacing: 12) {
            heading {
                (section.seasons.isEmpty ? Text("Episodes") : Text("Seasons"))
                    .font(.title3.bold())
                    .accessibilityAddTraits(.isHeader)
                if !typeSize.isAccessibilitySize { Spacer() }
                if section.seasons.count > 1 {
                    Picker("Season", selection: Binding(get: { section.selected ?? 0 },
                                                        set: { model.selectSeason($0) })) {
                        ForEach(section.seasons) { Text($0.title).tag($0.id) }
                    }
                    .pickerStyle(.menu)
                    .accessibilityIdentifier("item.season")
                }
                if onShow, let tab = section.selectedTab {
                    Button {
                        app.shell.open(.item(tab.id))
                    } label: {
                        Label(tab.title, systemImage: "chevron.right").labelStyle(TrailingIcon())
                    }
                    .accessibilityHint("Opens the season")
                    .accessibilityIdentifier("item.seasonPage")
                } else if let tab = section.selectedTab, section.seasons.count <= 1 {
                    Text(tab.title).foregroundStyle(.secondary)
                }
                if model.canDownloadSeason {
                    Button {
                        Task { await model.downloadSeason() }
                    } label: {
                        Label(DownloadWords.downloadSeason, systemImage: "arrow.down.circle")
                    }
                    .accessibilityIdentifier("item.downloadSeason")
                }
            }
            if let episodes = section.episodes {
                if episodes.isEmpty {
                    Text("No episodes yet.").foregroundStyle(.secondary)
                } else {
                    LazyVStack(alignment: .leading, spacing: 14) {
                        ForEach(episodes) { episode in
                            EpisodeRow(app: app, episode: episode, wide: twoColumns) {
                                app.shell.open(.item(episode.id))
                            } play: {
                                play(model.request(for: episode.item))
                            }
                        }
                    }
                }
            } else {
                ProgressView().frame(maxWidth: .infinity)
            }
        }
        .padding(.horizontal, gutter)
    }

    private func cast(_ page: ItemPage) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(page.castTitle)
                .font(.title3.bold())
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, gutter)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: 14) {
                    ForEach(page.cast, id: \.self) { person in
                        CastCircle(app: app, person: person) { app.shell.open(.person(person.id)) }
                    }
                }
                .padding(.horizontal, gutter)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("item.row.cast")
    }

    @ViewBuilder private func details(_ page: ItemPage) -> some View {
        let rows = page.details.rows
        if !rows.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text("Details").font(.title3.bold()).accessibilityAddTraits(.isHeader)
                ForEach(rows, id: \.label) { row in
                    LabeledContent(row.label, value: row.value)
                }
            }
            .frame(maxWidth: 760, alignment: .leading)
            .padding(.horizontal, gutter)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("item.details")
        }
    }

    private func mediaInfo(_ files: [FileInfo]) -> some View {
        DisclosureGroup(isExpanded: $showMediaInfo) {
            VStack(alignment: .leading, spacing: 16) {
                ForEach(files) { file in
                    VStack(alignment: .leading, spacing: 6) {
                        if let title = file.title {
                            Text(title).font(.headline)
                        }
                        ForEach(file.rows, id: \.self) { row in
                            LabeledContent(row.label) {
                                Text(row.value).multilineTextAlignment(.trailing).textSelection(.enabled)
                            }
                        }
                        ForEach(Array(file.streams.enumerated()), id: \.offset) { _, stream in
                            VStack(alignment: .leading, spacing: 2) {
                                Text([stream.kind, stream.codec, stream.language, stream.title]
                                    .compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · "))
                                    .font(.subheadline.weight(.semibold))
                                if !stream.details.isEmpty {
                                    Text(stream.details).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                            .accessibilityElement(children: .combine)
                        }
                    }
                }
            }
            .padding(.top, 8)
        } label: {
            Text("Media info").font(.title3.bold())
        }
        .frame(maxWidth: 760, alignment: .leading)
        .padding(.horizontal, gutter)
        .accessibilityIdentifier("item.mediaInfo")
    }

    // MARK: Doing

    private func run(_ action: ItemAction, on item: ItemSummary) {
        #if os(macOS)
        ItemActionsMenu.run(app, action, on: item) { openWindow(id: PlayerWindow.id, value: $0) }
        #else
        ItemActionsMenu.run(app, action, on: item, play: nil)
        #endif
    }

    /// The player: full screen on iPhone and iPad, its own window on the Mac.
    private func play(_ request: PlayRequest?) {
        guard let request else { return }
        #if os(macOS)
        openWindow(id: PlayerWindow.id, value: request)
        #else
        Task { await app.play(request) }
        #endif
    }
}

/// An episode in a season's list: its still with progress or a check, its
/// number and title, its length and air date, and two lines of summary.
/// Tapping it opens its page; its play button plays it. Its actions are
/// its context menu and VoiceOver's actions.
private struct EpisodeRow: View {
    let app: AppModel
    @ObservedObject private var actions: ItemActionsModel
    let episode: EpisodeLine
    let wide: Bool
    let open: () -> Void
    let play: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize

    init(app: AppModel, episode: EpisodeLine, wide: Bool, open: @escaping () -> Void, play: @escaping () -> Void) {
        self.app = app
        _actions = ObservedObject(wrappedValue: app.actions)
        self.episode = episode
        self.wide = wide
        self.open = open
        self.play = play
    }

    private var watched: Bool { actions.watched[episode.id] ?? episode.watched }

    var body: some View {
        let stillWidth: CGFloat = wide ? 220 : 150
        let stack = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8))
            : AnyLayout(HStackLayout(alignment: .top, spacing: 12))
        stack {
            Button(action: play) {
                ServerImage(app: app, paths: episode.pictures, width: stillWidth, symbol: "tv")
                    .frame(width: stillWidth, height: stillWidth * 9 / 16)
                    .overlay(alignment: .bottom) {
                        if episode.progress > 0 { ProgressBar(fraction: episode.progress) }
                    }
                    .overlay(alignment: .topTrailing) {
                        if watched {
                            Image(systemName: "checkmark.circle.fill")
                                .symbolRenderingMode(.palette)
                                .foregroundStyle(.white, Color.accentColor)
                                .padding(6)
                        }
                    }
                    .overlay {
                        Image(systemName: "play.circle.fill")
                            .font(.title)
                            .symbolRenderingMode(.palette)
                            .foregroundStyle(.white, .black.opacity(0.4))
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
            }
            .buttonStyle(.plain)
            .accessibilityHidden(true)
            Button(action: open) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(episode.title).font(.headline)
                    Text(episode.line).font(.subheadline).foregroundStyle(.secondary)
                    if let summary = episode.summary {
                        Text(summary)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(typeSize.isAccessibilitySize ? nil : 2)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(episode.spokenLabel)
        .accessibilityValue(episode.spokenValue)
        .accessibilityHint("Opens its page")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction(.default, open)
        .accessibilityAction(named: "Play", play)
        .itemActions(app, episode.item)
        .accessibilityIdentifier("item.episode.\(episode.id)")
    }
}

/// A person of the cast: their round photo (or initials), name and part.
private struct CastCircle: View {
    let app: AppModel
    let person: Credit
    let open: () -> Void
    @ScaledMetric(relativeTo: .body) private var size: CGFloat = 84

    var body: some View {
        Button(action: open) {
            VStack(spacing: 6) {
                AvatarView(app: app, name: person.name, path: person.photoUrl, color: nil, size: min(size, 140))
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
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([person.name, person.character.map { localized("as \($0)") }].compactMap { $0 }
            .joined(separator: ", "))
        .accessibilityHint("Opens their page")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("item.cast.\(person.id)")
    }
}

/// A label with its symbol after the words, as a link's chevron is.
private struct TrailingIcon: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            configuration.title
            configuration.icon.font(.footnote.weight(.semibold))
        }
    }
}

/// The page's width, for its two columns.
private struct WidthKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

extension Color {
    /// The page's own background, for gradients that fade into it.
    static var pageBackground: Color {
        #if os(macOS)
        Color(nsColor: .windowBackgroundColor)
        #else
        Color(uiColor: .systemBackground)
        #endif
    }
}
