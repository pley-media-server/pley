import Foundation
import PleyModel

/// Smart collection and playlist filters: a library listing in Plex's
/// syntax (`genre=Comedy&unwatched=1&sort=year:desc`, docs/COMPATIBILITY.md),
/// ported from Android's `SmartFilter` (the web's smartFilter.ts).
public enum SmartFilter {
    private static let plexSort: [LibrarySort: String] = [
        .title: "titleSort", .year: "year", .added: "addedAt", .rating: "rating", .userRating: "userRating",
        .release: "originallyAvailableAt", .duration: "duration", .lastPlayed: "lastViewedAt",
        .bitrate: "mediaBitrate",
    ]

    /// Each Plex sort's words, oldest-first order then newest-first.
    private static let sortNotes: [String: (String, String)] = [
        "titleSort": ("By title", "By title, Z to A"),
        "year": ("Oldest first", "Newest first"),
        "originallyAvailableAt": ("Earliest release first", "Latest release first"),
        "addedAt": ("First added first", "Recently added first"),
        "rating": ("Lowest rated first", "Best rated first"),
        "audienceRating": ("Lowest rated first", "Best rated first"),
        "userRating": ("Your lowest rated first", "Your best rated first"),
        "lastViewedAt": ("Least recently played first", "Recently played first"),
        "duration": ("Shortest first", "Longest first"),
        "viewCount": ("Least played first", "Most played first"),
    ]

    /// The library page's filters and order as a smart filter: what Save as… saves.
    public static func of(_ params: LibraryParams) -> String {
        var parts: [(String, String)] = []
        if let genre = params.genre { parts.append(("genre", genre)) }
        if let year = params.year { parts.append(("year", String(year))) }
        if let watched = params.watched { parts.append(("unwatched", watched ? "0" : "1")) }
        let field = plexSort[params.sort] ?? "titleSort"
        parts.append(("sort", params.effectiveOrder == .descending ? "\(field):desc" : field))
        return parts.map { "\(LibraryParams.encode($0.0))=\(LibraryParams.encode($0.1))" }.joined(separator: "&")
    }

    /// What a smart filter finds, in words: "Genre: Comedy", "Unwatched", "Newest first"…
    public static func describe(_ filter: String) -> [String] {
        var out: [String] = []
        for part in filter.split(separator: "&", omittingEmptySubsequences: true) {
            let pieces = part.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            let key = LibraryParams.decode(String(pieces[0]))
            let value = pieces.count > 1 ? LibraryParams.decode(String(pieces[1])) : ""
            let off = value == "0" || value == "false"
            switch key {
            case "genre": out.append("Genre: \(value)")
            case "label": out.append("Label: \(value)")
            case "studio": out.append("Studio: \(value)")
            case "contentRating": out.append("Rated \(value)")
            case "unwatched": out.append(off ? "Watched" : "Unwatched")
            case "inProgress": out.append(off ? "Not in progress" : "In progress")
            case "year": out.append("From \(value)")
            case "year>>": out.append("After \(value)")
            case "year<<": out.append("Before \(value)")
            case "decade": out.append("The \(value)s")
            case "sort": out.append(sortNote(value))
            case "limit": out.append("At most \(value)")
            // Plumbing, not part of what is found.
            case "type", "includeGuids": break
            default: out.append("\(key): \(value)")
            }
        }
        return out
    }

    /// A collection's order in words when it has no filter: "By release date".
    public static func sortNote(_ sort: CollectionSort) -> String? {
        switch sort {
        case .release: "By release date"
        case .title: "By title"
        case .custom: "Custom order"
        default: nil
        }
    }

    private static func sortNote(_ value: String) -> String {
        let first = value.split(separator: ",", omittingEmptySubsequences: false).first.map(String.init) ?? value
        let pieces = first.split(separator: ":", maxSplits: 1, omittingEmptySubsequences: false).map(String.init)
        let field = pieces[0]
        let direction = pieces.count > 1 ? pieces[1] : nil
        // python-plexapi may name the kind first: `movie.year`.
        let name = field.split(separator: ".", maxSplits: 1).count > 1
            ? String(field[field.index(after: field.firstIndex(of: ".")!)...]) : field
        guard let notes = sortNotes[name] else { return "Sorted by \(name)" }
        return direction == "desc" ? notes.1 : notes.0
    }
}
