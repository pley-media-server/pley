import Foundation
import PleyModel
@testable import PleyScreens
import Testing

/// Home's banner picks its titles as Android's (`HomeStateTest`'s banner
/// tests) and the web's do.
@Suite struct FeaturedTests {
    private func art(_ id: Int, kind: ItemKind = .movie, library: Int = 1, watched: Bool = false,
                     backdrop: Bool = true) -> ItemSummary {
        var item = ItemSummary(id: id, libraryId: library, kind: kind, title: "Title \(id)")
        item.backdropUrl = backdrop ? "/api/items/\(id)/images/backdrop" : nil
        item.userState.watched = watched
        return item
    }

    private func rows(_ hubs: [Hub]) -> [HomeRow] { hubs.map { HomeRow($0) } }

    @Test func theBannerFeaturesTheNewestOfEachLibraryInTurn() {
        let movies = Hub(key: "recently_added:1", title: "Recently Added in Movies", libraryId: 1,
                         items: [art(10), art(11), art(12)])
        let shows = Hub(key: "recently_added:2", title: "Recently Added in Shows", libraryId: 2,
                        items: [art(20, kind: .show, library: 2)])
        let home = rows([Hub(key: "continue_watching", title: "Continue Watching", items: [art(1)]), movies, shows])
        #expect(Featured.pick(home).map(\.item.id) == [10, 20, 11, 12])
        #expect(!Featured.pick(home).contains { $0.fromList })
    }

    @Test func theBannerMixesTopRatedTitlesAmongTheNewestTheSameOnesAllDay() {
        let home = rows([Hub(key: "recently_added:1", title: "Recently Added in Movies", libraryId: 1,
                             items: (10...15).map { art($0) })])
        let rated = (30...39).map { art($0) } + [art(10), art(40, watched: true), art(41, backdrop: false)]
        let today = Featured.pick(home, topRated: rated, day: 20_000)
        // Six titles: the newest in order, a top rated one in the third and the sixth place.
        #expect(today.filter { !$0.topRated }.map(\.item.id) == [10, 11, 12, 13])
        #expect(today.indices.filter { today[$0].topRated } == [2, 5])
        // Not one that is new already, watched, or without a backdrop.
        #expect(today.filter(\.topRated).allSatisfy { (30...39).contains($0.item.id) })
        // The same all day, whatever order the libraries give them in, and whatever else is watched or added.
        #expect(Featured.pick(home, topRated: rated.reversed(), day: 20_000) == today)
        let kept = today.filter(\.topRated).map(\.item.id)
        let others = Array(rated.filter { !kept.contains($0.id) }.dropFirst(3))
        #expect(Featured.pick(home, topRated: rated.filter { kept.contains($0.id) } + others, day: 20_000)
            .filter(\.topRated).map(\.item.id) == kept)
        // Another day, other ones.
        #expect((20_001...20_007).contains { day in
            Featured.pick(home, topRated: rated, day: Int64(day)).filter(\.topRated).map(\.item.id) != kept
        })
        // Without top rated titles, the newest alone; with little that is new, the top rated fill up.
        #expect(Featured.pick(home, topRated: [], day: 20_000).map(\.item.id) == [10, 11, 12, 13, 14, 15])
        let few = rows([Hub(key: "recently_added:1", title: "Recently Added in Movies", libraryId: 1, items: [art(10)])])
        #expect(Featured.pick(few, topRated: rated, day: 20_000).map(\.topRated) == [false, true, true, true, true, true])
    }

    @Test func theBannerLeavesOutWhatWasWatchedHasNoBackdropOrIsNotAMovieOrShow() {
        let added = Hub(key: "recently_added:1", title: "Recently Added", libraryId: 1,
                        items: [art(1, watched: true), art(2, backdrop: false), art(3, kind: .episode), art(4)])
        #expect(Featured.pick(rows([added])).map(\.item.id) == [4])
        // My List fills up after what is new, said as such.
        let list = Hub(key: "my_list", title: "My List", items: [art(4), art(5, kind: .show)])
        let picks = Featured.pick(rows([added, list]))
        #expect(picks.map(\.item.id) == [4, 5])
        #expect(picks.map(\.fromList) == [false, true])
        // Everything watched: the newest all the same.
        let watched = Hub(key: "recently_added:1", title: "Recently Added", libraryId: 1,
                          items: [art(6, watched: true), art(7, watched: true)])
        #expect(Featured.pick(rows([watched])).map(\.item.id) == [6, 7])
        #expect(Featured.pick(rows([Hub(key: "continue_watching", title: "Continue Watching", items: [art(8)])])).isEmpty)
    }

    @Test func theBannerSaysWhyEachTitleIsThere() {
        let libraries = [Library(id: 1, name: "Movies", kind: .movies)]
        #expect(Featured.label(FeaturedPick(art(1), fromList: false), libraries: libraries) == "New in Movies")
        #expect(Featured.label(FeaturedPick(art(1), fromList: false), libraries: []) == "New")
        #expect(Featured.label(FeaturedPick(art(1), fromList: false, topRated: true), libraries: libraries)
            == "Top rated in Movies")
        #expect(Featured.label(FeaturedPick(art(1), fromList: false, topRated: true), libraries: []) == "Top rated")
        #expect(Featured.label(FeaturedPick(art(1), fromList: true), libraries: libraries) == "On your list")
        let title = FeaturedTitle(art(1), kicker: "New in Movies")
        #expect(title.backdrop == "/api/items/1/images/backdrop")
        #expect(title.info.title == "Title 1")
        #expect(!title.inProgress)
    }

    @Test func theDayCountsFrom1970() {
        #expect(Featured.day(of: Date(timeIntervalSince1970: 0)) == 0)
        #expect(Featured.day(of: Date(timeIntervalSince1970: 86_400 * 20_000 + 3_600)) == 20_000)
    }
}
