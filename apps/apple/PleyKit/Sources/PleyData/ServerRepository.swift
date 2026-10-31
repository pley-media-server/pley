import Foundation
import os
import PleyAuth
import PleyCommon

/// Where the list of servers the person opened is kept.
public protocol ServerListStore: Sendable {
    func load() async throws -> [SavedServer]
    func save(_ servers: [SavedServer]) async throws
}

/// The server list in the Keychain (service `app.pley.apple.servers`,
/// account `list`), on this device only, where a Top Shelf extension can
/// read it later from the app's own group.
public struct KeychainServerListStore: ServerListStore {
    public static let service = "app.pley.apple.servers"

    private let item = KeychainItem(service: KeychainServerListStore.service, account: "list")

    public init() {}

    public func load() async throws -> [SavedServer] {
        guard let data = try item.read() else { return [] }
        // Unreadable (an older format): an empty list rather than no app.
        return (try? JSONDecoder().decode([SavedServer].self, from: data)) ?? []
    }

    public func save(_ servers: [SavedServer]) async throws {
        try item.write(try JSONEncoder().encode(servers))
    }
}

/// The server list in memory, for tests.
public final class MemoryServerListStore: ServerListStore {
    private let servers: OSAllocatedUnfairLock<[SavedServer]>

    public init(_ servers: [SavedServer] = []) {
        self.servers = OSAllocatedUnfairLock(initialState: servers)
    }

    public func load() async throws -> [SavedServer] { servers.withLock { $0 } }

    public func save(_ servers: [SavedServer]) async throws { self.servers.withLock { $0 = servers } }
}

/// The servers the person opened, newest kept, at most 50.
public actor ServerRepository {
    public static let maxServers = 50

    private let store: any ServerListStore
    private var cache: [SavedServer]?

    public init(store: any ServerListStore = KeychainServerListStore()) {
        self.store = store
    }

    public func saved() async -> [SavedServer] {
        if let cache { return cache }
        let loaded = (try? await store.load()) ?? []
        cache = loaded
        return loaded
    }

    /// Keeps `server`, replacing one with the same address or id; the
    /// least recently used go past 50.
    public func remember(_ server: SavedServer) async throws {
        var servers = Servers.withServer(await saved(), server)
        if servers.count > Self.maxServers {
            servers = Array(servers.sorted { $0.lastUsed > $1.lastUsed }.prefix(Self.maxServers))
        }
        try await store.save(servers)
        cache = servers
    }

    public func forget(origin: String) async throws {
        let servers = await saved().filter { $0.origin != origin }
        try await store.save(servers)
        cache = servers
    }

    /// The server to open by itself at start: the one opened last.
    public func lastUsed() async -> SavedServer? {
        Servers.lastUsed(await saved())
    }
}
