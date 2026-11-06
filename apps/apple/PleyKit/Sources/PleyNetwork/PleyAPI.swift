import Foundation
import PleyModel

/// The API calls the app makes, typed.
public struct PleyAPI: Sendable {
    /// The picture types Pley serves; nothing else is shown (no SVG, no HTML).
    public static let imageTypes: Set<String> = ["image/jpeg", "image/png", "image/webp", "image/gif"]

    public let client: APIClient

    public init(client: APIClient) {
        self.client = client
    }

    /// `GET /api/server`: the server's name and version, without signing in.
    public func server() async throws -> ServerInfo {
        try await client.get("/api/server", maxBytes: BodyLimit.small, as: ServerInfo.self)
    }

    /// `GET /api/auth/me`: who the app acts as, and what it was allowed.
    public func me() async throws -> User {
        try await client.get("/api/auth/me", as: User.self)
    }

    /// `GET /api/oauth/profiles`: the household's profiles.
    public func householdProfiles() async throws -> [HouseholdProfile] {
        try await client.get("/api/oauth/profiles", as: [HouseholdProfile].self)
    }

    /// `GET /api/hubs/home`: the home page's rows.
    public func home() async throws -> HomeHubs {
        try await client.get("/api/hubs/home", as: HomeHubs.self)
    }

    /// The address of a picture the server named (`/api/items/12/images/poster?v=…`),
    /// at `width` pixels when given. Only a path under `/api/` on this
    /// server: `nil` for anything else.
    public func imageURL(path: String, width: Int?) -> URL? {
        guard Self.isImagePath(path) else { return nil }
        return client.url(path, query: Self.widthQuery(width))
    }

    /// The bytes of a picture the server named, of one of `imageTypes` and
    /// at most 10 MiB.
    public func image(path: String, width: Int?) async throws -> Data {
        guard Self.isImagePath(path) else { throw PleyError.unreachable(PleyError.notOnServer) }
        let (data, _) = try await client.data(path, query: Self.widthQuery(width), maxBytes: BodyLimit.image,
                                               accept: Self.imageTypes)
        return data
    }

    private static func isImagePath(_ path: String) -> Bool {
        path.hasPrefix("/api/") && !(path.split(separator: "?").first ?? "").split(separator: "/").contains("..")
    }

    private static func widthQuery(_ width: Int?) -> [URLQueryItem] {
        guard let width, width > 0 else { return [] }
        return [URLQueryItem(name: "w", value: String(width))]
    }
}
