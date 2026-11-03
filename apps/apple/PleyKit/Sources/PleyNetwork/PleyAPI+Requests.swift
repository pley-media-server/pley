import Foundation
import PleyModel

/// A request without a body.
private struct NoBody: Encodable, Sendable {}

/// Requests (docs/API.md, "Requests"): what this sign-in may do with them,
/// Discover's lists, search and title pages, a library title as Discover
/// sees it, the requests themselves, and managers' decisions. The routes
/// answer in the error envelope; requests' own refusals come as
/// `PleyError.notAvailable` (`503`), `.refused` (`409` and `429` that say
/// when, or what is now) and `.api`.
extension PleyAPI {
    /// The longest search the server takes, in characters: it refuses longer ones.
    public static let discoverSearchMax = 200

    /// `GET /api/requests/summary`: what this sign-in may do with requests,
    /// and the numbers its badge shows. Any app gets it.
    public func requestSummary() async throws -> RequestSummary {
        try await client.get("/api/requests/summary", maxBytes: BodyLimit.small, as: RequestSummary.self)
    }

    /// `GET /api/discover/{list}?page=`: a page of one of Discover's lists.
    public func discover(_ list: DiscoverList, page: Int = 1) async throws -> DiscoverPage {
        try await client.get("/api/discover/\(list.rawValue)", query: [Self.page(page)], as: DiscoverPage.self)
    }

    /// `GET /api/discover/search?q=&page=&missing=`: movies and shows
    /// matching `query` (trimmed, and cut to what the server takes);
    /// `missing` leaves out those a library has.
    public func discoverSearch(_ query: String, page: Int = 1, missing: Bool = false) async throws -> DiscoverPage {
        var items = [URLQueryItem(name: "q", value: Self.searchText(query)), Self.page(page)]
        if missing { items.append(URLQueryItem(name: "missing", value: "1")) }
        return try await client.get("/api/discover/search", query: items, as: DiscoverPage.self)
    }

    /// `GET /api/discover/movie/{tmdb_id}` or `/show/{tmdb_id}`: a title page.
    public func discoverDetail(_ kind: TitleKind, tmdbId: Int) async throws -> DiscoverDetail {
        try await client.get("/api/discover/\(Self.segment(kind))/\(tmdbId)", as: DiscoverDetail.self)
    }

    /// `GET /api/items/{id}/request`: a library's movie or show as Discover
    /// sees it; `nil` (a `404`) for one that isn't, or that TMDB doesn't know.
    public func itemRequest(_ itemId: Int) async throws -> ItemRequest? {
        do {
            return try await client.get("/api/items/\(itemId)/request", as: ItemRequest.self)
        } catch PleyError.api(404, _, _) {
            return nil
        }
    }

    /// `GET /api/requests`: the requests `query` keeps, a page at a time.
    public func requests(_ query: RequestQuery) async throws -> RequestList {
        try await client.get("/api/requests", query: query.items, as: RequestList.self)
    }

    /// `GET /api/requests/{id}`: one request; `nil` (a `404`) once it is
    /// gone, or no longer this account's to read.
    public func request(_ id: Int) async throws -> MediaRequest? {
        do {
            return try await client.get("/api/requests/\(id)", as: MediaRequest.self)
        } catch PleyError.api(404, _, _) {
            return nil
        }
    }

    /// `POST /api/requests`: asks for a movie or seasons of a show. `201`
    /// is a new request (`created`); `200` one this account follows now,
    /// nothing being left to ask for.
    public func ask(_ new: NewRequest) async throws -> Asked {
        let (request, status) = try await client.sendWithStatus("POST", "/api/requests", body: new,
                                                                as: MediaRequest.self)
        return Asked(request: request, created: status == 201)
    }

    /// `POST /api/requests/seen`: what had news for this account is seen.
    public func markRequestsSeen() async throws {
        _ = try await client.send("POST", "/api/requests/seen", body: NoBody?.none, as: NoContent.self)
    }

    /// `DELETE /api/requests/{id}`: its maker withdraws it (removes a
    /// declined one); a manager deletes it for everyone.
    public func deleteRequest(_ id: Int) async throws {
        _ = try await client.send("DELETE", "/api/requests/\(id)", body: NoBody?.none, as: NoContent.self)
    }

    /// `DELETE /api/requests/{id}/watch`: this account follows it no more.
    public func unwatchRequest(_ id: Int) async throws {
        _ = try await client.send("DELETE", "/api/requests/\(id)/watch", body: NoBody?.none, as: NoContent.self)
    }

    /// `POST /api/requests/{id}/approve` (managers): approved with what
    /// `approval` chooses, the settings' choices for the rest.
    public func approveRequest(_ id: Int, _ approval: Approval = Approval()) async throws -> MediaRequest {
        try await client.send("POST", "/api/requests/\(id)/approve", body: approval, as: MediaRequest.self)
    }

    /// `POST /api/requests/{id}/decline` (managers), with the reason its
    /// maker reads; an empty one is left out.
    public func declineRequest(_ id: Int, reason: String?) async throws -> MediaRequest {
        let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines)
        return try await client.send("POST", "/api/requests/\(id)/decline",
                                     body: Decline(reason: reason?.isEmpty == false ? reason : nil),
                                     as: MediaRequest.self)
    }

    /// `POST /api/requests/{id}/handled` (managers): approved as added to
    /// Radarr or Sonarr by hand.
    public func markHandled(_ id: Int) async throws -> MediaRequest {
        try await client.send("POST", "/api/requests/\(id)/handled", body: NoBody?.none, as: MediaRequest.self)
    }

    /// `GET /api/requests/options` (managers): what approving may choose in Radarr and Sonarr.
    public func requestOptions() async throws -> RequestOptions {
        try await client.get("/api/requests/options", as: RequestOptions.self)
    }

    /// `query` as the server takes it: trimmed, and cut to
    /// `discoverSearchMax` characters (Unicode scalars, as the server counts
    /// them), never halfway through one.
    public static func searchText(_ query: String) -> String {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.unicodeScalars.count > discoverSearchMax else { return trimmed }
        var scalars = String.UnicodeScalarView()
        scalars.append(contentsOf: trimmed.unicodeScalars.prefix(discoverSearchMax))
        return String(scalars).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func page(_ page: Int) -> URLQueryItem {
        URLQueryItem(name: "page", value: String(min(max(page, 1), 500)))
    }

    /// A title's kind as its path segment: `movie` or `show`; anything else asks for a show's page.
    private static func segment(_ kind: TitleKind) -> String { kind == .movie ? "movie" : "show" }
}
