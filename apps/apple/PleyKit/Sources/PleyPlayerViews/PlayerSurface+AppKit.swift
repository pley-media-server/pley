#if os(macOS)
import AppKit
import AVKit
import PleyCaps
import PleyPlayer
import PleySubtitles
import SwiftUI

/// The Mac's player: AVKit's `AVPlayerView` with floating controls and a
/// full-screen button, showing the controller's AVPlayer.
///
/// - Its action menu (the controls' "…") has Quality, each choice with its
///   second line, Audio, Subtitles, Subtitle sync, Chapters and Details.
///   The menu is set once; its items are made from the controller's state
///   each time it opens (`ActionMenuSource`), so the state changing while
///   it plays doesn't touch it.
/// - The subtitles are drawn over the picture, in its content overlay
///   (`SubtitleView`), placed in the video's bounds.
/// - Automatic plays no stream size bigger than the view, in pixels.
/// - The web player's keys work while its window is key (`PlayerKeys`).
/// - When its window moves to another screen, or the screens change, the
///   device's caps are read again for that screen (its size and HDR), for
///   the next session.
/// - The Mac going to sleep pauses, and the pause is reported.
public struct PlayerSurface: NSViewRepresentable {
    let controller: PlaybackController
    let state: PlayerState
    let caps: DeviceCaps?

    public init(controller: PlaybackController, state: PlayerState, caps: DeviceCaps?) {
        self.controller = controller
        self.state = state
        self.caps = caps
    }

    public func makeNSView(context: Context) -> PleyPlayerView {
        let view = PleyPlayerView()
        view.controlsStyle = .floating
        view.showsFullScreenToggleButton = true
        view.allowsPictureInPicturePlayback = false
        view.addSubtitles()
        view.addActionMenu()
        return view
    }

    public func updateNSView(_ view: PleyPlayerView, context: Context) {
        view.controller = controller
        view.caps = caps
        if view.player !== controller.player { view.player = controller.player }
        view.subtitles?.shown = state.cues
    }

    public static func dismantleNSView(_ view: PleyPlayerView, coordinator: ()) {
        view.stop()
        view.stopFollowingVideo()
        view.player = nil
    }
}

/// `AVPlayerView` with the window's keys, its screen and the Mac's sleep.
public final class PleyPlayerView: AVPlayerView {
    weak var controller: PlaybackController? {
        didSet { actionMenuSource.controller = controller }
    }
    weak var caps: DeviceCaps?
    private(set) var subtitles: SubtitleView?
    /// Makes the action menu's items when it opens.
    let actionMenuSource = ActionMenuSource()
    private var boundsWatch: NSKeyValueObservation?
    private var monitor: Any?
    private var tokens: [any NSObjectProtocol] = []

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        stop()
        guard let window else { return }
        let center = NotificationCenter.default
        tokens = [
            center.addObserver(forName: NSWindow.didChangeScreenNotification, object: window, queue: .main) {
                [weak self] _ in
                Task { @MainActor in self?.screenChanged() }
            },
            center.addObserver(forName: NSApplication.didChangeScreenParametersNotification, object: nil,
                               queue: .main) { [weak self] _ in
                Task { @MainActor in self?.screenChanged() }
            },
            NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.willSleepNotification, object: nil,
                                                              queue: .main) { [weak self] _ in
                Task { @MainActor in self?.controller?.pause() }
            },
        ]
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let (characters, flags, number) = (event.charactersIgnoringModifiers, event.modifierFlags,
                                               event.windowNumber)
            let handled = MainActor.assumeIsolated {
                guard let self, let window = self.window, number == window.windowNumber, window.isKeyWindow else {
                    return false
                }
                return self.pressed(characters, flags)
            }
            return handled ? nil : event
        }
        screenChanged()
    }

    /// The subtitles over the video, kept in its bounds as the view and the video change size.
    func addSubtitles() {
        guard subtitles == nil, let overlay = contentOverlayView else { return }
        let view = SubtitleView(frame: overlay.bounds)
        overlay.addSubview(view)
        subtitles = view
        boundsWatch = observe(\.videoBounds, options: [.new]) { [weak self] _, _ in
            Task { @MainActor in self?.placeSubtitles() }
        }
    }

    /// The action menu, set once: its delegate fills it as it opens.
    func addActionMenu() {
        let menu = NSMenu(title: "Player")
        menu.delegate = actionMenuSource
        actionPopUpButtonMenu = menu
    }

    /// Ends the watch on `videoBounds` while the view still has its player
    /// layer: AVKit watches the layer for it, and the layer goes before the
    /// view's own properties as the view is freed, which AppKit stops the
    /// app for.
    func stopFollowingVideo() {
        boundsWatch?.invalidate()
        boundsWatch = nil
    }

    private func placeSubtitles() {
        guard let subtitles else { return }
        subtitles.video = subtitles.convert(videoBounds, from: self)
    }

    public override func layout() {
        super.layout()
        placeSubtitles()
        // The player's size in pixels: Automatic plays no stream size bigger.
        let pixels = convertToBacking(bounds).size
        controller?.setViewPixelSize(width: Int(pixels.width), height: Int(pixels.height))
    }

    /// Lets go of the observers and the key monitor.
    func stop() {
        tokens.forEach { NotificationCenter.default.removeObserver($0) }
        tokens.forEach { NSWorkspace.shared.notificationCenter.removeObserver($0) }
        tokens = []
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    private func screenChanged() {
        guard let screen = window?.screen else { return }
        caps?.screenChanged(ScreenFacts(screen))
    }

    /// Acts on a key of `PlayerKeys`; false for any other.
    private func pressed(_ characters: String?, _ flags: NSEvent.ModifierFlags) -> Bool {
        let command = !flags.intersection([.command, .control, .option]).isEmpty
        guard let characters,
              let action = PlayerKeys.action(characters, command: command), let controller
        else { return false }
        switch action {
        case .playPause:
            switch controller.state.phase {
            case .playing, .buffering: controller.pause()
            default: controller.play()
            }
        case .seek(let delta):
            controller.seek(byMs: delta)
        case .volume(let step):
            guard let player else { return false }
            player.isMuted = false
            player.volume = min(max(player.volume + step, 0), 1)
        case .mute:
            player?.isMuted.toggle()
        case .fullScreen:
            window?.toggleFullScreen(nil)
        case .leaveFullScreen:
            guard let window, window.styleMask.contains(.fullScreen) else { return false }
            window.toggleFullScreen(nil)
        case .skip:
            guard controller.state.skipLabel != nil else { return false }
            controller.skip()
        case .nextSubtitle:
            guard !controller.state.subtitles.isEmpty else { return false }
            controller.cycleSubtitles()
        }
        return true
    }
}

/// Fills the player's action menu as it opens, from the state then: the
/// menu itself is made once, and the state changing while it plays (every
/// half second) costs nothing until the menu is opened again.
@MainActor
public final class ActionMenuSource: NSObject, NSMenuDelegate {
    weak var controller: PlaybackController?
    /// The state the items are made from: the controller's, or a test's.
    var state: () -> PlayerState?

    init(state: (() -> PlayerState?)? = nil) {
        self.state = { nil }
        super.init()
        self.state = state ?? { [weak self] in self?.controller?.state }
    }

    public func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        guard let state = state() else { return }
        for item in PlayerMenus.actionItems(state, controller: controller) {
            menu.addItem(item)
        }
    }
}

/// The Mac's player menus, shared by the action menu and the menu bar's Playback menu.
public enum PlayerMenus {
    /// Quality (the one playing checked), Audio (when there is a choice),
    /// Subtitles and their sync (when the file has some), Chapters and Details.
    @MainActor
    public static func actionMenu(_ state: PlayerState, controller: PlaybackController?) -> NSMenu {
        let menu = NSMenu()
        actionItems(state, controller: controller).forEach { menu.addItem($0) }
        return menu
    }

    /// The action menu's items for `state`.
    @MainActor
    static func actionItems(_ state: PlayerState, controller: PlaybackController?) -> [NSMenuItem] {
        let menu = NSMenu()
        let quality = NSMenuItem(title: "Quality", action: nil, keyEquivalent: "")
        quality.submenu = NSMenu(title: "Quality")
        for choice in state.qualities {
            let item = ClosureMenuItem(title: choice.label) { [weak controller] in
                controller?.selectQuality(choice.id)
            }
            item.state = choice.selected ? .on : .off
            item.subtitle = choice.detail
            quality.submenu?.addItem(item)
        }
        menu.addItem(quality)
        if state.audio.count > 1 {
            menu.addItem(submenu("Audio", state.audio.map { choice in
                let item = ClosureMenuItem(title: choice.label) { [weak controller] in
                    if let index = Int(choice.id) { controller?.selectAudio(index) }
                }
                item.state = choice.selected ? .on : .off
                return item
            }))
        }
        if !state.subtitles.isEmpty {
            menu.addItem(subtitlesMenu(state, controller: controller))
            if state.canSyncSubtitles { menu.addItem(syncMenu(state, controller: controller)) }
        }
        if !state.chapters.isEmpty {
            menu.addItem(submenu("Chapters", state.chapters.map { chapter in
                let item = ClosureMenuItem(title: chapter.label) { [weak controller] in
                    controller?.jumpToChapter(chapter.index)
                }
                item.state = chapter.current ? .on : .off
                return item
            }))
        }
        let details = NSMenuItem(title: "Details", action: nil, keyEquivalent: "")
        details.submenu = NSMenu(title: "Details")
        for line in PlayerDetails.lines(state) {
            let item = NSMenuItem(title: "\(line.title): \(line.text)", action: nil, keyEquivalent: "")
            item.isEnabled = false
            details.submenu?.addItem(item)
        }
        menu.addItem(details)
        let items = menu.items
        menu.removeAllItems()
        return items
    }

    /// Subtitles: Off and each track, the one shown checked.
    @MainActor
    public static func subtitlesMenu(_ state: PlayerState, controller: PlaybackController?) -> NSMenuItem {
        submenu("Subtitles", state.subtitles.map { choice in
            let item = ClosureMenuItem(title: choice.label) { [weak controller] in
                controller?.selectSubtitle(choice.id == MenuChoice.off ? nil : Int(choice.id))
            }
            item.state = choice.selected ? .on : .off
            item.subtitle = choice.detail
            return item
        })
    }

    /// Subtitle sync: the offset, 0.1 s earlier or later, and back to 0.
    @MainActor
    public static func syncMenu(_ state: PlayerState, controller: PlaybackController?) -> NSMenuItem {
        let offset = state.subtitleOffsetMs
        let now = NSMenuItem(title: "Offset: \(state.subtitleOffsetLabel)", action: nil, keyEquivalent: "")
        now.isEnabled = false
        let reset = ClosureMenuItem(title: "Reset") { [weak controller] in controller?.setSubtitleOffset(0) }
        reset.isEnabled = offset != 0
        return submenu("Subtitle Sync", [
            now,
            ClosureMenuItem(title: "Earlier (−0.1 s)") { [weak controller] in
                controller?.moveSubtitles(steps: -1)
            },
            ClosureMenuItem(title: "Later (+0.1 s)") { [weak controller] in
                controller?.moveSubtitles(steps: 1)
            },
            reset,
        ])
    }

    private static func submenu(_ title: String, _ items: [NSMenuItem]) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.submenu = NSMenu(title: title)
        items.forEach { item.submenu?.addItem($0) }
        return item
    }
}

/// A menu item that runs a closure.
final class ClosureMenuItem: NSMenuItem {
    private let run: () -> Void

    init(title: String, run: @escaping () -> Void) {
        self.run = run
        super.init(title: title, action: #selector(fire), keyEquivalent: "")
        target = self
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not from a nib") }

    @objc private func fire() { run() }
}
#endif
