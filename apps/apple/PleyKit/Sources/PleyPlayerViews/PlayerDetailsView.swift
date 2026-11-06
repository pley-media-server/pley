import PleyPlayer
import SwiftUI

/// How the video plays, for Details: the way ("Direct stream"), the
/// quality and why Automatic went lower, why the server didn't play the
/// file as it is, what the screen gets of HDR video, what the server's
/// conversion makes, and the subtitles shown (and whether they are burned in). Apple TV shows it as an info panel tab, the other devices in the
/// Options menu (iPhone and iPad) or the action menu (Mac).
public struct PlayerDetailsView: View {
    let state: PlayerState

    public init(state: PlayerState) {
        self.state = state
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(PlayerDetails.lines(state), id: \.self) { line in
                VStack(alignment: .leading, spacing: 2) {
                    Text(line.title).font(.caption).foregroundStyle(.secondary)
                    Text(line.text)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Details as lines of a title and a text, for every platform's menu.
public enum PlayerDetails {
    public struct Line: Hashable, Sendable {
        public var title: String
        public var text: String
    }

    public static func lines(_ state: PlayerState) -> [Line] {
        var lines: [Line] = []
        lines.append(Line(title: localized("Playing"), text: state.methodLabel ?? localized("Starting…")))
        lines.append(Line(title: localized("Quality"), text: state.quality))
        if let note = state.qualityNote {
            lines.append(Line(title: localized("Lowered"), text: note))
        }
        if !state.reasons.isEmpty {
            lines.append(Line(title: localized("Why"), text: state.reasons.joined(separator: " ")))
        }
        if let range = state.videoRange, !range.isEmpty {
            lines.append(Line(title: localized("Video"), text: range))
        }
        if let out = state.videoOut, !out.isEmpty {
            lines.append(Line(title: localized("Video out"), text: out))
        }
        if state.subtitles.contains(where: { $0.selected && $0.id != MenuChoice.off }) {
            let label = state.subtitleLabel
            let text = state.subtitlesBurnedIn ? localized("\(label) · burned into the video") : label
            lines.append(Line(title: localized("Subtitles"), text: text))
        }
        return lines
    }
}
