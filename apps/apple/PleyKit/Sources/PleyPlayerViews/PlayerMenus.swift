#if os(iOS) || os(tvOS)
import PleyPlayer
import UIKit

/// The player's menus as UIKit makes them: on iPhone and iPad behind the
/// Options button, on Apple TV in the transport bar. The choice in use has
/// a checkmark for its picture, not the `.on` state: a checked row would
/// lose its second line ("The file as it is"). VoiceOver hears it as
/// "Original, The file as it is, selected".
@MainActor
public enum PlayerMenus {
    /// What a menu's rows do.
    public struct Actions {
        public var quality: @MainActor (String) -> Void = { _ in }
        public var audio: @MainActor (Int) -> Void = { _ in }
        /// `nil`: Off.
        public var subtitle: @MainActor (Int?) -> Void = { _ in }
        /// The sync, a step of 0.1 s earlier (−1) or later (+1).
        public var moveSubtitles: @MainActor (Int) -> Void = { _ in }
        public var resetSubtitleSync: @MainActor () -> Void = {}
        public var chapter: @MainActor (Int) -> Void = { _ in }
        public var speed: @MainActor (Double) -> Void = { _ in }
        public var sleep: @MainActor (SleepChoice) -> Void = { _ in }
        /// A Subtitle style menu's choice: Size, Background, Font or Position.
        public var subtitleStyle: @MainActor (SubtitleStyleMenu.Kind, String) -> Void = { _, _ in }
        /// Find Subtitles…: the screen opens the search; `nil` leaves it out.
        public var findSubtitles: (@MainActor () -> Void)?

        public init() {}

        /// The rows acting on `controller`.
        @MainActor
        public init(_ controller: PlaybackController?) {
            quality = { [weak controller] in controller?.selectQuality($0) }
            audio = { [weak controller] in controller?.selectAudio($0) }
            subtitle = { [weak controller] in controller?.selectSubtitle($0) }
            moveSubtitles = { [weak controller] in controller?.moveSubtitles(steps: $0) }
            resetSubtitleSync = { [weak controller] in controller?.setSubtitleOffset(0) }
            chapter = { [weak controller] in controller?.jumpToChapter($0) }
            speed = { [weak controller] in controller?.setSpeed($0) }
            sleep = { [weak controller] in controller?.setSleep($0) }
            subtitleStyle = { [weak controller] in controller?.setSubtitleStyle($0, $1) }
        }
    }

    /// One choice: its label, its second line, a checkmark when it is in use.
    public static func row(_ choice: MenuChoice, keepsMenu: Bool = false,
                           _ handler: @escaping @MainActor () -> Void) -> UIAction {
        let action = UIAction(title: choice.label, image: choice.selected ? UIImage(systemName: "checkmark") : nil,
                              attributes: keepsMenu ? .keepsMenuPresented : []) { _ in handler() }
        action.subtitle = choice.detail
        if choice.selected {
            action.accessibilityLabel = [choice.label, choice.detail, "selected"].compactMap { $0 }
                .joined(separator: ", ")
        }
        return action
    }

    /// Quality: each choice with its second line, the one playing checked.
    public static func quality(_ choices: [MenuChoice], _ actions: Actions) -> UIMenu {
        UIMenu(title: "Quality", image: UIImage(systemName: "slider.horizontal.3"),
               children: choices.map { choice in row(choice) { actions.quality(choice.id) } })
    }

    /// Audio, the track playing checked.
    public static func audio(_ choices: [MenuChoice], _ actions: Actions) -> UIMenu {
        UIMenu(title: "Audio", image: UIImage(systemName: "speaker.wave.2"), children: choices.map { choice in
            row(choice) { if let index = Int(choice.id) { actions.audio(index) } }
        })
    }

    /// Subtitles: Off and each track, the one shown checked, then the sync
    /// (0.1 s earlier or later, kept open while it is moved, and back to 0)
    /// when they can be moved, the style while text subtitles show, and
    /// last Find Subtitles… while it streams.
    public static func subtitles(_ state: PlayerState, _ actions: Actions) -> UIMenu {
        let tracks = state.subtitles.map { choice in
            row(choice) { actions.subtitle(choice.id == MenuChoice.off ? nil : Int(choice.id)) }
        }
        var children: [UIMenuElement] = tracks.isEmpty ? [] : [UIMenu(title: "", options: .displayInline,
                                                                      children: tracks)]
        if state.canSyncSubtitles {
            children.append(subtitleSync(state, actions))
        }
        if let style = subtitleStyle(state, actions) { children.append(style) }
        if let find = findSubtitles(state, actions) { children.append(find) }
        return UIMenu(title: "Subtitles", image: UIImage(systemName: "captions.bubble"), children: children)
    }

    /// Find Subtitles…, while the file streams from the server and the screen can open the search.
    public static func findSubtitles(_ state: PlayerState, _ actions: Actions) -> UIAction? {
        guard state.canFindSubtitles, let find = actions.findSubtitles else { return nil }
        return UIAction(title: "Find Subtitles…", image: UIImage(systemName: "magnifyingglass")) { _ in find() }
    }

    /// Speed, 0.5× to 2×, the one playing checked, with it as the menu's second line.
    public static func speed(_ state: PlayerState, _ actions: Actions) -> UIMenu {
        UIMenu(title: "Speed", subtitle: state.speedLabel, image: UIImage(systemName: "gauge.with.dots.needle.67percent"),
               children: state.speeds.map { choice in
                   row(choice) { if let speed = PlaybackSpeeds.speed(id: choice.id) { actions.speed(speed) } }
               })
    }

    /// Sleep Timer: Off, the minutes and the end of this episode, the one
    /// running checked, with what is left as the menu's second line.
    public static func sleepTimer(_ state: PlayerState, _ actions: Actions) -> UIMenu {
        UIMenu(title: "Sleep Timer", subtitle: state.sleepSummary, image: UIImage(systemName: "moon.zzz"),
               children: state.sleepChoices.map { choice in
                   row(choice) { if let sleep = SleepChoice(id: choice.id) { actions.sleep(sleep) } }
               })
    }

    /// The subtitle sync, with the offset as its second line.
    public static func subtitleSync(_ state: PlayerState, _ actions: Actions) -> UIMenu {
        UIMenu(title: "Subtitle sync", subtitle: state.subtitleOffsetLabel, image: UIImage(systemName: "timer"),
               children: [
                   UIAction(title: "Earlier", subtitle: "−0.1 s", attributes: .keepsMenuPresented) { _ in
                       actions.moveSubtitles(-1)
                   },
                   UIAction(title: "Later", subtitle: "+0.1 s", attributes: .keepsMenuPresented) { _ in
                       actions.moveSubtitles(1)
                   },
                   UIAction(title: "Reset", attributes: state.subtitleOffsetMs == 0 ? .disabled : []) { _ in
                       actions.resetSubtitleSync()
                   },
               ])
    }

    /// Subtitle style: Size, Background, Font and Position, as in Settings,
    /// each with its choice as its second line; `nil` unless text subtitles
    /// show, since pictures keep their own look.
    public static func subtitleStyle(_ state: PlayerState, _ actions: Actions) -> UIMenu? {
        let menus = state.subtitleStyleMenus
        guard !menus.isEmpty else { return nil }
        return UIMenu(title: "Subtitle style", image: UIImage(systemName: "textformat.size"),
                      children: menus.map { menu in
                          UIMenu(title: menu.title, subtitle: menu.summary, children: menu.choices.map { choice in
                              row(choice) { actions.subtitleStyle(menu.kind, choice.id) }
                          })
                      })
    }

    /// The chapters, the one playing checked.
    public static func chapters(_ chapters: [ChapterChoice], _ actions: Actions) -> UIMenu {
        UIMenu(title: "Chapters", image: UIImage(systemName: "list.bullet"), children: chapters.map { chapter in
            row(MenuChoice(id: String(chapter.index), label: chapter.label, selected: chapter.current)) {
                actions.chapter(chapter.index)
            }
        })
    }

    #if os(iOS)
    /// The iPhone's and iPad's Options menu: Quality, Audio and Subtitles
    /// (Find Subtitles… last) as sections, then the subtitle sync and
    /// style, Speed, Sleep Timer, the chapters and the details.
    public static func options(_ state: PlayerState, _ actions: Actions) -> UIMenu {
        var children: [UIMenuElement] = [section(quality(state.qualities, actions))]
        if state.audio.count > 1 { children.append(section(audio(state.audio, actions))) }
        let find = findSubtitles(state, actions)
        if !state.subtitles.isEmpty || find != nil {
            let tracks: [UIMenuElement] = state.subtitles.map { choice in
                row(choice) { actions.subtitle(choice.id == MenuChoice.off ? nil : Int(choice.id)) }
            } + [find].compactMap { $0 }
            children.append(UIMenu(title: "Subtitles", options: .displayInline, children: tracks))
            if state.canSyncSubtitles { children.append(subtitleSync(state, actions)) }
            if let style = subtitleStyle(state, actions) { children.append(style) }
        }
        children.append(speed(state, actions))
        children.append(sleepTimer(state, actions))
        if !state.chapters.isEmpty { children.append(chapters(state.chapters, actions)) }
        let details = PlayerDetails.lines(state).map { line in
            UIAction(title: line.title, subtitle: line.text, attributes: .disabled) { _ in }
        }
        children.append(UIMenu(title: "Details", options: .displayInline, children: details))
        return UIMenu(title: "", children: children)
    }

    /// `menu` as a titled section of a larger one.
    private static func section(_ menu: UIMenu) -> UIMenu {
        UIMenu(title: menu.title, options: .displayInline, children: menu.children)
    }
    #endif
}

#if os(iOS)
import SwiftUI

/// The Options button over the iPhone's and iPad's player: UIKit's menu
/// (`PlayerMenus.options`), made again whenever what it shows changes, also
/// while it is open, so the sync's offset moves as it is changed.
public struct PlayerOptionsButton: UIViewRepresentable {
    let state: PlayerState
    let actions: PlayerMenus.Actions
    let onTap: () -> Void

    public init(state: PlayerState, actions: PlayerMenus.Actions, onTap: @escaping () -> Void = {}) {
        self.state = state
        self.actions = actions
        self.onTap = onTap
    }

    public func makeCoordinator() -> Coordinator { Coordinator() }

    public func makeUIView(context: Context) -> UIButton {
        var look = UIButton.Configuration.plain()
        look.image = UIImage(systemName: "ellipsis",
                             withConfiguration: UIImage.SymbolConfiguration(textStyle: .body, scale: .large))
        look.baseForegroundColor = .white
        look.background.visualEffect = UIBlurEffect(style: .systemUltraThinMaterial)
        look.cornerStyle = .capsule
        let button = UIButton(configuration: look)
        button.showsMenuAsPrimaryAction = true
        button.accessibilityLabel = "Options"
        button.accessibilityIdentifier = "player.options"
        button.addAction(UIAction { [weak coordinator = context.coordinator] _ in coordinator?.onTap() },
                         for: .menuActionTriggered)
        return button
    }

    public func updateUIView(_ button: UIButton, context: Context) {
        let coordinator = context.coordinator
        coordinator.onTap = onTap
        let key = MenuKey(state)
        guard coordinator.shown != key else { return }
        coordinator.shown = key
        let menu = PlayerMenus.options(state, actions)
        button.menu = menu
        // Open: the menu showing follows too.
        button.contextMenuInteraction?.updateVisibleMenu { _ in menu }
    }

    public final class Coordinator {
        var onTap: () -> Void = {}
        var shown: MenuKey?
    }

    /// What the menu shows, so it is made again only when that changes (not
    /// at each position update).
    struct MenuKey: Equatable {
        var qualities: [MenuChoice]
        var audio: [MenuChoice]
        var subtitles: [MenuChoice]
        var offsetMs: Int
        var canSync: Bool
        var chapters: [ChapterChoice]
        var details: [PlayerDetails.Line]
        var speed: Double
        var sleep: [MenuChoice]
        var sleepSummary: String?
        var canFind: Bool
        var style: [SubtitleStyleMenu]

        init(_ state: PlayerState) {
            qualities = state.qualities
            audio = state.audio
            subtitles = state.subtitles
            offsetMs = state.subtitleOffsetMs
            canSync = state.canSyncSubtitles
            chapters = state.chapters
            details = PlayerDetails.lines(state)
            speed = state.speed
            sleep = state.sleepChoices
            sleepSummary = state.sleepSummary
            canFind = state.canFindSubtitles
            style = state.subtitleStyleMenus
        }
    }
}
#endif
#endif
