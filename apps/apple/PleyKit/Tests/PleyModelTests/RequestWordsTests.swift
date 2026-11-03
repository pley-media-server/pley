import Foundation
import PleyModel
import Testing

/// Requests' words, on a fixed clock: 2025-10-30 12:00 UTC, as Android's `RequestTextsTest`.
@Suite struct RequestWordsTests {
    let now = 1_761_825_600
    let day = 86_400
    let utc = TimeZone(identifier: "UTC")!
    let us = Locale(identifier: "en_US")

    @Test func seasonsAreNamedAsTheWebNamesThem() {
        #expect(RequestWords.seasons([2]) == "Season 2")
        #expect(RequestWords.seasons([5, 3, 2, 1]) == "Seasons 1–3, 5")
        #expect(RequestWords.seasons([0, 1, 2]) == "Seasons 1–2 and specials")
        #expect(RequestWords.seasons([3, 0]) == "Season 3 and specials")
        #expect(RequestWords.seasons([0]) == "Specials")
        #expect(RequestWords.seasons([]) == "")
        #expect(RequestWords.seasonRanges([1, 2, 3, 5, 7, 8, 2]) == "1–3, 5, 7–8")
        #expect(RequestWords.seasonName(DiscoverSeason(number: 2)) == "Season 2")
        #expect(RequestWords.seasonName(DiscoverSeason(number: 0, name: " ")) == "Specials")
        #expect(RequestWords.seasonName(DiscoverSeason(number: 1, name: "Book One")) == "Book One")
    }

    @Test func aSeasonsStateForThisAccount() {
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, state: .library)) == "In library")
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, state: .requested)) == "Requested")
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, state: .missing, declined: true)) == "Declined")
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, episodes: 10, inLibrary: 4, state: .partial))
            == "4 of 10 episodes in library")
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, airDate: "2026-11-04", state: .unaired), locale: us)
            == "Airs Nov 4, 2026")
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, state: .unaired)) == "Not aired yet")
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, episodes: 1, state: .missing)) == "1 episode")
        #expect(RequestWords.seasonState(DiscoverSeason(number: 1, episodes: 10, state: .missing)) == "10 episodes")
    }

    @Test func pillsSayTheStage() {
        func pill(_ pill: StagePill) -> String { RequestWords.pill(pill, now: now, zone: utc, locale: us) }
        #expect(pill(.pending) == "Waiting for approval")
        #expect(pill(.approved) == "Approved")
        #expect(pill(.downloading(progress: 0.42, eta: now + 20 * 60, state: .downloading)) == "Downloading 42% · in 20 min")
        #expect(pill(.downloading(progress: 0.42, eta: nil, state: .downloading)) == "Downloading 42%")
        #expect(pill(.downloading(progress: 0.42, eta: now - 5, state: .downloading)) == "Downloading 42%")
        #expect(pill(.downloading(progress: 0, eta: nil, state: .queued)) == "Waiting to download")
        #expect(pill(.downloading(progress: 0.5, eta: nil, state: .paused)) == "Download paused")
        #expect(pill(.downloading(progress: 0.5, eta: nil, state: .warning)) == "The download has a problem")
        // A release known by its day comes at 12:00 UTC of that day.
        #expect(pill(.waiting(show: false, at: 1_762_257_600)) == "Out Nov 4")
        #expect(pill(.waiting(show: true, at: 1_762_257_600)) == "Airs Nov 4")
        #expect(pill(.waiting(show: true, at: now + 9 * 3_600)) == "Airs today, 9:00\u{202F}PM"
            || pill(.waiting(show: true, at: now + 9 * 3_600)) == "Airs today, 9:00 PM")
        #expect(pill(.waiting(show: false, at: nil)) == "Not out yet")
        #expect(pill(.searching) == "Searching")
        #expect(pill(.importing) == "Adding to the library")
        #expect(pill(.partial(seasons: [1, 2])) == "Seasons 1–2 ready")
        #expect(pill(.available) == "Ready to watch")
        #expect(pill(.gone) == "No longer in a library")
        #expect(pill(.declined) == "Declined")
        #expect(pill(.couldNotSend(reason: "Radarr isn't answering", retryAt: nil)) == "Couldn't send: Radarr isn't answering")
        #expect(pill(.couldNotSend(reason: " ", retryAt: nil)) == "Couldn't send")
        #expect(pill(.notSentYet(retryAt: now + 300)) == "Not sent yet")
        #expect(pill(.unknown) == "Requested")
        #expect(RequestWords.pillLine(.notSentYet(retryAt: now + 300), now: now, zone: utc) == "Trying again in 5 min.")
        #expect(RequestWords.pillLine(.notSentYet(retryAt: nil), now: now, zone: utc)
            == "It couldn't be sent to download yet: an administrator will look at it.")
        #expect(RequestWords.pillLine(.couldNotSend(reason: nil, retryAt: nil), now: now, zone: utc) == nil)
        #expect(RequestWords.pillLine(.pending, now: now, zone: utc) == nil)
    }

    @Test func timesAreWrittenHere() {
        #expect(RequestWords.until(now + 20 * 60, now: now, zone: utc) == "in 20 min")
        #expect(RequestWords.until(now + 61, now: now, zone: utc) == "in 2 min")
        #expect(RequestWords.until(now - 1, now: now, zone: utc) == "in a moment")
        #expect(RequestWords.until(now + 3_600, now: now, zone: utc) == "in 1 hour")
        #expect(RequestWords.until(now + 3 * 3_600, now: now, zone: utc) == "in 3 hours")
        #expect(RequestWords.until(now + day, now: now, zone: utc) == "tomorrow")
        #expect(RequestWords.until(now + 12 * day, now: now, zone: utc) == "in 12 days")
        // Days are counted on the calendar of the zone: 11:00 tomorrow is still tomorrow.
        #expect(RequestWords.until(now + day + 23 * 3_600, now: now, zone: utc) == "in 2 days")
        #expect(RequestWords.ago(now - 3 * day, now: now) == "3 days ago")
        #expect(RequestWords.day(1_762_257_600, now: now, zone: utc, locale: us) == "Nov 4")
        #expect(RequestWords.day(now + 400 * day, now: now, zone: utc, locale: us) == "Dec 4, 2026")
        #expect(RequestWords.day(now + 13 * 3_600, now: now, zone: utc, locale: us).hasPrefix("tomorrow, 1:00"))
    }

    @Test func theAllowanceLine() {
        func lines(_ allowance: Allowance, _ kind: TitleKind? = nil) -> [String] {
            RequestWords.allowance(allowance, kind: kind, now: now, zone: utc)
        }
        let allowance = Allowance(moviesLeft: 2, seasonsLeft: 5, moviesLimit: 3, seasonsLimit: 10)
        #expect(lines(allowance) == ["You can ask for 2 more movies and 5 more seasons.",
                                     "Limits: 3 movies and 10 seasons per 7 days."])
        var one = allowance
        one.moviesLeft = 1
        #expect(lines(one, .movie) == ["You can ask for 1 more movie.", "Limit: 3 movies per 7 days."])
        var noMovies = allowance
        noMovies.moviesLeft = 0
        noMovies.moviesNextAt = now + 3 * day
        #expect(lines(noMovies, .movie) == ["You've asked for all the movies you can for now: you can ask for more in 3 days."])
        #expect(lines(noMovies) == ["You've asked for all the movies you can for now: you can ask for more in 3 days.",
                                    "You can ask for 5 more seasons.", "Limits: 3 movies and 10 seasons per 7 days."])
        var nothing = noMovies
        nothing.seasonsLeft = 0
        nothing.seasonsNextAt = now + 5 * day
        #expect(lines(nothing) == ["You've asked for all you can for now: more movies in 3 days, more seasons in 5 days."])
        nothing.seasonsNextAt = nil
        #expect(lines(nothing) == ["You've asked for all you can for now."])
        #expect(lines(Allowance()).isEmpty)
    }

    @Test func aDeclineSaysWhenToAskAgain() {
        #expect(RequestWords.declined(reason: "Not for us", againAt: now + 12 * day, now: now, zone: utc)
            == "Declined: “Not for us”. You can ask again in 12 days.")
        #expect(RequestWords.declined(reason: nil, againAt: now + 12 * day, now: now, zone: utc)
            == "Declined. You can ask again in 12 days.")
        #expect(RequestWords.declined(reason: " Not for us ", againAt: nil, now: now) == "Declined: “Not for us”.")
        #expect(RequestWords.declined(reason: "", againAt: nil, now: now) == "Declined.")
    }

    @Test func askingSaysWhatHappened() {
        let request = MediaRequest(id: 60, kind: .movie, tmdbId: 1, title: "Arrival", status: .pending)
        var approved = request
        approved.status = .approved
        var failed = request
        failed.status = .failed
        #expect(RequestWords.asked(Asked(request: request, created: true), title: "Arrival")
            == "Asked for “Arrival”: an administrator decides.")
        #expect(RequestWords.asked(Asked(request: approved, created: true), title: "Arrival")
            == "“Arrival” is approved and on its way.")
        #expect(RequestWords.asked(Asked(request: failed, created: true), title: "Arrival")
            == "Asked for “Arrival”, but it couldn't be sent to download yet.")
        #expect(RequestWords.asked(Asked(request: request, created: false), title: "Arrival")
            == "You'll hear when “Arrival” is ready.")
        #expect(RequestWords.pickerButton(0) == "Pick a season.")
        #expect(RequestWords.pickerButton(1) == "Request 1 season")
        #expect(RequestWords.pickerButton(3) == "Request 3 seasons")
    }

    @Test func whoAskedAndWhere() {
        var request = MediaRequest(id: 1, kind: .movie, tmdbId: 2, title: "X", mine: true, createdAt: now - 3 * day)
        #expect(RequestWords.asked(request, manage: false, now: now) == "3 days ago")
        request.requestedBy = RequestUser(id: 1, name: "Ann")
        #expect(RequestWords.asked(request, manage: true, now: now) == "Ann · 3 days ago")
        request.mine = false
        request.requestedBy = nil
        #expect(RequestWords.asked(request, manage: false, now: now) == "Asked for by someone else · 3 days ago")
        #expect(RequestWords.watchers(1) == "1 person follows it" && RequestWords.watchers(3) == "3 people follow it")
        #expect(RequestWords.sentTo(request) == nil)
        request.sentTo = SentTo(service: "radarr", qualityProfile: "HD-1080p", rootFolder: "/movies")
        #expect(RequestWords.sentTo(request) == "Radarr · HD-1080p · /movies")
        request.kind = .show
        request.sentTo = SentTo(service: "sonarr")
        #expect(RequestWords.sentTo(request) == "Sonarr")
    }

    @Test func noticesSayWhatHappened() {
        #expect(RequestWords.notice(.ready(requestId: 45, title: "Arrival", kind: .movie, itemId: 101, seasons: nil,
                                           complete: true)) == "“Arrival” is ready to watch.")
        #expect(RequestWords.notice(.ready(requestId: 47, title: "Severance", kind: .show, itemId: 201, seasons: [2],
                                           complete: false)) == "Season 2 of “Severance” is ready to watch.")
        #expect(RequestWords.notice(.ready(requestId: 47, title: "Severance", kind: .show, itemId: 201, seasons: [1, 2],
                                           complete: false)) == "Seasons 1–2 of “Severance” are ready to watch.")
        #expect(RequestWords.notice(.declined(requestId: 46, title: "Alien: Romulus", reason: "We have enough aliens for now"))
            == "“Alien: Romulus” was declined: “We have enough aliens for now”.")
        #expect(RequestWords.notice(.declined(requestId: 46, title: "Alien: Romulus", reason: nil))
            == "“Alien: Romulus” was declined.")
        #expect(RequestWords.notice(.approved(requestId: 45, title: "Arrival")) == "“Arrival” was approved.")
        #expect(RequestWords.notice(.asked(requestId: 49, title: "Arcane", by: "Kit", seasons: [1, 2, 3]))
            == "Kit asked for “Arcane” (Seasons 1–3).")
        #expect(RequestWords.notice(.asked(requestId: 49, title: "Arcane", by: nil, seasons: nil))
            == "Someone asked for “Arcane”.")
        #expect(RequestWords.notice(.failed(requestId: 48, title: "Inside Out 2", kind: .movie,
                                            reason: "Radarr isn't answering"))
            == "“Inside Out 2” couldn't be sent to Radarr: Radarr isn't answering")
        #expect(RequestWords.notice(.failed(requestId: 49, title: "Arcane", kind: .show, reason: nil))
            == "“Arcane” couldn't be sent to Sonarr.")
        #expect(RequestWords.notice(.news(count: 3)) == "3 of your requests have news")
    }

    @Test func theNumberOnTheWayInIsSaidInWords() {
        #expect(RequestWords.count(manage: false, badge: 1, failed: 0) == "1 of your requests has news")
        #expect(RequestWords.count(manage: true, badge: 1, failed: 0) == "1 request needs you")
        #expect(RequestWords.count(manage: true, badge: 3, failed: 1) == "3 requests need you, 1 couldn't be sent")
        #expect(RequestWords.badgeText(0) == nil && RequestWords.badgeText(7) == "7" && RequestWords.badgeText(120) == "99+")
    }

    @Test func titleBadges() {
        #expect(RequestWords.badge(.library, itemId: 1) == "In library")
        #expect(RequestWords.badge(.partial, itemId: 1) == "Partly in library")
        // Partly asked for, nothing in a library: Requested.
        #expect(RequestWords.badge(.partial, itemId: nil) == "Requested")
        #expect(RequestWords.badge(.requested, itemId: nil) == "Requested")
        #expect(RequestWords.badge(.declined, itemId: nil) == "Declined")
        #expect(RequestWords.badge(.none, itemId: nil) == nil && RequestWords.badge(.restricted, itemId: nil) == nil)
    }

    @Test func refusalsWithAndWithoutATime() {
        #expect(RequestWords.atMost(seasonsLimit: 5, moviesLimit: 3) == "You can ask for at most 5 seasons per 7 days: pick fewer.")
        #expect(RequestWords.atMost(seasonsLimit: nil, moviesLimit: 1) == "You can ask for at most 1 movie per 7 days.")
        #expect(RequestWords.atMost(seasonsLimit: nil, moviesLimit: nil) == "You've asked for all you can for now.")
        #expect(RequestWords.askMoreAt(now + 3 * day, now: now, zone: utc) == "You can ask for more in 3 days")
        #expect(RequestWords.askAgainAt(now + 12 * day, now: now, zone: utc) == "You can ask again in 12 days")
    }
}
