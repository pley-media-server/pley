import Foundation

/// Requests' words, as the Android app says them (`strings_requests.xml`,
/// `RequestTexts`). Buttons say Request; running text says "ask for".
/// Limits are "per 7 days" or "for now", never "this week". Times are
/// relative to `now` (unix seconds), and days are in `zone`: "in 3 days",
/// "tomorrow", "Out Nov 4".
public enum RequestWords {
    // MARK: Pages and ways in

    public static let discover = "Discover"
    public static let yourRequests = "Your requests"
    /// The managers' page: everyone's requests.
    public static let manage = "Requests"
    public static let attention = "Needs attention"
    /// Under Discover's way in (Search, Me).
    public static let discoverHint = "Find movies and shows to ask for"
    /// Under Requests' way in, for managers.
    public static let manageHint = "Approve, decline and follow what was asked for"
    public static let credit = "Details and pictures from TMDB."
    /// Settings → About.
    public static let creditAbout = "This product uses the TMDB API but is not endorsed or certified by TMDB."
    public static let seeAll = "See all"
    public static let nothingInLibraries = "Nothing in your libraries matches"
    public static let nothingHere = "Nothing here right now"
    public static let seasonsHeading = "Seasons"
    public static let cast = "Cast"
    public static let moreActions = "More actions"
    public static let retry = "Retry"
    public static let cancel = "Cancel"

    // MARK: A title page's main action

    public static let open = "Open"
    public static let request = "Request"
    public static let requested = "Requested"
    public static let following = "Following"
    public static let stopFollowing = "Stop following"
    public static let tellMeToo = "Tell me too"
    public static let askedAlready = "Someone asked for it already."
    public static let moreSeasons = "Request more seasons"
    public static let readyElsewhere = "Ready, in a library you can't open: ask an administrator"
    public static let watch = "Watch"
    public static let details = "Details"
    /// Where Request would be, with access "off".
    public static let noAccessLine = "Requests aren't open to this account."
    public static let pickerTitle = "Which seasons?"

    // MARK: Problems

    public static let offTitle = "Requests are off"
    public static let offBody = "This server doesn't take requests for movies and shows."
    public static let noAccessTitle = "Requests aren't open to this account"
    public static let noAccessBody = "An administrator can let it ask for movies and shows."
    public static let tmdbDown = "TMDB isn't answering"
    public static let tmdbDownBody = "Its details and pictures come back once it answers again."
    public static let tmdbKey = "TMDB refused this server's key"
    public static let tmdbKeyManage = "Fix it in the server's settings: Settings → Server, on the web."
    public static let tmdbKeyOthers = "An administrator needs to check it."
    public static let signInTitle = "Sign in again to ask for movies and shows"
    public static let signInBody = "This app signed in before it could ask. Signing in again lets it."
    public static let signInApprovals = "Sign in again to approve and decline here."
    public static let signIn = "Sign in again"
    /// What the servers' screen says while the app signs in again for requests.
    public static let signInNotice = "Sign in again to let Pley ask for movies and shows."
    public static let notFound = "This title isn't on TMDB"
    public static let notFoundBody = "It may have been taken off since."
    public static let discoverFailed = "Couldn't load Discover"
    public static let searchFailed = "Couldn't search TMDB"
    public static let titleFailed = "Couldn't load this title"
    public static let listFailed = "Couldn't load the requests"
    public static let allForNow = "You've asked for all you can for now."
    public static let declinedOnly = "Declined."

    // MARK: Lists

    public static let filterAll = "All"
    public static let filterMovies = "Movies"
    public static let filterShows = "Shows"
    public static let filterEveryone = "Everyone"
    public static let filterMine = "Yours"
    public static let filterTitle = "Search requests"
    public static let withdraw = "Withdraw"
    public static let remove = "Remove"
    public static let approve = "Approve"
    public static let approveWith = "Approve…"
    public static let tryAgain = "Try again"
    public static let decline = "Decline"
    public static let handled = "Mark as added by hand"
    public static let delete = "Delete"
    public static let askAgain = "Request again"

    /// A status filter's name: "Pending", "Failed".
    public static func filter(_ status: RequestStatus) -> String {
        switch status {
        case .pending: "Pending"
        case .approved: "Approved"
        case .available: "Available"
        case .declined: "Declined"
        case .failed: "Failed"
        default: status.rawValue.capitalized
        }
    }

    /// A filter with its count: "Needs attention · 3".
    public static func filterCount(_ name: String, _ count: Int) -> String { "\(name) · \(count)" }

    /// See all's heading: "Results for “dune”".
    public static func results(_ query: String) -> String { "Results for \u{201C}\(query)\u{201D}" }
    public static let noneTitle = "Nothing asked for yet"
    public static let noneBody = "Find movies and shows in Discover, and ask for what the libraries don't have."
    public static let noneBodyOff = "Requests you made or follow show here."
    public static let attentionNone = "Nothing needs you"
    public static let attentionNoneBody =
        "Requests waiting for a decision, and those that couldn't be sent, show here."
    public static let filteredNone = "No requests match"
    public static let filteredNoneBody = "Try other filters."

    // MARK: The managers' dialogs

    public static let approveProfile = "Quality profile"
    public static let approveFolder = "Folder"
    public static let approveSeasons = "Seasons"
    public static let approveSettings = "Approve with the settings' choices"
    public static let approveDefault = "As in the settings"
    public static let declineReason = "Reason (optional)"
    public static let declineReasonHint = "The person who asked sees it."

    /// "Approve “Arcane”".
    public static func approveTitle(_ title: String) -> String { "Approve \u{201C}\(title)\u{201D}" }

    /// "Decline “Arcane”?".
    public static func declineTitle(_ title: String) -> String { "Decline \u{201C}\(title)\u{201D}?" }

    /// "1.8 TB free".
    public static func folderFree(_ bytes: Int) -> String { "\(Words.bytes(bytes)) free" }

    // MARK: Titles

    /// "Movie", "Show".
    public static func kind(_ kind: TitleKind) -> String { kind == .show ? "Show" : "Movie" }

    /// A title's badge on its poster: none for titles that may be asked
    /// for, or can't be. A show partly asked for, with nothing in a
    /// library, is Requested.
    public static func badge(_ status: TitleStatus, itemId: Int?) -> String? {
        switch status {
        case .library: "In library"
        case .partial: itemId != nil ? "Partly in library" : "Requested"
        case .requested: "Requested"
        case .declined: "Declined"
        default: nil
        }
    }

    /// "Declined: “Not for us”. You can ask again in 12 days."
    public static func declined(reason: String?, againAt: Int?, now: Int, zone: TimeZone = .current) -> String {
        let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty
        let again = againAt.map { until($0, now: now, zone: zone) }
        switch (reason, again) {
        case (let reason?, let again?): return "Declined: \u{201C}\(reason)\u{201D}. You can ask again \(again)."
        case (nil, let again?): return "Declined. You can ask again \(again)."
        case (let reason?, nil): return "Declined: \u{201C}\(reason)\u{201D}."
        case (nil, nil): return declinedOnly
        }
    }

    /// "Search results for “dune”".
    public static func searchNone(_ query: String) -> String { "Nothing on TMDB matches \u{201C}\(query)\u{201D}" }
    public static let searchNoneBody = "Try a different spelling, or the title in its own language."

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
        case 0: return specials ? "Specials" : ""
        case 1: return specials ? "Season \(ranges) and specials" : "Season \(ranges)"
        default: return specials ? "Seasons \(ranges) and specials" : "Seasons \(ranges)"
        }
    }

    /// A season's name on its own: TMDB's, else "Season 2" or "Specials".
    public static func seasonName(_ season: DiscoverSeason) -> String {
        season.name.trimmingCharacters(in: .whitespaces).isEmpty ? seasons([season.number]) : season.name
    }

    /// How a season stands for this account: "In library", "4 of 10
    /// episodes in library", "Airs Nov 4, 2026", "10 episodes".
    public static func seasonState(_ season: DiscoverSeason, locale: Locale = .current) -> String {
        if season.declined { return "Declined" }
        switch season.state {
        case .library: return "In library"
        case .requested: return "Requested"
        case .partial: return "\(season.inLibrary) of \(max(season.episodes, season.inLibrary)) episodes in library"
        case .unaired:
            guard let date = season.airDate, !date.isEmpty else { return "Not aired yet" }
            return "Airs \(Words.date(date, locale: locale))"
        default: return Words.pluralize(season.episodes, "episode")
        }
    }

    // MARK: Stages

    /// The words of `pill`: "Waiting for approval", "Downloading 42% · in 20 min", "Out Nov 4".
    public static func pill(_ pill: StagePill, now: Int, zone: TimeZone = .current,
                            locale: Locale = .current) -> String {
        switch pill {
        case .pending: return "Waiting for approval"
        case .approved: return "Approved"
        case .waiting(let show, let at):
            guard let at else { return "Not out yet" }
            return "\(show ? "Airs" : "Out") \(day(at, now: now, zone: zone, locale: locale))"
        case .searching: return "Searching"
        case .downloading(let progress, let eta, let state):
            switch state {
            case .queued: return "Waiting to download"
            case .paused: return "Download paused"
            case .warning: return "The download has a problem"
            default:
                let words = "Downloading \(Int(progress * 100))%"
                guard let eta, eta > now else { return words }
                return "\(words) · \(until(eta, now: now, zone: zone))"
            }
        case .importing: return "Adding to the library"
        case .partial(let seasons): return "\(Self.seasons(seasons)) ready"
        case .available: return "Ready to watch"
        case .gone: return "No longer in a library"
        case .declined: return "Declined"
        case .couldNotSend(let reason, _):
            guard let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
                return "Couldn't send"
            }
            return "Couldn't send: \(reason)"
        case .notSentYet: return "Not sent yet"
        case .unknown: return "Requested"
        }
    }

    /// The line under a failed request's pill: when the server tries
    /// again, or that an administrator will look.
    public static func pillLine(_ pill: StagePill, now: Int, zone: TimeZone = .current) -> String? {
        switch pill {
        case .notSentYet(let retryAt):
            guard let retryAt else { return "It couldn't be sent to download yet: an administrator will look at it." }
            return "Trying again \(until(retryAt, now: now, zone: zone))."
        case .couldNotSend(_, let retryAt):
            return retryAt.map { "Trying again \(until($0, now: now, zone: zone))." }
        default:
            return nil
        }
    }

    /// Who asked and when: "Ann · 3 days ago", "Asked for by someone else · 3 days ago", or just when.
    public static func asked(_ request: MediaRequest, manage: Bool, now: Int) -> String {
        let at = ago(request.createdAt, now: now)
        if request.mine && !manage { return at }
        return "\(request.requestedBy?.name ?? "Asked for by someone else") · \(at)"
    }

    /// "1 person follows it", "3 people follow it".
    public static func watchers(_ count: Int) -> String {
        count == 1 ? "1 person follows it" : "\(count) people follow it"
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
            moviesAt.map { "You've asked for all the movies you can for now: you can ask for more \($0)." } ?? allForNow
        }
        func allSeasons() -> String {
            seasonsAt.map { "You've asked for all the seasons you can for now: you can ask for more \($0)." }
                ?? allForNow
        }
        if moviesOut && seasonsOut {
            if let moviesAt, let seasonsAt {
                return ["You've asked for all you can for now: more movies \(moviesAt), more seasons \(seasonsAt)."]
            }
            return [allForNow]
        }
        if moviesOut && !shows { return [allMovies()] }
        if seasonsOut && !movies { return [allSeasons()] }
        var lines: [String] = []
        if moviesOut { lines.append(allMovies()) }
        if seasonsOut { lines.append(allSeasons()) }
        let left = [
            allowance.moviesLeft.flatMap { movies && $0 > 0 ? "\($0) more \($0 == 1 ? "movie" : "movies")" : nil },
            allowance.seasonsLeft.flatMap { shows && $0 > 0 ? "\($0) more \($0 == 1 ? "season" : "seasons")" : nil },
        ].compactMap { $0 }
        switch left.count {
        case 2: lines.append("You can ask for \(left[0]) and \(left[1]).")
        case 1: lines.append("You can ask for \(left[0]).")
        default: break
        }
        let limits = [
            allowance.moviesLimit.flatMap { movies ? Words.pluralize($0, "movie") : nil },
            allowance.seasonsLimit.flatMap { shows ? Words.pluralize($0, "season") : nil },
        ].compactMap { $0 }
        switch limits.count {
        case 2: lines.append("Limits: \(limits[0]) and \(limits[1]) per 7 days.")
        case 1: lines.append("Limit: \(limits[0]) per 7 days.")
        default: break
        }
        return lines
    }

    /// A `429` with no time: more was asked than the limit itself allows.
    public static func atMost(seasonsLimit: Int?, moviesLimit: Int?) -> String {
        if let seasonsLimit { return "You can ask for at most \(Words.pluralize(seasonsLimit, "season")) per 7 days: pick fewer." }
        if let moviesLimit { return "You can ask for at most \(Words.pluralize(moviesLimit, "movie")) per 7 days." }
        return allForNow
    }

    // MARK: Asking

    /// What asking for `title` did: asked (waiting, approved, or not sent yet), or followed.
    public static func asked(_ asked: Asked, title: String) -> String {
        let quoted = "\u{201C}\(title)\u{201D}"
        if !asked.created { return "You'll hear when \(quoted) is ready." }
        switch asked.request.status {
        case .approved, .available: return "\(quoted) is approved and on its way."
        case .failed: return "Asked for \(quoted), but it couldn't be sent to download yet."
        default: return "Asked for \(quoted): an administrator decides."
        }
    }

    /// "Request 3 seasons", or "Pick a season." with none picked.
    public static func pickerButton(_ count: Int) -> String {
        count == 0 ? "Pick a season." : "Request \(count) \(count == 1 ? "season" : "seasons")"
    }

    /// "You can ask again in 12 days".
    public static func askAgainAt(_ ts: Int, now: Int, zone: TimeZone = .current) -> String {
        "You can ask again \(until(ts, now: now, zone: zone))"
    }

    /// "You can ask for more in 3 days".
    public static func askMoreAt(_ ts: Int, now: Int, zone: TimeZone = .current) -> String {
        "You can ask for more \(until(ts, now: now, zone: zone))"
    }

    // MARK: Actions

    /// "Withdraw your request for “X”?"
    public static func withdrawConfirm(_ title: String) -> String {
        "Withdraw your request for \u{201C}\(title)\u{201D}?"
    }

    /// "Remove “X” from your requests?"
    public static func removeConfirm(_ title: String) -> String { "Remove \u{201C}\(title)\u{201D} from your requests?" }

    public static let deleteConfirm = "Delete this request? Radarr and Sonarr keep what they have."

    // MARK: News and the badge

    /// The number on the way into requests, said for VoiceOver: "3
    /// requests need you, 1 couldn't be sent" for managers, "2 of your
    /// requests have news" for others.
    public static func count(manage: Bool, badge: Int, failed: Int) -> String {
        guard manage else { return news(badge) }
        let need = badge == 1 ? "1 request needs you" : "\(badge) requests need you"
        return failed > 0 ? "\(need), \(failed) couldn't be sent" : need
    }

    /// The badge's number: "99+" past 99; `nil` at 0.
    public static func badgeText(_ count: Int) -> String? {
        guard count > 0 else { return nil }
        return count > 99 ? "99+" : String(count)
    }

    /// "1 of your requests has news", "3 of your requests have news".
    public static func news(_ count: Int) -> String {
        "\(count) of your requests \(count == 1 ? "has" : "have") news"
    }

    /// What a toast says of `news`.
    public static func notice(_ news: RequestNews) -> String {
        func quoted(_ title: String) -> String { "\u{201C}\(title)\u{201D}" }
        switch news {
        case .ready(_, let title, _, _, let seasons, let complete):
            let seasons = seasons ?? []
            if complete || seasons.isEmpty { return "\(quoted(title)) is ready to watch." }
            return "\(Self.seasons(seasons)) of \(quoted(title)) \(seasons.count == 1 ? "is" : "are") ready to watch."
        case .approved(_, let title):
            return "\(quoted(title)) was approved."
        case .declined(_, let title, let reason):
            guard let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
                return "\(quoted(title)) was declined."
            }
            return "\(quoted(title)) was declined: \(quoted(reason))."
        case .asked(_, let title, let by, let seasons):
            let who = by?.trimmingCharacters(in: .whitespaces).nonEmpty ?? "Someone"
            guard let seasons, !seasons.isEmpty else { return "\(who) asked for \(quoted(title))." }
            return "\(who) asked for \(quoted(title)) (\(Self.seasons(seasons)))."
        case .failed(_, let title, let kind, let reason):
            let service = kind == .show ? "Sonarr" : "Radarr"
            guard let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty else {
                return "\(quoted(title)) couldn't be sent to \(service)."
            }
            return "\(quoted(title)) couldn't be sent to \(service): \(reason)"
        case .news(let count):
            return Self.news(count)
        }
    }

    // MARK: Times

    /// How long until `ts`: "in 5 min", "in 3 hours", "tomorrow", "in 12
    /// days"; "in a moment" once it's past.
    public static func until(_ ts: Int, now: Int, zone: TimeZone = .current) -> String {
        let diff = ts - now
        if diff <= 0 { return "in a moment" }
        if diff < 3_600 { return "in \(max(Int((Double(diff) / 60).rounded(.up)), 1)) min" }
        if diff < 86_400 {
            let hours = min(max(Int((Double(diff) / 3_600).rounded()), 1), 23)
            return hours == 1 ? "in 1 hour" : "in \(hours) hours"
        }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        let from = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(now)))
        let to = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(ts)))
        let days = max(calendar.dateComponents([.day], from: from, to: to).day ?? 1, 1)
        return days == 1 ? "tomorrow" : "in \(days) days"
    }

    /// How long ago `ts` was: "3 days ago", "yesterday".
    public static func ago(_ ts: Int, now: Int) -> String { Words.relative(ts, now: now) }

    /// A day, "Nov 4" ("Nov 4, 2027" in another year), with the time
    /// within 24 hours: "today, 9:00 PM".
    public static func day(_ ts: Int, now: Int, zone: TimeZone = .current, locale: Locale = .current) -> String {
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
            return "\(today ? "today" : "tomorrow"), \(format.string(from: at))"
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
