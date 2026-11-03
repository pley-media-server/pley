import PleyData
import PleyPlayer
import PleyPlayerViews
import PleyScreens
import SwiftUI

#if os(iOS)
/// The player on iPhone and iPad, full screen over the app: the system's
/// player (its own Close ends the playback) with Pley's Options button
/// (Quality, each with its second line, Audio, Subtitles with Find
/// Subtitles…, Subtitle sync, Speed, Sleep Timer, Chapters and Details)
/// under its top row. Options shows with the system's controls (a tap shows it
/// for 3 seconds) and stays while paused; an open menu stays open when
/// the button hides. Skip Intro, Skip Credits and the
/// Up Next card show bottom trailing (`PlayerOverlay`). A notice shows at
/// the top for 4 seconds. Find Subtitles… opens its search in a sheet.
struct PlayerScreen: View {
    let app: AppModel
    @ObservedObject var controller: PlaybackController
    @State private var search: SubtitleSearchModel?
    @State private var controlsUntil = Date.distantPast
    @State private var hideTask: Task<Void, Never>?
    @State private var notice: String?
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            GeometryReader { geometry in
                PlayerSurface(player: controller.player, state: controller.state, onTap: showControls,
                              onQuality: { controller.selectQuality($0) })
                    .onAppear { sized(geometry.size) }
                    .onChange(of: geometry.size) { sized($0) }
            }
            .ignoresSafeArea()
            PlayerOverlay(app: app, controller: controller)
            overlay
        }
        .statusBarHidden()
        .persistentSystemOverlays(.hidden)
        .sheet(item: $search) { model in
            SubtitleSearchView(model: model)
        }
        .onAppear { showControls() }
        .onChange(of: controller.state.notice) { text in
            guard let text else { return }
            notice = text
            controller.dismissNotice()
            Task {
                try? await Task.sleep(for: .seconds(4))
                if notice == text { notice = nil }
            }
        }
    }

    /// The player's size in pixels: Automatic plays no stream size bigger.
    private func sized(_ size: CGSize) {
        controller.setViewPixelSize(width: Int(size.width * displayScale), height: Int(size.height * displayScale))
    }

    private var controlsShown: Bool {
        switch controller.state.phase {
        case .playing, .buffering: return controlsUntil > Date()
        default: return true
        }
    }

    @ViewBuilder private var overlay: some View {
        VStack {
            if let notice {
                Text(notice)
                    .font(.callout)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(.ultraThinMaterial, in: Capsule())
                    .padding(.top, 8)
            }
            Spacer()
        }
        // Always in the view: hidden rather than removed when the controls
        // hide, since removing a Menu closes it while it is open.
        VStack {
            // Below the system's top row (Close, AirPlay, volume).
            HStack {
                Spacer()
                options
            }
            .padding(.horizontal)
            .padding(.top, 52)
            Spacer()
        }
        .opacity(controlsShown ? 1 : 0)
        .allowsHitTesting(controlsShown)
        .animation(.easeInOut(duration: 0.2), value: controlsShown)
        .accessibilityHidden(!controlsShown)
        switch controller.state.phase {
        case .preparing where controller.player?.currentItem == nil:
            ProgressView().controlSize(.large).tint(.white)
        case .failed(let message, let canRetry):
            VStack(spacing: 16) {
                Text(message).multilineTextAlignment(.center)
                HStack(spacing: 12) {
                    if canRetry {
                        Button("Try again") { controller.retry() }.buttonStyle(.borderedProminent)
                    }
                    Button("Close") { Task { await app.closePlayer(controller) } }.buttonStyle(.bordered)
                }
            }
            .padding(24)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
            .padding(32)
        default:
            EmptyView()
        }
    }

    /// UIKit's menu, so a checked Quality keeps its second line, and the
    /// sync's offset moves while its menu is open.
    private var options: some View {
        PlayerOptionsButton(state: controller.state, actions: actions, onTap: showControls)
            .frame(width: 44, height: 44)
    }

    private var actions: PlayerMenus.Actions {
        var actions = PlayerMenus.Actions(controller)
        actions.findSubtitles = { [app, controller] in search = SubtitleSearchModel(app: app, controller: controller) }
        return actions
    }

    private func showControls() {
        controlsUntil = Date().addingTimeInterval(3)
        hideTask?.cancel()
        hideTask = Task {
            try? await Task.sleep(for: .seconds(3.1))
            guard !Task.isCancelled else { return }
            // Read again: the body works out whether they still show.
            controlsUntil = controlsUntil
        }
    }
}
#endif

#if os(macOS)
import AppKit

/// The Mac's player window, one per playback: opened with the play
/// request, it plays it through the app (which closes any playback before,
/// and so that one's window), and closing it ends the session.
struct PlayerWindow: View {
    static let id = "player"

    @ObservedObject var app: AppModel
    let request: PlayRequest?
    @State private var controller: PlaybackController?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Group {
            if let controller {
                PlayerWindowContent(app: app, controller: controller)
                    .focusedSceneObject(controller)
            } else {
                Color.black
            }
        }
        .frame(minWidth: 480, minHeight: 270)
        .task {
            guard let request, let started = await app.play(request) else {
                dismiss()
                return
            }
            controller = started
        }
        .onReceive(app.$player) { current in
            // Another playback started, or this one ended elsewhere (a sign-out): the window goes.
            if let controller, current !== controller { dismiss() }
        }
        .onDisappear {
            if let controller { Task { await app.closePlayer(controller) } }
        }
    }
}

private struct PlayerWindowContent: View {
    let app: AppModel
    @ObservedObject var controller: PlaybackController
    @State private var notice: String?
    @State private var search: SubtitleSearchModel?

    var body: some View {
        ZStack {
            Color.black
            PlayerSurface(controller: controller, state: controller.state, caps: app.caps,
                          onFindSubtitles: findSubtitles)
            PlayerOverlay(app: app, controller: controller)
            VStack {
                if let notice {
                    Text(notice)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 8)
                        .background(.ultraThinMaterial, in: Capsule())
                        .padding(.top, 12)
                }
                Spacer()
            }
            if case .failed(let message, let canRetry) = controller.state.phase {
                VStack(spacing: 16) {
                    Text(message).multilineTextAlignment(.center)
                    if canRetry {
                        Button("Try again") { controller.retry() }.buttonStyle(.borderedProminent)
                    }
                }
                .padding(24)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                .padding(32)
            }
        }
        .navigationTitle(windowTitle)
        .sheet(item: $search) { model in
            SubtitleSearchView(model: model)
        }
        .focusedSceneValue(\.findSubtitles, controller.state.canFindSubtitles ? FindSubtitlesAction(run: findSubtitles) : nil)
        .onChange(of: controller.state.notice) { _, text in
            guard let text else { return }
            notice = text
            controller.dismissNotice()
            Task {
                try? await Task.sleep(for: .seconds(4))
                if notice == text { notice = nil }
            }
        }
    }

    private func findSubtitles() {
        search = SubtitleSearchModel(app: app, controller: controller)
    }

    private var windowTitle: String {
        let state = controller.state
        return [state.title, state.subtitle].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " — ")
    }
}

/// The menu bar's Playback menu: Play (⌘↩) plays the tile with the focus
/// or the title page showing, in a player window; the rest acts on the
/// player window that is key. Its Music section (Play/Pause Music, Next
/// Track ⌘→, Previous Track ⌘←) acts on music while no player window is key.
struct PlaybackCommands: Commands {
    let app: AppModel
    @ObservedObject private var music: MusicPlayback
    @FocusedObject private var player: PlaybackController?
    @FocusedObject private var live: LivePlayerModel?
    @FocusedValue(\.playTarget) private var target
    @FocusedValue(\.findSubtitles) private var findSubtitles
    @Environment(\.openWindow) private var openWindow

    init(app: AppModel) {
        self.app = app
        music = app.music
    }

    var body: some Commands {
        CommandMenu("Playback") {
            Button(target?.menuTitle ?? "Play Title") {
                guard let target else { return }
                Task {
                    if let request = await target.request() { openWindow(id: PlayerWindow.id, value: request) }
                }
            }
            .keyboardShortcut(.return, modifiers: .command)
            .disabled(target == nil)
            Divider()
            Button(isPlaying ? "Pause" : "Play") {
                guard let player else { return }
                if isPlaying { player.pause() } else { player.play() }
            }
            .disabled(player == nil)
            // Live TV's window: channel up and down.
            Button("Channel Up") { live?.zap(by: 1) }
                .keyboardShortcut(.upArrow, modifiers: .command)
                .disabled(live == nil)
            Button("Channel Down") { live?.zap(by: -1) }
                .keyboardShortcut(.downArrow, modifiers: .command)
                .disabled(live == nil)
            Button("Back 10 Seconds") { player?.seek(byMs: -PlayerKeys.seekMs) }
                .disabled(player == nil)
            Button("Forward 10 Seconds") { player?.seek(byMs: PlayerKeys.seekMs) }
                .disabled(player == nil)
            Divider()
            Button(player?.state.skipLabel ?? "Skip Intro") { player?.skip() }
                .disabled(player?.state.skipLabel == nil)
            Button("Play Next") { player?.playNext() }
                .disabled(player?.state.upNext == nil)
            Divider()
            Menu("Quality") {
                ForEach(player?.state.qualities ?? []) { choice in
                    Toggle(choice.label, isOn: Binding(get: { choice.selected },
                                                       set: { _ in player?.selectQuality(choice.id) }))
                }
            }
            .disabled(player == nil)
            Menu("Audio") {
                ForEach(player?.state.audio ?? []) { choice in
                    Toggle(choice.label, isOn: Binding(get: { choice.selected }, set: { _ in
                        if let index = Int(choice.id) { player?.selectAudio(index) }
                    }))
                }
            }
            .disabled((player?.state.audio.count ?? 0) < 2)
            Menu("Subtitles") {
                ForEach(player?.state.subtitles ?? []) { choice in
                    Toggle(choice.label, isOn: Binding(get: { choice.selected }, set: { _ in
                        player?.selectSubtitle(choice.id == MenuChoice.off ? nil : Int(choice.id))
                    }))
                }
            }
            .disabled(player?.state.subtitles.isEmpty ?? true)
            Button("Find Subtitles…") { findSubtitles?.run() }
                .disabled(findSubtitles == nil)
            Menu("Subtitle Sync (\(player?.state.subtitleOffsetLabel ?? "0.0 s"))") {
                Button("Earlier (−0.1 s)") { player?.moveSubtitles(steps: -1) }
                Button("Later (+0.1 s)") { player?.moveSubtitles(steps: 1) }
                Button("Reset") { player?.setSubtitleOffset(0) }
                    .disabled(player?.state.subtitleOffsetMs == 0)
            }
            .disabled(!(player?.state.canSyncSubtitles ?? false))
            Menu("Speed (\(player?.state.speedLabel ?? "1×"))") {
                ForEach(player?.state.speeds ?? []) { choice in
                    Toggle(choice.label, isOn: Binding(get: { choice.selected }, set: { _ in
                        if let speed = PlaybackSpeeds.speed(id: choice.id) { player?.setSpeed(speed) }
                    }))
                }
            }
            .disabled(player == nil)
            Menu(player?.state.sleepSummary.map { "Sleep Timer (\($0))" } ?? "Sleep Timer") {
                ForEach(player?.state.sleepChoices ?? []) { choice in
                    Toggle(choice.label, isOn: Binding(get: { choice.selected }, set: { _ in
                        if let sleep = SleepChoice(id: choice.id) { player?.setSleep(sleep) }
                    }))
                }
                if player?.state.sleep.secondsLeft != nil {
                    Divider()
                    Button("Keep Watching") { player?.keepWatching() }
                }
            }
            .disabled(player == nil)
            Menu("Chapters") {
                ForEach(player?.state.chapters ?? []) { chapter in
                    Toggle(chapter.label, isOn: Binding(get: { chapter.current },
                                                        set: { _ in player?.jumpToChapter(chapter.index) }))
                }
            }
            .disabled(player?.state.chapters.isEmpty ?? true)
            Divider()
            Section("Music") {
                Button(music.state.playing ? "Pause Music" : "Play Music") { music.toggle() }
                    .disabled(!musicKeys)
                Button("Next Track") { music.next() }
                    .keyboardShortcut(.rightArrow, modifiers: .command)
                    .disabled(!musicKeys || !music.state.hasNext)
                Button("Previous Track") { music.previous() }
                    .keyboardShortcut(.leftArrow, modifiers: .command)
                    .disabled(!musicKeys)
                Button("Now Playing") { openWindow(id: NowPlayingWindow.id) }
                    .disabled(!music.state.active)
            }
        }
    }

    /// Music's items act while music has a queue and no player window is key.
    private var musicKeys: Bool { music.state.active && player == nil && live == nil }

    private var isPlaying: Bool {
        switch player?.state.phase {
        case .playing?, .buffering?: true
        default: false
        }
    }
}

/// Quitting waits for the player's last reports (2 seconds at most), so
/// the session ends on the server and the place is kept. Its key monitor
/// takes ⌘+ (⇧⌘=) for Larger Tiles, whose menu item shows ⌘=.
final class MacAppDelegate: NSObject, NSApplicationDelegate {
    weak var app: AppModel?
    private var monitor: Any?

    func applicationDidFinishLaunching(_ notification: Notification) {
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let (characters, flags) = (event.charactersIgnoringModifiers, event.modifierFlags)
            let handled = MainActor.assumeIsolated { self?.largerTiles(characters, flags) ?? false }
            return handled ? nil : event
        }
    }

    /// ⌘+: one size larger, as the View menu's Larger Tiles (⌘=) does.
    @MainActor
    private func largerTiles(_ characters: String?, _ flags: NSEvent.ModifierFlags) -> Bool {
        guard characters == "+", let app, app.root == .signedIn,
              GridDensity.keyStep(characters, command: flags.contains(.command), option: flags.contains(.option),
                                  control: flags.contains(.control)) == 1
        else { return false }
        if let next = app.settings.gridDensity.stepped(by: 1) { app.settings.gridDensity = next }
        return true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let app, app.player != nil || app.music.active else { return .terminateNow }
        Task { @MainActor in
            await app.closePlayer()
            // Music's session ends too: `stopped` keeps the place.
            if app.music.active {
                app.music.stop()
                await app.reporter.flush(timeoutMs: PlaybackController.flushMs, clock: SystemPlayerClock())
            }
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}
#endif
