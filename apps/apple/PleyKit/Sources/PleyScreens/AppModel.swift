import Combine
import CoreGraphics
import Foundation
import Network
import PleyAuth
import PleyCaps
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
import PleyShelf
import PleySubtitles

/// The app's state above its screens: which server is open, who is signed
/// in, what shows, and what plays. Both apps' views follow `root`: the
/// servers (a navigation with a server's sign-in on top), "Who's watching?"
/// by itself, or the signed-in app, whose navigation is `shell`'s. One
/// playback at a time: starting another closes the one before, as on Android.
@MainActor
public final class AppModel: ObservableObject, SnapshotCache {
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
        /// The live player's engine.
        public var liveEngines: @MainActor () -> any LiveEngine
        /// The subtitle sync's offsets, per file and profile.
        public var subtitleOffsets: any SubtitleOffsets
        /// Music's output, made when music first plays.
        public var musicOutputs: @MainActor () -> any MusicOutput
        /// The system's Now Playing and remote commands.
        public var nowPlaying: @MainActor () -> any NowPlayingSystem
        /// A random number below its argument, for shuffling music.
        public var random: @Sendable (Int) -> Int

        public init(clientId: String = ClientID.current(), lateReports: URL? = nil,
                    quality: any QualityMemory = DefaultsQuality(),
                    ceilings: @escaping (String) -> any CeilingMemory = { DefaultsCeiling(origin: $0) },
                    clock: any PlayerClock = SystemPlayerClock(),
                    engines: @escaping @MainActor () -> any PlayerEngine = { AVPlayerEngine() },
                    liveEngines: @escaping @MainActor () -> any LiveEngine = { AVLiveEngine() },
                    subtitleOffsets: any SubtitleOffsets = DefaultsSubtitleOffsets(),
                    musicOutputs: @escaping @MainActor () -> any MusicOutput = { AVMusicOutput() },
                    nowPlaying: @escaping @MainActor () -> any NowPlayingSystem = { SystemNowPlaying() },
                    random: @escaping @Sendable (Int) -> Int = { Int.random(in: 0..<$0) }) {
            self.clientId = clientId
            self.lateReports = lateReports
            self.quality = quality
            self.ceilings = ceilings
            self.clock = clock
            self.engines = engines
            self.liveEngines = liveEngines
            self.subtitleOffsets = subtitleOffsets
            self.musicOutputs = musicOutputs
            self.nowPlaying = nowPlaying
            self.random = random
        }
    }

    @Published public private(set) var root: Root = .starting
    /// The servers' navigation: a server's sign-in on top of the list.
    /// Bound to its `NavigationStack`, so Back pops it.
    @Published public var signingIn: [ServerEntry] = []
    /// "Who's watching?" was opened from the signed-in app (Switch profile),
    /// so it can go back without a choice.
    @Published public private(set) var profilesFromShell = false
    /// "Who's watching?" is Apple TV's start gate: going on as the profile
    /// in use asks for its PIN when it has one, as on Android TV.
    @Published public private(set) var profilesAtStart = false
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
    /// The video plays in Picture in Picture: the player screen is put
    /// away meanwhile, and the playback stays open.
    @Published public private(set) var pictureInPicture = false
    /// The playback the player screen shows: `player`, except while it
    /// plays in Picture in Picture.
    public var playerShown: PlaybackController? { pictureInPicture ? nil : player }
    /// The live player open, if any: one playback at a time, so it and `player` are never both open.
    @Published public private(set) var livePlayer: LivePlayerModel?
    /// What changed on the server last (its events, gathered for 0.3 s,
    /// and this app's own writes): the screens showing it read it again.
    @Published public private(set) var change: ServerChange?
    /// The scans running, by library.
    @Published public private(set) var scans: [Int: ScanStatus] = [:]
    /// The server can't be reached: the screens show what was saved of
    /// them, the banner says so, and the app tries again (`reconnect`).
    @Published public private(set) var offline = false

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
    /// Live TV on the open server, shared by its pages, Search and the live player.
    public let live: LiveTvRepository
    /// Music, for the app's whole life: it plays on with no screen open.
    public private(set) lazy var music: MusicPlayback = makeMusic()
    /// The system's Now Playing: music, video or Live TV, whichever played last.
    public let nowPlaying: NowPlayingCenter
    #if os(iOS) || os(tvOS)
    /// The audio session: music's, a video's, or none.
    public let audio = AudioSessionController()
    #endif
    /// The item actions and the marks they made.
    public private(set) lazy var actions = ItemActionsModel(app: self)
    /// The downloads (iPhone, iPad and Mac); none on Apple TV.
    public let downloads: any DownloadsService
    /// The screens as last read, per server and profile, for when the
    /// server can't be reached; `nil`: nothing is saved (tests).
    public let snapshots: SnapshotStore?
    /// Apple TV's Top Shelf: Home's Continue Watching, written for the
    /// extension; `nil` elsewhere, and on a build without App Groups.
    public let shelf: TopShelfWriter?
    /// A Top Shelf link that came before the app could act on it (at
    /// start, or while "Who's watching?" shows).
    private var shelfLink: ShelfLink?
    private var rootWatch: AnyCancellable?
    /// When the server is tried again after it couldn't be reached: then every minute.
    public static let reconnectDelays: [Double] = [2, 5, 15, 30]
    public static let reconnectEvery: Double = 60
    /// How long a try waits for the server's answer.
    public static let reconnectWait: Duration = .seconds(10)
    private var reconnectTask: Task<Void, Never>?
    /// Wakes the reconnect loop when the network changes.
    private var paths: NWPathMonitor?
    /// The network changed since the last try.
    private var pathChanged = false
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
    /// What the video or the live player shows the system, as they change.
    private var playerWatch: AnyCancellable?
    private var liveWatch: AnyCancellable?
    /// What the system's Now Playing was told last of a video, to send only what changed.
    private var videoShown: (info: NowPlayingInfo, at: Int64)?
    /// The picture Now Playing shows for the music or video playing: its path and the image.
    private var artwork: (path: String, image: CGImage)?
    private var artworkTask: Task<Void, Never>?

    public var client: AppClient { session.client }

    public init(session: SessionManager, servers: ServerRepository, transport: HTTPTransport,
                authenticator: (any WebAuthenticator)?,
                discover: @escaping @Sendable () -> AsyncStream<DiscoveryState>,
                caps: DeviceCaps? = nil, playerSetup: PlayerSetup = PlayerSetup(),
                settings: AppSettings = AppSettings(), tv: Bool = ShellModel.isTV,
                downloads: (any DownloadsService)? = nil, snapshots: SnapshotStore? = nil,
                shelf: ((@escaping @Sendable (String, Int) async throws -> Data) -> TopShelfWriter?)? = nil,
                eventSignals: @escaping @Sendable (PleyAPI) -> AsyncStream<EventSignal> = {
                    EventStream(api: $0).signals()
                }) {
        self.session = session
        self.downloads = downloads ?? NoDownloads()
        self.snapshots = snapshots
        // The shelf reads the open server's pictures as they are.
        self.shelf = shelf? { [session] path, width in
            guard let api = await session.api else { throw PleyError.signedOut }
            return try await api.image(path: path, width: width)
        }
        self.servers = servers
        self.transport = transport
        self.authenticator = authenticator
        self.discover = discover
        self.caps = caps ?? DeviceCaps()
        self.playerSetup = playerSetup
        self.settings = settings
        self.tv = tv
        shell = ShellModel(settings: settings, tv: tv)
        nowPlaying = NowPlayingCenter(system: playerSetup.nowPlaying())
        self.eventSignals = eventSignals
        live = LiveTvRepository { [session] in
            guard let api = await session.api else { throw PleyError.signedOut }
            return api
        }
        // One loader for the app, always with the open server's API.
        images = ImageLoader { [session] path, width in
            guard let api = await session.api else { throw PleyError.signedOut }
            return try await api.image(path: path, width: width)
        }
        serverEvents.onChange = { [weak self] change in
            guard let self else { return }
            self.actions.followed(change)
            self.change = change
            if change.libraries || change.everything { self.loadLibraries() }
            if change.live || change.everything {
                self.loadLiveTv()
                self.live.refresh()
            }
        }
        serverEvents.onScans = { [weak self] scans in self?.scans = scans }
        // A link waiting acts once the app shows what it can act on.
        rootWatch = $root.dropFirst().removeDuplicates().sink { [weak self] _ in
            Task { @MainActor in self?.settleShelfLink() }
        }
        live.cache = self
        serverEvents.onSessionStopped = { [weak self] id, message, byAdmin in
            self?.sessionStopped(id, message: message, byAdmin: byAdmin)
        }
        #if os(iOS) || os(tvOS)
        audio.onPause = { [weak self] in
            self?.music.interrupted()
            self?.player?.pause()
        }
        audio.onResumeMusic = { [weak self] in self?.music.play() }
        #endif
    }

    /// The app as it runs: the Keychain, the system's sign-in sheet and
    /// Bonjour; `downloads` makes the downloads (iPhone, iPad and Mac) with
    /// the settings and the connection's gate.
    public static func live(downloads: ((AppSettings, ConnectionGate) -> any DownloadsService)? = nil) -> AppModel {
        let transport = HTTPTransport(userAgent: UserAgent.current)
        let servers = ServerRepository()
        let session = SessionManager(transport: transport, servers: servers)
        let discovery = LocalServers(transport: transport)
        let settings = AppSettings()
        #if os(iOS) || os(macOS)
        // Settings' "Sign in privately", read at each sign-in.
        let authenticator: (any WebAuthenticator)? = SystemWebAuthenticator { settings.privateSignIn }
        let shelf: ((@escaping @Sendable (String, Int) async throws -> Data) -> TopShelfWriter?)? = nil
        #else
        let authenticator: (any WebAuthenticator)? = nil
        let shelf: ((@escaping @Sendable (String, Int) async throws -> Data) -> TopShelfWriter?)? = {
            TopShelfWriter.appGroup(images: $0)
        }
        #endif
        return AppModel(session: session, servers: servers, transport: transport, authenticator: authenticator,
                        discover: { discovery.browse() }, settings: settings,
                        downloads: downloads?(settings, transport.gate), snapshots: SnapshotStore(), shelf: shelf)
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
            // Out of reach: what was saved shows, while the app tries again.
            if LateReports.unreachable(error), await startOffline(last) { return }
            root = .servers
            notice = "\(last.name): \(describe(error))"
        }
    }

    /// Starts on `last` without the server, as the profile used last, from
    /// its saved screens: when a sign-in is kept and its profile was saved.
    private func startOffline(_ last: SavedServer) async -> Bool {
        guard let snapshots, let profileId = settings.lastProfile(origin: last.origin),
              let data = await snapshots.read(origin: last.origin, profileId: profileId, key: SnapshotKeys.me),
              let saved = try? PleyJSON.decoder.decode(User.self, from: data),
              await session.openSaved(origin: last.origin, id: last.id)
        else { return false }
        await opened(last.origin, id: last.id, name: last.name)
        user = saved
        homeVersion += 1
        signingIn = []
        root = .signedIn
        wentOffline()
        loadLibraries()
        loadLiveTv()
        await scopeDownloads()
        return true
    }

    /// Opens `entry`: Home when the app is signed in to it, its sign-in otherwise.
    public func open(_ entry: ServerEntry) async throws {
        let state = try await session.open(origin: entry.origin, id: entry.id)
        notice = nil
        let before = server?.origin
        await opened(entry.origin, id: entry.id, name: entry.name, saved: entry.saved, found: entry.found)
        switch state {
        case .signedIn(let user):
            // Another server's pages don't belong to this one, nor its event stream.
            if before != server?.origin {
                music.stop()
                stopEvents()
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
        music.stop()
        shell.reset()
        actions.reset()
        // A new token, perhaps on another server: its own stream.
        stopEvents()
        show(user, chooseProfile: household.count > 1)
    }

    /// The sign-in moved to another profile: Home, as that profile.
    public func profileChanged() async {
        await closePlayer()
        closeLivePlayer()
        music.stop()
        if let api = await session.api, let user = try? await api.me() {
            self.user = user
            signedInNow(user)
        }
        homeVersion += 1
        shell.reset()
        actions.reset()
        profilesFromShell = false
        profilesAtStart = false
        root = .signedIn
        // Another profile's token: its own stream.
        restartEvents()
    }

    /// Ends the sign-in on the open server; the servers show.
    public func signOut() async {
        await closePlayer()
        closeLivePlayer()
        music.stop()
        stopEvents()
        await session.signOut()
        // The downloads stay on the device, for when this profile signs in again.
        downloads.signedIn(nil)
        shelf?.clear()
        stopReconnecting()
        user = nil
        shell.reset()
        actions.reset()
        root = .servers
        signingIn = []
    }

    /// Signs out of `entry` and removes it from the list.
    public func forget(_ entry: ServerEntry) async throws {
        try await session.forget(origin: entry.origin)
        // A server forgotten takes its downloads and saved screens with it.
        downloads.forgetServer(entry.origin)
        await snapshots?.clear(origin: entry.origin)
        shelf?.forget(origin: entry.origin)
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
        profilesAtStart = false
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
        profilesAtStart = false
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
        music.pauseForVideo()
        closeLivePlayer()
        await closePlayer()
        guard asked == plays, let user, let api = await session.api, let origin = await session.origin,
              asked == plays
        else { return nil }
        let environment = PlaybackEnvironment(
            api: api, caps: caps, clientId: playerSetup.clientId, reporter: reporter,
            late: lateReports(origin: origin, profileId: user.id), quality: playerSetup.quality,
            ceiling: playerSetup.ceilings(origin), clock: playerSetup.clock, engines: playerSetup.engines,
            subtitleOffsets: playerSetup.subtitleOffsets, profileId: user.id,
            autoplayNext: settings.autoplayNext, local: downloads.local)
        let controller = PlaybackController(request: request, environment: environment)
        // Started first, so the screens get its AVPlayer with it.
        controller.start()
        player = controller
        follow(controller)
        return controller
    }

    /// Ends the playback open (`controller`, when given, only if it is
    /// still the one open): `stopped`, then `DELETE`, waiting for the
    /// reports 2 s at most.
    public func closePlayer(_ controller: PlaybackController? = nil) async {
        guard let current = player, controller == nil || controller === current else { return }
        player = nil
        pictureInPicture = false
        playerWatch = nil
        videoShown = nil
        nowPlaying.release(.video)
        playbackChanged()
        await current.close()
    }

    /// Picture in Picture started: the player screen goes away, and the
    /// video goes on in its window.
    public func pictureInPictureStarted() {
        guard player != nil else { return }
        pictureInPicture = true
    }

    /// Picture in Picture asks for the player back: it shows again.
    public func pictureInPictureRestored() {
        pictureInPicture = false
    }

    /// Picture in Picture closed with its ✕: the playback ends, as it would
    /// closing the player.
    public func pictureInPictureClosed() {
        guard pictureInPicture else { return }
        pictureInPicture = false
        Task { await closePlayer() }
    }

    /// Opens the live player on `request`'s channel, closing what played
    /// before (a video, or another channel). `seen`: the channel as the
    /// list showed it, so its name shows at once.
    @discardableResult
    public func watchLive(_ request: LiveRequest, seen: LiveChannel? = nil) async -> LivePlayerModel? {
        plays += 1
        let asked = plays
        music.pauseForVideo()
        closeLivePlayer()
        await closePlayer()
        guard asked == plays, user != nil else { return nil }
        let playback = LivePlayback(live: live, clientId: playerSetup.clientId, clock: playerSetup.clock,
                                    engines: playerSetup.liveEngines)
        let model = LivePlayerModel(request: request, live: live, playback: playback, seen: seen,
                                    toast: { [weak self] text in self?.shell.show(text) })
        livePlayer = model
        follow(model)
        return model
    }

    /// Closes the live player: its session ends at the server.
    public func closeLivePlayer() {
        guard let current = livePlayer else { return }
        livePlayer = nil
        liveWatch = nil
        nowPlaying.release(.live)
        playbackChanged()
        current.close()
    }

    /// Live TV's page for the server and profile open.
    public func liveTvModel() -> LiveTvModel {
        LiveTvModel(live: live, settings: settings, origin: server?.origin ?? "", profileId: user?.id ?? 0,
                    toast: { [weak self] text in self?.shell.show(text) })
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

    // MARK: Music and Now Playing

    /// Plays `tracks` (music's, in the order given) from the one at
    /// `start`; with `shuffle`, in a random order that starts there. A
    /// video or Live TV playing closes first (on the Mac, its window). On
    /// Apple TV, the Now Playing tab shows.
    public func playMusic(_ tracks: [ItemSummary], start: Int = 0, shuffle: Bool = false) async {
        let first = tracks.indices.contains(start) ? start : 0
        let playable = tracks.enumerated().filter { $0.element.kind == .track }
        guard !playable.isEmpty, user != nil else { return }
        let from = playable.firstIndex { $0.offset >= first } ?? 0
        plays += 1
        closeLivePlayer()
        await closePlayer()
        music.play(tracks: playable.map(\.element), start: from, shuffle: shuffle)
        if tv { shell.select(.nowPlaying) }
    }

    private func makeMusic() -> MusicPlayback {
        let setup = playerSetup
        let environment = MusicEnvironment(
            api: { [session] in
                guard let api = await session.api else { throw PleyError.signedOut }
                return api
            },
            caps: { [caps] in await caps.warmUp().report },
            // Music's own id: a video starting doesn't end music's session, nor the other way round.
            clientId: String((setup.clientId + "-music").prefix(128)),
            reporter: reporter,
            late: { [weak self] in
                guard let self, let user = self.user, let origin = await self.session.origin else { return nil }
                return self.lateReports(origin: origin, profileId: user.id)
            },
            clock: setup.clock, outputs: setup.musicOutputs, random: setup.random)
        let music = MusicPlayback(environment: environment)
        music.onChange = { [weak self] state in self?.musicChanged(state) }
        return music
    }

    private func musicChanged(_ state: MusicState) {
        shell.setNowPlaying(state.active)
        playbackChanged()
        guard state.active else {
            nowPlaying.release(.music)
            return
        }
        let info = musicInfo(state)
        if nowPlaying.owner == .music {
            nowPlaying.update(.music, info: info)
        } else if player == nil, livePlayer == nil, state.playing || nowPlaying.owner == nil {
            // Music played last: it owns Now Playing.
            nowPlaying.take(.music, info: info) { [weak self] event in self?.musicCommand(event) }
        }
    }

    private func musicCommand(_ event: RemoteEvent) {
        switch event {
        case .play: music.play()
        case .pause: music.pause()
        case .toggle: music.toggle()
        case .next: music.next()
        case .previous: music.previous()
        case .seek(let ms): music.seek(toMs: ms)
        case .skip(let seconds): music.seek(toMs: music.positionMs + seconds * 1000)
        }
        // The place moved without the state changing: the system's clock follows.
        nowPlaying.update(.music, info: musicInfo(music.state))
    }

    private func musicInfo(_ state: MusicState) -> NowPlayingInfo {
        NowPlayingInfo(title: state.track?.title ?? "", artist: state.artist, album: state.album,
                       durationMs: state.durationMs, elapsedMs: music.positionMs, rate: state.playing ? 1 : 0,
                       kind: .audio, artwork: picture(state.artPath), artworkPath: state.artPath)
    }

    /// Follows the video playing for the system's Now Playing, which it owns
    /// now. On Apple TV, AVKit's player shows Now Playing and hears the
    /// remote itself (tvOS can't turn that off): music only lets go of it.
    private func follow(_ controller: PlaybackController) {
        playbackChanged()
        #if os(tvOS)
        nowPlaying.release(.music)
        playerWatch = nil
        #else
        videoShown = nil
        let info = videoInfo(controller)
        videoShown = (info, playerSetup.clock.now())
        nowPlaying.take(.video, info: info) { [weak self, weak controller] event in
            guard let controller, self?.player === controller else { return }
            switch event {
            case .play: controller.play()
            case .pause: controller.pause()
            case .toggle:
                if case .playing = controller.state.phase { controller.pause() } else { controller.play() }
            case .skip(let seconds): controller.seek(byMs: seconds * 1000)
            case .seek(let ms): controller.seek(toMs: ms)
            case .next, .previous: break
            }
        }
        playerWatch = controller.objectWillChange.sink { [weak self, weak controller] _ in
            // `objectWillChange` comes before the change: read it once made.
            Task { @MainActor in
                guard let self, let controller, self.player === controller else { return }
                self.videoChanged(controller)
            }
        }
        #endif
    }

    /// Tells the system what changed of the video: everything but the
    /// time, which its clock moves by itself, unless the time jumped.
    private func videoChanged(_ controller: PlaybackController) {
        let info = videoInfo(controller)
        let now = playerSetup.clock.now()
        if let shown = videoShown {
            var same = info
            same.elapsedMs = shown.info.elapsedMs
            let expected = shown.info.elapsedMs + Int(Double(now - shown.at) * shown.info.rate)
            if same == shown.info, abs(info.elapsedMs - expected) < 1_500 { return }
        }
        videoShown = (info, now)
        nowPlaying.update(.video, info: info)
    }

    private func videoInfo(_ controller: PlaybackController) -> NowPlayingInfo {
        let state = controller.state
        let playing: Bool = switch state.phase {
        case .playing: true
        default: false
        }
        let summary = controller.item?.summary
        let path = summary.flatMap { $0.kind == .episode ? $0.thumbUrl ?? $0.posterUrl : $0.posterUrl ?? $0.thumbUrl }
        return NowPlayingInfo(title: state.title, album: state.subtitle,
                              durationMs: state.durationMs > 0 ? state.durationMs : nil, elapsedMs: state.positionMs,
                              rate: playing ? state.speed : 0, kind: .video, artwork: picture(path), artworkPath: path)
    }

    /// Follows the live player: the channel and what is on, as live, with no commands.
    private func follow(_ model: LivePlayerModel) {
        playbackChanged()
        nowPlaying.take(.live, info: liveInfo(model)) { _ in }
        liveWatch = model.$channel.sink { [weak self, weak model] _ in
            Task { @MainActor in
                guard let self, let model, self.livePlayer === model else { return }
                self.nowPlaying.update(.live, info: self.liveInfo(model))
            }
        }
    }

    private func liveInfo(_ model: LivePlayerModel) -> NowPlayingInfo {
        let channel = model.channel
        return NowPlayingInfo(title: channel?.numberedName ?? "Live TV", album: channel?.now?.title, rate: 1,
                              kind: .video, isLive: true, artwork: picture(channel?.logo), artworkPath: channel?.logo)
    }

    /// The picture at `path` for Now Playing (600 px at most), once read;
    /// `nil` meanwhile, and Now Playing is told again when it comes.
    private func picture(_ path: String?) -> CGImage? {
        guard let path else { return nil }
        if let artwork, artwork.path == path { return artwork.image }
        guard artworkTask == nil || artwork?.path != path else { return nil }
        artworkTask?.cancel()
        let images = images
        artworkTask = Task { [weak self] in
            guard let image = try? await images.image(path: path, widthPx: 600), !Task.isCancelled else { return }
            guard let self else { return }
            self.artworkTask = nil
            self.artwork = (path, image)
            self.showAgain()
        }
        return nil
    }

    /// What Now Playing shows, read again (a picture came).
    private func showAgain() {
        switch nowPlaying.owner {
        case .music?: nowPlaying.update(.music, info: musicInfo(music.state))
        case .video?:
            if let player { videoChanged(player) }
        case .live?:
            if let livePlayer { nowPlaying.update(.live, info: liveInfo(livePlayer)) }
        case nil: break
        }
    }

    /// A video or Live TV opened or closed: the audio session follows, and
    /// music, when it has a queue, owns Now Playing again once they close.
    private func playbackChanged() {
        #if os(iOS) || os(tvOS)
        if player != nil || livePlayer != nil {
            audio.use(.video)
        } else if music.active {
            audio.use(.music)
        } else {
            audio.use(nil)
        }
        #endif
        if player == nil, livePlayer == nil, nowPlaying.owner == nil, music.active {
            nowPlaying.take(.music, info: musicInfo(music.state)) { [weak self] event in self?.musicCommand(event) }
        }
    }

    // MARK: The server's events

    /// The app came to the foreground (`true`) or left it: the event
    /// stream runs only in the foreground. Back in front, everything shown
    /// is read again, since events were missed.
    public func sceneActive(_ active: Bool) {
        guard active != self.active else { return }
        self.active = active
        livePlayer?.sceneActive(active)
        if active {
            if offline { Task { await reconnect() } }
            // Conversions on the server are followed only while Pley is open.
            downloads.resume()
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
        guard active, user != nil, !offline, !eventsRunning else { return }
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
        if music.sessionId == id {
            music.stoppedByServer()
            return
        }
        // A live session's stop shows in its player, which asks why.
        guard let player, player.session?.id == id else { return }
        let text = player.stoppedByServer(message: message, byAdmin: byAdmin)
        shell.show(text)
        Task { await closePlayer(player) }
    }

    // MARK: Offline

    /// `load`'s answer from the server, saved as the snapshot of `key` for
    /// this server and profile. While the server can't be reached, the
    /// snapshot saved last instead (and the app goes offline); with none,
    /// what went wrong.
    public func cached<T: Codable & Sendable>(_ key: String,
                                              _ load: @escaping (PleyAPI) async throws -> T) async throws -> T {
        guard let user, let origin = server?.origin else { throw PleyError.signedOut }
        let profileId = user.id
        if !offline, let api = await session.api {
            do {
                let fresh = try await load(api)
                if let snapshots {
                    Task.detached(priority: .utility) {
                        guard let data = try? PleyJSON.encoder.encode(fresh) else { return }
                        await snapshots.write(origin: origin, profileId: profileId, key: key, data: data)
                    }
                }
                return fresh
            } catch where LateReports.unreachable(error) {
                wentOffline()
                guard let saved = await saved(T.self, key, origin: origin, profileId: profileId) else { throw error }
                return saved
            }
        }
        guard let saved = await saved(T.self, key, origin: origin, profileId: profileId) else {
            if await session.api == nil { throw PleyError.signedOut }
            throw PleyError.offline
        }
        return saved
    }

    private func saved<T: Decodable & Sendable>(_ type: T.Type, _ key: String, origin: String, profileId: Int) async -> T? {
        guard let snapshots, let data = await snapshots.read(origin: origin, profileId: profileId, key: key) else {
            return nil
        }
        return try? PleyJSON.decoder.decode(T.self, from: data)
    }

    /// The server couldn't be reached: the banner shows, and the app tries
    /// again after 2, 5, 15 and 30 s, then every minute, and whenever the
    /// network changes.
    public func wentOffline() {
        guard user != nil else { return }
        if !offline { offline = true }
        guard reconnectTask == nil else { return }
        let monitor = NWPathMonitor()
        // The handler runs on the monitor's queue: the change is noted on the main actor.
        monitor.pathUpdateHandler = { @Sendable [weak self] path in
            guard path.status == .satisfied else { return }
            Task { @MainActor in self?.pathChanged = true }
        }
        monitor.start(queue: DispatchQueue(label: "pley.reconnect"))
        paths = monitor
        pathChanged = false
        reconnectTask = Task { [weak self] in
            var tries = 0
            while !Task.isCancelled {
                let delay = tries < Self.reconnectDelays.count ? Self.reconnectDelays[tries] : Self.reconnectEvery
                tries += 1
                // Counted in seconds, so a change of network tries again within one.
                for _ in 0..<Int(delay) {
                    try? await Task.sleep(for: .seconds(1))
                    guard !Task.isCancelled, let self else { return }
                    if self.pathChanged { break }
                }
                guard !Task.isCancelled, let self else { return }
                self.pathChanged = false
                if await self.reconnect() { return }
            }
        }
    }

    /// Tries the server now (the banner's Try again, the app coming back to
    /// the front); `true` when it answered and the app is online again.
    @discardableResult
    public func reconnect() async -> Bool {
        guard offline, user != nil else { return !offline }
        let session = session
        let answered = await withTaskGroup(of: Bool.self) { group in
            group.addTask { (try? await session.reconnected()) != nil }
            group.addTask {
                try? await Task.sleep(for: Self.reconnectWait)
                return false
            }
            let first = await group.next() ?? false
            group.cancelAll()
            return first
        }
        guard answered, offline, let user else { return false }
        stopReconnecting()
        offline = false
        if let api = await session.api, let fresh = try? await api.me() { self.user = fresh }
        if let server, let name = await session.serverInfo?.name {
            self.server = ServerEntry(origin: server.origin, name: BonjourTXT.cleanName(name).isEmpty ? server.name
                                        : BonjourTXT.cleanName(name), id: server.id, saved: server.saved,
                                      found: server.found)
        }
        signedInNow(self.user ?? user)
        downloads.resume()
        homeVersion += 1
        note(ServerChange(home: true, libraries: true, watchlist: true, everything: true))
        return true
    }

    private func stopReconnecting() {
        reconnectTask?.cancel()
        reconnectTask = nil
        paths?.cancel()
        paths = nil
    }

    /// The downloads follow who is signed in.
    private func scopeDownloads() async {
        guard downloads.available else { return }
        guard let user, let api = await session.api, let origin = await session.origin else {
            return downloads.signedIn(nil)
        }
        let tokens = await session.tokens
        let caps = caps
        downloads.signedIn(DownloadScope(origin: origin, profileId: user.id, api: api, tokens: tokens,
                                         caps: { await caps.warmUp().report }))
    }

    // MARK: Top Shelf

    /// Whose Top Shelf row the app writes: the server and profile signed in.
    public var shelfOwner: String? {
        guard let user, let origin = server?.origin else { return nil }
        return ShelfSnapshot.owner(origin: origin, profileId: user.id)
    }

    /// Home's rows were read for `owner`: Continue Watching goes to the
    /// Top Shelf, unless the server or profile changed meanwhile.
    func homeRead(_ hubs: [Hub], owner: String) {
        guard let shelf, owner == shelfOwner, root == .signedIn else { return }
        shelf.show(hubs, owner: owner)
    }

    /// A Top Shelf link opened the app (`app.pley.apple:/shelf/…`). Only a
    /// link signed with the shelf's key, for the server and profile open,
    /// acts: `open` shows the title's page over Home, `play` plays it. At
    /// start and while "Who's watching?" shows, it waits, and acts once the
    /// same profile is chosen; another profile, or the servers, drop it.
    public func openShelfLink(_ url: URL) {
        guard let key = shelf?.linkKey, let link = ShelfLink.verified(url, key: key) else { return }
        shelfLink = link
        settleShelfLink()
    }

    private func settleShelfLink() {
        guard let link = shelfLink else { return }
        switch root {
        case .starting, .profiles:
            return
        case .servers:
            shelfLink = nil
        case .signedIn:
            shelfLink = nil
            guard link.owner == shelfOwner else { return }
            Task { await act(link) }
        }
    }

    private func act(_ link: ShelfLink) async {
        switch link.action {
        case .open:
            closeLivePlayer()
            await closePlayer()
            shell.select(.home)
            shell.popToRoot()
            shell.open(.item(link.itemId))
        case .play:
            await play(PlayRequest(itemId: link.itemId))
        }
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
            show(user, chooseProfile: true, atStart: true)
        case .switchTo(let id):
            if let switched = await switchAtStart(to: id) {
                show(switched, chooseProfile: false)
            } else {
                // One with a PIN, or one gone: the person chooses.
                show(user, chooseProfile: true, atStart: true)
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

    private func show(_ user: User, chooseProfile: Bool, atStart: Bool = false) {
        self.user = user
        homeVersion += 1
        signingIn = []
        profilesFromShell = false
        profilesAtStart = chooseProfile && atStart
        root = chooseProfile ? .profiles : .signedIn
        signedInNow(user)
    }

    /// Signed in: the device's caps are read ahead of the first Play, and
    /// reports that couldn't reach this server and profile before go now.
    private func signedInNow(_ user: User) {
        Task { await caps.warmUp() }
        if let origin = server?.origin {
            // Another server's or profile's row leaves the Top Shelf.
            shelf?.owner(is: ShelfSnapshot.owner(origin: origin, profileId: user.id))
            settings.setLastProfile(user.id, origin: origin)
            if let snapshots {
                Task {
                    await snapshots.activate(origin: origin, profileId: user.id)
                    if let data = try? PleyJSON.encoder.encode(user) {
                        await snapshots.write(origin: origin, profileId: user.id, key: SnapshotKeys.me, data: data)
                    }
                }
            }
        }
        Task { await scopeDownloads() }
        startEvents()
        loadLibraries()
        loadLiveTv()
        Task { await actions.loadWatchlist() }
        Task { [weak self] in
            guard let self, let api = await self.session.api, let origin = await self.session.origin else { return }
            await self.lateReports(origin: origin, profileId: user.id).send(using: api)
        }
    }

    /// Apple TV's library tabs, read whatever tab shows: the tab shown
    /// last may be a library's, or one (Search, Playlists) that reads none.
    /// iPad's and the Mac's sidebar reads them itself.
    private func loadLibraries() {
        guard tv, user != nil else { return }
        Task { [weak self] in
            guard let self, let libraries = try? await self.cached(SnapshotKeys.libraries, { try await $0.libraries() }),
                  self.user != nil else { return }
            self.shell.setLibraries(libraries)
        }
    }

    /// Whether Live TV shows, read at sign-in, at a profile change, when
    /// the lineup changes and when the stream connects again.
    private func loadLiveTv() {
        guard user != nil else { return }
        Task { [weak self] in
            guard let self else { return }
            let summary = try? await self.live.summary()
            guard self.user != nil else { return }
            // Unknown when it couldn't be read: the entry waits rather than going.
            if let summary { self.shell.setLiveTv(summary) }
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
        closeLivePlayer()
        music.stop()
        stopEvents()
        stopReconnecting()
        downloads.signedIn(nil)
        shelf?.clear()
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
