import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// Home: the server's rows for the profile signed in.
@MainActor
public final class HomeModel: ObservableObject {
    public enum State {
        case loading
        case loaded([Hub])
        case failed(String)
    }

    @Published public private(set) var state: State = .loading

    public let app: AppModel

    public init(app: AppModel) {
        self.app = app
    }

    /// Loads the rows again; the rows shown stay until the new ones come.
    public func reload() async {
        guard let api = await app.session.api else { return }
        if case .failed = state { state = .loading }
        do {
            state = .loaded(try await HomeRepository(api: api).load())
        } catch is CancellationError {
        } catch PleyError.signedOut {
            // The session says so, and sign-in shows.
        } catch {
            if case .loaded = state { return }
            state = .failed(describe(error))
        }
    }

    /// The picture for `item`'s tile: its poster, else its thumbnail (an episode's).
    public static func posterPath(_ item: ItemSummary) -> String? {
        [item.posterUrl, item.thumbUrl].compactMap { $0 }.first { !$0.isEmpty }
    }

    /// Pixels to ask for a tile `points` wide on a screen of `scale`.
    public static func pixels(_ points: Double, scale: Double) -> Int {
        max(1, Int((points * max(scale, 1)).rounded()))
    }
}
