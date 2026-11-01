import Combine
import Foundation
import PleyModel
import PleyNetwork

/// Titles of one role on a person's page: "Acting", "Directing"…
public struct RoleRow: Hashable, Sendable, Identifiable {
    public var id: String { role.rawValue }
    public var role: CreditRole
    /// "Acting", "Directing", "Creating", "Writing", "Producing".
    public var title: String
    public var tiles: [Tile]
}

/// A person's page (Android's `personPage`): what they are in, in this
/// server's libraries, newest first.
public struct PersonPage: Hashable, Sendable {
    public var id: Int
    public var name: String
    /// The photo's server path; `nil` without one.
    public var photo: String?
    /// "3 movies · 2 shows", for "… in your libraries"; empty when there are none.
    public var counts: String
    /// Their movies, each with "2019 · as Neo".
    public var movies: [Tile]
    public var shows: [Tile]
    /// The same titles by role (Apple TV's rows): acting first, then
    /// directing, creating, writing and producing.
    public var roles: [RoleRow]

    /// The page of `person`.
    public init(_ person: PersonDetail) {
        var seen = Set<Int>()
        let credits = person.credits.filter { seen.insert($0.item.id).inserted }
        let movies = credits.filter { $0.item.kind == .movie }
        let shows = credits.filter { $0.item.kind == .show }
        id = person.id
        name = person.name
        photo = person.photoUrl.flatMap { $0.isEmpty ? nil : $0 }
        counts = [movies.isEmpty ? nil : Words.pluralize(movies.count, "movie"),
                  shows.isEmpty ? nil : Words.pluralize(shows.count, "show")].compactMap { $0 }
            .joined(separator: " · ")
        self.movies = movies.map { Self.tile($0.item, $0.roles) }
        self.shows = shows.map { Self.tile($0.item, $0.roles) }
        let order: [(CreditRole, String)] = [(.actor, "Acting"), (.director, "Directing"), (.creator, "Creating"),
                                             (.writer, "Writing"), (.producer, "Producing")]
        roles = order.compactMap { role, title in
            let titles = credits.filter { $0.roles.contains { $0.role == role } }
            guard !titles.isEmpty else { return nil }
            return RoleRow(role: role, title: title,
                           tiles: titles.map { Self.tile($0.item, $0.roles.filter { $0.role == role }) })
        }
    }

    /// A poster with "2019 · as Neo" under it.
    private static func tile(_ item: ItemSummary, _ roles: [PersonRole]) -> Tile {
        let line = [item.year.map(String.init) ?? "", Words.roleText(roles)].filter { !$0.isEmpty }
            .joined(separator: " · ")
        return Tile(item, shape: .poster, line: line)
    }
}

/// A person's page: `GET /api/people/{id}`. A person with nothing this
/// profile may see is not found (404).
@MainActor
public final class PersonModel: ObservableObject {
    public enum State: Equatable {
        case loading
        case loaded
        case notFound
        case failed(String)
    }

    public static let notFoundTitle = "Person not found"

    @Published public private(set) var state: State = .loading
    @Published public private(set) var page: PersonPage?

    public let app: AppModel
    public let personId: Int

    public init(app: AppModel, personId: Int) {
        self.app = app
        self.personId = personId
    }

    public func load() async {
        guard let api = await app.session.api else { return }
        if case .failed = state { state = .loading }
        do {
            page = PersonPage(try await api.person(personId))
            state = .loaded
        } catch is CancellationError {
        } catch PleyError.signedOut {
        } catch PleyError.api(status: 404, _, _) {
            page = nil
            state = .notFound
        } catch {
            if page == nil { state = .failed(describe(error)) }
        }
    }
}
