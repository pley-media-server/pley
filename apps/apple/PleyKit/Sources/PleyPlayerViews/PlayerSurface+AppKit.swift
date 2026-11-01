#if os(macOS)
import AppKit
import AVKit
import PleyCaps
import PleyPlayer
import SwiftUI

/// The Mac's player: AVKit's `AVPlayerView` with floating controls and a
/// full-screen button, showing the controller's AVPlayer.
///
/// - Its action menu (the controls' "…") has Quality, each choice with its
///   second line, Audio, Chapters and Details.
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
        return view
    }

    public func updateNSView(_ view: PleyPlayerView, context: Context) {
        view.controller = controller
        view.caps = caps
        if view.player !== controller.player { view.player = controller.player }
        if view.shownState != state {
            view.shownState = state
            view.actionPopUpButtonMenu = PlayerMenus.actionMenu(state, controller: controller)
        }
    }

    public static func dismantleNSView(_ view: PleyPlayerView, coordinator: ()) {
        view.stop()
        view.player = nil
    }
}

/// `AVPlayerView` with the window's keys, its screen and the Mac's sleep.
public final class PleyPlayerView: AVPlayerView {
    weak var controller: PlaybackController?
    weak var caps: DeviceCaps?
    var shownState: PlayerState?
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

    public override func layout() {
        super.layout()
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
        }
        return true
    }
}

/// The Mac's player menus, shared by the action menu and the menu bar's Playback menu.
public enum PlayerMenus {
    /// Quality (the one playing checked), Audio (when there is a choice), Chapters and Details.
    @MainActor
    public static func actionMenu(_ state: PlayerState, controller: PlaybackController) -> NSMenu {
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
        return menu
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
