import Foundation

/// Who the app acts as: `GET /api/auth/me` (docs/API.md, `User`), with the
/// `scope` an app's token was allowed. `isAdmin`, `allLibraries` and
/// `maxAge` are the app's rights, not the account's: `isAdmin` is always
/// false for an app. Times are unix seconds.
public struct User: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var username: String
    /// The display name, or the username: what to show.
    public var name: String
    public var displayName: String?
    public var isAdmin: Bool
    /// A profile without a password, reached by switching from a home member.
    public var managed: Bool
    /// Listed in the household's profile switcher.
    public var home: Bool
    public var hasPin: Bool
    /// `/api/users/2/avatar?v=…`, or `nil` without a picture.
    public var avatarUrl: String?
    /// `#38bdf8`: the circle behind the initials when there's no picture.
    public var avatarColor: String?
    /// False: only the libraries in `libraryIds`.
    public var allLibraries: Bool
    public var libraryIds: [Int]
    /// The age limit for movies and shows; `nil`: none.
    public var maxAge: Int?
    public var disabled: Bool
    public var createdAt: Int
    public var audioLanguage: String?
    public var subtitleLanguage: String?
    /// `off`, `forced`, `foreign` or `always`.
    public var subtitleMode: String?
    /// The language the profile sees Pley in ("en", "fr", "es"); `nil`:
    /// Automatic, each device's (docs/API.md, "Language").
    public var language: String?
    public var hasPassword: Bool
    public var twoStep: Bool
    /// What the app was allowed, separated by spaces (`library play`);
    /// `nil` for a browser's session.
    public var scope: String?

    /// `scope` as a list: `library`, `play`, `playlists`, `profiles`.
    public var scopes: [String] {
        (scope ?? "").split(separator: " ").map(String.init)
    }

    public init(id: Int, username: String, name: String) {
        self.id = id
        self.username = username
        self.name = name
        isAdmin = false
        managed = false
        home = false
        hasPin = false
        allLibraries = true
        libraryIds = []
        disabled = false
        createdAt = 0
        hasPassword = false
        twoStep = false
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        username = try c.decode(String.self, forKey: .username)
        name = try c.decode(String.self, forKey: .name)
        displayName = try c.decodeIfPresent(String.self, forKey: .displayName)
        isAdmin = try c.value(.isAdmin, or: false)
        managed = try c.value(.managed, or: false)
        home = try c.value(.home, or: false)
        hasPin = try c.value(.hasPin, or: false)
        avatarUrl = try c.decodeIfPresent(String.self, forKey: .avatarUrl)
        avatarColor = try c.decodeIfPresent(String.self, forKey: .avatarColor)
        allLibraries = try c.value(.allLibraries, or: true)
        libraryIds = try c.value(.libraryIds, or: [])
        maxAge = try c.decodeIfPresent(Int.self, forKey: .maxAge)
        disabled = try c.value(.disabled, or: false)
        createdAt = try c.value(.createdAt, or: 0)
        audioLanguage = try c.decodeIfPresent(String.self, forKey: .audioLanguage)
        subtitleLanguage = try c.decodeIfPresent(String.self, forKey: .subtitleLanguage)
        subtitleMode = try c.decodeIfPresent(String.self, forKey: .subtitleMode)
        language = try c.decodeIfPresent(String.self, forKey: .language)
        hasPassword = try c.value(.hasPassword, or: false)
        twoStep = try c.value(.twoStep, or: false)
        scope = try c.decodeIfPresent(String.self, forKey: .scope)
    }
}

/// One profile of the household, as `GET /api/oauth/profiles` lists them.
/// No usernames, roles or limits: the server doesn't send them to apps.
public struct HouseholdProfile: Codable, Hashable, Sendable, Identifiable {
    public var id: Int
    public var name: String
    public var avatarUrl: String?
    public var avatarColor: String?
    /// The profile the app acts as.
    public var current: Bool
    public var needs: ProfileNeeds

    public init(id: Int, name: String, avatarUrl: String? = nil, avatarColor: String? = nil, current: Bool = false,
                needs: ProfileNeeds = .none) {
        self.id = id
        self.name = name
        self.avatarUrl = avatarUrl
        self.avatarColor = avatarColor
        self.current = current
        self.needs = needs
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(Int.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        avatarUrl = try c.decodeIfPresent(String.self, forKey: .avatarUrl)
        avatarColor = try c.decodeIfPresent(String.self, forKey: .avatarColor)
        current = try c.value(.current, or: false)
        // A profile whose needs the app can't read is one it can't switch to.
        needs = try c.value(.needs, or: .locked)
    }
}
