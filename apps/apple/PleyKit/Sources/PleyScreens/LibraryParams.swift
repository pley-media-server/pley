import Foundation
import PleyModel
import PleyNetwork

/// A library page's tabs.
public enum LibraryTab: String, CaseIterable, Hashable, Sendable {
    /// The library's rows (Continue Watching, Recently Added…).
    case recommended
    /// Every title, sorted and filtered.
    case library
    /// Its collections (not for music).
    case collections

    public var title: String {
        switch self {
        case .recommended: localized("Recommended")
        case .library: localized("Library")
        case .collections: localized("Collections")
        }
    }
}

/// How a library's titles are listed (docs/API.md, `sort`).
public enum LibrarySort: String, CaseIterable, Hashable, Sendable {
    case title, year, added, rating
    case userRating = "user_rating"
    case release, duration
    case lastPlayed = "last_played"
    /// The best file's bitrate (a show's: its episodes' average); tiles then show the file.
    case bitrate

    public var title: String {
        switch self {
        case .title: localized("Title")
        case .year: localized("Year")
        case .added: localized("Date added")
        case .rating: localized("Rating")
        case .userRating: localized("Your rating")
        case .release: localized("Release date")
        case .duration: localized("Duration")
        case .lastPlayed: localized("Last played")
        case .bitrate: localized("Bitrate")
        }
    }
}

/// Up or down a listing.
public enum SortOrder: String, Hashable, Sendable {
    case ascending = "asc"
    case descending = "desc"
}

/// How a library page is shown: its tab, and the Library tab's sort, order
/// and filters, as the web keeps them in the page's address (Android's
/// `LibraryParams`). `query` and `parse` turn them into that address's
/// query and back ("tab=recommended", "sort=year&watched=false&genre=Comedy"),
/// which is also how a library is remembered as it was left.
///
/// `order` `nil` is the sort's own order (`defaultOrder`); `watched` is the
/// web's: `false` lists unwatched titles only, `true` watched ones, `nil` both.
public struct LibraryParams: Hashable, Sendable {
    public var tab: LibraryTab
    public var sort: LibrarySort
    public var order: SortOrder?
    public var watched: Bool?
    public var genre: String?
    public var year: Int?

    public init(tab: LibraryTab = .library, sort: LibrarySort = .title, order: SortOrder? = nil,
                watched: Bool? = nil, genre: String? = nil, year: Int? = nil) {
        self.tab = tab
        self.sort = sort
        self.order = order
        self.watched = watched
        self.genre = genre
        self.year = year
    }

    /// The order the listing is in: the one chosen, else the sort's own.
    public var effectiveOrder: SortOrder { order ?? Self.defaultOrder(sort) }

    /// A filter narrows the listing (the sort doesn't count).
    public var filtered: Bool { watched != nil || genre != nil || year != nil }

    /// The listing as the server is asked for it.
    public var query: LibraryQuery {
        LibraryQuery(sort: sort.rawValue, order: effectiveOrder.rawValue, watched: watched, genre: genre, year: year)
    }

    /// Without any filter: "Clear filters".
    public func cleared() -> LibraryParams {
        var out = self
        out.watched = nil
        out.genre = nil
        out.year = nil
        return out
    }

    /// Sorted by `sort` in its own order, as choosing a sort does on the web.
    public func sorted(by sort: LibrarySort) -> LibraryParams {
        var out = self
        out.sort = sort
        out.order = nil
        return out
    }

    /// The other order.
    public func flipped() -> LibraryParams {
        var out = self
        out.order = effectiveOrder == .ascending ? .descending : .ascending
        return out
    }

    /// What a library of `kind` can show: a sort it offers (else Title),
    /// and for music no filters, which it has none of on the web, nor a
    /// Collections tab.
    public func valid(for kind: LibraryKind) -> LibraryParams {
        var out = self
        if !Self.sorts(for: kind).contains(sort) {
            out.sort = .title
            out.order = nil
        }
        if kind == .music {
            if out.tab == .collections { out.tab = .library }
            out.watched = nil
            out.genre = nil
            out.year = nil
        }
        return out
    }

    /// The web's query: parameters at their default are left out, so a
    /// fresh page is "" and the Library tab is never named.
    public var queryString: String {
        var parts: [(String, String)] = []
        if tab != .library { parts.append(("tab", tab.rawValue)) }
        if sort != .title { parts.append(("sort", sort.rawValue)) }
        if let order, order != Self.defaultOrder(sort) { parts.append(("order", order.rawValue)) }
        if let watched { parts.append(("watched", watched ? "true" : "false")) }
        if let genre { parts.append(("genre", genre)) }
        if let year { parts.append(("year", String(year))) }
        return parts.map { "\($0.0)=\(Self.encode($0.1))" }.joined(separator: "&")
    }

    /// Sorts each kind of library offers, in the web's order.
    public static func sorts(for kind: LibraryKind) -> [LibrarySort] {
        switch kind {
        case .music: [.title, .added, .userRating, .lastPlayed]
        case .shows: [.title, .year, .added, .rating, .userRating, .release, .lastPlayed, .bitrate]
        default: [.title, .year, .added, .rating, .userRating, .release, .duration, .lastPlayed, .bitrate]
        }
    }

    /// The tabs of a library of `kind`: music has no collections.
    public static func tabs(for kind: LibraryKind?) -> [LibraryTab] {
        kind == .music ? [.recommended, .library] : LibraryTab.allCases
    }

    /// Titles A to Z; everything else newest, biggest or best first, as on the web.
    public static func defaultOrder(_ sort: LibrarySort) -> SortOrder {
        sort == .title ? .ascending : .descending
    }

    /// `query` as the web's page reads it: unknown values fall back to the
    /// defaults, and the page opens on `tab` unless the query names one.
    public static func parse(_ query: String, tab: LibraryTab = .library) -> LibraryParams {
        var values: [String: String] = [:]
        for part in query.split(separator: "&") where !part.isEmpty {
            let pieces = part.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            let name = decode(String(pieces[0]))
            if values[name] == nil { values[name] = pieces.count > 1 ? decode(String(pieces[1])) : "" }
        }
        return LibraryParams(
            tab: values["tab"].flatMap(LibraryTab.init(rawValue:)) ?? tab,
            sort: values["sort"].flatMap(LibrarySort.init(rawValue:)) ?? .title,
            order: values["order"].flatMap(SortOrder.init(rawValue:)),
            watched: values["watched"] == "true" ? true : values["watched"] == "false" ? false : nil,
            genre: values["genre"].flatMap { $0.isEmpty ? nil : $0 },
            year: values["year"].flatMap { Int($0) })
    }

    private static let allowed: CharacterSet = {
        var set = CharacterSet.alphanumerics
        set.insert(charactersIn: "-._*")
        return set
    }()

    /// As a form encodes it: spaces as `+`.
    static func encode(_ value: String) -> String {
        (value.addingPercentEncoding(withAllowedCharacters: allowed.union(.init(charactersIn: " "))) ?? value)
            .replacingOccurrences(of: " ", with: "+")
    }

    static func decode(_ value: String) -> String {
        value.replacingOccurrences(of: "+", with: " ").removingPercentEncoding ?? value
    }
}

/// The A to Z of a listing sorted by title (the web's jump bar, Android's
/// `AlphaIndex`): for "#" and each letter, where its first title is.
/// Titles go under the first letter of their sort title (else title),
/// accents taken off ("Élan" under E); anything else, digits and other
/// alphabets, under "#". Letters without titles are kept, so screens can
/// show them dimmed.
public struct AlphaIndex: Hashable, Sendable {
    /// "#" then A to Z.
    public static let letters: [String] = ["#"] + (UInt8(ascii: "A")...UInt8(ascii: "Z")).map {
        String(UnicodeScalar($0))
    }
    /// Listings this short don't need one (the web's 30).
    public static let minItems = 30

    /// For each of `letters`, the position of its first title, or -1.
    private let firsts: [Int]
    /// Sections in listing order: where each starts and its letter.
    private let starts: [Int]
    private let startLetters: [Int]

    /// The index of `items`, in their order.
    public static func of(_ items: [ItemSummary]) -> AlphaIndex {
        of(titles: items.map { $0.sortTitle.trimmingCharacters(in: .whitespaces).isEmpty ? $0.title : $0.sortTitle })
    }

    /// The index of titles in listing order.
    public static func of(titles: [String]) -> AlphaIndex {
        var firsts = Array(repeating: -1, count: letters.count)
        var starts: [Int] = []
        var startLetters: [Int] = []
        var previous = -1
        for (position, title) in titles.enumerated() {
            let letter = letter(of: title)
            if firsts[letter] < 0 { firsts[letter] = position }
            if letter != previous {
                starts.append(position)
                startLetters.append(letter)
                previous = letter
            }
        }
        return AlphaIndex(firsts: firsts, starts: starts, startLetters: startLetters)
    }

    /// The letter `title` goes under: 0 for "#", 1 to 26 for A to Z.
    public static func letter(of title: String) -> Int {
        guard let first = title.trimmingCharacters(in: .whitespaces).first else { return 0 }
        let plain = String(first).uppercased().folding(options: [.diacriticInsensitive], locale: nil)
        guard let scalar = plain.unicodeScalars.first, ("A"..."Z").contains(scalar) else { return 0 }
        return Int(scalar.value) - Int(UnicodeScalar("A").value) + 1
    }

    /// Whether `letter` (an index into `letters`) has titles.
    public func has(_ letter: Int) -> Bool { firsts.indices.contains(letter) && firsts[letter] >= 0 }

    /// Where `letter`'s first title is; -1 without one.
    public func position(of letter: Int) -> Int { firsts.indices.contains(letter) ? firsts[letter] : -1 }

    /// Where to go for `letter`: its first title, else the next letter's
    /// that has one, else the last letter's before it.
    public func jumpTarget(_ letter: Int) -> Int {
        let letter = min(max(letter, 0), firsts.count - 1)
        for l in letter..<firsts.count where firsts[l] >= 0 { return firsts[l] }
        for l in stride(from: letter, through: 0, by: -1) where firsts[l] >= 0 { return firsts[l] }
        return 0
    }

    /// The letter (an index into `letters`) of the section `position` is in.
    public func letter(at position: Int) -> Int {
        guard let first = starts.first else { return 0 }
        if position < first { return startLetters[0] }
        var lo = 0, hi = starts.count - 1
        while lo < hi {
            let mid = (lo + hi + 1) / 2
            if starts[mid] <= position { lo = mid } else { hi = mid - 1 }
        }
        return startLetters[lo]
    }

    /// The letters that have titles.
    public var present: [Int] { firsts.indices.filter(has) }
}
