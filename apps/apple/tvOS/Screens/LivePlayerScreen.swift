import PleyModel
import PleyPlayer
import PleyPlayerViews
import PleyScreens
import SwiftUI

/// The live player on Apple TV. A banner with the channel and what's on
/// shows for 5 seconds when a channel is tuned. Up and down change
/// channel; Select opens the overlay (Channels, Channel number,
/// Favourite, Back to the guide); Play/Pause shows the banner, as live
/// plays on. Menu hides the overlay, then leaves. The number pad tunes a
/// channel on OK, or 1.4 seconds after its last digit. A channel cast from
/// another device says where it came from at the top for 4 seconds.
struct LivePlayerScreen: View {
    let app: AppModel
    @ObservedObject var model: LivePlayerModel
    @ObservedObject var playback: LivePlayback
    @State private var bannerShown = true
    @State private var banner: Task<Void, Never>?
    @State private var overlay = false
    @State private var numberPad = false
    @State private var channelsShown = false
    @FocusState private var screenFocused: Bool

    init(app: AppModel, model: LivePlayerModel) {
        self.app = app
        self.model = model
        playback = model.playback
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            LiveVideoSurface(player: AVLiveEngine.player(of: playback.engine))
                .ignoresSafeArea()
                .allowsHitTesting(false)
            if model.state.audioOnly {
                VStack(spacing: 30) {
                    if let channel = model.channel { TVChannelLogo(app: app, channel: channel, size: 240) }
                    Text("Sound only").font(.title3)
                }
            }
            if model.state.phase == .tuning || model.state.buffering {
                ProgressView().accessibilityLabel("Tuning")
            }
            if case .failed(let trouble) = model.state.phase {
                TVTrouble(model: model, trouble: trouble) { app.closeLivePlayer() }
                    .focusSection()
            } else if numberPad {
                NumberPad(model: model) { numberPad = false }
                    .focusSection()
            } else if overlay {
                overlayView.focusSection()
            } else {
                screen
            }
            if bannerShown && !overlay && !numberPad { bannerView }
            // "Playing from …", as a cast from another device starts.
            if let said = model.banner {
                Text(said)
                    .padding(.horizontal, 30)
                    .padding(.vertical, 14)
                    .background(.regularMaterial, in: Capsule())
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .padding(.top, 60)
                    .allowsHitTesting(false)
                    .accessibilityIdentifier("live.castBanner")
            }
            if !model.digits.isEmpty || model.notice != nil {
                Text(model.notice ?? model.digits)
                    .font(.system(size: 64, weight: .semibold, design: .rounded).monospacedDigit())
                    .padding(.horizontal, 30)
                    .padding(.vertical, 14)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18))
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                    .padding(60)
                    .accessibilityIdentifier("live.number")
            }
            Text(stateText)
                .opacity(0)
                .accessibilityIdentifier("live.player.state")
        }
        .onChange(of: model.channelId) { _, _ in showBanner() }
        .onAppear { showBanner() }
        .fullScreenCover(isPresented: $channelsShown) {
            TVChannelsList(app: app, model: model) { channel in
                channelsShown = false
                overlay = false
                model.tune(channel)
            }
        }
    }

    private var stateText: String {
        switch model.state.phase {
        case .idle: "idle"
        case .tuning: "tuning"
        case .playing: model.state.buffering ? "buffering" : "playing"
        case .failed: "failed"
        }
    }

    /// The picture, holding the focus: up and down zap, Select opens the overlay.
    private var screen: some View {
        Color.clear
            .contentShape(Rectangle())
            .focusable()
            .focused($screenFocused)
            .onAppear { screenFocused = true }
            .onMoveCommand { direction in
                switch direction {
                case .up: model.zap(by: 1)
                case .down: model.zap(by: -1)
                default: break
                }
            }
            .onTapGesture { overlay = true }
            .onPlayPauseCommand { showBanner() }
            .onExitCommand { app.closeLivePlayer() }
            .accessibilityLabel(model.channel?.numberedName ?? localized("Live TV"))
            .accessibilityHint("Up and down change channel. Select shows more.")
            .accessibilityIdentifier("live.player")
    }

    private var bannerView: some View {
        VStack {
            Spacer()
            HStack(alignment: .top, spacing: 30) {
                if let channel = model.channel {
                    TVChannelLogo(app: app, channel: channel, size: 110)
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(spacing: 16) {
                            Text(channel.numberedName).font(.title3.bold())
                                .accessibilityIdentifier("live.channelName")
                            if channel.favorite { Image(systemName: "star.fill").foregroundStyle(.yellow) }
                            Text("Live").font(.caption.bold()).padding(.horizontal, 10).padding(.vertical, 4)
                                .background(Color.red, in: Capsule())
                        }
                        if let on = channel.on(at: model.now) {
                            Text(on.title.isEmpty ? localized("On now") : on.title).font(.headline)
                            HStack(spacing: 16) {
                                ProgressView(value: on.progress(at: model.now)).frame(width: 500)
                                Text(LiveTvRules.leftText(on, now: model.now)).font(.caption)
                            }
                        } else {
                            Text(channel.hasGuide ? localized("Nothing in the guide now") : localized("No guide for this channel"))
                        }
                        if let next = channel.after(at: model.now) {
                            Text("Next \(LiveTvRules.time(next.start)) · \(next.title)").font(.callout)
                                .foregroundStyle(.secondary)
                        }
                    }
                } else {
                    Text("Tuning…").font(.title3)
                }
                Spacer()
            }
            .padding(50)
            .background(LinearGradient(colors: [.clear, .black.opacity(0.85)], startPoint: .top, endPoint: .bottom))
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("live.banner")
    }

    private var overlayView: some View {
        VStack {
            Spacer()
            HStack(spacing: 40) {
                Button { channelsShown = true } label: { Label("Channels", systemImage: "list.bullet") }
                    .accessibilityIdentifier("live.channels")
                Button { overlay = false; numberPad = true } label: {
                    Label("Channel number", systemImage: "number")
                }
                .accessibilityIdentifier("live.numberPad")
                Button { model.toggleFavorite() } label: {
                    Label(model.channel?.favorite == true ? localized("Remove from favourites") : localized("Add to favourites"),
                          systemImage: model.channel?.favorite == true ? "star.fill" : "star")
                }
                .accessibilityIdentifier("live.favorite")
                Button { app.closeLivePlayer() } label: { Label("Back to the guide", systemImage: "rectangle.grid.1x2") }
                    .accessibilityIdentifier("live.close")
            }
            .padding(50)
        }
        .onExitCommand { overlay = false }
    }

    private func showBanner() {
        bannerShown = true
        banner?.cancel()
        banner = Task {
            try? await Task.sleep(for: .seconds(5))
            guard !Task.isCancelled else { return }
            bannerShown = false
        }
    }
}

/// The remote's number pad: 1 to 9, Delete, 0 and OK.
private struct NumberPad: View {
    @ObservedObject var model: LivePlayerModel
    let done: () -> Void
    /// The digits as VoiceOver says them, in the language shown.
    private static var words: [String] {
        [localized("Zero"), localized("One"), localized("Two"), localized("Three"), localized("Four"),
         localized("Five"), localized("Six"), localized("Seven"), localized("Eight"), localized("Nine")]
    }

    var body: some View {
        VStack(spacing: 24) {
            Text(model.digits.isEmpty ? localized("Channel number") : model.digits)
                .font(.title2.monospacedDigit())
            Grid(horizontalSpacing: 24, verticalSpacing: 24) {
                ForEach(0..<3, id: \.self) { row in
                    GridRow {
                        ForEach(1...3, id: \.self) { column in
                            digit(row * 3 + column)
                        }
                    }
                }
                GridRow {
                    Button { model.deleteDigit() } label: { Image(systemName: "delete.left").frame(width: 120, height: 80) }
                        .accessibilityLabel("Delete")
                        .accessibilityIdentifier("live.pad.delete")
                    digit(0)
                    Button {
                        model.enterNumber()
                        done()
                    } label: { Text("OK").frame(width: 120, height: 80) }
                        .accessibilityLabel("Tune")
                        .accessibilityIdentifier("live.pad.ok")
                }
            }
        }
        .padding(50)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 30))
        .onExitCommand(perform: done)
        .onChange(of: model.channelId) { _, _ in done() }
        // A container, so the keys keep their own identifiers.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("live.pad")
    }

    private func digit(_ value: Int) -> some View {
        Button { model.type(value, waits: false) } label: {
            Text(verbatim: "\(value)").font(.title2.monospacedDigit()).frame(width: 120, height: 80)
        }
        .accessibilityLabel(Self.words[value])
        .accessibilityIdentifier("live.pad.\(value)")
    }
}

/// What went wrong: Try again, Watch here instead, or Back to the guide.
private struct TVTrouble: View {
    @ObservedObject var model: LivePlayerModel
    let trouble: LiveTrouble
    let close: () -> Void

    var body: some View {
        VStack(spacing: 30) {
            Text(trouble.title(channel: model.channel?.name ?? localized("The channel"))).font(.title3.bold())
                .accessibilityIdentifier("live.trouble.title")
            Text(trouble.message).multilineTextAlignment(.center)
            if trouble.kind == .busy {
                Text("Its \(trouble.connections.count) connections are watching:")
                ForEach(trouble.connections, id: \.channelId) { connection in
                    HStack(spacing: 30) {
                        Text(connection.channelName)
                        if connection.canTakeOver {
                            Button("Watch here instead") { model.takeOver(connection.channelId) }
                                .accessibilityIdentifier("live.takeOver")
                        }
                    }
                }
            }
            HStack(spacing: 30) {
                if trouble.canRetry && trouble.kind != .busy {
                    Button("Try again") { model.retry() }.accessibilityIdentifier("live.retry")
                }
                Button("Back to the guide", action: close)
            }
        }
        .padding(60)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 24))
        .onExitCommand(perform: close)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("live.trouble")
        .task(id: spoken) { AccessibilityNotification.Announcement(spoken).post() }
    }

    /// What VoiceOver reads out as the trouble shows.
    private var spoken: String {
        "\(trouble.title(channel: model.channel?.name ?? localized("The channel"))). \(trouble.message)"
    }
}

/// The Channels list over the player: the category's channels; another category from Other channels.
private struct TVChannelsList: View {
    let app: AppModel
    @ObservedObject var model: LivePlayerModel
    let tune: (LiveChannel) -> Void
    @StateObject private var paging: ChannelPaging
    @State private var groups: [LiveGroup] = []

    init(app: AppModel, model: LivePlayerModel, tune: @escaping (LiveChannel) -> Void) {
        self.app = app
        self.model = model
        self.tune = tune
        _paging = StateObject(wrappedValue: ChannelPaging(live: model.live))
    }

    var body: some View {
        HStack(alignment: .top, spacing: 60) {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    Text("Other channels").font(.headline).foregroundStyle(.secondary)
                    category(.favorites, localized("Favourites"))
                    category(.recent, localized("Recently watched"))
                    category(.all, localized("All channels"))
                    ForEach(groups) { category(.group($0.name), LiveTvRules.groupName($0, name: $0.name)) }
                }
            }
            .frame(width: 440)
            .focusSection()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    Text(LiveTvRules.categoryName(model.category, groups: groups)).font(.title3.bold())
                    ForEach(0..<(paging.pages.total ?? 0), id: \.self) { index in
                        Group {
                            if let channel = paging.pages[index] {
                                Button { tune(channel) } label: {
                                    HStack(spacing: 20) {
                                        TVChannelLogo(app: app, channel: channel, size: 60)
                                        VStack(alignment: .leading) {
                                            Text(channel.numberedName)
                                            if let on = channel.on(at: model.now) {
                                                Text(on.title).font(.caption).foregroundStyle(.secondary)
                                            }
                                        }
                                        Spacer()
                                        if channel.id == model.channelId {
                                            Image(systemName: "speaker.wave.2.fill").accessibilityLabel("Playing")
                                        }
                                    }
                                }
                                .accessibilityIdentifier("live.list.\(channel.id)")
                            } else {
                                Color.secondary.opacity(0.1).frame(height: 80)
                            }
                        }
                        .onAppear { paging.showingRows(first: index - 15, last: index + 15) }
                    }
                }
            }
            .focusSection()
        }
        .padding(60)
        .task {
            paging.show(model.category)
            groups = (try? await model.live.groups().groups) ?? []
        }
        .onChange(of: model.category) { _, category in paging.show(category) }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("live.list")
    }

    private func category(_ category: LiveCategory, _ title: String) -> some View {
        Button(title) { model.show(category) }
            .accessibilityAddTraits(model.category == category ? .isSelected : [])
            .accessibilityIdentifier("live.list.category.\(category.key)")
    }
}
