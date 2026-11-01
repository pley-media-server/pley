import PleyPlayer
import SwiftUI

/// How the video plays, for Details: the way ("Direct stream"), the
/// quality and why Automatic went lower, why the server didn't play the
/// file as it is, what the screen gets of HDR video, and what the server's
/// conversion makes. Apple TV shows it as an info panel tab, the other devices in the
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
        lines.append(Line(title: "Playing", text: state.methodLabel ?? "Starting…"))
        lines.append(Line(title: "Quality", text: state.quality))
        if let note = state.qualityNote {
            lines.append(Line(title: "Lowered", text: note))
        }
        if !state.reasons.isEmpty {
            lines.append(Line(title: "Why", text: state.reasons.joined(separator: " ")))
        }
        if let range = state.videoRange, !range.isEmpty {
            lines.append(Line(title: "Video", text: range))
        }
        if let out = state.videoOut, !out.isEmpty {
            lines.append(Line(title: "Video out", text: out))
        }
        return lines
    }
}
