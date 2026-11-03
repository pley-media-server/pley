import PleyModel
import PleyPlayer
import PleyPlayerViews
import PleyScreens
import SwiftUI

/// The live player on iPhone and iPad (over everything), and in a window
/// of its own on the Mac: the channel's picture, and over it, for 3
/// seconds after a tap, Close, the channel's logo, number and name, what's
/// on with its progress and what's next, the star, channel up and down,
/// the Channels list, AirPlay and "Live". Live can't pause or seek. Digits
/// on a keyboard tune a number, ↑ and ↓ change channel.
struct LivePlayerScreen: View {
    let app: AppModel
    @ObservedObject var model: LivePlayerModel
    @ObservedObject var playback: LivePlayback
    let close: () -> Void
    @State private var controlsShown = true
    @State private var hiding: Task<Void, Never>?
    @State private var channelsShown = false
    @State private var stalled = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(app: AppModel, model: LivePlayerModel, close: @escaping () -> Void) {
        self.app = app
        self.model = model
        playback = model.playback
        self.close = close
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            LiveVideoSurface(player: AVLiveEngine.player(of: playback.engine))
                .ignoresSafeArea()
                // Taps go to the screen under it, which shows the overlay.
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            if model.state.audioOnly { soundOnly }
            if model.state.phase == .tuning || model.state.buffering {
                ProgressView().controlSize(.large).tint(.white)
                    .accessibilityLabel("Tuning")
            }
            if case .failed(let trouble) = model.state.phase {
                TroubleView(model: model, trouble: trouble, close: close)
            } else if controlsShown {
                overlay
                    .transition(.opacity)
            }
            if !model.digits.isEmpty || model.notice != nil {
                Text(model.notice ?? model.digits)
                    .font(.system(size: 44, weight: .semibold, design: .rounded).monospacedDigit())
                    .padding(.horizontal, 20)
                    .padding(.vertical, 10)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                    .padding(24)
                    .accessibilityIdentifier("live.number")
            }
            keys
        }
        .foregroundStyle(.white)
        .contentShape(Rectangle())
        .onTapGesture { show() }
        .sheet(isPresented: $channelsShown) {
            LiveChannelsSheet(app: app, model: model) { channel in
                channelsShown = false
                model.tune(channel)
                show()
            }
        }
        .onAppear { show() }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("live.player")
        .accessibilityValue(stateText)
        .overlay(alignment: .topLeading) {
            // The checks read the state here.
            Text(stateText)
                .opacity(0)
                .allowsHitTesting(false)
                .accessibilityIdentifier("live.player.state")
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

    private var soundOnly: some View {
        VStack(spacing: 16) {
            if let channel = model.channel {
                ChannelLogo(app: app, channel: channel, size: 120)
            }
            Text("Sound only").font(.headline)
        }
        .accessibilityElement(children: .combine)
    }

    // MARK: Overlay

    private var overlay: some View {
        VStack(spacing: 0) {
            // Close sits on the trailing side: an iPad's window controls take
            // the top-leading corner when the app is in a window.
            HStack(spacing: 16) {
                Text("Live")
                    .font(.caption.bold())
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(Color.red, in: Capsule())
                    .accessibilityIdentifier("live.badge")
                Spacer()
                #if os(iOS) || os(macOS)
                RoutePicker()
                #endif
                Button(action: close) {
                    Image(systemName: "xmark").font(.title2).frame(width: 44, height: 44)
                }
                .accessibilityLabel("Close")
                .accessibilityIdentifier("live.close")
            }
            .padding(.horizontal)
            .padding(.top, 8)
            Spacer()
            HStack(alignment: .bottom, spacing: 16) {
                info
                Spacer(minLength: 0)
                VStack(spacing: 4) {
                    Button { model.zap(by: 1) } label: {
                        Image(systemName: "chevron.up").font(.title2).frame(width: 44, height: 44)
                    }
                    .accessibilityLabel("Channel up")
                    .accessibilityIdentifier("live.channelUp")
                    Button { model.zap(by: -1) } label: {
                        Image(systemName: "chevron.down").font(.title2).frame(width: 44, height: 44)
                    }
                    .accessibilityLabel("Channel down")
                    .accessibilityIdentifier("live.channelDown")
                }
                Button { channelsShown = true } label: {
                    Image(systemName: "list.bullet").font(.title2).frame(width: 44, height: 44)
                }
                .accessibilityLabel("Channels")
                .accessibilityIdentifier("live.channels")
            }
            .padding()
            .background(LinearGradient(colors: [.clear, .black.opacity(0.75)], startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea())
        }
        .buttonStyle(.plain)
    }

    private var info: some View {
        HStack(alignment: .top, spacing: 12) {
            if let channel = model.channel {
                ChannelLogo(app: app, channel: channel, size: 56)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Text(channel.numberedName).font(.headline)
                            .accessibilityIdentifier("live.channelName")
                        FavoriteButton(favorite: channel.favorite) { model.toggleFavorite() }
                            .accessibilityIdentifier("live.favorite")
                    }
                    if let on = channel.on(at: model.now) {
                        Text(on.title.isEmpty ? "On now" : on.title).font(.subheadline)
                        HStack(spacing: 8) {
                            ProgressView(value: on.progress(at: model.now)).frame(maxWidth: 220).tint(.white)
                            Text(LiveTvRules.leftText(on, now: model.now)).font(.caption)
                        }
                        .accessibilityElement(children: .combine)
                    } else {
                        Text(channel.hasGuide ? "Nothing in the guide now" : "No guide for this channel")
                            .font(.subheadline)
                    }
                    if let next = channel.after(at: model.now) {
                        Text("Next \(LiveTvRules.time(next.start)) · \(next.title)")
                            .font(.caption)
                            .opacity(0.8)
                    }
                }
            } else {
                Text("Tuning…").font(.headline)
            }
        }
    }

    /// The overlay shows for 3 seconds after a tap (and stays while VoiceOver reads it).
    private func show() {
        if reduceMotion { controlsShown = true } else { withAnimation(.easeInOut(duration: 0.2)) { controlsShown = true } }
        hiding?.cancel()
        hiding = Task {
            try? await Task.sleep(for: .seconds(3))
            guard !Task.isCancelled, !UIAccessibilityRunning.isOn else { return }
            withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.3)) { controlsShown = false }
        }
    }

    // MARK: Keys

    /// A keyboard's digits, ↑ and ↓ (an iPad's, the Mac's), as hidden buttons.
    private var keys: some View {
        ZStack {
            ForEach(0..<10, id: \.self) { digit in
                Button("\(digit)") { model.type(digit) }
                    .keyboardShortcut(KeyEquivalent(Character(String(digit))), modifiers: [])
            }
            Button("Channel up") { model.zap(by: 1) }
                .keyboardShortcut(.upArrow, modifiers: [])
            Button("Channel down") { model.zap(by: -1) }
                .keyboardShortcut(.downArrow, modifiers: [])
            Button("Tune") { model.enterNumber() }
                .keyboardShortcut(.return, modifiers: [])
        }
        .opacity(0)
        .frame(width: 0, height: 0)
        .accessibilityHidden(true)
    }
}

/// Whether VoiceOver is on: the overlay then stays.
enum UIAccessibilityRunning {
    @MainActor static var isOn: Bool {
        #if os(iOS)
        UIAccessibility.isVoiceOverRunning
        #else
        NSWorkspace.shared.isVoiceOverEnabled
        #endif
    }
}

/// What went wrong, and what can be done: Try again, Watch here instead
/// (every connection in use, one that may be taken), or Close.
struct TroubleView: View {
    @ObservedObject var model: LivePlayerModel
    let trouble: LiveTrouble
    let close: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            Text(trouble.title(channel: model.channel?.name ?? "The channel"))
                .font(.headline)
                .accessibilityIdentifier("live.trouble.title")
            Text(trouble.message).multilineTextAlignment(.center)
            if trouble.kind == .busy {
                VStack(alignment: .leading, spacing: 6) {
                    Text(trouble.connections.count == 1 ? "Its one connection is watching:"
                         : "Its \(trouble.connections.count) connections are watching:")
                        .font(.subheadline)
                    ForEach(trouble.connections, id: \.channelId) { connection in
                        HStack {
                            Text(connection.channelName)
                            Spacer()
                            if connection.canTakeOver {
                                Button("Watch here instead") { model.takeOver(connection.channelId) }
                                    .buttonStyle(.borderedProminent)
                                    .accessibilityIdentifier("live.takeOver")
                            }
                        }
                    }
                }
                .frame(maxWidth: 420)
            }
            HStack(spacing: 12) {
                if trouble.canRetry && trouble.kind != .busy {
                    Button("Try again") { model.retry() }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("live.retry")
                }
                Button("Close", action: close)
                    .buttonStyle(.bordered)
            }
        }
        .padding(24)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
        .foregroundStyle(.primary)
        .padding(24)
        // Over the black player, dark whatever the system's appearance: the
        // material and the words then go together.
        .environment(\.colorScheme, .dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("live.trouble")
        .task(id: spoken) { announce(spoken) }
    }

    /// What VoiceOver reads out as the trouble shows.
    private var spoken: String {
        "\(trouble.title(channel: model.channel?.name ?? "The channel")). \(trouble.message)"
    }
}

/// The Channels list over the player: the category's channels, another
/// category to choose, and a channel to tune.
struct LiveChannelsSheet: View {
    let app: AppModel
    @ObservedObject var model: LivePlayerModel
    let tune: (LiveChannel) -> Void
    @StateObject private var paging: ChannelPaging
    @State private var groups: [LiveGroup] = []
    @Environment(\.dismiss) private var dismiss

    init(app: AppModel, model: LivePlayerModel, tune: @escaping (LiveChannel) -> Void) {
        self.app = app
        self.model = model
        self.tune = tune
        _paging = StateObject(wrappedValue: ChannelPaging(live: model.live))
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(0..<(paging.pages.total ?? 0), id: \.self) { index in
                    Group {
                        if let channel = paging.pages[index] {
                            Button { tune(channel) } label: {
                                HStack(spacing: 12) {
                                    ChannelLogo(app: app, channel: channel, size: 40)
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
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("live.list.\(channel.id)")
                        } else {
                            Color.secondary.opacity(0.08).frame(height: 44)
                        }
                    }
                    .onAppear { paging.showingRows(first: index - 15, last: index + 15) }
                }
            }
            .navigationTitle(LiveTvRules.categoryName(model.category, groups: groups))
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Menu {
                        Button("Favourites") { model.show(.favorites) }
                        Button("Recently watched") { model.show(.recent) }
                        Button("All channels") { model.show(.all) }
                        ForEach(groups) { group in
                            Button(LiveTvRules.groupName(group, name: group.name)) { model.show(.group(group.name)) }
                        }
                    } label: {
                        Label("Other channels", systemImage: "line.3.horizontal.decrease.circle")
                    }
                    .accessibilityIdentifier("live.list.category")
                }
            }
        }
        .task {
            paging.show(model.category)
            groups = (try? await model.live.groups().groups) ?? []
        }
        .onValueChange(of: model.category) { paging.show($0) }
        .accessibilityIdentifier("live.list")
    }
}

#if os(macOS)
/// The live player in a window of its own (⌘W leaves): digits, ↑ and ↓
/// change channel, F fills the screen.
struct LiveWindow: View {
    static let id = "live"

    @ObservedObject var app: AppModel
    let request: LiveRequest?
    @State private var model: LivePlayerModel?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Group {
            if let model {
                LivePlayerScreen(app: app, model: model) { dismiss() }
                    .focusedSceneObject(model)
                    .navigationTitle(model.channel?.numberedName ?? "Live TV")
                    .background {
                        Button("Full Screen") { NSApp.keyWindow?.toggleFullScreen(nil) }
                            .keyboardShortcut("f", modifiers: [])
                            .opacity(0)
                    }
            } else {
                Color.black
            }
        }
        .frame(minWidth: 480, minHeight: 270)
        .task {
            guard let request, let started = await app.watchLive(request) else {
                dismiss()
                return
            }
            model = started
        }
        .onReceive(app.$livePlayer) { current in
            // Another playback started, or this one ended elsewhere: the window goes.
            if let model, current !== model { dismiss() }
        }
        .onDisappear {
            if let model, app.livePlayer === model { app.closeLivePlayer() }
        }
    }
}
#endif
