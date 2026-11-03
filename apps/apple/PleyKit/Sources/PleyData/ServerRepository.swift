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
/// account `list`), on this device only, in the app's own group.
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
        (try? await load()) ?? []
    }

    /// The list as kept. A Keychain error is thrown, not taken for an
    /// empty list, and isn't kept: the next call reads again.
    private func load() async throws -> [SavedServer] {
        if let cache { return cache }
        let loaded = try await store.load()
        cache = loaded
        return loaded
    }

    /// Saves `servers`, kept at once so that a change made while it is
    /// saved starts from it; put back as it was when the save fails.
    private func save(_ servers: [SavedServer], replacing before: [SavedServer]) async throws {
        cache = servers
        do {
            try await store.save(servers)
        } catch {
            if cache == servers { cache = before }
            throw error
        }
    }

    /// Keeps `server`, replacing one with the same address or id; the
    /// least recently used go past 50.
    public func remember(_ server: SavedServer) async throws {
        let before = try await load()
        var servers = Servers.withServer(before, server)
        if servers.count > Self.maxServers {
            servers = Array(servers.sorted { $0.lastUsed > $1.lastUsed }.prefix(Self.maxServers))
        }
        try await save(servers, replacing: before)
    }

    public func forget(origin: String) async throws {
        let before = try await load()
        try await save(before.filter { $0.origin != origin }, replacing: before)
    }

    /// The server to open by itself at start: the one opened last.
    public func lastUsed() async -> SavedServer? {
        Servers.lastUsed(await saved())
    }
}
