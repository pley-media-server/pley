import Foundation

/// Reads the server's WebVTT (`/api/files/{id}/subtitles/{index}.vtt`)
/// into cues: their times, their lines with `<i>`, `<b>` and `<u>` kept
/// and every other tag (voices, classes, ruby, timestamps) taken out, and
/// `line:` read as top or bottom only. Blocks it can't read are skipped.
public enum WebVTT {
    public static func parse(_ text: String) -> [TextCue] {
        var cues: [TextCue] = []
        let normalised = text.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
        let lines = normalised.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        var i = 0
        while i < lines.count {
            // A block: its lines up to an empty one.
            var block: [String] = []
            while i < lines.count, !lines[i].trimmingCharacters(in: .whitespaces).isEmpty {
                block.append(lines[i])
                i += 1
            }
            i += 1
            guard let timing = block.firstIndex(where: { $0.contains("-->") }) else { continue }
            // The header, NOTE, STYLE and REGION blocks have no timing line before their text.
            let head = block[0].hasPrefix("\u{FEFF}") ? String(block[0].dropFirst()) : block[0]
            if head.hasPrefix("NOTE") || head.hasPrefix("STYLE") || head.hasPrefix("REGION") { continue }
            guard timing <= 1, let cue = cue(block[timing], text: block[(timing + 1)...]) else { continue }
            cues.append(cue)
        }
        return cues
    }

    /// A cue from its timing line and its text lines.
    static func cue(_ timing: String, text: ArraySlice<String>) -> TextCue? {
        let parts = timing.components(separatedBy: "-->")
        guard parts.count == 2 else { return nil }
        let end = parts[1].split(whereSeparator: { $0 == " " || $0 == "\t" })
        guard let startMs = time(parts[0].trimmingCharacters(in: .whitespaces)),
              let first = end.first, let endMs = time(String(first)), endMs > startMs
        else { return nil }
        var top = false
        for setting in end.dropFirst() where setting.hasPrefix("line:") {
            top = lineIsTop(String(setting.dropFirst(5)))
        }
        let styled = styled(text.joined(separator: "\n"))
        guard !styled.characters.isEmpty else { return nil }
        return TextCue(startMs: startMs, endMs: endMs, text: styled, top: top)
    }

    /// A cue time's hours or minutes at most: 100,000 is over eleven years
    /// of hours, and keeps the sum far from overflowing.
    static let maxField = 100_000

    /// `hh:mm:ss.ttt` or `mm:ss.ttt` in ms.
    static func time(_ text: String) -> Int? {
        let parts = text.split(separator: ":")
        guard (2...3).contains(parts.count), let last = parts.last else { return nil }
        let seconds = last.split(separator: ".", maxSplits: 1, omittingEmptySubsequences: false)
        guard let first = seconds.first, let s = Int(first), s < 60 else { return nil }
        var ms = 0
        // "00:01." reads as it did: no fraction.
        if seconds.count == 2, !seconds[1].isEmpty {
            let digits = seconds[1].prefix(3)
            guard let fraction = Int(digits), digits.allSatisfy(\.isNumber) else { return nil }
            ms = fraction * [100, 10, 1][digits.count - 1]
        }
        var total = s * 1000 + ms
        var unit = 60_000
        for part in parts.dropLast().reversed() {
            // Hours or minutes this large are no cue's, and would overflow.
            guard let value = Int(part), (0...Self.maxField).contains(value) else { return nil }
            total += value * unit
            unit *= 60
        }
        return total
    }

    /// `line:` puts the cue in the top half: a line number from the top (0
    /// and up), or a percentage under 50 (`line:10%`, `line:0,start`).
    static func lineIsTop(_ value: String) -> Bool {
        let line = value.split(separator: ",").first.map(String.init) ?? value
        if line.hasSuffix("%") { return (Double(line.dropLast()) ?? 100) < 50 }
        guard let number = Int(line) else { return false }
        return number >= 0
    }

    /// The cue's text with `<i>`, `<b>` and `<u>` as styles, other tags
    /// taken out and character references read.
    static func styled(_ text: String) -> AttributedString {
        var out = AttributedString()
        var italic = 0
        var bold = 0
        var underline = 0
        var rest = Substring(text)
        func add(_ words: Substring) {
            guard !words.isEmpty else { return }
            var piece = AttributedString(unescaped(words))
            var intent: InlinePresentationIntent = []
            if italic > 0 { intent.insert(.emphasized) }
            if bold > 0 { intent.insert(.stronglyEmphasized) }
            if !intent.isEmpty { piece.inlinePresentationIntent = intent }
            if underline > 0 { piece[SubtitleUnderline.self] = true }
            out += piece
        }
        while let open = rest.firstIndex(of: "<") {
            add(rest[..<open])
            guard let close = rest[open...].firstIndex(of: ">") else {
                // An unclosed tag: what follows is text.
                rest = rest[rest.index(after: open)...]
                continue
            }
            let tag = rest[rest.index(after: open)..<close].lowercased()
            let closing = tag.hasPrefix("/")
            let name = (closing ? String(tag.dropFirst()) : tag).split(whereSeparator: { ".  \t".contains($0) })
                .first.map(String.init) ?? ""
            let step = closing ? -1 : 1
            switch name {
            case "i": italic = max(0, italic + step)
            case "b": bold = max(0, bold + step)
            case "u": underline = max(0, underline + step)
            default: break
            }
            rest = rest[rest.index(after: close)...]
        }
        add(rest)
        return out
    }

    private static let entities = ["&amp;": "&", "&lt;": "<", "&gt;": ">", "&nbsp;": "\u{00A0}", "&lrm;": "\u{200E}",
                                   "&rlm;": "\u{200F}", "&quot;": "\"", "&apos;": "'"]

    private static func unescaped(_ text: Substring) -> String {
        guard text.contains("&") else { return String(text) }
        var out = String(text)
        for (entity, value) in entities where entity != "&amp;" {
            out = out.replacingOccurrences(of: entity, with: value)
        }
        return out.replacingOccurrences(of: "&amp;", with: "&")
    }
}
