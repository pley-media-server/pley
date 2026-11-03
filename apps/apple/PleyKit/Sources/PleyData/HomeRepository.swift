import Foundation
import PleyModel
import PleyNetwork

/// The home page's rows.
public struct HomeRepository: Sendable {
    public let api: PleyAPI

    public init(api: PleyAPI) {
        self.api = api
    }

    /// The rows with something in them, in the server's order.
    public func load() async throws -> [Hub] {
        try await api.home().hubs.filter { !$0.items.isEmpty }
    }
}
