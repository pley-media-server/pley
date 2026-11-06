import Foundation
import PleyData

/// Where the subtitle style is kept: the app's settings, which Settings'
/// Subtitles shows too, or memory for tests.
public protocol SubtitleStyleMemory: AnyObject, Sendable {
    var subtitles: SubtitleOverrides { get set }
}

extension AppSettings: SubtitleStyleMemory {}

/// Kept while the app runs: for tests and previews.
public final class MemorySubtitleStyle: SubtitleStyleMemory, @unchecked Sendable {
    // Unchecked Sendable: only the player, on the main actor, reads and changes it.
    public var subtitles: SubtitleOverrides

    public init(_ subtitles: SubtitleOverrides = SubtitleOverrides()) {
        self.subtitles = subtitles
    }
}

/// One of the player's Subtitle style menus: Size, Background, Font or
/// Position, as Settings offers them, the one chosen selected.
public struct SubtitleStyleMenu: Equatable, Hashable, Sendable, Identifiable {
    public enum Kind: String, CaseIterable, Sendable {
        case size, background, font, position

        public var title: String {
            switch self {
            case .size: localized("Size")
            case .background: localized("Background")
            case .font: localized("Font")
            case .position: localized("Position")
            }
        }
    }

    public var id: Kind { kind }
    public let kind: Kind
    /// Ids are the setting's raw values.
    public let choices: [MenuChoice]

    public var title: String { kind.title }

    /// The choice in use, for the menu's second line: "125 %", "System".
    public var summary: String? { choices.first(where: \.selected)?.label }

    /// The four menus for `style`.
    public static func all(_ style: SubtitleOverrides) -> [SubtitleStyleMenu] {
        Kind.allCases.map { kind in
            let choices: [MenuChoice] = switch kind {
            case .size:
                SubtitleSize.allCases.map { MenuChoice(id: $0.rawValue, label: $0.label, selected: $0 == style.size) }
            case .background:
                SubtitleBackground.allCases.map {
                    MenuChoice(id: $0.rawValue, label: $0.label, selected: $0 == style.background)
                }
            case .font:
                SubtitleFont.offered.map { MenuChoice(id: $0.rawValue, label: $0.label, selected: $0 == style.font) }
            case .position:
                SubtitleRaise.allCases.map {
                    MenuChoice(id: String($0.rawValue), label: $0.label, selected: $0 == style.raise)
                }
            }
            return SubtitleStyleMenu(kind: kind, choices: choices)
        }
    }

    /// `style` with this menu's choice `id`; unchanged for an id it doesn't offer.
    public func choosing(_ id: String, in style: SubtitleOverrides) -> SubtitleOverrides {
        var next = style
        switch kind {
        case .size: next.size = SubtitleSize(rawValue: id) ?? style.size
        case .background: next.background = SubtitleBackground(rawValue: id) ?? style.background
        case .font: next.font = SubtitleFont(rawValue: id) ?? style.font
        case .position: next.raise = Int(id).flatMap(SubtitleRaise.init(rawValue:)) ?? style.raise
        }
        return next
    }
}
