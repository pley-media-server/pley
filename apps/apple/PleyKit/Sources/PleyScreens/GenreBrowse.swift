import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// Search's "Browse by genre" before anything is typed: the genres of one
/// of the profile's movie and show libraries, as its Filter offers them
/// (`GET /api/libraries/{id}/filters`), each opening that library's
/// listing filtered by it. With more than one such library, a menu picks
/// which, the first in the profile's order to begin with; the one picked
/// stays while the app is open. A library whose genres can't be read, or
/// that has none, shows nothing; music libraries have no genres to offer.
@MainActor
public final class GenreBrowseModel: ObservableObject {
    /// The profile's movie and show libraries, in its order.
    @Published public private(set) var libraries: [Library] = []
    /// The library whose genres show.
    @Published public private(set) var libraryId: Int?
    /// Its genres, as the server lists them, `limit` at most.
    @Published public private(set) var genres: [String] = []

    /// How many genres show: two columns of six rows.
    public static let limit = 12
    /// How many colors the tiles take turns with (`tint`).
    public static let tints = 8

    private unowned let app: AppModel

    public init(app: AppModel) {
        self.app = app
    }

    /// Reads the libraries, and the genres of the one picked (or the first).
    public func load() async {
        let video = app.shell.libraries.filter { $0.kind == .movies || $0.kind == .shows }
        libraries = video
        let kept = libraryId.flatMap { id in video.contains { $0.id == id } ? id : nil }
        guard let id = kept ?? video.first?.id else {
            libraryId = nil
            genres = []
            return
        }
        await choose(id)
    }

    /// Shows `libraryId`'s genres.
    public func choose(_ libraryId: Int) async {
        guard libraries.contains(where: { $0.id == libraryId }) else { return }
        self.libraryId = libraryId
        let filters = try? await app.cached(SnapshotKeys.libraryFilters(libraryId)) { try await $0.libraryFilters(libraryId) }
        guard self.libraryId == libraryId else { return }
        genres = Array((filters?.genres ?? []).prefix(Self.limit))
    }

    /// The library showing, by name, for the menu that picks another.
    public var libraryName: String? { libraries.first { $0.id == libraryId }?.name }

    /// The page `genre` opens: its library's listing filtered by it.
    public func screen(for genre: String) -> Screen? {
        libraryId.map { .library($0, view: LibraryParams(genre: genre).queryString) }
    }

    /// Which of `tints` colors `genre`'s tile has: the same for a genre
    /// wherever and whenever it shows, whatever else is listed.
    public static func tint(_ genre: String) -> Int {
        let sum = genre.lowercased().unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0x7FFF_FFFF }
        return sum % tints
    }
}
