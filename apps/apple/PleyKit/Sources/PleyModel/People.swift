import Foundation

/// What someone did on a title (docs/API.md, `Credit.role`).
public struct CreditRole: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let actor = CreditRole(rawValue: "actor")
    public static let creator = CreditRole(rawValue: "creator")
    public static let director = CreditRole(rawValue: "director")
    public static let writer = CreditRole(rawValue: "writer")
    public static let producer = CreditRole(rawValue: "producer")
}

/// A person in a title's cast or crew (docs/API.md, `Credit`).
public struct Credit: Codable, Hashable, Sendable, Identifiable {
    /// The person: `GET /api/people/{id}`.
    public var id: Int
    public var name: String
    public var role: CreditRole
    /// Actors: the part played.
    public var character: String?
    /// "/api/people/7/photo?v=…".
    public var photoUrl: String?

    public init(id: Int, name: String, role: CreditRole, character: String? = nil, photoUrl: String? = nil) {
        self.id = id
        self.name = name
        self.role = role
        self.character = character
        self.photoUrl = photoUrl
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        name = try c.value(.name, or: "")
        role = try c.value(.role, or: CreditRole(rawValue: ""))
        character = try c.decodeIfPresent(String.self, forKey: .character)
        photoUrl = try c.decodeIfPresent(String.self, forKey: .photoUrl)
    }
}

/// A person (docs/API.md, `PersonSummary`).
public struct PersonSummary: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var name: String
    public var photoUrl: String?

    public init(id: Int, name: String, photoUrl: String? = nil) {
        self.id = id
        self.name = name
        self.photoUrl = photoUrl
    }
}

/// One of a person's parts in a title: "actor" as "Neo", "director" of 6 episodes.
public struct PersonRole: Codable, Hashable, Sendable {
    public var role: CreditRole
    public var character: String?
    /// A show's credits counted by episode.
    public var episodes: Int?

    public init(role: CreditRole, character: String? = nil, episodes: Int? = nil) {
        self.role = role
        self.character = character
        self.episodes = episodes
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        role = try c.value(.role, or: CreditRole(rawValue: ""))
        character = try c.decodeIfPresent(String.self, forKey: .character)
        episodes = try c.decodeIfPresent(Int.self, forKey: .episodes)
    }
}

/// A movie or show a person is credited in, with their parts in it.
public struct PersonCredit: Codable, Hashable, Sendable {
    public var item: ItemSummary
    public var roles: [PersonRole]

    public init(item: ItemSummary, roles: [PersonRole]) {
        self.item = item
        self.roles = roles
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        item = try c.decode(ItemSummary.self, forKey: .item)
        roles = try c.value(.roles, or: [])
    }
}

/// `GET /api/people/{id}`: a person and what of theirs this profile may
/// see, newest first (episode credits count towards their show).
public struct PersonDetail: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var name: String
    public var photoUrl: String?
    public var credits: [PersonCredit]

    public init(id: Int, name: String, photoUrl: String? = nil, credits: [PersonCredit] = []) {
        self.id = id
        self.name = name
        self.photoUrl = photoUrl
        self.credits = credits
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        name = try c.value(.name, or: "")
        photoUrl = try c.decodeIfPresent(String.self, forKey: .photoUrl)
        credits = try c.value(.credits, or: [])
    }
}

/// A collection an item is in, by its title ("Part of …").
public struct CollectionRef: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var title: String

    public init(id: Int, title: String) {
        self.id = id
        self.title = title
    }
}

/// `GET /api/items/{id}/collections`: the collections a movie or show is in.
public struct ItemCollections: Codable, Hashable, Sendable {
    public var collections: [CollectionRef]

    public init(collections: [CollectionRef]) { self.collections = collections }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        collections = try c.value(.collections, or: [])
    }
}
