import PleyPlayer
import PleyPlayerViews
import PleyScreens
import SwiftUI

/// The mini player, while music has a queue (docs/APPLE.md, Music): the
/// album art, the title and artist, Play/Pause and Next, and a thin line
/// of how far the track is. A tap opens Now Playing (a sheet on iPhone
/// and iPad, a window on the Mac). VoiceOver reads it as one element
/// ("Now playing, Song, Artist") with Play/Pause, Next and Open as
/// actions; at the accessibility text sizes its words and buttons stack,
/// except in the tab bar's accessory, which keeps one line and, as the tab
/// bar's own labels, stops growing at the largest size below those.
struct MiniPlayer: View {
    let app: AppModel
    @ObservedObject var music: MusicPlayback
    /// In the tab bar's accessory, which draws its own background.
    var inAccessory = false
    @Environment(\.dynamicTypeSize) private var typeSize
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    init(app: AppModel, inAccessory: Bool = false) {
        self.app = app
        music = app.music
        self.inAccessory = inAccessory
    }

    var body: some View {
        let page = NowPlayingModel(music.state)
        VStack(spacing: 0) {
            content(page)
            if !inAccessory { MusicProgressLine(progress: music.progress) }
        }
        .background { if !inAccessory { Rectangle().fill(.bar) } }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(page.spoken)
        .accessibilityHint("Opens Now Playing")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { open() }
        .accessibilityAction(named: page.playing ? Text("Pause") : Text("Play")) { music.toggle() }
        .accessibilityActions {
            if page.hasNext { Button("Next") { music.next() } }
        }
        .accessibilityIdentifier("music.mini")
    }

    @ViewBuilder private func content(_ page: NowPlayingModel) -> some View {
        // The tab bar's accessory has a height of its own: one line there.
        if typeSize.isAccessibilitySize && !inAccessory {
            VStack(alignment: .leading, spacing: 8) {
                Button(action: open) { words(page, lines: 3) }
                    .buttonStyle(.plain)
                HStack(spacing: 16) {
                    art(page, size: 56)
                    Spacer(minLength: 0)
                    buttons(page)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
        } else {
            HStack(spacing: 12) {
                Button(action: open) {
                    HStack(spacing: 12) {
                        art(page, size: inAccessory ? 32 : 44)
                        words(page, lines: 1)
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                buttons(page)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, inAccessory ? 0 : 8)
            // Larger, its words and symbols would spill out of the accessory.
            .dynamicTypeSize(...(inAccessory ? DynamicTypeSize.xxxLarge : .accessibility5))
        }
    }

    private func art(_ page: NowPlayingModel, size: CGFloat) -> some View {
        ServerImage(app: app, path: page.artPath, width: size, symbol: "music.note")
            .frame(width: size, height: size)
            .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private func words(_ page: NowPlayingModel, lines: Int) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(page.title)
                .font(.subheadline.weight(.semibold))
                .lineLimit(lines)
                .accessibilityIdentifier("music.mini.title")
            Text(page.error ?? page.byline)
                .font(.footnote)
                .foregroundStyle(page.error == nil ? Color.secondary : Color.red)
                .lineLimit(lines)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func buttons(_ page: NowPlayingModel) -> some View {
        HStack(spacing: 4) {
            Button { music.toggle() } label: {
                Image(systemName: page.playing ? "pause.fill" : "play.fill")
                    .font(.title2)
                    .frame(minWidth: 44, minHeight: 44)
                    .contentShape(Rectangle())
            }
            .accessibilityLabel(page.playing ? Text("Pause") : Text("Play"))
            .accessibilityIdentifier("music.mini.toggle")
            Button { music.next() } label: {
                Image(systemName: "forward.fill")
                    .font(.title3)
                    .frame(minWidth: 44, minHeight: 44)
                    .contentShape(Rectangle())
            }
            .disabled(!page.hasNext)
            .accessibilityLabel("Next")
            .accessibilityIdentifier("music.mini.next")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
    }

    private func open() {
        #if os(macOS)
        openWindow(id: NowPlayingWindow.id)
        #else
        app.shell.nowPlayingOpen = true
        #endif
    }
}

/// How far the track is: a thin line under the mini player.
private struct MusicProgressLine: View {
    @ObservedObject var progress: MusicProgress

    var body: some View {
        GeometryReader { geometry in
            Rectangle()
                .fill(Color.accentColor)
                .frame(width: geometry.size.width * progress.fraction)
        }
        .frame(height: 2)
        .accessibilityHidden(true)
    }
}

/// Now Playing (docs/APPLE.md, Music): the album art (600 px at most), the
/// title with links to the artist and the album, a scrubber with the time
/// played and left, Previous, Play/Pause and Next, Shuffle, the sleep
/// timer, AirPlay, and Up next (a tap plays that track). A sheet on iPhone
/// and iPad, a window of its own on the Mac.
struct NowPlayingView: View {
    let app: AppModel
    @ObservedObject var music: MusicPlayback
    /// Closes the sheet (iPhone, iPad); `nil` in the Mac's window.
    var close: (() -> Void)?
    @Environment(\.dynamicTypeSize) private var typeSize

    init(app: AppModel, close: (() -> Void)? = nil) {
        self.app = app
        music = app.music
        self.close = close
    }

    var body: some View {
        let page = NowPlayingModel(music.state)
        Group {
            if music.state.active {
                ScrollView {
                    VStack(spacing: 20) {
                        art(page)
                        heading(page)
                        Scrubber(music: music, progress: music.progress)
                        controls(page)
                        extras(page)
                        upNext(page)
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 16)
                    .frame(maxWidth: 640)
                    .frame(maxWidth: .infinity)
                }
            } else {
                Text("Nothing is playing")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle("Now Playing")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let close {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done", action: close).accessibilityIdentifier("music.nowPlaying.done")
                }
            }
        }
        #endif
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("music.nowPlaying")
    }

    private func art(_ page: NowPlayingModel) -> some View {
        let side: CGFloat = typeSize.isAccessibilitySize ? 200 : 300
        return ServerImage(app: app, paths: page.artPath.map { [$0] } ?? [], width: side, symbol: "music.note",
                           limit: 600)
            .frame(width: side, height: side)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .shadow(radius: 8, y: 3)
            .accessibilityHidden(true)
    }

    private func heading(_ page: NowPlayingModel) -> some View {
        VStack(spacing: 4) {
            Text(page.title)
                .font(.title2.bold())
                .multilineTextAlignment(.center)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("music.nowPlaying.title")
            if let artist = page.artist {
                link(artist, id: page.artistId.map(Screen.artist), identifier: "music.nowPlaying.artist")
                    .font(.title3)
            }
            if let album = page.album {
                link(album, id: page.albumId.map { Screen.album($0, track: music.state.track?.id) },
                     identifier: "music.nowPlaying.album")
                    .foregroundStyle(.secondary)
            }
            if let error = page.error {
                Text(error)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
                    .accessibilityIdentifier("music.nowPlaying.error")
            }
        }
    }

    @ViewBuilder private func link(_ text: String, id screen: Screen?, identifier: String) -> some View {
        if let screen {
            Button(text) {
                close?()
                app.shell.open(screen)
            }
            .buttonStyle(.plain)
            .foregroundStyle(Color.accentColor)
            .multilineTextAlignment(.center)
            .accessibilityHint("Opens its page")
            .accessibilityIdentifier(identifier)
        } else {
            Text(text).multilineTextAlignment(.center)
        }
    }

    private func controls(_ page: NowPlayingModel) -> some View {
        HStack(spacing: 28) {
            Button { music.previous() } label: {
                Image(systemName: "backward.fill").font(.title)
            }
            .accessibilityLabel("Previous")
            .accessibilityIdentifier("music.previous")
            Button { music.toggle() } label: {
                ZStack {
                    if page.loading {
                        ProgressView().controlSize(.large)
                    } else {
                        Image(systemName: page.playing ? "pause.circle.fill" : "play.circle.fill")
                            .font(.system(size: 64))
                    }
                }
                .frame(width: 72, height: 72)
            }
            .accessibilityLabel(page.playing ? Text("Pause") : Text("Play"))
            .accessibilityIdentifier("music.toggle")
            Button { music.next() } label: {
                Image(systemName: "forward.fill").font(.title)
            }
            .disabled(!page.hasNext)
            .accessibilityLabel("Next")
            .accessibilityIdentifier("music.next")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
    }

    /// Shuffle, the sleep timer and AirPlay; under each other when the text is large.
    private func extras(_ page: NowPlayingModel) -> some View {
        let stack = typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(spacing: 12))
            : AnyLayout(HStackLayout(spacing: 20))
        return VStack(spacing: 10) {
            stack {
                Toggle(isOn: Binding(get: { page.shuffle }, set: { music.setShuffle($0) })) {
                    Label("Shuffle", systemImage: "shuffle")
                }
                .toggleStyle(.button)
                .accessibilityIdentifier("music.shuffle")
                SleepMenu(music: music, page: page)
                RoutePicker(video: false, id: "music.airplay")
            }
            if let notice = page.sleepNotice {
                HStack(spacing: 12) {
                    Text(notice).monospacedDigit()
                    Button("Keep listening") { music.keepListening() }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("music.keepListening")
                }
                .accessibilityElement(children: .contain)
            }
        }
    }

    @ViewBuilder private func upNext(_ page: NowPlayingModel) -> some View {
        if !page.upNext.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Text("Up next")
                    .font(.headline)
                    .padding(.bottom, 6)
                    .accessibilityAddTraits(.isHeader)
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(page.upNext) { row in
                        Button { music.jump(to: row.position) } label: {
                            HStack(spacing: 12) {
                                if !typeSize.isAccessibilitySize {
                                    ServerImage(app: app, path: row.artPath, width: 40, symbol: "music.note")
                                        .frame(width: 40, height: 40)
                                        .clipShape(RoundedRectangle(cornerRadius: 4))
                                }
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(row.title).lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                                    if !row.byline.isEmpty {
                                        Text(row.byline)
                                            .font(.footnote)
                                            .foregroundStyle(.secondary)
                                            .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                                    }
                                }
                                Spacer(minLength: 8)
                                Text(row.duration).font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 8)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(row.spoken)
                        .accessibilityHint("Plays it")
                        .accessibilityAddTraits(.isButton)
                        .accessibilityIdentifier("music.upNext.\(row.position)")
                        Divider()
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// The scrubber: an adjustable slider (VoiceOver moves it 10 s at a time),
/// with the time played and left. Moving it seeks once it is let go.
private struct Scrubber: View {
    let music: MusicPlayback
    @ObservedObject var progress: MusicProgress
    @State private var dragging: Double?

    var body: some View {
        let duration = max(progress.durationMs, 1)
        let shown = dragging ?? Double(progress.positionMs)
        VStack(spacing: 4) {
            Slider(value: Binding(get: { shown }, set: { dragging = $0 }), in: 0...Double(duration)) { editing in
                guard !editing, let target = dragging else { return }
                music.seek(toMs: Int(target))
                dragging = nil
            }
            .disabled(progress.durationMs <= 0)
            .accessibilityLabel("Position")
            .accessibilityValue(NowPlayingModel.spokenTime(Int(shown), progress.durationMs))
            .accessibilityAdjustableAction { direction in
                let step = 10_000
                switch direction {
                case .increment: music.seek(toMs: min(progress.positionMs + step, progress.durationMs))
                case .decrement: music.seek(toMs: max(progress.positionMs - step, 0))
                @unknown default: break
                }
            }
            .accessibilityIdentifier("music.scrubber")
            HStack {
                Text(NowPlayingModel.elapsed(Int(shown)))
                Spacer()
                Text(NowPlayingModel.left(Int(shown), progress.durationMs))
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
            .accessibilityHidden(true)
        }
    }
}

/// The sleep timer: Off, 15 to 90 minutes, or the end of this track, with
/// what is left beside it.
struct SleepMenu: View {
    let music: MusicPlayback
    let page: NowPlayingModel

    var body: some View {
        Menu {
            Picker("Sleep Timer", selection: Binding(get: { page.sleep }, set: { music.setSleep($0) })) {
                ForEach(SleepChoice.all) { choice in
                    Text(choice.label(item: "track")).tag(choice)
                }
            }
        } label: {
            Label(page.sleepSummary ?? localized("Sleep Timer"), systemImage: page.sleep == .off ? "moon" : "moon.fill")
                .monospacedDigit()
        }
        .accessibilityLabel("Sleep timer")
        .accessibilityValue(page.sleepSummary
                            ?? localized(keyed: LocalizedStringResource("sleepTimer.off", defaultValue: "Off")))
        .accessibilityIdentifier("music.sleep")
    }
}

#if os(macOS)
/// The Mac's Now Playing: a window of its own, from the mini player and the
/// Window menu (⌥⌘P).
struct NowPlayingWindow: Scene {
    static let id = "now-playing"
    /// Observed, so the window's title follows the language shown.
    @ObservedObject var app: AppModel

    var body: some Scene {
        Window(localized("Now Playing"), id: Self.id) {
            InLanguage(app: app) {
                NowPlayingView(app: app)
                    .frame(minWidth: 360, idealWidth: 420, minHeight: 520, idealHeight: 720)
            }
        }
        .keyboardShortcut("p", modifiers: [.command, .option])
        .defaultSize(width: 420, height: 720)
        .restorationBehavior(.disabled)
    }
}
#endif
