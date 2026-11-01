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
/// in, what shows, and what plays. Both apps' views follow `root`: the
/// servers (a navigation with a server's sign-in on top), "Who's watching?"
/// by itself, or the signed-in app, whose navigation is `shell`'s. One
/// playback at a time: starting another closes the one before, as on Android.
@MainActor
public final class AppModel: ObservableObject {
    /// What fills the window.
    public enum Root: Hashable, Sendable {
        /// Opening the server used last, at start.
        case starting
        /// The servers, with `signingIn` on top.
        case servers
        /// "Who's watching?", by itself.
        case profiles
        /// The signed-in app: `shell`'s sections.
        case signedIn
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
    /// The servers' navigation: a server's sign-in on top of the list.
    /// Bound to its `NavigationStack`, so Back pops it.
    @Published public var signingIn: [ServerEntry] = []
    /// "Who's watching?" was opened from the signed-in app (Switch profile),
    /// so it can go back without a choice.
    @Published public private(set) var profilesFromShell = false
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
    /// What changed on the server last (its events, gathered for 0.3 s,
    /// and this app's own writes): the screens showing it read it again.
    @Published public private(set) var change: ServerChange?
    /// The scans running, by library.
    @Published public private(set) var scans: [Int: ScanStatus] = [:]

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
    /// This device's settings.
    public let settings: AppSettings
    /// The signed-in app's sections and their navigation.
    public let shell: ShellModel
    /// The server's events, made into changes.
    public let serverEvents = ServerEvents()
    /// The item actions and the marks they made.
    public private(set) lazy var actions = ItemActionsModel(app: self)
    /// The event stream of a server's API (the tests give their own).
    private let eventSignals: @Sendable (PleyAPI) -> AsyncStream<EventSignal>
    /// Whether this is the Apple TV app, which asks "Who's watching?" at start.
    private let tv: Bool
    /// Whether the app is in the foreground: the event stream runs only then.
    private var active = true
    private var eventsRunning = false

    private var listener: Task<Void, Never>?
    /// Counts playbacks asked for, so only the latest of two quick ones opens.
    private var plays = 0
    /// One queue of late reports per server and profile, shared by every
    /// sign-in and playback, so two never send the same report.
    private var late: [String: LateReports] = [:]

    public var client: AppClient { session.client }

    public init(session: SessionManager, servers: ServerRepository, transport: HTTPTransport,
                authenticator: (any WebAuthenticator)?,
                discover: @escaping @Sendable () -> AsyncStream<DiscoveryState>,
                caps: DeviceCaps? = nil, playerSetup: PlayerSetup = PlayerSetup(),
                settings: AppSettings = AppSettings(), tv: Bool = ShellModel.isTV,
                eventSignals: @escaping @Sendable (PleyAPI) -> AsyncStream<EventSignal> = {
                    EventStream(api: $0).signals()
                }) {
        self.session = session
        self.servers = servers
        self.transport = transport
        self.authenticator = authenticator
        self.discover = discover
        self.caps = caps ?? DeviceCaps()
        self.playerSetup = playerSetup
        self.settings = settings
        self.tv = tv
        shell = ShellModel(settings: settings, tv: tv)
        self.eventSignals = eventSignals
        // One loader for the app, always with the open server's API.
        images = ImageLoader { [session] path, width in
            guard let api = await session.api else { throw PleyError.signedOut }
            return try await api.image(path: path, width: width)
        }
        serverEvents.onChange = { [weak self] change in
            guard let self else { return }
            self.actions.followed(change)
            self.change = change
        }
        serverEvents.onScans = { [weak self] scans in self?.scans = scans }
        serverEvents.onSessionStopped = { [weak self] id, message, byAdmin in
            self?.sessionStopped(id, message: message, byAdmin: byAdmin)
        }
    }

    /// The app as it runs: the Keychain, the system's sign-in sheet and Bonjour.
    public static func live() -> AppModel {
        let transport = HTTPTransport(userAgent: UserAgent.current)
        let servers = ServerRepository()
        let session = SessionManager(transport: transport, servers: servers)
        let discovery = LocalServers(transport: transport)
        let settings = AppSettings()
        #if os(iOS) || os(macOS)
        // Settings' "Sign in privately", read at each sign-in.
        let authenticator: (any WebAuthenticator)? = SystemWebAuthenticator { settings.privateSignIn }
        #else
        let authenticator: (any WebAuthenticator)? = nil
        #endif
        return AppModel(session: session, servers: servers, transport: transport, authenticator: authenticator,
                        discover: { discovery.browse() }, settings: settings)
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
                await startAs(user)
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
        let before = server?.origin
        await opened(entry.origin, id: entry.id, name: entry.name, saved: entry.saved, found: entry.found)
        switch state {
        case .signedIn(let user):
            // Another server's pages don't belong to this one.
            if before != server?.origin {
                shell.reset()
                actions.reset()
            }
            show(user, chooseProfile: false)
        case .signedOut:
            user = nil
            root = .servers
            signingIn = [server ?? entry]
        }
    }

    /// A sign-in finished: Home, with "Who's watching?" first when the
    /// household has more than one profile.
    public func signedIn(_ user: User) async {
        notice = nil
        let household = (try? await session.profiles?.profiles()) ?? []
        noteHousehold(household)
        shell.reset()
        actions.reset()
        show(user, chooseProfile: household.count > 1)
    }

    /// The sign-in moved to another profile: Home, as that profile.
    public func profileChanged() async {
        await closePlayer()
        if let api = await session.api, let user = try? await api.me() {
            self.user = user
            signedInNow(user)
        }
        homeVersion += 1
        shell.reset()
        actions.reset()
        profilesFromShell = false
        root = .signedIn
        // Another profile's token: its own stream.
        restartEvents()
    }

    /// Ends the sign-in on the open server; the servers show.
    public func signOut() async {
        await closePlayer()
        stopEvents()
        await session.signOut()
        user = nil
        shell.reset()
        actions.reset()
        root = .servers
        signingIn = []
    }

    /// Signs out of `entry` and removes it from the list.
    public func forget(_ entry: ServerEntry) async throws {
        try await session.forget(origin: entry.origin)
        if server?.origin == entry.origin {
            server = nil
            user = nil
        }
    }

    /// The servers, keeping the sign-in; with `adding`, the address
    /// field is ready for a new one (the Mac's File → Add Server…).
    public func showServers(adding: Bool = false) {
        Task { await closePlayer() }
        root = .servers
        signingIn = []
        if adding { addingServer = true }
    }

    /// Set by Add Server…: the servers' address field takes the keys. The
    /// servers screen clears it once it has (`addingServerShown`).
    @Published public private(set) var addingServer = false

    public func addingServerShown() {
        addingServer = false
    }

    /// Whether Switch profile is offered: the sign-in has the `profiles`
    /// scope, and the household, as last seen, has more than one profile
    /// (or hasn't been seen yet).
    public var canSwitchProfile: Bool {
        guard let user, user.scopes.contains("profiles") else { return false }
        guard let origin = server?.origin else { return true }
        let household = settings.household(origin: origin)
        return household.isEmpty || household.count > 1
    }

    /// Back from a server's sign-in to the servers.
    public func closeSignIn() {
        signingIn = []
    }

    /// "Who's watching?", from the signed-in app (Switch profile).
    public func showProfiles() {
        guard user != nil else { return }
        profilesFromShell = true
        root = .profiles
    }

    /// The profile as the server now says it is (after a change to its track defaults).
    func updated(_ user: User) {
        guard user.id == self.user?.id else { return }
        var user = user
        // The server's answer to a write may leave the token's scope out.
        if user.scope == nil { user.scope = self.user?.scope }
        self.user = user
    }

    /// The household as just read: kept, so the next start knows it
    /// without the network, and Settings can offer "Always start as".
    public func noteHousehold(_ profiles: [HouseholdProfile]) {
        guard let origin = server?.origin, !profiles.isEmpty else { return }
        // Switch Profile follows it.
        objectWillChange.send()
        settings.setHousehold(profiles.map { ($0.id, $0.name) }, origin: origin)
    }

    /// "Who's watching?" ends without a switch: the profile in use stays.
    public func keepProfile() {
        guard user != nil else { return }
        profilesFromShell = false
        root = .signedIn
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
            subtitleOffsets: playerSetup.subtitleOffsets, profileId: user.id,
            autoplayNext: settings.autoplayNext)
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

    /// The reports of this server and profile that couldn't go out before:
    /// one queue per file, whoever asks for it.
    func lateReports(origin: String, profileId: Int) -> LateReports {
        let key = LateReports.key(origin: origin, profileId: profileId)
        if let queue = late[key] { return queue }
        let queue = LateReports(origin: origin, profileId: profileId, directory: playerSetup.lateReports,
                                clock: playerSetup.clock)
        late[key] = queue
        return queue
    }

    // MARK: The server's events

    /// The app came to the foreground (`true`) or left it: the event
    /// stream runs only in the foreground. Back in front, everything shown
    /// is read again, since events were missed.
    public func sceneActive(_ active: Bool) {
        guard active != self.active else { return }
        self.active = active
        if active {
            startEvents()
            if user != nil { note(ServerChange(home: true, libraries: true, watchlist: true, everything: true)) }
        } else {
            stopEvents()
        }
    }

    /// A change this app made itself: the other screens showing it follow.
    public func note(_ change: ServerChange) {
        serverEvents.note(change)
    }

    private func startEvents() {
        guard active, user != nil, !eventsRunning else { return }
        eventsRunning = true
        let signals = eventSignals
        Task { [weak self] in
            guard let self, let api = await self.session.api, self.eventsRunning else { return }
            self.serverEvents.start(signals(api))
        }
    }

    private func stopEvents() {
        eventsRunning = false
        serverEvents.stop()
    }

    private func restartEvents() {
        stopEvents()
        startEvents()
    }

    /// One of this profile's playbacks was stopped from elsewhere: the
    /// player showing it closes, and says why.
    private func sessionStopped(_ id: String, message: String?, byAdmin: Bool) {
        guard let player, player.session?.id == id else { return }
        let text = player.stoppedByServer(message: message, byAdmin: byAdmin)
        shell.show(text)
        Task { await closePlayer(player) }
    }

    // MARK: Inside

    /// The start on a server signed in to: on Apple TV, "Who's watching?"
    /// when the household had several profiles and no "Always start as" is
    /// set, or a switch to that profile; Home everywhere else.
    private func startAs(_ user: User) async {
        let origin = server?.origin ?? ""
        let gate = StartGate.decide(tv: tv, household: settings.household(origin: origin).map(\.id),
                                    alwaysStartAs: settings.alwaysStartAs(origin: origin), current: user.id)
        switch gate {
        case .home:
            show(user, chooseProfile: false)
        case .profiles:
            show(user, chooseProfile: true)
        case .switchTo(let id):
            if let switched = await switchAtStart(to: id) {
                show(switched, chooseProfile: false)
            } else {
                // One with a PIN, or one gone: the person chooses.
                show(user, chooseProfile: true)
            }
        }
    }

    /// Moves the sign-in to profile `id` when it needs nothing (no PIN):
    /// the profile it is then, or `nil` when it couldn't.
    private func switchAtStart(to id: Int) async -> User? {
        guard let switcher = await session.profiles, let profiles = try? await switcher.profiles() else { return nil }
        noteHousehold(profiles)
        guard let profile = profiles.first(where: { $0.id == id }), profile.needs == .none else { return nil }
        do {
            try await switcher.switchTo(id, pin: nil)
            guard let api = await session.api else { return nil }
            return try await api.me()
        } catch {
            return nil
        }
    }

    private func show(_ user: User, chooseProfile: Bool) {
        self.user = user
        homeVersion += 1
        signingIn = []
        profilesFromShell = false
        root = chooseProfile ? .profiles : .signedIn
        signedInNow(user)
    }

    /// Signed in: the device's caps are read ahead of the first Play, and
    /// reports that couldn't reach this server and profile before go now.
    private func signedInNow(_ user: User) {
        Task { await caps.warmUp() }
        startEvents()
        Task { await actions.loadWatchlist() }
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
        stopEvents()
        user = nil
        notice = message
        shell.reset()
        actions.reset()
        root = .servers
        signingIn = [server]
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

/// Where the app opens on a server it is signed in to (Android TV's start
/// gate): Apple TV asks "Who's watching?" when the household, as last seen,
/// has several profiles and Settings' "Always start as" isn't set, and
/// starts as that profile when it is. iPhone, iPad and Mac open Home as the
/// profile used last, as Android's phone app does.
public enum StartGate {
    public enum Outcome: Hashable, Sendable {
        case home
        case profiles
        /// Starts as this profile, switching to it first.
        case switchTo(Int)
    }

    public static func decide(tv: Bool, household: [Int], alwaysStartAs: Int?, current: Int) -> Outcome {
        guard tv, household.count > 1 else { return .home }
        guard let always = alwaysStartAs, household.contains(always) else { return .profiles }
        return always == current ? .home : .switchTo(always)
    }
}
