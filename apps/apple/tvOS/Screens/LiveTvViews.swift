import PleyModel
import PleyScreens
import SwiftUI

/// Live TV on Apple TV: the categories down the left, and the category's
/// channels as the guide or as cards. The guide shows two hours at a time:
/// moving the focus past its right edge moves it on half an hour, past its
/// left edge back. A channel plays; a programme opens what it is, with
/// Watch now while it is on.
struct LiveTvView: View {
    @ObservedObject var app: AppModel
    @StateObject private var model: LiveTvModel
    @State private var program: TVProgramChoice?

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: app.liveTvModel())
    }

    var body: some View {
        content
            .task { model.start() }
            .onAppear { model.visible(true) }
            .onDisappear { model.visible(false) }
            .onChange(of: app.change) { _, change in
                if change?.live == true || change?.everything == true { model.reload() }
            }
            .fullScreenCover(item: $program) { choice in
                TVProgramView(app: app, model: model, channel: choice.channel, program: choice.program) {
                    program = nil
                    watch(choice.channel)
                }
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
            HStack(alignment: .top, spacing: 40) {
                TVCategories(model: model)
                    .frame(width: 440)
                    .focusSection()
                VStack(alignment: .leading, spacing: 20) {
                    HStack {
                        Text(model.title).font(.title2.bold())
                        Spacer()
                        Picker("View", selection: Binding(get: { model.view }, set: { model.show($0) })) {
                            ForEach(LiveViewKind.allCases, id: \.self) { Text($0.title).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 420)
                        .accessibilityIdentifier("live.view")
                    }
                    .focusSection()
                    channels
                        .focusSection()
                }
            }
            .padding(.horizontal, Metrics.gutter)
        }
    }

    @ViewBuilder private var channels: some View {
        if let error = model.paging.pages.error {
            StateView(kind: .failed(title: "Live TV didn't load", why: error) { model.paging.reload() })
        } else if model.paging.pages.total == 0 {
            StateView(kind: .empty(symbol: "tv", text: "\(model.empty.title). \(model.empty.detail)", action: nil))
        } else if model.paging.pages.total == nil {
            StateView(kind: .loading)
        } else if model.view == .guide {
            TVGuide(app: app, model: model, paging: model.paging, open: watch) { channel, program in
                self.program = TVProgramChoice(channel: channel, program: program)
            }
        } else {
            TVChannelCards(app: app, model: model, paging: model.paging, open: watch)
        }
    }

    private func watch(_ channel: LiveChannel) {
        model.watched(channel.id)
        Task {
            await app.watchLive(LiveRequest(channelId: channel.id, category: model.category), seen: channel)
        }
    }
}

private struct TVProgramChoice: Identifiable {
    let channel: LiveChannel
    let program: LiveProgram
    var id: String { "\(channel.id):\(program.start)" }
}

/// Favourites, Recently watched and All channels, then the regions' groups and the others.
private struct TVCategories: View {
    @ObservedObject var model: LiveTvModel

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                row(.favorites, "Favourites", model.groups.favorites)
                row(.recent, "Recently watched", model.groups.recent)
                row(.all, "All channels", model.groups.total)
                let sections = model.sections
                ForEach(sections.regions) { region in
                    Text(region.region).font(.headline).foregroundStyle(.secondary).padding(.top, 16)
                    ForEach(region.groups) { row(.group($0.name), $0.label, $0.count) }
                }
                if !sections.others.isEmpty {
                    Text(sections.regions.isEmpty ? "Groups" : "More groups")
                        .font(.headline).foregroundStyle(.secondary).padding(.top, 16)
                    ForEach(sections.others) { row(.group($0.name), LiveTvRules.groupName($0, name: $0.name), $0.count) }
                }
            }
            .padding(.vertical, 20)
        }
        .accessibilityIdentifier("live.categoryList")
    }

    private func row(_ category: LiveCategory, _ title: String, _ count: Int) -> some View {
        Button {
            model.show(category)
        } label: {
            HStack {
                Text(title).lineLimit(1)
                Spacer(minLength: 12)
                Text("\(count)").foregroundStyle(.secondary).monospacedDigit()
            }
            .font(.callout)
        }
        .accessibilityLabel("\(title), \(count) channels")
        .accessibilityAddTraits(model.category == category ? .isSelected : [])
        .accessibilityIdentifier("live.category.\(category.key)")
    }
}

/// The guide on Apple TV: two hours across, programmes the focus moves through.
private struct TVGuide: View {
    let app: AppModel
    @ObservedObject var model: LiveTvModel
    @ObservedObject var paging: ChannelPaging
    let open: (LiveChannel) -> Void
    let choose: (LiveChannel, LiveProgram) -> Void
    private let channelWidth: CGFloat = 300
    private let rowHeight: CGFloat = 100

    private var layout: GuideLayout {
        GuideLayout(start: model.window, pointsPerMinute: GuideLayout.pointsPerMinute(textScale: 1, tv: true))
    }

    var body: some View {
        let layout = layout
        VStack(alignment: .leading, spacing: 8) {
            header(layout)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 8) {
                        ForEach(0..<(paging.pages.total ?? 0), id: \.self) { index in
                            row(index, layout).id(index)
                        }
                    }
                    .padding(.vertical, 12)
                }
                .onAppear { land(proxy) }
                .onChange(of: model.landing) { _, _ in land(proxy) }
            }
        }
        .onAppear { model.showingTimes(from: model.window, to: model.window + LiveTvModel.windowSpan) }
        .accessibilityIdentifier("live.guide")
    }

    private func land(_ proxy: ScrollViewProxy) {
        guard let landing = model.landing else { return }
        proxy.scrollTo(landing, anchor: .top)
        model.landed()
    }

    private func header(_ layout: GuideLayout) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(stride(from: model.window, to: model.window + LiveTvModel.windowSpan,
                                 by: Int(LiveTvRules.slot))), id: \.self) { slot in
                Text(LiveTvRules.time(slot))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(width: CGFloat(layout.x(slot + LiveTvRules.slot) - layout.x(slot)), alignment: .leading)
            }
        }
        .padding(.leading, channelWidth + 12)
        .accessibilityHidden(true)
    }

    @ViewBuilder private func row(_ index: Int, _ layout: GuideLayout) -> some View {
        if let channel = paging.pages[index] {
            let to = model.window + LiveTvModel.windowSpan
            let programs = model.programs(of: channel, from: model.window, to: to)
            HStack(spacing: 12) {
                Button { open(channel) } label: {
                    HStack(spacing: 12) {
                        TVChannelLogo(app: app, channel: channel, size: 60)
                        VStack(alignment: .leading) {
                            Text(LiveTvRules.channelName(channel, in: model.category)).font(.callout).lineLimit(1)
                            if let number = channel.number { Text(number).font(.caption).foregroundStyle(.secondary) }
                        }
                        Spacer(minLength: 0)
                    }
                    .frame(width: channelWidth - 40, height: rowHeight - 30)
                }
                .accessibilityLabel("\(channel.name)\(channel.number.map { ", \($0)" } ?? ""), watch")
                .accessibilityIdentifier("live.guide.channel.\(channel.id)")
                if programs.isEmpty {
                    Text(channel.hasGuide ? "Nothing in the guide now" : "No guide for this channel")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .frame(width: CGFloat(layout.width(LiveTvModel.windowSpan)), alignment: .leading)
                } else {
                    HStack(spacing: 0) {
                        ForEach(Array(programs.enumerated()), id: \.element.start) { place, program in
                            programButton(channel, program, layout, first: place == 0,
                                          last: place == programs.count - 1)
                        }
                    }
                }
            }
            .frame(height: rowHeight)
            .onAppear { paging.showingRows(first: index - 10, last: index + 10) }
        } else {
            Color.secondary.opacity(0.1)
                .frame(height: rowHeight)
                .onAppear { paging.showingRows(first: index - 10, last: index + 10) }
        }
    }

    private func programButton(_ channel: LiveChannel, _ program: LiveProgram, _ layout: GuideLayout, first: Bool,
                               last: Bool) -> some View {
        let from = max(program.start, model.window)
        let to = min(program.stop, model.window + LiveTvModel.windowSpan)
        let width = max(CGFloat(layout.x(to) - layout.x(from)) - 8, 40)
        return Button {
            choose(channel, program)
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                Text(program.title.isEmpty ? "No information for this time" : program.title)
                    .font(.callout)
                    .lineLimit(1)
                Text(LiveTvRules.span(program, now: model.now)).font(.caption2).foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(width: width - 32, alignment: .leading)
        }
        .padding(.trailing, 8)
        // Past the window's edges: the window moves half an hour.
        .onMoveCommand { direction in
            if direction == .right && last { model.moveWindow(by: 1) }
            if direction == .left && first { model.moveWindow(by: -1) }
        }
        .accessibilityLabel(LiveTvRules.spoken(program, channel: channel, now: model.now))
        .accessibilityIdentifier("live.guide.program.\(channel.id).\(program.start)")
    }
}

private extension GuideLayout {
    /// The width of `seconds`.
    func width(_ seconds: Int64) -> Double { Double(seconds) / 60 * pointsPerMinute }
}

/// The channels as cards: logo, number and name, what's on and its progress; the star in the context menu.
private struct TVChannelCards: View {
    let app: AppModel
    @ObservedObject var model: LiveTvModel
    @ObservedObject var paging: ChannelPaging
    let open: (LiveChannel) -> Void

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 420), spacing: 40)], spacing: 40) {
                    ForEach(0..<(paging.pages.total ?? 0), id: \.self) { index in
                        Group {
                            if let channel = paging.pages[index] {
                                card(channel)
                            } else {
                                Color.secondary.opacity(0.1).frame(height: 180)
                            }
                        }
                        .id(index)
                        .onAppear { paging.showingRows(first: index - 15, last: index + 15) }
                    }
                }
                .padding(.vertical, 30)
            }
            .onAppear {
                if let landing = model.landing {
                    proxy.scrollTo(landing, anchor: .top)
                    model.landed()
                }
            }
        }
        .accessibilityIdentifier("live.cards")
    }

    private func card(_ channel: LiveChannel) -> some View {
        Button { open(channel) } label: {
            HStack(alignment: .top, spacing: 20) {
                TVChannelLogo(app: app, channel: channel, size: 100)
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(channel.numberedName).font(.headline).lineLimit(1)
                        if channel.favorite { Image(systemName: "star.fill").foregroundStyle(.yellow) }
                    }
                    if let on = channel.on(at: model.now) {
                        Text(on.title).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                        ProgressView(value: on.progress(at: model.now)).accessibilityHidden(true)
                    } else {
                        Text(channel.hasGuide ? "Nothing in the guide now" : "No guide for this channel")
                            .font(.callout).foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 0)
            }
            .frame(height: 140)
        }
        .contextMenu {
            Button(channel.favorite ? "Remove from favourites" : "Add to favourites") { model.toggleFavorite(channel) }
                .accessibilityIdentifier("live.channel.\(channel.id).favorite")
        }
        .accessibilityIdentifier("live.channel.\(channel.id)")
    }
}

/// A channel's logo at the size it shows; its initial until it comes.
struct TVChannelLogo: View {
    let app: AppModel
    let channel: LiveChannel
    let size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10).fill(Color.secondary.opacity(0.15))
            Text(String(channel.name.prefix(1))).font(.title3).foregroundStyle(.secondary)
            if channel.logo != nil {
                ServerImage(app: app, path: channel.logo, width: size, placeholder: false)
                    .scaledToFit()
                    .padding(6)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// A programme: its picture, time, Live and New, what it is about, and Watch now while it is on.
private struct TVProgramView: View {
    let app: AppModel
    @ObservedObject var model: LiveTvModel
    let channel: LiveChannel
    let program: LiveProgram
    let watch: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 60) {
            if program.image != nil {
                ServerImage(app: app, path: program.image, width: 720)
                    .aspectRatio(16 / 9, contentMode: .fit)
                    .frame(width: 720)
                    .clipShape(RoundedRectangle(cornerRadius: 16))
            }
            VStack(alignment: .leading, spacing: 20) {
                Text(program.title.isEmpty ? "No information for this time" : program.title).font(.title2.bold())
                if let label = program.episodeLabel { Text(label).foregroundStyle(.secondary) }
                HStack(spacing: 16) {
                    Text("\(channel.numberedName) · \(LiveTvRules.span(program, now: model.now))")
                        .foregroundStyle(.secondary)
                    if program.isLive { badge("Live") }
                    if program.isNew { badge("New") }
                }
                .font(.callout)
                Text(program.description ?? "The guide says nothing more of it.")
                    .foregroundStyle(program.description == nil ? .secondary : .primary)
                if program.airing(at: model.now) {
                    Button(action: watch) { Label("Watch now", systemImage: "play.fill") }
                        .accessibilityIdentifier("live.program.watch")
                } else if program.stop <= model.now {
                    Text("It has ended.").foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: 900, alignment: .leading)
        }
        .padding(80)
    }

    private func badge(_ text: String) -> some View {
        Text(text).font(.caption.bold()).padding(.horizontal, 10).padding(.vertical, 4)
            .background(Color.red, in: Capsule())
    }
}
