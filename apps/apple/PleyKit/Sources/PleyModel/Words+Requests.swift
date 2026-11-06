import Foundation

/// Requests' words, as the Android app says them (`strings_requests.xml`,
/// `RequestTexts`). Buttons say Request; running text says "ask for".
/// Limits are "per 7 days" or "for now", never "this week". Times are
/// relative to `now` (unix seconds), and days are in `zone`: "in 3 days",
/// "tomorrow", "Out Nov 4". In the language shown, dates in its locale.
public enum RequestWords {
    // MARK: Pages and ways in

    public static var discover: String { localized("Discover") }
    public static var yourRequests: String { localized("Your requests") }
    /// The managers' page: everyone's requests.
    public static var manage: String { localized("Requests") }
    public static var attention: String { localized("Needs attention") }
    /// Under Discover's way in (Search, Me).
    public static var discoverHint: String { localized("Find movies and shows to ask for") }
    /// Under Requests' way in, for managers.
    public static var manageHint: String { localized("Approve, decline and follow what was asked for") }
    public static var credit: String { localized("Details and pictures from TMDB.") }
    /// Settings → About.
    public static var creditAbout: String { localized("This product uses the TMDB API but is not endorsed or certified by TMDB.") }
    public static var seeAll: String { localized("See all") }
    public static var nothingInLibraries: String { localized("Nothing in your libraries matches") }
    public static var nothingHere: String { localized("Nothing here right now") }
    public static var seasonsHeading: String { localized("Seasons") }
    public static var cast: String { localized("Cast") }
    public static var moreActions: String { localized("More actions") }
    public static var retry: String { localized("Retry") }
    public static var cancel: String { localized("Cancel") }

    // MARK: A title page's main action

    public static var open: String { localized("Open") }
    public static var request: String { localized("Request") }
    public static var requested: String { localized("Requested") }
    public static var following: String { localized("Following") }
    public static var stopFollowing: String { localized("Stop following") }
    public static var tellMeToo: String { localized("Tell me too") }
    public static var askedAlready: String { localized("Someone asked for it already.") }
    public static var moreSeasons: String { localized("Request more seasons") }
    public static var readyElsewhere: String { localized("Ready, in a library you can't open: ask an administrator") }
    public static var watch: String { localized("Watch") }
    public static var details: String { localized("Details") }
    /// Where Request would be, with access "off".
    public static var noAccessLine: String { localized("Requests aren't open to this account.") }
    public static var pickerTitle: String { localized("Which seasons?") }

    // MARK: Problems

    public static var offTitle: String { localized("Requests are off") }
    public static var offBody: String { localized("This server doesn't take requests for movies and shows.") }
    public static var noAccessTitle: String { localized("Requests aren't open to this account") }
    public static var noAccessBody: String { localized("An administrator can let it ask for movies and shows.") }
    public static var tmdbDown: String { localized("TMDB isn't answering") }
    public static var tmdbDownBody: String { localized("Its details and pictures come back once it answers again.") }
    public static var tmdbKey: String { localized("TMDB refused this server's key") }
    public static var tmdbKeyManage: String { localized("Fix it in the server's settings: Settings → Server, on the web.") }
    public static var tmdbKeyOthers: String { localized("An administrator needs to check it.") }
    public static var signInTitle: String { localized("Sign in again to ask for movies and shows") }
    public static var signInApprovals: String { localized("Sign in again to approve and decline here.") }
    public static var signIn: String { localized("Sign in") }
    /// What the servers' screen says while the app signs in again for requests.
    public static var signInNotice: String { localized("Sign in again to let Pley ask for movies and shows.") }
    public static var notFound: String { localized("This title isn't on TMDB") }
    public static var notFoundBody: String { localized("It may have been taken off since.") }
    public static var discoverFailed: String { localized("Couldn't load Discover") }
    public static var searchFailed: String { localized("Couldn't search TMDB") }
    public static var titleFailed: String { localized("Couldn't load this title") }
    public static var listFailed: String { localized("Couldn't load the requests") }
    public static var allForNow: String { localized("You've asked for all you can for now.") }
    public static var declinedOnly: String { localized("Declined.") }

    // MARK: Lists

    public static var filterAll: String { localized("All") }
    public static var filterMovies: String { localized("Movies") }
    public static var filterShows: String { localized("Shows") }
    public static var filterEveryone: String { localized("Everyone") }
    public static var filterMine: String { localized("Yours") }
    public static var filterTitle: String { localized("Search requests") }
    public static var withdraw: String { localized("Withdraw") }
    public static var remove: String { localized("Remove") }
    public static var approve: String { localized("Approve") }
    public static var approveWith: String { localized("Approve…") }
    public static var tryAgain: String { localized("Try again") }
    public static var decline: String { localized("Decline") }
    public static var handled: String { localized("Mark as added by hand") }
    public static var delete: String { localized("Delete") }
    public static var askAgain: String { localized("Request again") }

    /// A status filter's name: "Pending", "Failed".
    public static func filter(_ status: RequestStatus) -> String {
        switch status {
        case .pending: localized("Pending")
        case .approved: localized(keyed: LocalizedStringResource("requests.filter.approved", defaultValue: "Approved",
                                                                  bundle: .atURL(Bundle.module.bundleURL)))
        case .available: localized("Available")
        case .declined: localized(keyed: LocalizedStringResource("requests.filter.declined", defaultValue: "Declined",
                                                                  bundle: .atURL(Bundle.module.bundleURL)))
        case .failed: localized("Failed")
        default: status.rawValue.capitalized
        }
    }

    /// A filter with its count: "Needs attention · 3".
    public static func filterCount(_ name: String, _ count: Int) -> String { "\(name) · \(count)" }

    /// See all's heading: "Results for “dune”".
    public static func results(_ query: String) -> String { localized("Results for “\(query)”") }
    public static var noneTitle: String { localized("Nothing asked for yet") }
    public static var noneBody: String { localized("Find movies and shows in Discover, and ask for what the libraries don't have.") }
    public static var noneBodyOff: String { localized("Requests you made or follow show here.") }
    public static var attentionNone: String { localized("Nothing needs you") }
    public static var attentionNoneBody: String {
        localized("Requests waiting for a decision, and those that couldn't be sent, show here.")
    }
    public static var filteredNone: String { localized("No requests match") }
    public static var filteredNoneBody: String { localized("Try other filters.") }

    // MARK: The managers' dialogs

    public static var approveProfile: String { localized("Quality profile") }
    public static var approveFolder: String { localized("Folder") }
    public static var approveSeasons: String { localized("Seasons") }
    public static var approveSettings: String { localized("Approve with the settings' choices") }
    public static var approveDefault: String { localized("As in the settings") }
    public static var declineReason: String { localized("Reason (optional)") }
    public static var declineReasonHint: String { localized("The person who asked sees it.") }

    /// "Approve “Arcane”".
    public static func approveTitle(_ title: String) -> String { localized("Approve “\(title)”") }

    /// "Decline “Arcane”?".
    public static func declineTitle(_ title: String) -> String { localized("Decline “\(title)”?") }

    /// "1.8 TB free".
    public static func folderFree(_ bytes: Int) -> String { localized("\(Words.bytes(bytes)) free") }

    // MARK: Titles

    /// "Movie", "Show".
    public static func kind(_ kind: TitleKind) -> String { kind == .show ? localized("Show") : localized("Movie") }

    /// A title's badge on its poster: none for titles that may be asked
    /// for, or can't be. A show partly asked for, with nothing in a
    /// library, is Requested.
    public static func badge(_ status: TitleStatus, itemId: Int?) -> String? {
        switch status {
        case .library: localized("In library")
        case .partial: itemId != nil ? localized("Partly in library") : localized("Requested")
        case .requested: localized("Requested")
        case .declined: localized("Declined")
        default: nil
        }
    }

    /// "Declined: “Not for us”. You can ask again in 12 days."
    public static func declined(reason: String?, againAt: Int?, now: Int, zone: TimeZone = .current) -> String {
        let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty
        let again = againAt.map { until($0, now: now, zone: zone) }
        switch (reason, again) {
        case (let reason?, let again?): return localized("Declined: “\(reason)”. You can ask again \(again).")
        case (nil, let again?): return localized("Declined. You can ask again \(again).")
        case (let reason?, nil): return localized("Declined: “\(reason)”.")
        case (nil, nil): return declinedOnly
        }
    }

    /// "Search results for “dune”".
    public static func searchNone(_ query: String) -> String { localized("Nothing on TMDB matches “\(query)”") }
    public static var searchNoneBody: String { localized("Try a different spelling, or the title in its own language.") }

    // MARK: Seasons

    /// Season numbers as ranges: `[1, 2, 3, 5]` → "1–3, 5" (an en dash), in
    /// order, each once. The specials (0) aren't among them.
    public static func seasonRanges(_ numbers: [Int]) -> String {
        let sorted = Array(Set(numbers.filter { $0 > 0 })).sorted()
        guard var start = sorted.first else { return "" }
        var end = start
        var parts: [String] = []
        func range(_ a: Int, _ b: Int) -> String { a == b ? "\(a)" : "\(a)–\(b)" }
        for n in sorted.dropFirst() {
            if n == end + 1 {
                end = n
            } else {
                parts.append(range(start, end))
                start = n
                end = n
            }
        }
        parts.append(range(start, end))
        return parts.joined(separator: ", ")
    }

    /// "Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials".
    public static func seasons(_ numbers: [Int]) -> String {
        let specials = numbers.contains(0)
        let rest = Set(numbers.filter { $0 > 0 })
        let ranges = seasonRanges(Array(rest))
        switch rest.count {
        case 0: return specials ? localized("Specials") : ""
        case 1: return specials ? localized("Season \(ranges) and specials") : localized("Season \(ranges)")
        default: return specials ? localized("Seasons \(ranges) and specials") : localized("Seasons \(ranges)")
        }
    }

    /// A season's name on its own: TMDB's, else "Season 2" or "Specials".
    public static func seasonName(_ season: DiscoverSeason) -> String {
        season.name.trimmingCharacters(in: .whitespaces).isEmpty ? seasons([season.number]) : season.name
    }

    /// How a season stands for this account: "In library", "4 of 10
    /// episodes in library", "Airs Nov 4, 2026", "10 episodes".
    public static func seasonState(_ season: DiscoverSeason, locale: Locale = ShownLanguage.locale) -> String {
        if season.declined { return stageDeclined }
        switch season.state {
        case .library:
            return localized(keyed: LocalizedStringResource("requests.season.library", defaultValue: "In library",
                                                            bundle: .atURL(Bundle.module.bundleURL)))
        case .requested: return stageRequested
        case .partial:
            let total = max(season.episodes, season.inLibrary)
            return localized("\(season.inLibrary) of \(total) episodes in library")
        case .unaired:
            guard let date = season.airDate, !date.isEmpty else { return localized("Not aired yet") }
            return localized("Airs \(Words.date(date, locale: locale))")
        default: return localized("\(season.episodes) episodes")
        }
    }

    // MARK: Stages

    /// The words of `pill`: "Waiting for approval", "Downloading 42% · in 20 min", "Out Nov 4".
    public static func pill(_ pill: StagePill, now: Int, zone: TimeZone = .current,
                            locale: Locale = ShownLanguage.locale) -> String {
        switch pill {
        case .pending: return localized("Waiting for approval")
        case .approved: return localized("Approved")
        case .waiting(let show, let at):
            guard let at else { return localized("Not out yet") }
            let day = day(at, now: now, zone: zone, locale: locale)
            return show ? localized("Airs \(day)") : localized("Out \(day)")
        case .searching: return localized("Searching")
        case .downloading(let progress, let eta, let state):
            switch state {
            case .queued: return localized("Waiting to download")
            case .paused: return localized("Download paused")
            case .warning: return localized("The download has a problem")
            default:
                let words = localized("Downloading \(Int(progress * 100))%")
                guard let eta, eta > now else { return words }
                return "\(words) · \(until(eta, now: now, zone: zone))"
            }
        case .importing: return localized("Adding to the library")
        case .partial(let seasons): return localized("\(Self.seasons(seasons)) ready")
        case .available: return localized("Ready to watch")
        case .gone: return localized("No longer in a library")
        case .declined: return stageDeclined
        case .couldNotSend(let reason, _):
            guard let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
                return localized("Couldn't send")
            }
            return localized("Couldn't send: \(reason)")
        case .notSentYet: return localized("Not sent yet")
        case .unknown: return stageRequested
        }
    }

    /// A request's or a season's state (Spanish and French say it of a
    /// request, feminine), apart from a title's badge.
    private static var stageRequested: String {
        localized(keyed: LocalizedStringResource("requests.stage.requested", defaultValue: "Requested", bundle: .atURL(Bundle.module.bundleURL)))
    }

    private static var stageDeclined: String {
        localized(keyed: LocalizedStringResource("requests.stage.declined", defaultValue: "Declined", bundle: .atURL(Bundle.module.bundleURL)))
    }

    /// The line under a failed request's pill: when the server tries
    /// again, or that an administrator will look.
    public static func pillLine(_ pill: StagePill, now: Int, zone: TimeZone = .current) -> String? {
        switch pill {
        case .notSentYet(let retryAt):
            guard let retryAt else {
                return localized("It couldn't be sent to download yet: an administrator will look at it.")
            }
            let again = until(retryAt, now: now, zone: zone)
            return localized("Trying again \(again).")
        case .couldNotSend(_, let retryAt):
            return retryAt.map { let again = until($0, now: now, zone: zone); return localized("Trying again \(again).") }
        default:
            return nil
        }
    }

    /// Who asked and when: "Ann · 3 days ago", "Asked for by someone else · 3 days ago", or just when.
    public static func asked(_ request: MediaRequest, manage: Bool, now: Int) -> String {
        let at = ago(request.createdAt, now: now)
        if request.mine && !manage { return at }
        return "\(request.requestedBy?.name ?? localized("Asked for by someone else")) · \(at)"
    }

    /// "1 person follows it", "3 people follow it".
    public static func watchers(_ count: Int) -> String {
        localized("\(count) people follow it")
    }

    /// Managers: where it went, "Radarr · HD-1080p · /movies"; `nil` before it was sent.
    public static func sentTo(_ request: MediaRequest) -> String? {
        guard let sent = request.sentTo else { return nil }
        let service = request.kind == .show ? "Sonarr" : "Radarr"
        return [service, sent.qualityProfile, sent.rootFolder].compactMap { $0 }.joined(separator: " · ")
    }

    // MARK: Allowance

    /// The allowance lines, for `kind` (`nil`: both): "You can ask for 2
    /// more movies and 5 more seasons." with "Limits: 3 movies and 10
    /// seasons per 7 days.", or when more can be asked for. None without limits.
    public static func allowance(_ allowance: Allowance, kind: TitleKind? = nil, now: Int,
                                 zone: TimeZone = .current) -> [String] {
        let movies = kind != .show && allowance.moviesLeft != nil
        let shows = kind != .movie && allowance.seasonsLeft != nil
        guard movies || shows else { return [] }
        let moviesOut = movies && allowance.moviesLeft == 0
        let seasonsOut = shows && allowance.seasonsLeft == 0
        let moviesAt = allowance.moviesNextAt.map { until($0, now: now, zone: zone) }
        let seasonsAt = allowance.seasonsNextAt.map { until($0, now: now, zone: zone) }
        func allMovies() -> String {
            moviesAt.map { localized("You've asked for all the movies you can for now: you can ask for more \($0).") }
                ?? allForNow
        }
        func allSeasons() -> String {
            seasonsAt.map { localized("You've asked for all the seasons you can for now: you can ask for more \($0).") }
                ?? allForNow
        }
        if moviesOut && seasonsOut {
            if let moviesAt, let seasonsAt {
                return [localized("You've asked for all you can for now: more movies \(moviesAt), more seasons \(seasonsAt).")]
            }
            return [allForNow]
        }
        if moviesOut && !shows { return [allMovies()] }
        if seasonsOut && !movies { return [allSeasons()] }
        var lines: [String] = []
        if moviesOut { lines.append(allMovies()) }
        if seasonsOut { lines.append(allSeasons()) }
        let left = [
            allowance.moviesLeft.flatMap { movies && $0 > 0 ? localized("\($0) more movies") : nil },
            allowance.seasonsLeft.flatMap { shows && $0 > 0 ? localized("\($0) more seasons") : nil },
        ].compactMap { $0 }
        switch left.count {
        case 2: lines.append(localized("You can ask for \(left[0]) and \(left[1])."))
        case 1: lines.append(localized("You can ask for \(left[0])."))
        default: break
        }
        let limits = [
            allowance.moviesLimit.flatMap { movies ? movieCount($0) : nil },
            allowance.seasonsLimit.flatMap { shows ? seasonCount($0) : nil },
        ].compactMap { $0 }
        switch limits.count {
        case 2: lines.append(localized("Limits: \(limits[0]) and \(limits[1]) per 7 days."))
        case 1: lines.append(localized("Limit: \(limits[0]) per 7 days."))
        default: break
        }
        return lines
    }

    /// A `429` with no time: more was asked than the limit itself allows.
    public static func atMost(seasonsLimit: Int?, moviesLimit: Int?) -> String {
        if let seasonsLimit {
            let most = seasonCount(seasonsLimit)
            return localized("You can ask for at most \(most) per 7 days: pick fewer.")
        }
        if let moviesLimit {
            let most = movieCount(moviesLimit)
            return localized("You can ask for at most \(most) per 7 days.")
        }
        return allForNow
    }

    /// "3 movies", "1 season": a limit's count, put in a sentence.
    private static func movieCount(_ n: Int) -> String { localized("\(n) movies") }
    private static func seasonCount(_ n: Int) -> String { localized("\(n) seasons") }

    // MARK: Asking

    /// What asking for `title` did: asked (waiting, approved, or not sent yet), or followed.
    public static func asked(_ asked: Asked, title: String) -> String {
        if !asked.created { return localized("You'll hear when “\(title)” is ready.") }
        switch asked.request.status {
        case .approved, .available: return localized("“\(title)” is approved and on its way.")
        case .failed: return localized("Asked for “\(title)”, but it couldn't be sent to download yet.")
        default: return localized("Asked for “\(title)”: an administrator decides.")
        }
    }

    /// "Request 3 seasons", or "Pick a season." with none picked.
    public static func pickerButton(_ count: Int) -> String {
        count == 0 ? localized("Pick a season.") : localized("Request \(count) seasons")
    }

    /// "You can ask again in 12 days".
    public static func askAgainAt(_ ts: Int, now: Int, zone: TimeZone = .current) -> String {
        let again = until(ts, now: now, zone: zone)
        return localized("You can ask again \(again)")
    }

    /// "You can ask for more in 3 days".
    public static func askMoreAt(_ ts: Int, now: Int, zone: TimeZone = .current) -> String {
        let more = until(ts, now: now, zone: zone)
        return localized("You can ask for more \(more)")
    }

    // MARK: Actions

    /// "Withdraw your request for “X”?"
    public static func withdrawConfirm(_ title: String) -> String {
        localized("Withdraw your request for “\(title)”?")
    }

    /// "Remove “X” from your requests?"
    public static func removeConfirm(_ title: String) -> String { localized("Remove “\(title)” from your requests?") }

    public static var deleteConfirm: String { localized("Delete this request? Radarr and Sonarr keep what they have.") }

    // MARK: On Apple TV

    public static var declineWithReason: String { localized("Decline with a reason…") }
    public static var showMore: String { localized("Show more") }
    public static var close: String { localized("Close") }
    public static var kindAll: String { localized("Movies and shows") }
    public static var askedBy: String { localized("Asked by") }
    public static var find: String { localized("Find something to ask for") }
    public static var nothingBody: String { localized("TMDB's lists have nothing this account can ask for. Search for a title instead.") }
    public static var listNothingBody: String { localized("TMDB's list has nothing this account can ask for.") }
    public static var gridFailed: String { localized("Couldn't load it") }
    /// Beside a grid's lone Show more: the pages read so far had nothing.
    public static var noneYet: String { localized("Nothing here yet") }
    public static var noneYetBody: String { localized("TMDB's pages read so far have nothing this account can ask for.") }
    public static func searchNoneYet(_ query: String) -> String { localized("Nothing yet for “\(query)”") }
    public static var signInAsk: String { localized("Sign in again?") }
    /// "This Apple TV shows a code to link it to Home again."
    public static func signInAskBody(_ server: String) -> String {
        localized("This Apple TV shows a code to link it to \(server) again.")
    }

    // MARK: News and the badge

    /// The number on the way into requests, said for VoiceOver: "3
    /// requests need you, 1 couldn't be sent" for managers, "2 of your
    /// requests have news" for others.
    public static func count(manage: Bool, badge: Int, failed: Int) -> String {
        guard manage else { return news(badge) }
        let need = localized("\(badge) requests need you")
        guard failed > 0 else { return need }
        let unsent = localized("\(failed) couldn't be sent")
        return "\(need), \(unsent)"
    }

    /// The badge's number: "99+" past 99; `nil` at 0.
    public static func badgeText(_ count: Int) -> String? {
        guard count > 0 else { return nil }
        return count > 99 ? "99+" : String(count)
    }

    /// "1 of your requests has news", "3 of your requests have news".
    public static func news(_ count: Int) -> String {
        localized("\(count) of your requests have news")
    }

    /// What a toast says of `news`.
    public static func notice(_ news: RequestNews) -> String {
        switch news {
        case .ready(_, let title, _, _, let seasons, let complete, _):
            let seasons = seasons ?? []
            if complete || seasons.isEmpty { return localized("“\(title)” is ready to watch.") }
            let which = Self.seasons(seasons)
            return seasons.count == 1 ? localized("\(which) of “\(title)” is ready to watch.")
                : localized("\(which) of “\(title)” are ready to watch.")
        case .approved(_, let title):
            return localized("“\(title)” was approved.")
        case .declined(_, let title, let reason):
            guard let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
                return localized("“\(title)” was declined.")
            }
            return localized("“\(title)” was declined: “\(reason)”.")
        case .asked(_, let title, let by, let seasons):
            let who = by?.trimmingCharacters(in: .whitespaces).nonEmpty ?? localized("Someone")
            guard let seasons, !seasons.isEmpty else { return localized("\(who) asked for “\(title)”.") }
            let which = Self.seasons(seasons)
            return localized("\(who) asked for “\(title)” (\(which)).")
        case .failed(_, let title, let kind, let reason):
            guard let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
                return kind == .show ? localized("“\(title)” couldn't be sent to Sonarr.")
                    : localized("“\(title)” couldn't be sent to Radarr.")
            }
            return kind == .show ? localized("“\(title)” couldn't be sent to Sonarr: \(reason)")
                : localized("“\(title)” couldn't be sent to Radarr: \(reason)")
        case .news(let count):
            return Self.news(count)
        }
    }

    // MARK: Times

    /// How long until `ts`: "in 5 min", "in 3 hours", "tomorrow", "in 12
    /// days"; "in a moment" once it's past.
    public static func until(_ ts: Int, now: Int, zone: TimeZone = .current) -> String {
        let diff = ts - now
        if diff <= 0 { return localized("in a moment") }
        if diff < 3_600 { return localized("in \(max(Int((Double(diff) / 60).rounded(.up)), 1)) min") }
        if diff < 86_400 {
            let hours = min(max(Int((Double(diff) / 3_600).rounded()), 1), 23)
            return localized("in \(hours) hours")
        }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        let from = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(now)))
        let to = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(ts)))
        let days = max(calendar.dateComponents([.day], from: from, to: to).day ?? 1, 1)
        return days == 1 ? localized("tomorrow") : localized("in \(days) days")
    }

    /// How long ago `ts` was: "3 days ago", "yesterday".
    public static func ago(_ ts: Int, now: Int) -> String { Words.relative(ts, now: now) }

    /// A day, "Nov 4" ("Nov 4, 2027" in another year), with the time
    /// within 24 hours: "today, 9:00 PM".
    public static func day(_ ts: Int, now: Int, zone: TimeZone = .current,
                           locale: Locale = ShownLanguage.locale) -> String {
        let at = Date(timeIntervalSince1970: TimeInterval(ts))
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        let diff = ts - now
        let format = DateFormatter()
        format.locale = locale
        format.timeZone = zone
        if (0..<86_400).contains(diff) {
            format.dateStyle = .none
            format.timeStyle = .short
            let today = calendar.isDate(at, inSameDayAs: Date(timeIntervalSince1970: TimeInterval(now)))
            let time = format.string(from: at)
            return today ? localized("today, \(time)") : localized("tomorrow, \(time)")
        }
        let sameYear = calendar.component(.year, from: at)
            == calendar.component(.year, from: Date(timeIntervalSince1970: TimeInterval(now)))
        format.setLocalizedDateFormatFromTemplate(sameYear ? "MMMd" : "yMMMd")
        return format.string(from: at)
    }
}

extension String {
    /// The string, or `nil` when it's empty.
    var nonEmpty: String? { isEmpty ? nil : self }
}
