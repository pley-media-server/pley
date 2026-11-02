import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// Live TV: the categories (a sheet on iPhone, a column on iPad and the
/// Mac), and the category's channels as the guide (channels by time, the
/// channel column pinned while the times scroll) or as cards. A channel
/// opens the live player; a programme opens its sheet. The list opens on
/// the channel watched last.
struct LiveTvView: View {
    @ObservedObject var app: AppModel
    @StateObject private var model: LiveTvModel
    @State private var categoriesShown = false
    @State private var query = ""
    @State private var program: ProgramChoice?
    private let layout = WideLayout()
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: app.liveTvModel())
    }

    var body: some View {
        content
            .navigationTitle(model.status == .ready ? model.title : "Live TV")
            .toolbar {
                if model.status == .ready {
                    if !layout.wide {
                        ToolbarItem(placement: .navigation) {
                            Button {
                                categoriesShown = true
                            } label: {
                                Label("Categories", systemImage: "list.bullet")
                            }
                            .accessibilityIdentifier("live.categories")
                        }
                    }
                    ToolbarItem(placement: .primaryAction) {
                        Picker("View", selection: Binding(get: { model.view }, set: { model.show($0) })) {
                            ForEach(LiveViewKind.allCases, id: \.self) { kind in
                                Text(kind.title).tag(kind)
                            }
                        }
                        .pickerStyle(.segmented)
                        .accessibilityIdentifier("live.view")
                    }
                }
            }
            .searchable(text: $query, prompt: "Channels and programmes")
            .onValueChange(of: query) { model.search($0) }
            .sheet(isPresented: $categoriesShown) {
                NavigationStack {
                    LiveCategoriesList(model: model) { category in
                        model.show(category)
                        categoriesShown = false
                    }
                    .navigationTitle("Categories")
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") { categoriesShown = false }
                        }
                    }
                }
                .presentationDetents([.medium, .large])
            }
            .sheet(item: $program) { choice in
                ProgramSheet(app: app, model: model, channel: choice.channel, program: choice.program) {
                    program = nil
                    watch(choice.channel)
                }
            }
            .task { model.start() }
            .onAppear { model.visible(true) }
            .onDisappear { model.visible(false) }
            .reloads(app) { model.reload() }
            .onValueChange(of: app.change) { change in
                if change?.live == true || change?.everything == true { model.reload() }
            }
    }

    @ViewBuilder private var content: some View {
        switch model.status {
        case .loading:
            StateView(kind: .loading)
        case .notAllowed:
            StateView(kind: .empty(symbol: "tv",
                                   text: "Live TV isn't open to this profile. An administrator can open it in "
                                       + "Pley's web app, under Settings → Live TV.", action: nil))
        case .noChannels:
            StateView(kind: .empty(symbol: "tv",
                                   text: "No channels yet. Channels appear here once an administrator adds an "
                                       + "IPTV provider in Pley's web app.", action: nil))
        case .failed(let why):
            StateView(kind: .failed(title: "Live TV didn't load", why: why) { model.reload() })
        case .ready:
            if layout.wide {
                HStack(spacing: 0) {
                    LiveCategoriesList(model: model) { model.show($0) }
                        .frame(width: 260)
                    Divider()
                    channels
                }
            } else {
                channels
            }
        }
    }

    @ViewBuilder private var channels: some View {
        if let error = model.paging.pages.error {
            StateView(kind: .failed(title: "Live TV didn't load", why: error) { model.paging.reload() })
        } else if model.paging.pages.total == 0 {
            StateView(kind: .empty(symbol: "tv", text: "\(model.empty.title). \(model.empty.detail)", action: nil))
        } else if model.paging.pages.total == nil {
            StateView(kind: .loading)
        } else {
            switch model.view {
            case .guide:
                GuideView(app: app, model: model, paging: model.paging, open: watch) { channel, program in
                    self.program = ProgramChoice(channel: channel, program: program)
                }
            case .channels:
                ChannelCards(app: app, model: model, paging: model.paging, open: watch)
            }
        }
    }

    private func watch(_ channel: LiveChannel) {
        model.watched(channel.id)
        let request = LiveRequest(channelId: channel.id, category: model.query.isEmpty ? model.category : .all)
        #if os(macOS)
        openWindow(id: LiveWindow.id, value: request)
        #else
        Task { await app.watchLive(request, seen: channel) }
        #endif
    }
}

/// A programme chosen in the guide.
private struct ProgramChoice: Identifiable {
    let channel: LiveChannel
    let program: LiveProgram
    var id: String { "\(channel.id):\(program.start)" }
}

// MARK: Categories

/// Favourites, Recently watched and All channels, then the groups: a
/// region's under it, the others after. A search lists the groups it names first.
struct LiveCategoriesList: View {
    @ObservedObject var model: LiveTvModel
    let choose: (LiveCategory) -> Void

    var body: some View {
        List {
            if !model.foundGroups.isEmpty {
                Section("Groups") {
                    ForEach(model.foundGroups) { row(.group($0.name), LiveTvRules.groupName($0, name: $0.name), $0.count) }
                }
            }
            Section {
                row(.favorites, "Favourites", model.groups.favorites, symbol: "star")
                row(.recent, "Recently watched", model.groups.recent, symbol: "clock")
                row(.all, "All channels", model.groups.total, symbol: "tv")
            }
            let sections = model.sections
            if !sections.regions.isEmpty {
                Section("Regions") {
                    ForEach(sections.regions) { region in
                        DisclosureGroup {
                            ForEach(region.groups) { row(.group($0.name), $0.label, $0.count) }
                        } label: {
                            countLabel(region.region, region.count)
                        }
                        .accessibilityIdentifier("live.region.\(region.region)")
                    }
                }
            }
            if !sections.others.isEmpty {
                Section(sections.regions.isEmpty ? "Groups" : "More groups") {
                    ForEach(sections.others) { row(.group($0.name), LiveTvRules.groupName($0, name: $0.name), $0.count) }
                }
            }
        }
        .accessibilityIdentifier("live.categoryList")
    }

    private func row(_ category: LiveCategory, _ title: String, _ count: Int, symbol: String? = nil) -> some View {
        Button {
            choose(category)
        } label: {
            HStack {
                if let symbol { Image(systemName: symbol).frame(width: 24).accessibilityHidden(true) }
                countLabel(title, count)
                if model.category == category {
                    Image(systemName: "checkmark").foregroundStyle(Color.accentColor).accessibilityHidden(true)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(title), \(count) channels")
        .accessibilityAddTraits(model.category == category ? .isSelected : [])
        .accessibilityIdentifier("live.category.\(category.key)")
    }

    private func countLabel(_ title: String, _ count: Int) -> some View {
        HStack {
            Text(title).lineLimit(2)
            Spacer(minLength: 8)
            Text("\(count)").foregroundStyle(.secondary).monospacedDigit()
        }
    }
}

// MARK: The guide

/// How far the guide's times are scrolled.
private struct GuideOffsetKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

/// Channels by time: a row per channel, its programmes placed by their
/// times, the channel column pinned at the left as the times scroll, the
/// times pinned at the top, and a line at the time now.
private struct GuideView: View {
    let app: AppModel
    @ObservedObject var model: LiveTvModel
    @ObservedObject var paging: ChannelPaging
    let open: (LiveChannel) -> Void
    let choose: (LiveChannel, LiveProgram) -> Void
    @State private var offset: CGFloat = 0
    @State private var viewWidth: CGFloat = 0
    @State private var scrolled = false
    @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 64
    @ScaledMetric(relativeTo: .body) private var textScale: CGFloat = 1
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var channelWidth: CGFloat { 150 * min(textScale, 1.6) }
    private var layout: GuideLayout {
        GuideLayout(start: model.guideStart, pointsPerMinute: GuideLayout.pointsPerMinute(textScale: textScale))
    }

    var body: some View {
        let layout = layout
        GeometryReader { outer in
            ScrollViewReader { proxy in
                ScrollView(.horizontal) {
                    ScrollView(.vertical) {
                        LazyVStack(alignment: .leading, spacing: 0, pinnedViews: [.sectionHeaders]) {
                            Section {
                                ForEach(0..<(paging.pages.total ?? 0), id: \.self) { index in
                                    row(index, layout)
                                        .id("row.\(index)")
                                }
                            } header: {
                                header(layout)
                            }
                        }
                        .overlay(alignment: .topLeading) { nowLine(layout) }
                    }
                    .frame(width: channelWidth + layout.width)
                    .background(GeometryReader { inner in
                        Color.clear.preference(key: GuideOffsetKey.self,
                                               value: -inner.frame(in: .named("guide")).minX)
                    })
                }
                .coordinateSpace(name: "guide")
                .onPreferenceChange(GuideOffsetKey.self) { value in
                    offset = max(value, 0)
                    shown(layout)
                }
                .onAppear {
                    viewWidth = outer.size.width
                    shown(layout)
                    scroll(proxy, layout)
                }
                .onValueChange(of: model.landing) { _ in scroll(proxy, layout) }
            }
        }
        .accessibilityIdentifier("live.guide")
    }

    /// The times in view, so their guide comes.
    private func shown(_ layout: GuideLayout) {
        let from = layout.time(atX: Double(offset))
        let to = layout.time(atX: Double(offset + max(viewWidth - channelWidth, 0)))
        model.showingTimes(from: from, to: to)
    }

    /// Opens on the time now and on the channel watched last.
    private func scroll(_ proxy: ScrollViewProxy, _ layout: GuideLayout) {
        if let landing = model.landing {
            proxy.scrollTo("row.\(landing)", anchor: .top)
            model.landed()
        }
        guard !scrolled else { return }
        scrolled = true
        proxy.scrollTo("slot.\(LiveTvRules.guideStart(model.now))", anchor: .leading)
    }

    private func header(_ layout: GuideLayout) -> some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                ForEach(layout.slots, id: \.self) { slot in
                    Text(LiveTvRules.time(slot))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.leading, 6)
                        .frame(width: CGFloat(layout.x(slot + LiveTvRules.slot) - layout.x(slot)), alignment: .leading)
                        .id("slot.\(slot)")
                }
            }
            .padding(.leading, channelWidth)
            Color.clear.frame(width: channelWidth, height: 1)
                .background(.bar)
                .offset(x: offset)
        }
        .frame(height: 28)
        .background(.bar)
        .accessibilityHidden(true)
    }

    @ViewBuilder private func row(_ index: Int, _ layout: GuideLayout) -> some View {
        ZStack(alignment: .topLeading) {
            if let channel = paging.pages[index] {
                let from = layout.time(atX: Double(offset)) - LiveTvRules.slot * 4
                let to = layout.time(atX: Double(offset + viewWidth)) + LiveTvRules.slot * 4
                let programs = model.programs(of: channel, from: max(from, layout.start), to: to)
                if programs.isEmpty {
                    Text(channel.hasGuide ? "Nothing in the guide now" : "No guide for this channel")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .padding(.leading, channelWidth + offset + 12)
                        .frame(height: rowHeight)
                } else {
                    ForEach(programs, id: \.start) { program in
                        let frame = layout.frame(program)
                        programCell(channel, program)
                            .frame(width: CGFloat(frame.width), height: rowHeight)
                            .offset(x: channelWidth + CGFloat(frame.x))
                    }
                }
                channelCell(channel)
                    .offset(x: offset)
            } else {
                Color.secondary.opacity(0.08).frame(width: channelWidth, height: rowHeight - 4).offset(x: offset)
            }
        }
        .frame(width: channelWidth + CGFloat(layout.width), height: rowHeight, alignment: .topLeading)
        .onAppear { paging.showingRows(first: index - 15, last: index + 15) }
    }

    private func channelCell(_ channel: LiveChannel) -> some View {
        Button {
            open(channel)
        } label: {
            HStack(spacing: 8) {
                ChannelLogo(app: app, channel: channel, size: 36)
                VStack(alignment: .leading, spacing: 2) {
                    Text(LiveTvRules.channelName(channel, in: model.category)).font(.subheadline).lineLimit(2)
                    if let number = channel.number {
                        Text(number).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 0)
                if channel.favorite {
                    Image(systemName: "star.fill").font(.caption).foregroundStyle(.yellow)
                        .accessibilityLabel("Favourite")
                }
            }
            .padding(.horizontal, 8)
            .frame(width: channelWidth, height: rowHeight)
            .background(.background)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .overlay(alignment: .trailing) { Divider() }
        .accessibilityLabel("\(channel.name)\(channel.number.map { ", \($0)" } ?? ""), watch")
        .accessibilityIdentifier("live.guide.channel.\(channel.id)")
    }

    private func programCell(_ channel: LiveChannel, _ program: LiveProgram) -> some View {
        let on = program.airing(at: model.now)
        return Button {
            choose(channel, program)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                Text(program.title.isEmpty ? "No information for this time" : program.title)
                    .font(.subheadline)
                    .lineLimit(1)
                Text(LiveTvRules.span(program, now: model.now))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            .background(on ? Color.accentColor.opacity(0.18) : Color.secondary.opacity(0.08),
                        in: RoundedRectangle(cornerRadius: 6))
            .padding(2)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(LiveTvRules.spoken(program, channel: channel, now: model.now))
        .accessibilityIdentifier("live.guide.program.\(channel.id).\(program.start)")
    }

    /// The time now, across the rows; it moves every 15 seconds (it jumps under Reduce Motion).
    private func nowLine(_ layout: GuideLayout) -> some View {
        Rectangle()
            .fill(Color.red)
            .frame(width: 2)
            .frame(maxHeight: .infinity)
            .offset(x: channelWidth + CGFloat(layout.x(model.now)))
            .animation(reduceMotion ? nil : .linear(duration: 0.5), value: model.now)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

// MARK: Cards

/// The channels as cards: the logo, the number and name, what's on with
/// its progress and what's next, and the star.
private struct ChannelCards: View {
    let app: AppModel
    @ObservedObject var model: LiveTvModel
    @ObservedObject var paging: ChannelPaging
    let open: (LiveChannel) -> Void

    var body: some View {
        ScrollViewReader { proxy in
            List {
                ForEach(0..<(paging.pages.total ?? 0), id: \.self) { index in
                    Group {
                        if let channel = paging.pages[index] {
                            card(channel)
                        } else {
                            Color.secondary.opacity(0.08).frame(height: 60)
                        }
                    }
                    .id(index)
                    .onAppear { paging.showingRows(first: index - 15, last: index + 15) }
                }
            }
            .listStyle(.plain)
            .refreshable { model.reload() }
            .onAppear { land(proxy) }
            .onValueChange(of: model.landing) { _ in land(proxy) }
        }
        .accessibilityIdentifier("live.cards")
    }

    private func land(_ proxy: ScrollViewProxy) {
        guard let landing = model.landing else { return }
        proxy.scrollTo(landing, anchor: .top)
        model.landed()
    }

    private func card(_ channel: LiveChannel) -> some View {
        HStack(spacing: 12) {
            Button {
                open(channel)
            } label: {
                HStack(spacing: 12) {
                    ChannelLogo(app: app, channel: channel, size: 52)
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 8) {
                            if let number = channel.number {
                                Text(number).foregroundStyle(.secondary).monospacedDigit()
                            }
                            Text(LiveTvRules.channelName(channel, in: model.category))
                        }
                        if let on = channel.on(at: model.now) {
                            Text(on.title).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                            ProgressView(value: on.progress(at: model.now)).accessibilityHidden(true)
                        } else {
                            Text(channel.hasGuide ? "Nothing in the guide now" : "No guide for this channel")
                                .font(.subheadline).foregroundStyle(.secondary)
                        }
                        if let next = channel.after(at: model.now) {
                            Text("Next \(LiveTvRules.time(next.start)) · \(next.title)")
                                .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .combine)
            .accessibilityHint("Watch")
            .accessibilityIdentifier("live.channel.\(channel.id)")
            FavoriteButton(favorite: channel.favorite) { model.toggleFavorite(channel) }
                .accessibilityIdentifier("live.channel.\(channel.id).favorite")
        }
        .padding(.vertical, 4)
    }
}

/// The star: Add to favourites, or Remove from favourites.
struct FavoriteButton: View {
    let favorite: Bool
    let toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            Image(systemName: favorite ? "star.fill" : "star")
                .foregroundStyle(favorite ? Color.yellow : Color.secondary)
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(favorite ? "Remove from favourites" : "Add to favourites")
        .accessibilityAddTraits(favorite ? .isSelected : [])
    }
}

/// A channel's logo, asked for at the size it shows; its initial until it comes.
struct ChannelLogo: View {
    let app: AppModel
    let channel: LiveChannel
    let size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.12))
            Text(String(channel.name.prefix(1))).font(.headline).foregroundStyle(.secondary)
            if channel.logo != nil {
                ServerImage(app: app, path: channel.logo, width: size, placeholder: false)
                    .scaledToFit()
                    .padding(3)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

// MARK: A programme

/// A programme of the guide: its picture, its time, Live and New, what
/// it is about, and Watch now while it is on.
private struct ProgramSheet: View {
    let app: AppModel
    @ObservedObject var model: LiveTvModel
    let channel: LiveChannel
    let program: LiveProgram
    let watch: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if program.image != nil {
                        ServerImage(app: app, path: program.image, width: 480)
                            .aspectRatio(16 / 9, contentMode: .fit)
                            .clipShape(RoundedRectangle(cornerRadius: 10))
                    }
                    Text(program.title.isEmpty ? "No information for this time" : program.title)
                        .font(.title2.bold())
                    if let label = program.episodeLabel {
                        Text(label).foregroundStyle(.secondary)
                    }
                    HStack(spacing: 8) {
                        Text("\(channel.numberedName) · \(LiveTvRules.span(program, now: model.now))")
                            .foregroundStyle(.secondary)
                        if program.isLive { badge("Live") }
                        if program.isNew { badge("New") }
                    }
                    .font(.subheadline)
                    if program.airing(at: model.now) {
                        Text(LiveTvRules.leftText(program, now: model.now)).font(.subheadline)
                        Button(action: watch) {
                            Label("Watch now", systemImage: "play.fill").frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .accessibilityIdentifier("live.program.watch")
                    } else if program.stop <= model.now {
                        Text("It has ended.").foregroundStyle(.secondary)
                    }
                    Text(program.description ?? "The guide says nothing more of it.")
                        .foregroundStyle(program.description == nil ? .secondary : .primary)
                }
                .padding()
                .frame(maxWidth: 640, alignment: .leading)
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private func badge(_ text: String) -> some View {
        Text(text)
            .font(.caption.bold())
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Color.red.opacity(0.85), in: Capsule())
            .foregroundStyle(.white)
    }
}
