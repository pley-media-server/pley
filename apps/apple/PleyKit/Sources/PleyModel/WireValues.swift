import Foundation

/// A value the server names with a string ("movie", "pin"). Any string
/// reads, also one this app doesn't know, so a newer server can add a kind
/// or a state without breaking the answers an older app reads; the app
/// compares against the values it knows and treats the rest as unknown.
public protocol WireValue: RawRepresentable, Codable, Hashable, Sendable, CustomStringConvertible
where RawValue == String {
    init(rawValue: String)
}

extension WireValue {
    public init(from decoder: any Decoder) throws {
        self.init(rawValue: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }

    public var description: String { rawValue }
}

/// What an item is (docs/API.md, `ItemKind`).
public struct ItemKind: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let movie = ItemKind(rawValue: "movie")
    public static let show = ItemKind(rawValue: "show")
    public static let season = ItemKind(rawValue: "season")
    public static let episode = ItemKind(rawValue: "episode")
    public static let artist = ItemKind(rawValue: "artist")
    public static let album = ItemKind(rawValue: "album")
    public static let track = ItemKind(rawValue: "track")
    /// Trailers and other bonus material.
    public static let extra = ItemKind(rawValue: "extra")

    /// The kinds this app knows; anything else came from a newer server.
    public static let known: Set<ItemKind> = [.movie, .show, .season, .episode, .artist, .album, .track, .extra]
}

/// What switching to a profile takes (`needs` of `GET /api/oauth/profiles`).
public struct ProfileNeeds: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// Nothing: it switches at once.
    public static let none = ProfileNeeds(rawValue: "none")
    /// Its PIN, typed into the app.
    public static let pin = ProfileNeeds(rawValue: "pin")
    /// Its password, which only the browser asks for: the app signs in again.
    public static let password = ProfileNeeds(rawValue: "password")
    /// It can't be used from the app.
    public static let locked = ProfileNeeds(rawValue: "locked")
}
