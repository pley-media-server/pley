import Foundation

/// The Mac player's keys, as the web player has them: space or k plays
/// and pauses, ← or j and → or l go back and forward 10 seconds, ↑ and ↓
/// change the volume by 5 %, m mutes, f goes full screen and Esc leaves it,
/// and s skips the intro or the credits the playhead is in.
/// Keys with ⌘, ⌃ or ⌥ are left to the menus.
public enum PlayerKeys {
    public enum Action: Equatable, Sendable {
        case playPause
        case seek(byMs: Int)
        case volume(by: Float)
        case mute
        case fullScreen
        case leaveFullScreen
        case skip
    }

    public static let seekMs = 10_000
    public static let volumeStep: Float = 0.05

    /// The arrows' and Esc's characters as AppKit gives them (`NSEvent`'s
    /// function keys), so this needs no AppKit.
    public enum Special {
        public static let up: Character = "\u{F700}"
        public static let down: Character = "\u{F701}"
        public static let left: Character = "\u{F702}"
        public static let right: Character = "\u{F703}"
        public static let escape: Character = "\u{1B}"
    }

    /// What `characters` (ignoring modifiers but Shift) does; `nil` for a
    /// key the player leaves alone, or one pressed with ⌘, ⌃ or ⌥.
    public static func action(_ characters: String, command: Bool = false) -> Action? {
        guard !command, characters.count == 1, let key = characters.lowercased().first else { return nil }
        switch key {
        case " ", "k": return .playPause
        case Special.left, "j": return .seek(byMs: -seekMs)
        case Special.right, "l": return .seek(byMs: seekMs)
        case Special.up: return .volume(by: volumeStep)
        case Special.down: return .volume(by: -volumeStep)
        case "m": return .mute
        case "f": return .fullScreen
        case Special.escape: return .leaveFullScreen
        case "s": return .skip
        default: return nil
        }
    }
}
