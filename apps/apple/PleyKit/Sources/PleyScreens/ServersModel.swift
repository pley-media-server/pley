import Combine
import Foundation
import PleyCommon
import PleyNetwork

/// The servers screen: the ones found on the network and the ones opened
/// before, in one list (`Servers.merge`), and adding one by its address.
@MainActor
public final class ServersModel: ObservableObject {
    @Published public private(set) var entries: [ServerEntry] = []
    /// The system refused the app the local network, so finding nothing says nothing.
    @Published public private(set) var blocked = false
    @Published public private(set) var searching = false
    /// What is typed under "Add a server".
    @Published public var address = ""
    @Published public private(set) var adding = false
    /// The server being opened, by origin.
    @Published public private(set) var opening: String?
    @Published public var error: String?

    public let app: AppModel
    private var saved: [SavedServer] = []
    private var found: [FoundServer] = []

    public init(app: AppModel) {
        self.app = app
    }

    /// Servers found on this network and not saved.
    public var foundEntries: [ServerEntry] { entries.filter { !$0.saved } }
    /// Servers opened before, the last one first.
    public var savedEntries: [ServerEntry] { entries.filter(\.saved) }

    /// Reads the saved servers.
    public func load() async {
        saved = await app.servers.saved()
        merge()
    }

    /// Searches the network until the task running it is cancelled (the
    /// screen goes away).
    public func browse() async {
        searching = true
        for await state in app.discover() {
            found = state.servers
            blocked = state.blocked
            searching = state.searching
            merge()
        }
        searching = false
    }

    /// Checks the typed address is a Pley server, then opens it.
    public func add() async {
        let text = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !adding else { return }
        error = nil
        adding = true
        defer { adding = false }
        switch await ServerProbe(transport: app.transport).probe(text) {
        case .pley(let origin, let info):
            let name = BonjourTXT.cleanName(info.name)
            let entry = ServerEntry(origin: origin, name: name.isEmpty ? Servers.displayAddress(origin) : name,
                                    saved: false, found: false)
            if await open(entry) { address = "" }
        case .notPley:
            error = PleyError.notPley.userMessage
        case .unreachable(let why):
            error = why
        case .blocked:
            error = PleyError.localNetworkBlocked.userMessage
        }
    }

    /// Opens `entry`; `false` when it couldn't be (`error` says why).
    @discardableResult
    public func open(_ entry: ServerEntry) async -> Bool {
        guard opening == nil else { return false }
        error = nil
        opening = entry.origin
        defer { opening = nil }
        do {
            try await app.open(entry)
            return true
        } catch is CancellationError {
            return false
        } catch {
            self.error = localized("\(entry.name): \(describe(error))")
            return false
        }
    }

    /// Signs out of `entry` and removes it.
    public func forget(_ entry: ServerEntry) async {
        do {
            try await app.forget(entry)
        } catch {
            self.error = describe(error)
        }
        await load()
    }

    private func merge() {
        entries = Servers.merge(saved: saved, found: found)
    }
}
