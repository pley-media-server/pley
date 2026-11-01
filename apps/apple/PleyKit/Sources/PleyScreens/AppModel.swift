import Combine
import Foundation
import PleyAuth
import PleyCaps
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
import PleySubtitles

/// The app's state above its screens: which server is open, who is signed
/// in, which screen shows, and what plays. Both apps' views follow it: a
/// `NavigationStack` with `root` at the bottom and `path` on top, so Back
/// (and Menu on the Siri Remote) goes where people expect. One playback at
/// a time: starting another closes the one before, as on Android.
@MainActor
public final class AppModel: ObservableObject {
    /// The bottom of the navigation.
    public enum Root: Hashable, Sendable {
        /// Opening the server used last, at start.
        case starting
        case servers
        case home
    }

    /// A screen on top of the root.
    public enum Screen: Hashable, Sendable {
        case signIn(ServerEntry)
        case profiles
        /// An item's page.
        case item(Int)
    }

    /// The screen showing.
    public enum Route: Hashable, Sendable {
        case starting
        case servers
        case signIn(ServerEntry)
        case profiles
        case home
        case item(Int)
    }

    /// What the player needs beside the server: this install's client id,
    /// where late reports are kept, the quality picked and Automatic's
    /// ceilings as remembered, a clock and the engine. Tests replace them.
    public struct PlayerSetup {
        public var clientId: String
        /// `nil`: `Application Support/Pley`.
        public var lateReports: URL?
        /// The quality picked, per kind of network.
        public var quality: any QualityMemory
        /// Automatic's ceiling of the server at an origin.
        public var ceilings: (String) -> any CeilingMemory
        public var clock: any PlayerClock
        public var engines: @MainActor () -> any PlayerEngine
        /// The subtitle sync's offsets, per file and profile.
        public var subtitleOffsets: any SubtitleOffsets

        public init(clientId: String = ClientID.current(), lateReports: URL? = nil,
                    quality: any QualityMemory = DefaultsQuality(),
                    ceilings: @escaping (String) -> any CeilingMemory = { DefaultsCeiling(origin: $0) },
                    clock: any PlayerClock = SystemPlayerClock(),
                    engines: @escaping @MainActor () -> any PlayerEngine = { AVPlayerEngine() },
                    subtitleOffsets: any SubtitleOffsets = DefaultsSubtitleOffsets()) {
            self.clientId = clientId
            self.lateReports = lateReports
            self.quality = quality
            self.ceilings = ceilings
            self.clock = clock
            self.engines = engines
            self.subtitleOffsets = subtitleOffsets
        }
    }

    @Published public private(set) var root: Root = .starting
    /// Bound to the `NavigationStack`, so Back pops it.
    @Published public var path: [Screen] = []
    /// Who the app acts as on the open server, when signed in.
    @Published public private(set) var user: User?
    /// The server open, by the name it gave.
    @Published public private(set) var server: ServerEntry?
    /// Why the servers or sign-in show by themselves (a sign-in that ended,
    /// the last server not answering at start); the screen clears it.
    @Published public var notice: String?
    /// Goes up whenever Home's rows may have changed (a new sign-in, another profile).
    @Published public private(set) var homeVersion = 0
    /// The playback open, if any; the player screens show it.
    @Published public private(set) var player: PlaybackController?

    public let session: SessionManager
    public let servers: ServerRepository
    public let images: ImageLoader
    public let transport: HTTPTransport
    /// The sign-in sheet, on iPhone, iPad and Mac; `nil` on Apple TV.
    public let authenticator: (any WebAuthenticator)?
    /// Starts a search of the local network.
    public let discover: @Sendable () -> AsyncStream<DiscoveryState>
    /// What this device plays, read once signed in, before anything plays.
    public let caps: DeviceCaps
    /// The players' report queue: it outlives each player, so a closed
    /// player's last report and the end of its session still go out.
    public let reporter = SessionReporter()
    public let playerSetup: PlayerSetup

    private var listener: Task<Void, Never>?
    /// Counts playbacks asked for, so only the latest of two quick ones opens.
    private var plays = 0

    public var route: Route {
        switch path.last {
        case .signIn(let entry): return .signIn(entry)
        case .profiles: return .profiles
        case .item(let id): return .item(id)
        case nil:
            switch root {
            case .starting: return .starting
            case .servers: return .servers
            case .home: return .home
            }
        }
    }

    public var client: AppClient { session.client }

    public init(session: SessionManager, servers: ServerRepository, transport: HTTPTransport,
                authenticator: (any WebAuthenticator)?,
                discover: @escaping @Sendable () -> AsyncStream<DiscoveryState>,
                caps: DeviceCaps? = nil, playerSetup: PlayerSetup = PlayerSetup()) {
        self.session = session
        self.servers = servers
        self.transport = transport
        self.authenticator = authenticator
        self.discover = discover
        self.caps = caps ?? DeviceCaps()
        self.playerSetup = playerSetup
        // One loader for the app, always with the open server's API.
        images = ImageLoader { [session] path, width in
            guard let api = await session.api else { throw PleyError.signedOut }
            return try await api.image(path: path, width: width)
        }
    }

    /// The app as it runs: the Keychain, the system's sign-in sheet and Bonjour.
    public static func live() -> AppModel {
        let transport = HTTPTransport(userAgent: UserAgent.current)
        let servers = ServerRepository()
        let session = SessionManager(transport: transport, servers: servers)
        let discovery = LocalServers(transport: transport)
        #if os(iOS) || os(macOS)
        let authenticator: (any WebAuthenticator)? = SystemWebAuthenticator()
        #else
        let authenticator: (any WebAuthenticator)? = nil
        #endif
        return AppModel(session: session, servers: servers, transport: transport, authenticator: authenticator,
                        discover: { discovery.browse() })
    }

    /// At start: opens the server used last when the app is signed in to
    /// it, and shows the servers otherwise.
    public func start() async {
        guard root == .starting else { return }
        listen()
        guard let last = await servers.lastUsed() else {
            root = .servers
            return
        }
        do {
            switch try await session.open(origin: last.origin, id: last.id) {
            case .signedIn(let user):
                await opened(last.origin, id: last.id, name: last.name)
                show(user, chooseProfile: false)
            case .signedOut:
                root = .servers
            }
        } catch {
            root = .servers
            notice = "\(last.name): \(describe(error))"
        }
    }

    /// Opens `entry`: Home when the app is signed in to it, its sign-in otherwise.
    public func open(_ entry: ServerEntry) async throws {
        let state = try await session.open(origin: entry.origin, id: entry.id)
        notice = nil
        await opened(entry.origin, id: entry.id, name: entry.name, saved: entry.saved, found: entry.found)
        switch state {
        case .signedIn(let user):
            show(user, chooseProfile: false)
        case .signedOut:
            user = nil
            root = .servers
            path = [.signIn(server ?? entry)]
        }
    }

    /// A sign-in finished: Home, with the household first when it has
    /// more than one profile.
    public func signedIn(_ user: User) async {
        notice = nil
        let household = (try? await session.profiles?.profiles()) ?? []
        show(user, chooseProfile: household.count > 1)
    }

    /// The sign-in moved to another profile.
    public func profileChanged() async {
        await closePlayer()
        if let api = await session.api, let user = try? await api.me() {
            self.user = user
            signedInNow(user)
        }
        homeVersion += 1
        path = []
    }

    /// Ends the sign-in on the open server; the servers show.
    public func signOut() async {
        await closePlayer()
        await session.signOut()
        user = nil
        root = .servers
        path = []
    }

    /// Signs out of `entry` and removes it from the list.
    public func forget(_ entry: ServerEntry) async throws {
        try await session.forget(origin: entry.origin)
        if server?.origin == entry.origin {
            server = nil
            user = nil
        }
    }

    /// The servers, keeping the sign-in.
    public func showServers() {
        Task { await closePlayer() }
        root = .servers
        path = []
    }

    public func showProfiles() {
        path = [.profiles]
    }

    /// An item's page, on top of the screen showing.
    public func showItem(_ id: Int) {
        path.append(.item(id))
    }

    /// Back from the screen on top.
    public func back() {
        if !path.isEmpty { path.removeLast() }
    }

    // MARK: Playing

    /// Plays `request` on the open server, after closing the playback
    /// before (its `stopped` report and `DELETE` go first). `nil` when
    /// signed out, or when another play was asked for meanwhile.
    @discardableResult
    public func play(_ request: PlayRequest) async -> PlaybackController? {
        plays += 1
        let asked = plays
        await closePlayer()
        guard asked == plays, let user, let api = await session.api, let origin = await session.origin,
              asked == plays
        else { return nil }
        let environment = PlaybackEnvironment(
            api: api, caps: caps, clientId: playerSetup.clientId, reporter: reporter,
            late: lateReports(origin: origin, profileId: user.id), quality: playerSetup.quality,
            ceiling: playerSetup.ceilings(origin), clock: playerSetup.clock, engines: playerSetup.engines,
            subtitleOffsets: playerSetup.subtitleOffsets, profileId: user.id)
        let controller = PlaybackController(request: request, environment: environment)
        // Started first, so the screens get its AVPlayer with it.
        controller.start()
        player = controller
        return controller
    }

    /// Ends the playback open (`controller`, when given, only if it is
    /// still the one open): `stopped`, then `DELETE`, waiting for the
    /// reports 2 s at most.
    public func closePlayer(_ controller: PlaybackController? = nil) async {
        guard let current = player, controller == nil || controller === current else { return }
        player = nil
        await current.close()
    }

    /// The reports of this server and profile that couldn't go out before.
    private func lateReports(origin: String, profileId: Int) -> LateReports {
        LateReports(origin: origin, profileId: profileId, directory: playerSetup.lateReports, clock: playerSetup.clock)
    }

    // MARK: Inside

    private func show(_ user: User, chooseProfile: Bool) {
        self.user = user
        homeVersion += 1
        root = .home
        path = chooseProfile ? [.profiles] : []
        signedInNow(user)
    }

    /// Signed in: the device's caps are read ahead of the first Play, and
    /// reports that couldn't reach this server and profile before go now.
    private func signedInNow(_ user: User) {
        Task { await caps.warmUp() }
        Task { [weak self] in
            guard let self, let api = await self.session.api, let origin = await self.session.origin else { return }
            await self.lateReports(origin: origin, profileId: user.id).send(using: api)
        }
    }

    /// Notes the server just opened, by the name it gave.
    private func opened(_ origin: String, id: String?, name: String, saved: Bool = true, found: Bool = false) async {
        let home = await session.origin ?? origin
        if server?.origin != home {
            // Another server's pictures can share these paths.
            await images.empty()
        }
        let given = await session.serverInfo?.name
        let shown = given.map { BonjourTXT.cleanName($0) }.flatMap { $0.isEmpty ? nil : $0 } ?? name
        server = ServerEntry(origin: home, name: shown, id: id, saved: saved, found: found)
    }

    private func listen() {
        guard listener == nil else { return }
        let events = session.events
        listener = Task { [weak self] in
            for await event in events {
                self?.heard(event)
            }
        }
    }

    /// A sign-in ended without the person asking: its sign-in shows, with why.
    private func heard(_ event: SessionEvent) {
        guard case .signedOut(let origin, let message) = event, let server, server.origin == origin else { return }
        Task { await closePlayer() }
        user = nil
        notice = message
        root = .servers
        path = [.signIn(server)]
    }
}

/// `error` in words for people.
public func describe(_ error: any Error) -> String {
    if let error = error as? any LocalizedError, let text = error.errorDescription, !text.isEmpty { return text }
    return "something went wrong: try again"
}

/// `text` as a sentence: a capital first, a full stop last. The app's own
/// texts come in lower case without one, as the Android app's do.
public func asSentence(_ text: String) -> String {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let first = trimmed.first else { return trimmed }
    let capital = first.uppercased() + trimmed.dropFirst()
    return ".…!?".contains(capital.last!) ? capital : capital + "."
}
