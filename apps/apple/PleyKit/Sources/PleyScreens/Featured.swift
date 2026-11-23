import Foundation
import PleyModel

/// A title Home's banner features (Android's `FeaturedPick`): new in its
/// library, on My List (`fromList`), or one of its library's top rated
/// (`topRated`).
public struct FeaturedPick: Hashable, Sendable {
    public let item: ItemSummary
    public let fromList: Bool
    public let topRated: Bool

    public init(_ item: ItemSummary, fromList: Bool, topRated: Bool = false) {
        self.item = item
        self.fromList = fromList
        self.topRated = topRated
    }
}

/// The titles Home's banner features, picked as Android and the web pick
/// them (Android's `featuredOf`, apps/web/src/lib/featured.ts).
public enum Featured {
    /// How many titles the banner takes turns showing.
    public static let count = 6
    /// One place in this many is a top rated title's: the third and the sixth of six.
    public static let topRatedEvery = 3
    /// How many movie and show libraries' Top Rated rows are read for the banner.
    public static let topRatedLibraries = 6
    /// How long each title shows before the next, while nothing touches the banner.
    public static let turnSeconds: Double = 9

    /// Mostly what is new: the newest unwatched movies and shows with a
    /// backdrop, taken in turn from each library's Recently Added (so
    /// movies and shows alternate), then from My List when that is too
    /// few. When everything has been watched, the newest are featured all
    /// the same. In every third place comes one of `topRated` not yet
    /// watched, while there are some. Which ones depends on `day` and on
    /// nothing else that changes, so the banner shows the same titles all
    /// day, and a title watched or added moves no other. The first two
    /// places are always the newest, which Home has at once: the top rated
    /// come a moment later, and never change the title the banner opens on.
    public static func pick(_ rows: [HomeRow], topRated: [ItemSummary] = [], day: Int64 = 0,
                            count: Int = count) -> [FeaturedPick] {
        let unwatched = newest(rows, count) { hasArt($0) && !$0.userState.watched }
        let fresh = unwatched.isEmpty ? newest(rows, count, hasArt) : unwatched
        let taken = Set(fresh.map(\.item.id))
        var seen = Set<Int>()
        let rated = topRated
            .filter { hasArt($0) && !$0.userState.watched && !taken.contains($0.id) && seen.insert($0.id).inserted }
            .sorted { dayOrder($0.id, day) < dayOrder($1.id, day) }
        if rated.isEmpty { return fresh }
        var newOnes = fresh.makeIterator(), ratedOnes = rated.makeIterator()
        var nextNew = newOnes.next(), nextRated = ratedOnes.next()
        var picked: [FeaturedPick] = []
        while picked.count < count, nextNew != nil || nextRated != nil {
            let itsPlace = picked.count % topRatedEvery == topRatedEvery - 1
            if let item = nextRated, itsPlace || nextNew == nil {
                picked.append(FeaturedPick(item, fromList: false, topRated: true))
                nextRated = ratedOnes.next()
            } else if let pick = nextNew {
                picked.append(pick)
                nextNew = newOnes.next()
            }
        }
        return picked
    }

    /// Why `pick` is there: "New in Movies" or "Top rated in Movies" with
    /// its library's name, "New" or "Top rated" without it, "On your list".
    public static func label(_ pick: FeaturedPick, libraries: [Library]) -> String {
        if pick.fromList { return localized("On your list") }
        let library = libraries.first { $0.id == pick.item.libraryId }?.name ?? ""
        if pick.topRated {
            return library.isEmpty ? localized("Top rated") : localized("Top rated in \(library)")
        }
        return library.isEmpty ? localized("New") : localized("New in \(library)")
    }

    /// The day `date` is in, counted from 1970, which picks the day's top rated titles.
    public static func day(of date: Date) -> Int64 {
        Int64((date.timeIntervalSince1970 / 86_400).rounded(.down))
    }

    /// Movies and shows with a backdrop: what the banner can show.
    static func hasArt(_ item: ItemSummary) -> Bool {
        (item.kind == .movie || item.kind == .show) && !(item.backdropUrl ?? "").isEmpty
    }

    private static func newest(_ rows: [HomeRow], _ count: Int,
                               _ featurable: (ItemSummary) -> Bool) -> [FeaturedPick] {
        let sources = rows.filter { $0.key.hasPrefix("recently_added") }.map { $0.tiles.map(\.item).filter(featurable) }
        var picked: [FeaturedPick] = []
        var seen = Set<Int>()
        func take(_ item: ItemSummary, fromList: Bool) {
            if picked.count < count, seen.insert(item.id).inserted { picked.append(FeaturedPick(item, fromList: fromList)) }
        }
        var index = 0
        while picked.count < count, sources.contains(where: { index < $0.count }) {
            for source in sources where index < source.count { take(source[index], fromList: false) }
            index += 1
        }
        for item in rows.first(where: { $0.key == "my_list" })?.tiles.map(\.item) ?? [] where featurable(item) {
            take(item, fromList: true)
        }
        return picked
    }

    /// Where `id` comes among the top rated on `day`: an order that looks
    /// shuffled, is the same all day, and in which a title's place doesn't
    /// depend on which other titles there are (SplitMix64's mixing, as
    /// Android's `dayOrder`).
    static func dayOrder(_ id: Int, _ day: Int64) -> Int64 {
        var x = UInt64(bitPattern: Int64(id)) &* 0x9E37_79B9_7F4A_7C15 &+ UInt64(bitPattern: day) &* 0xB4B8_2F3A_5181_F3B5
        x = (x ^ (x >> 30)) &* 0xBF58_476D_1CE4_E5B9
        x = (x ^ (x >> 27)) &* 0x94D0_49BB_1331_11EB
        return Int64(bitPattern: x ^ (x >> 31))
    }
}
