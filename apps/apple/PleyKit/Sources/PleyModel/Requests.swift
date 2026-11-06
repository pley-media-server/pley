import Foundation

// MARK: Requests (docs/API.md, "Requests")
//
// Movies and shows found on TMDB that people ask for, managers approve, and
// Radarr and Sonarr fetch. Times are unix seconds; pictures are the
// server's own paths (`/api/discover/images/{kind}/{file}`), sized with `w`
// as library artwork is.

/// What TMDB has: a movie or a show.
public struct TitleKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let movie = TitleKind(rawValue: "movie")
    public static let show = TitleKind(rawValue: "show")

    /// A library item's kind as a title's: movies and shows; `nil` for the rest.
    public static func of(_ kind: ItemKind) -> TitleKind? {
        switch kind {
        case .movie: .movie
        case .show: .show
        default: nil
        }
    }
}

/// The decision on a request.
public struct RequestStatus: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let pending = RequestStatus(rawValue: "pending")
    public static let approved = RequestStatus(rawValue: "approved")
    public static let declined = RequestStatus(rawValue: "declined")
    public static let failed = RequestStatus(rawValue: "failed")
    public static let available = RequestStatus(rawValue: "available")
}

/// What asking does for this sign-in's account.
public struct RequestAccess: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// It may not ask: Discover doesn't show.
    public static let off = RequestAccess(rawValue: "off")
    /// Its requests wait for a manager.
    public static let ask = RequestAccess(rawValue: "ask")
    /// Its requests are approved as they are made.
    public static let auto = RequestAccess(rawValue: "auto")
}

/// Where a request is, from its decision to the library.
public struct RequestStage: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// It waits for a manager.
    public static let pending = RequestStage(rawValue: "pending")
    public static let declined = RequestStage(rawValue: "declined")
    /// It couldn't be sent to Radarr or Sonarr (`MediaRequest.retryAt`).
    public static let failed = RequestStage(rawValue: "failed")
    /// Approved, not sent: no Radarr or Sonarr for its kind, being sent, or added by hand.
    public static let approved = RequestStage(rawValue: "approved")
    /// Sent, not out yet: `MediaRequest.stageAt` says when.
    public static let waiting = RequestStage(rawValue: "waiting")
    /// Sent and out, nothing downloading yet.
    public static let searching = RequestStage(rawValue: "searching")
    /// See `MediaRequest.download`.
    public static let downloading = RequestStage(rawValue: "downloading")
    /// Downloaded, going into the library.
    public static let importing = RequestStage(rawValue: "importing")
    /// Shows: some seasons asked for are ready (`MediaRequest.availableSeasons`).
    public static let partial = RequestStage(rawValue: "partial")
    /// Ready to watch.
    public static let available = RequestStage(rawValue: "available")
    /// It was available; no library has it any more.
    public static let gone = RequestStage(rawValue: "gone")
}

/// What this sign-in may do to a request now; apps offer exactly these, in their order.
public struct RequestAction: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// Its maker: `DELETE /api/requests/{id}` (Remove, on a declined one).
    public static let withdraw = RequestAction(rawValue: "withdraw")
    /// A follower: `DELETE /api/requests/{id}/watch`.
    public static let unwatch = RequestAction(rawValue: "unwatch")
    public static let approve = RequestAction(rawValue: "approve")
    /// A failed one, approved again.
    public static let retry = RequestAction(rawValue: "retry")
    public static let decline = RequestAction(rawValue: "decline")
    /// Approved without sending: added by hand.
    public static let handled = RequestAction(rawValue: "handled")
    /// Managers: removed for everyone.
    public static let delete = RequestAction(rawValue: "delete")
    /// Asked for again, as it was.
    public static let askAgain = RequestAction(rawValue: "ask_again")
}

/// What a title is for this account: the first of these that holds.
public struct TitleStatus: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// A library it can open has it whole.
    public static let library = TitleStatus(rawValue: "library")
    /// Above its age limit: it can't ask.
    public static let restricted = TitleStatus(rawValue: "restricted")
    /// Active requests cover everything it lacks.
    public static let requested = TitleStatus(rawValue: "requested")
    /// Its last request for it was declined in the last 30 days.
    public static let declined = TitleStatus(rawValue: "declined")
    /// Shows: some of it is in a library or asked for, and more seasons can be.
    public static let partial = TitleStatus(rawValue: "partial")
    /// It may be asked for.
    public static let none = TitleStatus(rawValue: "none")
}

/// A season's state for this account.
public struct SeasonState: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let library = SeasonState(rawValue: "library")
    public static let requested = SeasonState(rawValue: "requested")
    public static let unaired = SeasonState(rawValue: "unaired")
    public static let partial = SeasonState(rawValue: "partial")
    public static let missing = SeasonState(rawValue: "missing")
}

/// What Radarr's or Sonarr's queue says of a download.
public struct RequestDownloadState: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let queued = RequestDownloadState(rawValue: "queued")
    public static let downloading = RequestDownloadState(rawValue: "downloading")
    public static let paused = RequestDownloadState(rawValue: "paused")
    public static let importing = RequestDownloadState(rawValue: "importing")
    public static let warning = RequestDownloadState(rawValue: "warning")
}

/// Discover's lists, by their path, in the order its rows show.
public enum DiscoverList: String, CaseIterable, Codable, Hashable, Sendable {
    case trending, movies, shows, upcoming, anime

    /// The row's title.
    public var title: String {
        switch self {
        case .trending: localized("Trending this week")
        case .movies: localized("Popular movies")
        case .shows: localized("Popular shows")
        case .upcoming: localized("Coming to cinemas")
        case .anime: localized("Popular anime")
        }
    }
}

/// `GET /api/requests/summary`: what this sign-in may do with requests,
/// and the numbers its badge shows. Any signed-in app gets it; while
/// requests are off every number is 0 and `manage` false.
public struct RequestSummary: Codable, Hashable, Sendable {
    /// `signInAgain`'s values: the scope signing in again would give.
    public static let requestsScope = "requests"
    public static let approvalsScope = "approvals"

    /// What an older server stands for: requests off.
    public static let off = RequestSummary()

    /// Requests are on, and TMDB has a key.
    public var enabled: Bool
    public var access: RequestAccess
    /// Movies it may still ask for in the last 7 days; `nil`: no limit.
    public var moviesLeft: Int?
    public var seasonsLeft: Int?
    /// The limits per 7 days; `nil`: none.
    public var moviesLimit: Int?
    public var seasonsLimit: Int?
    /// With none left: when one more may be asked for.
    public var moviesNextAt: Int?
    public var seasonsNextAt: Int?
    /// This sign-in approves and declines: an administrator's, with `approvals` for an app.
    public var manage: Bool
    /// Managers: requests waiting for a decision.
    public var pending: Int
    /// Managers: failed requests the server won't try again by itself.
    public var failed: Int
    /// This account's requests (made or followed) with news since it last looked.
    public var unseen: Int
    /// This account's requests, made or followed, whatever their status.
    public var own: Int
    /// The newest change among the requests this sign-in sees: polled, it says when to read them again.
    public var changedAt: Int?
    /// `requests` or `approvals`: the scope this sign-in lacks.
    public var signInAgain: String?

    public init(enabled: Bool = false, access: RequestAccess = .off, moviesLeft: Int? = nil, seasonsLeft: Int? = nil,
                moviesLimit: Int? = nil, seasonsLimit: Int? = nil, moviesNextAt: Int? = nil, seasonsNextAt: Int? = nil,
                manage: Bool = false, pending: Int = 0, failed: Int = 0, unseen: Int = 0, own: Int = 0,
                changedAt: Int? = nil, signInAgain: String? = nil) {
        self.enabled = enabled
        self.access = access
        self.moviesLeft = moviesLeft
        self.seasonsLeft = seasonsLeft
        self.moviesLimit = moviesLimit
        self.seasonsLimit = seasonsLimit
        self.moviesNextAt = moviesNextAt
        self.seasonsNextAt = seasonsNextAt
        self.manage = manage
        self.pending = pending
        self.failed = failed
        self.unseen = unseen
        self.own = own
        self.changedAt = changedAt
        self.signInAgain = signInAgain
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try c.value(.enabled, or: false)
        access = try c.value(.access, or: .off)
        moviesLeft = try c.decodeIfPresent(Int.self, forKey: .moviesLeft)
        seasonsLeft = try c.decodeIfPresent(Int.self, forKey: .seasonsLeft)
        moviesLimit = try c.decodeIfPresent(Int.self, forKey: .moviesLimit)
        seasonsLimit = try c.decodeIfPresent(Int.self, forKey: .seasonsLimit)
        moviesNextAt = try c.decodeIfPresent(Int.self, forKey: .moviesNextAt)
        seasonsNextAt = try c.decodeIfPresent(Int.self, forKey: .seasonsNextAt)
        manage = try c.value(.manage, or: false)
        pending = try c.value(.pending, or: 0)
        failed = try c.value(.failed, or: 0)
        unseen = try c.value(.unseen, or: 0)
        own = try c.value(.own, or: 0)
        changedAt = try c.decodeIfPresent(Int.self, forKey: .changedAt)
        signInAgain = try c.decodeIfPresent(String.self, forKey: .signInAgain)
    }

    /// Discover shows: requests are on and this account may ask (perhaps only once signed in again).
    public var discover: Bool { enabled && access != .off }

    /// Asking works from this sign-in: Discover shows, and the sign-in has `requests`.
    public var mayAsk: Bool { discover && signInAgain != Self.requestsScope }

    /// "Your requests" shows.
    public var yourRequests: Bool {
        enabled && signInAgain != Self.requestsScope && (access != .off || own > 0)
    }

    /// The way into requests shows: Your requests, or for managers Requests.
    public var requestsEntry: Bool { yourRequests || (enabled && manage) }

    /// The number on the way into requests: for managers, what waits for them too.
    public var badge: Int { manage ? pending + failed + unseen : unseen }

    /// The badge is in the danger colour: requests failed and wait for a manager.
    public var badgeAlarm: Bool { manage && failed > 0 }
}

/// A title of Discover's lists and search, and the start of a title page.
public struct DiscoverTitle: Codable, Hashable, Sendable, Identifiable {
    public var kind: TitleKind
    public var tmdbId: Int
    public var title: String
    public var year: Int?
    public var overview: String
    public var posterUrl: String?
    public var backdropUrl: String?
    /// TMDB users', 0-10; `nil` with too few votes.
    public var rating: Double?
    public var status: TitleStatus
    /// The library's title, where this account can open it.
    public var itemId: Int?
    /// An active request for it this account made or follows (any, for managers): never a status.
    public var requestId: Int?

    public init(kind: TitleKind, tmdbId: Int, title: String, year: Int? = nil, overview: String = "",
                posterUrl: String? = nil, backdropUrl: String? = nil, rating: Double? = nil,
                status: TitleStatus = .none, itemId: Int? = nil, requestId: Int? = nil) {
        self.kind = kind
        self.tmdbId = tmdbId
        self.title = title
        self.year = year
        self.overview = overview
        self.posterUrl = posterUrl
        self.backdropUrl = backdropUrl
        self.rating = rating
        self.status = status
        self.itemId = itemId
        self.requestId = requestId
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        kind = try c.decode(TitleKind.self, forKey: .kind)
        tmdbId = try c.decode(Int.self, forKey: .tmdbId)
        title = try c.value(.title, or: "")
        year = try c.decodeIfPresent(Int.self, forKey: .year)
        overview = try c.value(.overview, or: "")
        posterUrl = try c.decodeIfPresent(String.self, forKey: .posterUrl)
        backdropUrl = try c.decodeIfPresent(String.self, forKey: .backdropUrl)
        rating = try c.decodeIfPresent(Double.self, forKey: .rating)
        status = try c.value(.status, or: .none)
        itemId = try c.decodeIfPresent(Int.self, forKey: .itemId)
        requestId = try c.decodeIfPresent(Int.self, forKey: .requestId)
    }

    /// What tells titles apart in lists: a movie and a show may share a TMDB id.
    public var key: String { "\(kind.rawValue):\(tmdbId)" }
    public var id: String { key }
}

/// `{ page, total_pages, results }`: a page of a list or of search. A page
/// may hold fewer than 20 titles, even none, before the last one.
public struct DiscoverPage: Codable, Hashable, Sendable {
    public var page: Int
    public var totalPages: Int
    public var results: [DiscoverTitle]

    public init(page: Int = 1, totalPages: Int = 1, results: [DiscoverTitle] = []) {
        self.page = page
        self.totalPages = totalPages
        self.results = results
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        page = try c.value(.page, or: 1)
        totalPages = try c.value(.totalPages, or: 1)
        results = (try? c.value(.results, or: [DiscoverTitle]())) ?? []
    }
}

/// One of a title's cast, in billing order.
public struct CastMember: Codable, Hashable, Sendable {
    public var name: String
    public var character: String?
    public var photoUrl: String?

    public init(name: String, character: String? = nil, photoUrl: String? = nil) {
        self.name = name
        self.character = character
        self.photoUrl = photoUrl
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.value(.name, or: "")
        character = try c.decodeIfPresent(String.self, forKey: .character)
        photoUrl = try c.decodeIfPresent(String.self, forKey: .photoUrl)
    }
}

/// A decline that still holds for this account: it may ask again at `againAt`.
public struct DeclinedNote: Codable, Hashable, Sendable {
    /// The manager's reason; `nil` without one, and for a follower.
    public var reason: String?
    public var at: Int
    public var againAt: Int

    public init(reason: String? = nil, at: Int = 0, againAt: Int = 0) {
        self.reason = reason
        self.at = at
        self.againAt = againAt
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
        at = try c.value(.at, or: 0)
        againAt = try c.value(.againAt, or: 0)
    }
}

/// A season of a show on its title page; number 0 is the specials.
public struct DiscoverSeason: Codable, Hashable, Sendable, Identifiable {
    public var number: Int
    public var name: String
    /// Episodes TMDB lists.
    public var episodes: Int
    /// Of those, aired by today.
    public var aired: Int
    public var airDate: String?
    public var posterUrl: String?
    /// Episodes in a library this account can open.
    public var inLibrary: Int
    public var requested: Bool
    public var state: SeasonState
    /// A request for it this account made or followed was declined in the last 30 days.
    public var declined: Bool
    /// The active request asking for it, where this account made or follows it (any, for managers).
    public var requestId: Int?

    public var id: Int { number }

    public init(number: Int, name: String = "", episodes: Int = 0, aired: Int = 0, airDate: String? = nil,
                posterUrl: String? = nil, inLibrary: Int = 0, requested: Bool = false, state: SeasonState = .missing,
                declined: Bool = false, requestId: Int? = nil) {
        self.number = number
        self.name = name
        self.episodes = episodes
        self.aired = aired
        self.airDate = airDate
        self.posterUrl = posterUrl
        self.inLibrary = inLibrary
        self.requested = requested
        self.state = state
        self.declined = declined
        self.requestId = requestId
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        number = try c.decode(Int.self, forKey: .number)
        name = try c.value(.name, or: "")
        episodes = try c.value(.episodes, or: 0)
        aired = try c.value(.aired, or: 0)
        airDate = try c.decodeIfPresent(String.self, forKey: .airDate)
        posterUrl = try c.decodeIfPresent(String.self, forKey: .posterUrl)
        inLibrary = try c.value(.inLibrary, or: 0)
        requested = try c.value(.requested, or: false)
        state = try c.value(.state, or: SeasonState(rawValue: "unknown"))
        declined = try c.value(.declined, or: false)
        requestId = try c.decodeIfPresent(Int.self, forKey: .requestId)
    }
}

/// A title page: `GET /api/discover/movie/{tmdb_id}` and `/show/{tmdb_id}`.
/// The JSON is the title's fields and more, flat; `title` gives it as
/// lists show it.
public struct DiscoverDetail: Codable, Hashable, Sendable {
    public var kind: TitleKind
    public var tmdbId: Int
    public var title: String
    public var year: Int?
    public var overview: String
    public var posterUrl: String?
    public var backdropUrl: String?
    public var rating: Double?
    public var status: TitleStatus
    public var itemId: Int?
    public var requestId: Int?
    /// Only where it differs from `title`.
    public var originalTitle: String?
    public var tagline: String?
    public var genres: [String]
    /// A movie's release date, a show's first air date: "2008-01-20".
    public var releaseDate: String?
    public var runtimeMinutes: Int?
    /// For the server's region: "PG-13", "TV-MA".
    public var certification: String?
    /// A show's first network, a movie's first studio.
    public var network: String?
    /// Shows: "Returning Series", "Ended".
    public var showStatus: String?
    public var anime: Bool
    public var imdbId: String?
    public var cast: [CastMember]
    /// Shows, in TMDB's order (specials first when there are some); `nil` for movies.
    public var seasons: [DiscoverSeason]?
    public var declined: DeclinedNote?
    /// Why it can't be asked for: "Rated R: above this account's age limit".
    public var restriction: String?
    /// This account follows an active request for it.
    public var watching: Bool
    /// This account's requests for it and those it follows (everyone's, for managers), newest first.
    public var requests: [MediaRequest]
    public var moviesLeft: Int?
    public var seasonsLeft: Int?
    public var moviesNextAt: Int?
    public var seasonsNextAt: Int?

    public init(kind: TitleKind, tmdbId: Int, title: String, year: Int? = nil, overview: String = "",
                posterUrl: String? = nil, backdropUrl: String? = nil, rating: Double? = nil,
                status: TitleStatus = .none, itemId: Int? = nil, requestId: Int? = nil, originalTitle: String? = nil,
                tagline: String? = nil, genres: [String] = [], releaseDate: String? = nil, runtimeMinutes: Int? = nil,
                certification: String? = nil, network: String? = nil, showStatus: String? = nil, anime: Bool = false,
                imdbId: String? = nil, cast: [CastMember] = [], seasons: [DiscoverSeason]? = nil,
                declined: DeclinedNote? = nil, restriction: String? = nil, watching: Bool = false,
                requests: [MediaRequest] = [], moviesLeft: Int? = nil, seasonsLeft: Int? = nil,
                moviesNextAt: Int? = nil, seasonsNextAt: Int? = nil) {
        self.kind = kind
        self.tmdbId = tmdbId
        self.title = title
        self.year = year
        self.overview = overview
        self.posterUrl = posterUrl
        self.backdropUrl = backdropUrl
        self.rating = rating
        self.status = status
        self.itemId = itemId
        self.requestId = requestId
        self.originalTitle = originalTitle
        self.tagline = tagline
        self.genres = genres
        self.releaseDate = releaseDate
        self.runtimeMinutes = runtimeMinutes
        self.certification = certification
        self.network = network
        self.showStatus = showStatus
        self.anime = anime
        self.imdbId = imdbId
        self.cast = cast
        self.seasons = seasons
        self.declined = declined
        self.restriction = restriction
        self.watching = watching
        self.requests = requests
        self.moviesLeft = moviesLeft
        self.seasonsLeft = seasonsLeft
        self.moviesNextAt = moviesNextAt
        self.seasonsNextAt = seasonsNextAt
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        kind = try c.decode(TitleKind.self, forKey: .kind)
        tmdbId = try c.decode(Int.self, forKey: .tmdbId)
        title = try c.value(.title, or: "")
        year = try c.decodeIfPresent(Int.self, forKey: .year)
        overview = try c.value(.overview, or: "")
        posterUrl = try c.decodeIfPresent(String.self, forKey: .posterUrl)
        backdropUrl = try c.decodeIfPresent(String.self, forKey: .backdropUrl)
        rating = try c.decodeIfPresent(Double.self, forKey: .rating)
        status = try c.value(.status, or: .none)
        itemId = try c.decodeIfPresent(Int.self, forKey: .itemId)
        requestId = try c.decodeIfPresent(Int.self, forKey: .requestId)
        originalTitle = try c.decodeIfPresent(String.self, forKey: .originalTitle)
        tagline = try c.decodeIfPresent(String.self, forKey: .tagline)
        genres = (try? c.value(.genres, or: [String]())) ?? []
        releaseDate = try c.decodeIfPresent(String.self, forKey: .releaseDate)
        runtimeMinutes = try c.decodeIfPresent(Int.self, forKey: .runtimeMinutes)
        certification = try c.decodeIfPresent(String.self, forKey: .certification)
        network = try c.decodeIfPresent(String.self, forKey: .network)
        showStatus = try c.decodeIfPresent(String.self, forKey: .showStatus)
        anime = try c.value(.anime, or: false)
        imdbId = try c.decodeIfPresent(String.self, forKey: .imdbId)
        cast = (try? c.value(.cast, or: [CastMember]())) ?? []
        // A movie has none; a show whose list can't be read has none either.
        seasons = try? c.decodeIfPresent([DiscoverSeason].self, forKey: .seasons)
        declined = try? c.decodeIfPresent(DeclinedNote.self, forKey: .declined)
        restriction = try c.decodeIfPresent(String.self, forKey: .restriction)
        watching = try c.value(.watching, or: false)
        requests = (try? c.value(.requests, or: [MediaRequest]())) ?? []
        moviesLeft = try c.decodeIfPresent(Int.self, forKey: .moviesLeft)
        seasonsLeft = try c.decodeIfPresent(Int.self, forKey: .seasonsLeft)
        moviesNextAt = try c.decodeIfPresent(Int.self, forKey: .moviesNextAt)
        seasonsNextAt = try c.decodeIfPresent(Int.self, forKey: .seasonsNextAt)
    }

    /// The title as lists show it: rows are patched from a page read again.
    public var asTitle: DiscoverTitle {
        DiscoverTitle(kind: kind, tmdbId: tmdbId, title: title, year: year, overview: overview, posterUrl: posterUrl,
                      backdropUrl: backdropUrl, rating: rating, status: status, itemId: itemId, requestId: requestId)
    }
}

/// Someone who asked for, or decided on, a request.
public struct RequestUser: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var name: String
    public var avatarUrl: String?
    /// "#38bdf8": the circle behind the initial when there's no picture.
    public var avatarColor: String?

    public init(id: Int, name: String, avatarUrl: String? = nil, avatarColor: String? = nil) {
        self.id = id
        self.name = name
        self.avatarUrl = avatarUrl
        self.avatarColor = avatarColor
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        name = try c.value(.name, or: "")
        avatarUrl = try c.decodeIfPresent(String.self, forKey: .avatarUrl)
        avatarColor = try c.decodeIfPresent(String.self, forKey: .avatarColor)
    }
}

/// Managers: the service a request was sent to (`radarr`, `sonarr`), with
/// the quality profile's name and the folder.
public struct SentTo: Codable, Hashable, Sendable {
    public var service: String
    public var qualityProfile: String?
    public var rootFolder: String?

    public init(service: String, qualityProfile: String? = nil, rootFolder: String? = nil) {
        self.service = service
        self.qualityProfile = qualityProfile
        self.rootFolder = rootFolder
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        service = try c.value(.service, or: "")
        qualityProfile = try c.decodeIfPresent(String.self, forKey: .qualityProfile)
        rootFolder = try c.decodeIfPresent(String.self, forKey: .rootFolder)
    }
}

/// A request's download, while Radarr or Sonarr has it queued.
public struct RequestDownload: Codable, Hashable, Sendable {
    /// 0-1, over everything queued for it.
    public var progress: Double
    /// `nil`: the queue didn't say, which counts as downloading.
    public var state: RequestDownloadState?
    /// When it should be done.
    public var eta: Int?
    /// Shows: episodes queued; movies: 1.
    public var episodes: Int

    public init(progress: Double = 0, state: RequestDownloadState? = nil, eta: Int? = nil, episodes: Int = 0) {
        self.progress = progress
        self.state = state
        self.eta = eta
        self.episodes = episodes
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        progress = try c.value(.progress, or: 0)
        state = try c.decodeIfPresent(RequestDownloadState.self, forKey: .state)
        eta = try c.decodeIfPresent(Int.self, forKey: .eta)
        episodes = try c.value(.episodes, or: 0)
    }
}

/// A request for a movie, or for seasons of a show.
public struct MediaRequest: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var kind: TitleKind
    public var tmdbId: Int
    public var title: String
    public var year: Int?
    public var posterUrl: String?
    public var backdropUrl: String?
    /// Shows: the seasons asked for; `nil` for movies.
    public var seasons: [Int]?
    /// Shows: those ready.
    public var availableSeasons: [Int]
    public var anime: Bool
    public var status: RequestStatus
    public var stage: RequestStage
    /// Waiting: when it comes out (a release known by its day is that day at 12:00 UTC).
    public var stageAt: Int?
    /// Declined: the manager's (`nil` for followers); failed: what went wrong (managers only).
    public var reason: String?
    /// Managers: what to know about it ("Added by hand").
    public var note: String?
    /// This account made it.
    public var mine: Bool
    /// This account follows it.
    public var watching: Bool
    /// Managers: how many follow it.
    public var watchers: Int
    /// It has news this account hasn't seen: available, partly available or declined.
    public var unseen: Bool
    public var actions: [RequestAction]
    /// `nil` for someone else's request seen by someone who doesn't manage.
    public var requestedBy: RequestUser?
    public var decidedBy: RequestUser?
    public var decidedAt: Int?
    public var createdAt: Int
    public var updatedAt: Int
    public var availableAt: Int?
    /// Failed: when the server tries again by itself; `nil`: it won't.
    public var retryAt: Int?
    public var sentTo: SentTo?
    /// The library's title once there, where this account can open it.
    public var itemId: Int?
    public var download: RequestDownload?

    public init(id: Int, kind: TitleKind, tmdbId: Int, title: String, year: Int? = nil, posterUrl: String? = nil,
                backdropUrl: String? = nil, seasons: [Int]? = nil, availableSeasons: [Int] = [], anime: Bool = false,
                status: RequestStatus = .pending, stage: RequestStage = .pending, stageAt: Int? = nil,
                reason: String? = nil, note: String? = nil, mine: Bool = false, watching: Bool = false,
                watchers: Int = 0, unseen: Bool = false, actions: [RequestAction] = [],
                requestedBy: RequestUser? = nil, decidedBy: RequestUser? = nil, decidedAt: Int? = nil,
                createdAt: Int = 0, updatedAt: Int = 0, availableAt: Int? = nil, retryAt: Int? = nil,
                sentTo: SentTo? = nil, itemId: Int? = nil, download: RequestDownload? = nil) {
        self.id = id
        self.kind = kind
        self.tmdbId = tmdbId
        self.title = title
        self.year = year
        self.posterUrl = posterUrl
        self.backdropUrl = backdropUrl
        self.seasons = seasons
        self.availableSeasons = availableSeasons
        self.anime = anime
        self.status = status
        self.stage = stage
        self.stageAt = stageAt
        self.reason = reason
        self.note = note
        self.mine = mine
        self.watching = watching
        self.watchers = watchers
        self.unseen = unseen
        self.actions = actions
        self.requestedBy = requestedBy
        self.decidedBy = decidedBy
        self.decidedAt = decidedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.availableAt = availableAt
        self.retryAt = retryAt
        self.sentTo = sentTo
        self.itemId = itemId
        self.download = download
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        kind = try c.decode(TitleKind.self, forKey: .kind)
        tmdbId = try c.decode(Int.self, forKey: .tmdbId)
        title = try c.value(.title, or: "")
        year = try c.decodeIfPresent(Int.self, forKey: .year)
        posterUrl = try c.decodeIfPresent(String.self, forKey: .posterUrl)
        backdropUrl = try c.decodeIfPresent(String.self, forKey: .backdropUrl)
        seasons = try? c.decodeIfPresent([Int].self, forKey: .seasons)
        availableSeasons = (try? c.value(.availableSeasons, or: [Int]())) ?? []
        anime = try c.value(.anime, or: false)
        status = try c.value(.status, or: RequestStatus(rawValue: "unknown"))
        stage = try c.value(.stage, or: RequestStage(rawValue: "unknown"))
        stageAt = try c.decodeIfPresent(Int.self, forKey: .stageAt)
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
        note = try c.decodeIfPresent(String.self, forKey: .note)
        mine = try c.value(.mine, or: false)
        watching = try c.value(.watching, or: false)
        watchers = try c.value(.watchers, or: 0)
        unseen = try c.value(.unseen, or: false)
        actions = (try? c.value(.actions, or: [RequestAction]())) ?? []
        requestedBy = try? c.decodeIfPresent(RequestUser.self, forKey: .requestedBy)
        decidedBy = try? c.decodeIfPresent(RequestUser.self, forKey: .decidedBy)
        decidedAt = try c.decodeIfPresent(Int.self, forKey: .decidedAt)
        createdAt = try c.value(.createdAt, or: 0)
        updatedAt = try c.value(.updatedAt, or: 0)
        availableAt = try c.decodeIfPresent(Int.self, forKey: .availableAt)
        retryAt = try c.decodeIfPresent(Int.self, forKey: .retryAt)
        sentTo = try? c.decodeIfPresent(SentTo.self, forKey: .sentTo)
        itemId = try c.decodeIfPresent(Int.self, forKey: .itemId)
        download = try? c.decodeIfPresent(RequestDownload.self, forKey: .download)
    }

    /// Pending, approved or failed: it still asks for something.
    public var active: Bool { status == .pending || status == .approved || status == .failed }

    /// Something of it downloads or goes into the library: worth reading again every 10 s.
    public var moving: Bool { download != nil || stage == .downloading || stage == .importing }

    /// Ready (some or all of it) in a library this account can open: Watch opens it there.
    public var readyToWatch: Bool { itemId != nil && (stage == .available || stage == .partial) }

    /// Ready for someone who follows it, but in no library they can open.
    public var readyElsewhere: Bool { itemId == nil && (stage == .available || stage == .partial) }

    /// How far its download is, 0-1, while there is one.
    public var downloadProgress: Double? { download.map { min(max($0.progress, 0), 1) } }
}

/// How many requests have each status, with every filter but `status`.
public struct RequestCounts: Codable, Hashable, Sendable {
    public var pending: Int
    public var approved: Int
    public var declined: Int
    public var failed: Int
    public var available: Int

    public init(pending: Int = 0, approved: Int = 0, declined: Int = 0, failed: Int = 0, available: Int = 0) {
        self.pending = pending
        self.approved = approved
        self.declined = declined
        self.failed = failed
        self.available = available
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        pending = try c.value(.pending, or: 0)
        approved = try c.value(.approved, or: 0)
        declined = try c.value(.declined, or: 0)
        failed = try c.value(.failed, or: 0)
        available = try c.value(.available, or: 0)
    }

    /// How many have `status`; 0 for one this app doesn't know.
    public subscript(status: RequestStatus) -> Int {
        get {
            switch status {
            case .pending: pending
            case .approved: approved
            case .declined: declined
            case .failed: failed
            case .available: available
            default: 0
            }
        }
        set {
            switch status {
            case .pending: pending = newValue
            case .approved: approved = newValue
            case .declined: declined = newValue
            case .failed: failed = newValue
            case .available: available = newValue
            default: break
            }
        }
    }

    /// One request moved from `old` to `new` (`nil`: it was or is gone).
    public mutating func move(from old: RequestStatus?, to new: RequestStatus?) {
        guard old != new else { return }
        if let old { self[old] = max(0, self[old] - 1) }
        if let new { self[new] += 1 }
    }
}

/// `GET /api/requests`: a page of requests, how many match (`total`), how
/// many have each status (`counts`), and for managers the people with
/// requests matching every filter but the person (`people`, by name).
public struct RequestList: Codable, Hashable, Sendable {
    public var total: Int
    public var counts: RequestCounts
    public var requests: [MediaRequest]
    public var people: [RequestUser]

    public init(total: Int = 0, counts: RequestCounts = RequestCounts(), requests: [MediaRequest] = [],
                people: [RequestUser] = []) {
        self.total = total
        self.counts = counts
        self.requests = requests
        self.people = people
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        total = try c.value(.total, or: 0)
        counts = (try? c.value(.counts, or: RequestCounts())) ?? RequestCounts()
        requests = (try? c.value(.requests, or: [MediaRequest]())) ?? []
        people = (try? c.value(.people, or: [RequestUser]())) ?? []
    }
}

/// `GET /api/items/{id}/request`: a library title as Discover sees it, with
/// the newest active request for it this account made or follows (any, for
/// managers). `askable`: the seasons the picker would offer without a limit
/// (`[]` for movies; `nil` while the server has no details for it yet).
public struct ItemRequest: Codable, Hashable, Sendable {
    public var kind: TitleKind
    public var tmdbId: Int
    public var status: TitleStatus
    public var request: MediaRequest?
    public var askable: [Int]?

    public init(kind: TitleKind, tmdbId: Int, status: TitleStatus = .none, request: MediaRequest? = nil,
                askable: [Int]? = nil) {
        self.kind = kind
        self.tmdbId = tmdbId
        self.status = status
        self.request = request
        self.askable = askable
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        kind = try c.decode(TitleKind.self, forKey: .kind)
        tmdbId = try c.decode(Int.self, forKey: .tmdbId)
        status = try c.value(.status, or: .none)
        request = try? c.decodeIfPresent(MediaRequest.self, forKey: .request)
        askable = try? c.decodeIfPresent([Int].self, forKey: .askable)
    }
}

/// A quality profile of Radarr or Sonarr.
public struct QualityProfile: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var name: String

    public init(id: Int, name: String) {
        self.id = id
        self.name = name
    }
}

/// A root folder of Radarr or Sonarr.
public struct RootFolder: Codable, Hashable, Sendable {
    public var path: String
    public var freeBytes: Int?

    public init(path: String, freeBytes: Int? = nil) {
        self.path = path
        self.freeBytes = freeBytes
    }
}

/// What a manager may pick when approving, from one service: its quality
/// profiles and root folders, with the settings' choices (anime's for
/// anime). `error`: it didn't answer, and these are the last lists known.
public struct ApprovalOptions: Codable, Hashable, Sendable {
    public var qualityProfiles: [QualityProfile]
    public var rootFolders: [RootFolder]
    public var qualityProfile: Int?
    public var rootFolder: String?
    public var animeQualityProfile: Int?
    public var animeRootFolder: String?
    public var error: String?

    public init(qualityProfiles: [QualityProfile] = [], rootFolders: [RootFolder] = [], qualityProfile: Int? = nil,
                rootFolder: String? = nil, animeQualityProfile: Int? = nil, animeRootFolder: String? = nil,
                error: String? = nil) {
        self.qualityProfiles = qualityProfiles
        self.rootFolders = rootFolders
        self.qualityProfile = qualityProfile
        self.rootFolder = rootFolder
        self.animeQualityProfile = animeQualityProfile
        self.animeRootFolder = animeRootFolder
        self.error = error
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        qualityProfiles = (try? c.value(.qualityProfiles, or: [QualityProfile]())) ?? []
        rootFolders = (try? c.value(.rootFolders, or: [RootFolder]())) ?? []
        qualityProfile = try c.decodeIfPresent(Int.self, forKey: .qualityProfile)
        rootFolder = try c.decodeIfPresent(String.self, forKey: .rootFolder)
        animeQualityProfile = try c.decodeIfPresent(Int.self, forKey: .animeQualityProfile)
        animeRootFolder = try c.decodeIfPresent(String.self, forKey: .animeRootFolder)
        error = try c.decodeIfPresent(String.self, forKey: .error)
    }
}

/// `GET /api/requests/options`: each service's choices, `nil` where it isn't connected.
public struct RequestOptions: Codable, Hashable, Sendable {
    public var radarr: ApprovalOptions?
    public var sonarr: ApprovalOptions?

    public init(radarr: ApprovalOptions? = nil, sonarr: ApprovalOptions? = nil) {
        self.radarr = radarr
        self.sonarr = sonarr
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        radarr = try? c.decodeIfPresent(ApprovalOptions.self, forKey: .radarr)
        sonarr = try? c.decodeIfPresent(ApprovalOptions.self, forKey: .sonarr)
    }

    /// The service a request of `kind` goes to.
    public func of(_ kind: TitleKind) -> ApprovalOptions? { kind == .show ? sonarr : radarr }
}

/// The body of `POST /api/requests`: a movie, or seasons of a show (`nil`: those the server picks).
public struct NewRequest: Codable, Hashable, Sendable {
    public var kind: TitleKind
    public var tmdbId: Int
    public var seasons: [Int]?

    public init(kind: TitleKind, tmdbId: Int, seasons: [Int]? = nil) {
        self.kind = kind
        self.tmdbId = tmdbId
        self.seasons = seasons
    }
}

/// What `POST /api/requests` answered: a new request (`created`, 201), or
/// one this account now follows (200).
public struct Asked: Hashable, Sendable {
    public var request: MediaRequest
    public var created: Bool

    public init(request: MediaRequest, created: Bool) {
        self.request = request
        self.created = created
    }
}

/// The body of `POST /api/requests/{id}/approve`: the dialog's choices;
/// `nil`s take the settings'.
public struct Approval: Codable, Hashable, Sendable {
    /// A quality profile's id (`ApprovalOptions.qualityProfiles`).
    public var qualityProfile: Int?
    public var rootFolder: String?
    /// Only these of the seasons asked for.
    public var seasons: [Int]?

    public init(qualityProfile: Int? = nil, rootFolder: String? = nil, seasons: [Int]? = nil) {
        self.qualityProfile = qualityProfile
        self.rootFolder = rootFolder
        self.seasons = seasons
    }
}

/// The body of `POST /api/requests/{id}/decline`.
public struct Decline: Codable, Hashable, Sendable {
    public var reason: String?

    public init(reason: String? = nil) {
        self.reason = reason
    }
}

/// Which requests `GET /api/requests` lists, as its query.
public struct RequestQuery: Hashable, Sendable {
    /// One status or several.
    public var statuses: [RequestStatus]
    /// Managers: pending ones and the failed ones the server won't retry.
    public var attention: Bool
    public var kind: TitleKind?
    /// Managers: one person's.
    public var userId: Int?
    /// Titles containing this.
    public var q: String?
    /// Only this account's and those it follows (managers see everyone's without it).
    public var mine: Bool
    /// Only those with news, newest news first.
    public var unseen: Bool
    public var changedSince: Int?
    /// Oldest first, instead of newest.
    public var oldest: Bool
    public var offset: Int
    public var limit: Int

    public init(statuses: [RequestStatus] = [], attention: Bool = false, kind: TitleKind? = nil, userId: Int? = nil,
                q: String? = nil, mine: Bool = false, unseen: Bool = false, changedSince: Int? = nil,
                oldest: Bool = false, offset: Int = 0, limit: Int = 30) {
        self.statuses = statuses
        self.attention = attention
        self.kind = kind
        self.userId = userId
        self.q = q
        self.mine = mine
        self.unseen = unseen
        self.changedSince = changedSince
        self.oldest = oldest
        self.offset = offset
        self.limit = limit
    }

    /// The query string's items: flags as `1`, statuses comma-joined,
    /// `order=oldest` only when asked for.
    public var items: [URLQueryItem] {
        var items: [URLQueryItem] = []
        if !statuses.isEmpty { items.append(URLQueryItem(name: "status", value: statuses.map(\.rawValue).joined(separator: ","))) }
        if attention { items.append(URLQueryItem(name: "attention", value: "1")) }
        if let kind { items.append(URLQueryItem(name: "kind", value: kind.rawValue)) }
        if let userId { items.append(URLQueryItem(name: "user_id", value: String(userId))) }
        if let q = q?.trimmingCharacters(in: .whitespacesAndNewlines), !q.isEmpty {
            items.append(URLQueryItem(name: "q", value: q))
        }
        if mine { items.append(URLQueryItem(name: "mine", value: "1")) }
        if unseen { items.append(URLQueryItem(name: "unseen", value: "1")) }
        if let changedSince { items.append(URLQueryItem(name: "changed_since", value: String(changedSince))) }
        if oldest { items.append(URLQueryItem(name: "order", value: "oldest")) }
        if offset > 0 { items.append(URLQueryItem(name: "offset", value: String(offset))) }
        items.append(URLQueryItem(name: "limit", value: String(min(max(limit, 1), 200))))
        return items
    }
}

// MARK: What the screens make of them

/// A pill's colour: pending amber, on its way blue, ready green, refused red, gone gray.
public enum PillTone: Hashable, Sendable {
    case amber, blue, green, red, gray
}

/// Where a request is, as its pill says it.
public enum StagePill: Hashable, Sendable {
    case pending
    case approved
    /// Not out yet: out at `at` (a movie), airing at `at` (a show), or when isn't known.
    case waiting(show: Bool, at: Int?)
    case searching
    /// `progress` 0-1, done at `eta`; `state` says when it waits, is paused or has a problem.
    case downloading(progress: Double, eta: Int?, state: RequestDownloadState)
    case importing
    /// Shows: `seasons` are ready.
    case partial(seasons: [Int])
    case available
    case gone
    case declined
    /// Failed, as managers see it: what went wrong, and when the server tries again.
    case couldNotSend(reason: String?, retryAt: Int?)
    /// Failed, as everyone else sees it: the server tries again at `retryAt`, or leaves it to a manager.
    case notSentYet(retryAt: Int?)
    /// A stage this app doesn't know.
    case unknown

    public var tone: PillTone {
        switch self {
        case .pending: .amber
        case .approved, .waiting, .searching, .downloading, .importing: .blue
        case .partial, .available: .green
        case .gone, .unknown: .gray
        case .declined, .couldNotSend, .notSentYet: .red
        }
    }

    /// `request`'s pill, as a manager (`manage`) or anyone else sees it.
    public static func of(_ request: MediaRequest, manage: Bool) -> StagePill {
        switch request.stage {
        case .pending: .pending
        case .approved: .approved
        case .waiting: .waiting(show: request.kind == .show, at: request.stageAt)
        case .searching: .searching
        case .downloading:
            .downloading(progress: request.downloadProgress ?? 0, eta: request.download?.eta,
                         state: request.download?.state ?? .downloading)
        case .importing: .importing
        case .partial: .partial(seasons: request.availableSeasons)
        case .available: .available
        case .gone: .gone
        case .declined: .declined
        case .failed: manage ? .couldNotSend(reason: request.reason, retryAt: request.retryAt)
            : .notSentYet(retryAt: request.retryAt)
        default: .unknown
        }
    }
}

/// How much this account may still ask for in the last 7 days, and when
/// more. `nil`s: no limit.
public struct Allowance: Hashable, Sendable {
    public var moviesLeft: Int?
    public var seasonsLeft: Int?
    public var moviesLimit: Int?
    public var seasonsLimit: Int?
    public var moviesNextAt: Int?
    public var seasonsNextAt: Int?

    public init(moviesLeft: Int? = nil, seasonsLeft: Int? = nil, moviesLimit: Int? = nil, seasonsLimit: Int? = nil,
                moviesNextAt: Int? = nil, seasonsNextAt: Int? = nil) {
        self.moviesLeft = moviesLeft
        self.seasonsLeft = seasonsLeft
        self.moviesLimit = moviesLimit
        self.seasonsLimit = seasonsLimit
        self.moviesNextAt = moviesNextAt
        self.seasonsNextAt = seasonsNextAt
    }

    public init(summary: RequestSummary) {
        self.init(moviesLeft: summary.moviesLeft, seasonsLeft: summary.seasonsLeft, moviesLimit: summary.moviesLimit,
                  seasonsLimit: summary.seasonsLimit, moviesNextAt: summary.moviesNextAt,
                  seasonsNextAt: summary.seasonsNextAt)
    }

    /// A title page's, which the server sends with it; the limits from `summary`.
    public init(detail: DiscoverDetail, summary: RequestSummary?) {
        self.init(moviesLeft: detail.moviesLeft, seasonsLeft: detail.seasonsLeft, moviesLimit: summary?.moviesLimit,
                  seasonsLimit: summary?.seasonsLimit, moviesNextAt: detail.moviesNextAt,
                  seasonsNextAt: detail.seasonsNextAt)
    }

    /// Some limit applies: the allowance line has something to say.
    public var limited: Bool { moviesLeft != nil || seasonsLeft != nil }

    /// Whether one more of `kind` may be asked for.
    public func allows(_ kind: TitleKind) -> Bool {
        switch kind {
        case .movie: moviesLeft.map { $0 > 0 } ?? true
        case .show: seasonsLeft.map { $0 > 0 } ?? true
        default: true
        }
    }

    /// What's left of `kind`: movies, or seasons; `nil`: no limit.
    public func left(_ kind: TitleKind) -> Int? { kind == .show ? seasonsLeft : moviesLeft }
}

/// Something to tell the person about their requests, once: a toast.
public enum RequestNews: Hashable, Sendable {
    /// Ready to watch where `itemId` is: the whole of it (`complete`), or
    /// `seasons` of a show; `poster`, its poster's path on the server.
    case ready(requestId: Int, title: String, kind: TitleKind, itemId: Int?, seasons: [Int]?, complete: Bool,
               poster: String? = nil)
    case approved(requestId: Int, title: String)
    /// `reason`: the manager's, `nil` without one (and for followers).
    case declined(requestId: Int, title: String, reason: String?)
    /// Managers: `by` asked for it, waiting for a decision.
    case asked(requestId: Int, title: String, by: String?, seasons: [Int]?)
    /// Managers: it couldn't be sent to Radarr (a movie) or Sonarr, and waits for one.
    case failed(requestId: Int, title: String, kind: TitleKind, reason: String?)
    /// Several of the person's requests have news.
    case news(count: Int)
}

/// What requests are for the profile in use, from its summary: whether
/// any of it shows, and what (Android's `RequestsState`).
public enum RequestsGate: Hashable, Sendable {
    /// Not known yet: starting, or the server never answered.
    case unknown
    /// None of it shows: requests are off, or the server is older.
    case hidden
    /// None of it shows either: requests are on, but this account may not ask, and has no requests.
    case noAccess
    /// Discover shows only "Sign in again": this sign-in came before requests.
    case signInAgain(RequestSummary)
    /// Requests work: the summary says what shows.
    case open(RequestSummary)

    public static func of(_ summary: RequestSummary?) -> RequestsGate {
        guard let summary else { return .unknown }
        if !summary.enabled { return .hidden }
        if summary.signInAgain == RequestSummary.requestsScope && summary.discover { return .signInAgain(summary) }
        if summary.discover || summary.requestsEntry { return .open(summary) }
        return .noAccess
    }

    /// The summary behind the gate, where there is one.
    public var summary: RequestSummary? {
        switch self {
        case .open(let summary), .signInAgain(let summary): summary
        case .unknown, .hidden, .noAccess: nil
        }
    }

    /// Asking works from here now: Discover's rows, Search's Discover row, title pages' Request.
    public var mayAsk: Bool {
        if case .open(let summary) = self { return summary.mayAsk }
        return false
    }

    /// Which gate this is, without the summary's counts.
    public enum Kind: Hashable, Sendable {
        case unknown, hidden, noAccess, signInAgain
        case open(mayAsk: Bool)
    }

    /// Which gate this is: pages read again when it changes (the gate
    /// opens or closes, asking starts or stops), not each time a count moves.
    public var kind: Kind {
        switch self {
        case .unknown: .unknown
        case .hidden: .hidden
        case .noAccess: .noAccess
        case .signInAgain: .signInAgain
        case .open(let summary): .open(mayAsk: summary.mayAsk)
        }
    }
}
