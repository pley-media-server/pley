import Foundation

/// A sentence from a server or a browser, safe to show: one line without
/// control or format characters (no bidirectional overrides), trimmed, at
/// most 200 characters.
public func plain(_ text: String) -> String {
    let kept = text.unicodeScalars.filter {
        let category = $0.properties.generalCategory
        return category != .control && category != .format
    }
    let line = String(String.UnicodeScalarView(kept)).trimmingCharacters(in: .whitespaces)
    return String(line.prefix(200))
}
