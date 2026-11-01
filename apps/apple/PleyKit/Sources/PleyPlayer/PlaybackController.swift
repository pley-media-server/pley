import AVFoundation
import Combine
import Foundation
import PleyCaps
import PleyCommon
import PleyModel
import PleyNetwork

/// What a playback needs from the app: the server's API, what the device
/// plays, this install's client id, the report queue and the late
/// reports (both the app's, outliving any player), the quality picked and
/// Automatic's ceiling as remembered, a clock and the engine.
@MainActor
public struct PlaybackEnvironment {
    public var api: PleyAPI
    public var caps: DeviceCaps
    public var clientId: String
    public var reporter: SessionReporter
    public var late: LateReports?
    /// The quality picked in the menu, per kind of network.
    public var quality: any QualityMemory
    /// Automatic's ceiling for this server, for when the server can't say.
    public var ceiling: any CeilingMemory
    public var clock: any PlayerClock
    public var engines: @MainActor () -> any PlayerEngine
    /// Asks for a new stream ahead of the player, while the one before plays on.
    public var warmer: any StreamWarmer
    public var networkTiming: NetworkStart.Timing

    public init(api: PleyAPI, caps: DeviceCaps, clientId: String, reporter: SessionReporter, late: LateReports?,
                quality: any QualityMemory = MemoryQuality(), ceiling: any CeilingMemory = MemoryCeiling(),
                clock: any PlayerClock = SystemPlayerClock(),
                engines: @escaping @MainActor () -> any PlayerEngine = { AVPlayerEngine() },
                warmer: (any StreamWarmer)? = nil, networkTiming: NetworkStart.Timing = NetworkStart.Timing()) {
        self.api = api
        self.caps = caps
        self.clientId = clientId
        self.reporter = reporter
        self.late = late
        self.quality = quality
        self.ceiling = ceiling
        self.clock = clock
        self.engines = engines
        self.warmer = warmer ?? APIStreamWarmer(client: api.client)
        self.networkTiming = networkTiming
    }
}

/// The playback of one item, a port of the Android app's
/// `SessionPlaybackController` (docs/APPLE.md, "Playing"):
///
/// - **Start.** The item's details and the device's caps, then
///   `POST /api/playback` (`PlaybackPlan`) with keyed URLs, and the engine
///   opens the keyed address the answer names.
/// - **Reports.** `ProgressSchedule`'s cadence, through the app's
///   `SessionReporter`; `stopped` then `DELETE` at the end, and before any
///   new session. A `410` answer means the session was stopped from
///   elsewhere: the player stops and says so. A `404` means it expired: the
///   next play or seek starts a new session where it was.
/// - **Recovery.** `Recovery`'s ladder for engine errors, 12 s stalls, a
///   remux without picture, and dropped frames (`StruggleMeter`).
/// - **Quality.** The quality picked plays as it is; Automatic
///   (docs/QUALITY.md) plays the original and lets `AutoQuality` step down
///   and back up, starting from what the server knows of the network
///   (`NetworkStart`). The pick is remembered per kind of network.
/// - **Switches.** A quality picked, or Automatic's step, starts the next
///   session beside the one playing (`keep_previous`): its stream is asked
///   for ahead (`StreamWarmer`, 20 s at most) while the one before plays
///   on, then the player moves over where it got to and the one before is
///   deleted.
/// - **Background.** iPhone, iPad and Apple TV pause and report when the
///   app goes to the background; back after more than 60 s, the session is
///   checked and started again, paused, if the server forgot it.
///
/// One session plays at a time; a switch has a second one ready for a moment.
@MainActor
public final class PlaybackController: ObservableObject {
    public static let tickMs: Int64 = 500
    /// How long playback may stand still, meant to play, before it counts as stalled.
    public static let stallMs: Int64 = 12_000
    /// How long a remux's time may run without a picture.
    public static let noPictureMs = 5_000
    /// How long a player away in the background may be before its session is checked.
    public static let backgroundCheckMs: Int64 = 60_000
    /// How long closing and moving to a new session wait for the reports before them.
    public static let flushMs: Int64 = 2_000
    static let starvingMs = 1_000
    static let seekEndMarginMs = 250
    /// How often dropped frames are sampled while it plays.
    static let struggleSampleMs: Int64 = 2_000
    /// The longest a switch's next stream gets ready before the player moves over all the same.
    public static let prepareMaxMs: Int64 = 20_000
    /// The next stream is asked for this far past where the switch began: where the player will be.
    static let prepareLeadMs = 4_000
    /// The answer of a server with no transcode free for a second session.
    static let noTranscodeFree = 503

    @Published public private(set) var state = PlayerState()

    /// The engine's player, for the views; `nil` until started.
    public var player: AVPlayer? { (engine as? AVPlayerEngine)?.player }
    /// The session playing; `nil` between sessions.
    public private(set) var session: ServerPlaybackSession?
    /// The item and the file playing, once known.
    public private(set) var item: ItemDetail?
    public private(set) var file: MediaFile?

    private let request: PlayRequest
    private let env: PlaybackEnvironment
    private var engine: (any PlayerEngine)?
    private var caps: ClientCaps?
    private var recovery = Recovery()
    private var schedule = ProgressSchedule()
    private var struggle = StruggleMeter()
    private var lastStruggleSample: Int64 = 0
    private let network: NetworkStart
    private let auto: AutoQuality
    /// The quality chosen: the menu's or the remembered one, Automatic included.
    private var picked = Qualities.automatic
    /// The quality playing: the original or a transcode (Automatic's step, or the one picked).
    private var quality = Qualities.original
    /// "Quality lowered to …" is said once the next session loads.
    private var autoNoticePending = false
    /// A switch's next session is getting ready: this one's stalls and drops no longer count.
    private var preparing: NSObject?
    /// The player's size on screen, in pixels: Automatic plays no stream size bigger.
    private var viewPixels: (width: Int, height: Int) = (0, 0)

    private var startTask: Task<Void, Never>?
    private var sessionTask: Task<Void, Never>?
    private var tickTask: Task<Void, Never>?
    private var eventTask: Task<Void, Never>?

    private var wantPlay = true
    private var mediaShown = false
    private var ended = false
    private var closed = false
    private var stopped = false
    /// The server answered `404` to a report: the next play or seek starts a new session.
    private var expired = false
    private var lastPositionMs = 0
    private var lastAdvanceAt: Int64 = 0
    private var lastAdvancePos = -1
    private var noPictureFrom: Int?
    private var pictureSeen = false
    private var backgroundedAt: Int64?
    /// Counts restarts, so only the latest clears `sessionTask`.
    private var restarts = 0

    public init(request: PlayRequest, environment: PlaybackEnvironment) {
        self.request = request
        env = environment
        let network = NetworkStart(api: environment.api, clientId: environment.clientId,
                                   reporter: environment.reporter, clock: environment.clock,
                                   timing: environment.networkTiming)
        self.network = network
        auto = AutoQuality(memory: environment.ceiling, onCeiling: { network.report($0) })
    }

    /// How many speed tests this playback ran (for the end-to-end check).
    public var speedTests: Int { network.speedTests }

    // MARK: Commands

    /// Starts preparing; called once, when someone presses play.
    public func start() {
        guard startTask == nil, !closed else { return }
        let engine = env.engines()
        self.engine = engine
        let events = engine.events
        eventTask = Task { [weak self] in
            for await event in events { self?.heard(event) }
        }
        startTask = Task { [weak self] in await self?.prepareFirst() }
        let clock = env.clock
        tickTask = Task { [weak self] in
            while !Task.isCancelled {
                do { try await clock.sleep(ms: Self.tickMs) } catch { return }
                self?.tick()
            }
        }
    }

    public func play() {
        guard !closed, !stopped else { return }
        wantPlay = true
        if let due = schedule.wantsToPlay(true, at: env.clock.now()) { report(due) }
        if ended || expired || session == nil && item != nil && sessionTask == nil {
            restart(at: ended ? 0 : lastPositionMs)
            return
        }
        engine?.play()
        lastAdvanceAt = env.clock.now()
    }

    public func pause() {
        guard !closed else { return }
        wantPlay = false
        struggle.reset()
        engine?.pause()
        if let due = schedule.wantsToPlay(false, at: env.clock.now()) { report(due) }
        if mediaShown && !ended { setPhase(.paused) }
    }

    public func seek(toMs positionMs: Int) {
        guard !closed, !stopped else { return }
        let duration = state.durationMs
        let target = duration > 0
            ? min(max(positionMs, 0), max(duration - Self.seekEndMarginMs, 0))
            : max(positionMs, 0)
        lastPositionMs = target
        state.positionMs = target
        if ended || expired || session == nil {
            restart(at: target)
            return
        }
        engine?.seek(toMs: target)
        schedule.sought(at: env.clock.now())
        recovery.onJump()
        struggle.reset()
        auto.jumped()
        noPictureFrom = nil
        lastAdvanceAt = env.clock.now()
    }

    public func seek(byMs delta: Int) {
        seek(toMs: livePosition() + delta)
    }

    /// A choice of the Quality menu (`state.qualities`), remembered for
    /// this kind of network. Another quality than what plays starts a new
    /// session beside it; Original and Automatic at the original play on.
    /// Automatic picked again starts afresh: what was known of the network
    /// is forgotten.
    public func selectQuality(_ id: String) {
        guard !closed, !stopped else { return }
        let wanted = Qualities.byIdOrLabel(id)
        guard let choice = Qualities.forFile(file).first(where: { $0.id == wanted.id }), choice.id != picked.id else {
            return
        }
        let before = quality
        picked = choice
        if choice.automatic { auto.forgetCeiling() }
        quality = playedFor(choice)
        autoNoticePending = false
        env.quality.remember(choice.id)
        let same = quality.id == before.id
        if same {
            if automatic && recovery.fallbackReason == .notSmooth {
                auto.deviceFellBehindAtOriginal(nowMs: env.clock.now(),
                                                convertedKbps: serverQuality(session?.info)?.maxBitrateKbps)
            }
            if let info = session?.info { syncAutoOriginal(info) }
        } else {
            recovery.qualityChanged()
        }
        publishChoices()
        applyMaximumResolution()
        if item != nil && !same { restart(at: livePosition(), alongside: true) }
    }

    /// The player's size on screen in pixels: while Automatic plays, a
    /// stream in several sizes plays none bigger (as on the web).
    public func setViewPixelSize(width: Int, height: Int) {
        guard (width, height) != viewPixels else { return }
        viewPixels = (width, height)
        applyMaximumResolution()
    }

    /// The notice was shown: it goes.
    public func dismissNotice() {
        state.notice = nil
    }

    /// Try again after an error: every recovery budget again, from where it was.
    public func retry() {
        guard !closed, !stopped else { return }
        recovery.retry()
        state.phase = .preparing
        if item == nil {
            startTask?.cancel()
            startTask = Task { [weak self] in await self?.prepareFirst() }
        } else {
            restart(at: lastPositionMs)
        }
    }

    /// iPhone, iPad and Apple TV: the app went to the background. It
    /// pauses and reports; the session is kept. Returns once the report
    /// went out (the app holds a background task until then).
    public func appWillBackground() async {
        guard !closed else { return }
        backgroundedAt = env.clock.now()
        pause()
        await env.reporter.flush(timeoutMs: Self.flushMs, clock: env.clock)
    }

    /// Back from the background: after more than 60 s, the session is
    /// checked, and a forgotten one starts again where it was, paused.
    public func appDidBecomeActive() async {
        guard let since = backgroundedAt, !closed, !stopped else { return }
        backgroundedAt = nil
        guard env.clock.now() - since > Self.backgroundCheckMs, let session else { return }
        do {
            _ = try await env.api.playbackStatus(session.id)
        } catch PleyError.sessionStopped(let message) {
            stopByServer(message)
        } catch PleyError.api(404, _, _) {
            wantPlay = false
            expired = true
            restart(at: lastPositionMs)
        } catch {
            // Unreachable for now: the next report or play says more.
        }
    }

    /// Ends the playback: `stopped` at the position, then `DELETE`, and
    /// waits for the reports (2 s at most).
    public func close() async {
        guard !closed else { return }
        let position = livePosition()
        closed = true
        startTask?.cancel()
        sessionTask?.cancel()
        tickTask?.cancel()
        eventTask?.cancel()
        endSession(at: position)
        engine?.close()
        await env.reporter.flush(timeoutMs: Self.flushMs, clock: env.clock)
    }

    // MARK: Sessions

    private func prepareFirst() async {
        // What the server knows of the network, asked at once when the quality remembered is Automatic.
        let remembered = env.quality.remembered()
        let network = network
        let lookup: Task<NetworkStart.Lookup, Never>? = Qualities.choose(nil, remembered: remembered).automatic
            ? Task { await network.lookUp() } : nil
        defer { lookup?.cancel() }
        do {
            async let detail = env.api.item(request.itemId)
            let snapshot = await env.caps.warmUp()
            let found = try await detail
            guard !closed else { return }
            caps = snapshot.report
            // A call to this server and profile answered: what couldn't be reported before goes now.
            if let late = env.late {
                let api = env.api
                Task { await late.send(using: api) }
            }
            item = found
            guard let chosen = PlaybackPlan.file(found, requested: request.fileId, caps: caps) else {
                fail(PlayerTexts.nothingToPlay, canRetry: false)
                return
            }
            file = chosen
            describe(found, chosen)
            picked = Qualities.choose(chosen, remembered: remembered)
            var start = StartCeiling.device
            if picked.automatic, let lookup { start = await network.resolve(await lookup.value) }
            guard !closed else { return }
            quality = playedFor(picked, start: start)
            publishChoices()
            try await begin(at: PlaybackPlan.startMs(found, request))
        } catch is CancellationError {
        } catch {
            failWith(error)
        }
    }

    private func describe(_ item: ItemDetail, _ file: MediaFile) {
        let summary = item.summary
        if item.kind == .episode {
            state.title = summary.grandparentTitle ?? summary.title
            let label = PlayerTexts.episodeLabel(season: summary.parentIndex, episode: summary.index)
            state.subtitle = [label, summary.title].filter { !$0.isEmpty }.joined(separator: " · ")
        } else {
            state.title = summary.year.map { "\(summary.title) (\($0))" } ?? summary.title
            state.subtitle = nil
        }
        state.durationMs = file.durationMs ?? item.durationMs ?? 0
        lastPositionMs = PlaybackPlan.startMs(item, request)
        state.positionMs = lastPositionMs
    }

    /// A new session at `atMs`, ending the one before; the previous start
    /// is cancelled (a new choice wins). `alongside`: a quality change or
    /// Automatic's step while it plays, made ready beside the session
    /// playing (`beginBeside`). Recoveries from errors and stalls start at
    /// once: the stream playing is what failed.
    private func restart(at atMs: Int, alongside: Bool = false) {
        guard !stopped, !closed else { return }
        sessionTask?.cancel()
        restarts += 1
        let run = restarts
        sessionTask = Task { [weak self] in
            guard let self else { return }
            do {
                try await self.begin(at: atMs, alongside: alongside)
            } catch is CancellationError {
            } catch {
                self.failWith(error)
            }
            if self.restarts == run { self.sessionTask = nil }
        }
    }

    private func begin(at atMs: Int, alongside: Bool = false) async throws {
        guard item != nil, file != nil else { return }
        // A change while the media shows: the session playing plays on while the next one gets ready.
        if alongside, let playing = session, mediaShown, !ended, !state.isFailed {
            return try await beginBeside(playing, at: atMs)
        }
        // The session before ends first ("stopped", then DELETE), unless the server is slow to say so.
        if endSession(at: atMs) { await env.reporter.flush(timeoutMs: Self.flushMs, clock: env.clock) }
        try Task.checkCancellation()
        reset(at: atMs)
        let (started, url) = try await startSession(at: atMs, keepPrevious: false)
        load(started, url: url, at: atMs)
    }

    /// A change while it plays (a quality, Automatic's step): the next
    /// session starts beside `playing` (`keep_previous`), which plays on
    /// while the next one's stream is asked for as AVPlayer will ask for it
    /// (`StreamWarmer`), so the transcoder makes its first segment; then the
    /// player moves over from where `playing` got to, and `playing` is
    /// deleted. With no transcode free for two at once (`503`) it moves as
    /// any new session does: `playing` ends first.
    private func beginBeside(_ playing: ServerPlaybackSession, at atMs: Int) async throws {
        let token = NSObject()
        preparing = token
        defer { if preparing === token { preparing = nil } }
        // The stall watch starts over: the next session is on its way.
        lastAdvanceAt = env.clock.now()
        let ready: (ServerPlaybackSession, URL)
        do {
            ready = try await startSession(at: atMs, keepPrevious: true)
        } catch PleyError.api(Self.noTranscodeFree, _, _) {
            let at = livePosition()
            if endSession(at: at) { await env.reporter.flush(timeoutMs: Self.flushMs, clock: env.clock) }
            try Task.checkCancellation()
            reset(at: at)
            let (started, url) = try await startSession(at: at, keepPrevious: false)
            return load(started, url: url, at: at)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            // As when the session before ended first: it ends, and the screen says what failed.
            lastPositionMs = livePosition()
            endSession(at: lastPositionMs)
            throw error
        }
        let (started, url) = ready
        if started.info.hls {
            let (warmer, clock, start) = (env.warmer, env.clock, atMs + Self.prepareLeadMs)
            await withTaskGroup(of: Void.self) { group in
                group.addTask { try? await warmer.warm(url, startMs: start) }
                group.addTask { try? await clock.sleep(ms: Self.prepareMaxMs) }
                await group.next()
                group.cancelAll()
            }
        }
        // Cancelled for a newer choice, or the session playing ended meanwhile (its end, the server's stop).
        guard !Task.isCancelled, session === playing, !closed, !stopped else {
            started.discard()
            if Task.isCancelled { throw CancellationError() }
            return
        }
        // From where the one before got to while this one got ready; it goes without a "stopped",
        // which would only move the resume point back.
        let at = livePosition()
        session = nil
        playing.discard()
        reset(at: at)
        load(started, url: url, at: at)
    }

    /// `POST /api/playback` for the choices of the moment at `atMs`, asked
    /// again with the track indexes when the answer needs them; and the
    /// keyed address the player opens.
    private func startSession(at atMs: Int, keepPrevious: Bool) async throws -> (ServerPlaybackSession, URL) {
        guard let item, let file else { throw CancellationError() }
        // A lower quality is always a transcode; the original keeps a fallback's mode.
        let mode: PlaybackMode = quality.original ? recovery.fallbackMode ?? .auto : .transcode
        let decision = caps.map { CapsPolicy.decide(file, $0) }
        var choices = PlaybackPlan.Choices(file: file, startMs: atMs, maxBitrate: quality.maxBitrateKbps,
                                           maxHeight: quality.maxHeight, mode: mode,
                                           streaming: PlaybackPlan.streams(mode: mode, decision: decision, file: file,
                                                                           audio: nil, caps: caps))
        var sent = PlaybackPlan.request(itemId: item.id, choices, clientId: env.clientId, caps: caps,
                                        keepPrevious: keepPrevious)
        var started = try await ServerPlaybackSession.start(api: env.api, request: sent, reporter: env.reporter,
                                                            late: env.late)
        if PlaybackPlan.needsIndexes(started.info, sent: sent, choices) {
            started.discard()
            choices.streaming = true
            sent = PlaybackPlan.request(itemId: item.id, choices, clientId: env.clientId, caps: caps,
                                        keepPrevious: keepPrevious)
            started = try await ServerPlaybackSession.start(api: env.api, request: sent, reporter: env.reporter,
                                                            late: env.late)
        }
        guard !closed, !Task.isCancelled else {
            started.discard()
            throw CancellationError()
        }
        do {
            return (started, try await env.api.mediaURL(started.info.url))
        } catch {
            started.discard()
            throw error
        }
    }

    private func load(_ started: ServerPlaybackSession, url: URL, at atMs: Int) {
        session = started
        expired = false
        started.onAnswer = { [weak self, weak started] answer in
            guard let self, let started, self.session === started else { return }
            switch answer {
            case .stopped(let message): self.stopByServer(message)
            case .expired: self.expired = true
            }
        }
        let info = started.info
        state.method = info.method
        state.audioConverted = info.audioConverted
        state.reasons = info.reasons
        state.videoRange = info.videoRange
        state.videoOut = nil
        if info.durationMs > 0 { state.durationMs = info.durationMs }
        auto.jumped()
        syncAutoOriginal(info)
        if autoNoticePending {
            autoNoticePending = false
            let playing = playingPreset(info)
            if !playing.original { state.notice = PlayerTexts.lowered(to: label(playing)) }
        }
        publishChoices()
        applyMaximumResolution()
        engine?.load(url: url, startMs: atMs, play: wantPlay)
    }

    /// A new session's start at `atMs`: nothing shown yet, nothing ended.
    private func reset(at atMs: Int) {
        lastPositionMs = atMs
        mediaShown = false
        ended = false
        noPictureFrom = nil
        pictureSeen = false
        struggle.reset()
        auto.jumped()
        schedule.restart()
        recovery.onJump()
        lastAdvanceAt = env.clock.now()
        lastAdvancePos = -1
        state.phase = .preparing
        state.positionMs = atMs
    }

    /// Reports `stopped` and deletes the session playing; false when none plays.
    @discardableResult
    private func endSession(at positionMs: Int) -> Bool {
        guard let session else { return false }
        self.session = nil
        session.end(positionMs: positionMs)
        return true
    }

    /// The server ended the session: the engine stops, only `stopped` is
    /// sent (for the resume point), and the screen says why without Try again.
    private func stopByServer(_ message: String?) {
        guard !closed, !stopped else { return }
        stopped = true
        let position = livePosition()
        lastPositionMs = position
        session?.stoppedByServer(positionMs: position)
        session = nil
        sessionTask?.cancel()
        startTask?.cancel()
        engine?.pause()
        state.stoppedByServer = true
        state.phase = .failed(message: PlayerTexts.stopped(message), canRetry: false)
    }

    // MARK: The engine

    private func heard(_ event: EngineEvent) {
        guard !closed else { return }
        switch event {
        // The system's controls act on AVPlayer itself: the controller follows them.
        case .playRequested: return play()
        case .pauseRequested: return pause()
        case .jumped: return jumped()
        default: break
        }
        guard let session else { return }
        switch event {
        case .ready:
            mediaShown = true
            lastAdvanceAt = env.clock.now()
            if let state = schedule.firstFrame(at: env.clock.now()) { session.progress(positionMs: lastPositionMs, state: state) }
            setPhase(wantPlay ? .playing : .paused)
            if session.info.method == .transcode { describeConversion(session) }
        case .ended:
            onEnded()
        case .failed(let error):
            onError(error, in: session)
        case .playRequested, .pauseRequested, .jumped:
            break
        }
    }

    /// The time jumped (the system's scrubber, or a seek): reported a
    /// second later, as a seek is. A playback that ended, or whose session
    /// expired, starts a new session where the scrubber left it.
    private func jumped() {
        guard mediaShown, !stopped, let engine else { return }
        let now = env.clock.now()
        if ended || expired || session == nil {
            guard sessionTask == nil else { return }
            restart(at: engine.sample().positionMs)
            return
        }
        schedule.sought(at: now)
        recovery.onJump()
        struggle.reset()
        auto.jumped()
        noPictureFrom = nil
        lastAdvanceAt = now
    }

    /// What the server's conversion makes of the video (Details), once it runs.
    private func describeConversion(_ session: ServerPlaybackSession) {
        let api = env.api
        Task { [weak self] in
            let status = try? await api.playbackStatus(session.id)
            guard let self, self.session === session else { return }
            self.state.videoOut = status?.transcode?.videoOut
        }
    }

    private func onError(_ error: EngineError, in session: ServerPlaybackSession) {
        if case .http(410, _) = error {
            // A stopped session answers 410 at its media too: asked, so a stop isn't taken for an expiry.
            Task { [weak self] in
                guard let self else { return }
                do {
                    _ = try await self.env.api.playbackStatus(session.id)
                } catch PleyError.sessionStopped(let message) {
                    if self.session === session { self.stopByServer(message) }
                    return
                } catch {}
                if self.session === session { self.decide(error, session) }
            }
            return
        }
        decide(error, session)
    }

    private func decide(_ error: EngineError, _ session: ServerPlaybackSession) {
        act(recovery.decide(error, method: session.info.method, videoCodec: file?.video?.codec), starving: false)
    }

    /// Does what the recovery ladder says; `alongside`: a new session is
    /// made ready beside the one playing, which plays on.
    private func act(_ action: Recovery.Action, starving: Bool, alongside: Bool = false) {
        switch action {
        case .newSession:
            restart(at: livePosition(), alongside: alongside)
        case .checkServer:
            guard let session else { return }
            Task { [weak self] in
                guard let self else { return }
                var speed: Double?
                do {
                    speed = try await self.env.api.playbackStatus(session.id).transcode?.speed
                } catch PleyError.sessionStopped(let message) {
                    if self.session === session { self.stopByServer(message) }
                    return
                } catch {}
                guard self.session === session else { return }
                self.act(self.recovery.afterServerCheck(speed: speed, starving: starving), starving: starving)
            }
        case .suggestLowerQuality:
            // Automatic goes lower by itself; otherwise the person is told a lower quality would help.
            if automatic, let step = auto.serverTooSlow(nowMs: env.clock.now()) {
                applyAuto(step)
            } else {
                state.notice = PlayerTexts.slowServer
            }
        case .fail(let failure, let detail):
            if let detail { Log.player.error("playback failed: \(detail, privacy: .public)") }
            fail(PlayerTexts.failure(failure, detail: detail), canRetry: failure != .unplayable)
        }
    }

    private func onEnded() {
        guard !ended else { return }
        ended = true
        let duration = session.map { $0.info.durationMs > 0 ? $0.info.durationMs : 0 } ?? 0
        let end = duration > 0 ? duration : max(engine?.sample().durationMs ?? 0, lastPositionMs)
        lastPositionMs = end
        endSession(at: end)
        state.positionMs = end
        setPhase(.ended)
    }

    // MARK: The clock

    /// Every 500 ms: the position, the reports due, stalls and a missing picture.
    func tick() {
        guard let engine, let session, !closed, !stopped else { return }
        let now = env.clock.now()
        let sample = engine.sample()
        if mediaShown && !ended {
            lastPositionMs = sample.positionMs
            if sample.playing { recovery.onPosition(sample.positionMs) }
            state.positionMs = sample.positionMs
            state.bufferedMs = sample.bufferedMs
            if session.info.durationMs <= 0, sample.durationMs > 0 { state.durationMs = sample.durationMs }
            if case .failed = state.phase {} else {
                setPhase(sample.playing ? .playing : wantPlay ? .buffering : .paused)
            }
            if let due = schedule.due(at: now) { session.progress(positionMs: sample.positionMs, state: due) }
        }
        if case .failed = state.phase { return }
        guard sessionTask == nil, preparing == nil else {
            // The next session is on its way: this one's stalls and drops no longer count.
            lastAdvanceAt = now
            return
        }
        // A stall: meant to play, not moving.
        let position = mediaShown ? sample.positionMs : -1
        if !wantPlay || ended || position != lastAdvancePos {
            lastAdvanceAt = now
            lastAdvancePos = position
        } else if now - lastAdvanceAt >= Self.stallMs {
            lastAdvanceAt = now
            let starving = sample.waiting && sample.bufferedMs - sample.positionMs < Self.starvingMs
            act(recovery.onStall(method: session.info.method, starving: starving), starving: starving)
            return
        }
        checkPicture(sample, session)
        guard sessionTask == nil else { return }
        let method = session.info.method
        // Dropped frames, sampled every 2 s of playback.
        let moving = mediaShown && wantPlay && !ended && sample.playing
        if moving, now - lastStruggleSample >= Self.struggleSampleMs {
            lastStruggleSample = now
            let frameRate = file?.video?.frameRate.flatMap { $0 > 0 ? $0 : nil } ?? 30
            if struggle.sample(atMs: now, FrameCounts(sample, frameRate: frameRate)) {
                struggle.reset()
                onStruggling(method, now)
                return
            }
        }
        // A stream in several sizes: AVPlayer moves between them as the
        // connection allows, without a new session (stalls and dropped
        // frames still lower it); Automatic only steps back up from it.
        guard automatic, !state.isFailed else { return }
        let step = auto.sample(AutoSample(
            atMs: now, watching: mediaShown && wantPlay && !ended, buffering: sample.waiting,
            positionMs: sample.positionMs, bufferedMs: sample.bufferedMs, durationMs: state.durationMs,
            bandwidthKbps: sample.bandwidthKbps, sizes: session.info.adaptive))
        if let step { applyAuto(step) }
    }

    // MARK: Quality

    /// The decoder drops frames. The original switches to the server's
    /// quality, once, by the recovery ladder (in Automatic, that is its
    /// step down); a transcode in Automatic steps down to fewer lines.
    private func onStruggling(_ method: PlaybackMethod, _ now: Int64) {
        if let action = recovery.onStruggling(method: method) {
            Log.player.notice("frames dropped: asking for a conversion")
            if automatic {
                auto.deviceFellBehindAtOriginal(nowMs: now, convertedKbps: nil)
                autoNoticePending = true
            }
            act(action, starving: false, alongside: true)
        } else if automatic, let step = auto.deviceFellBehind(nowMs: now) {
            applyAuto(step)
        }
    }

    /// Plays what Automatic asked for, from where it is: a new session at
    /// that quality. Going down says so once, as the new session starts;
    /// going back up is silent.
    private func applyAuto(_ step: AutoStep) {
        guard !closed, !stopped, item != nil else { return }
        auto.applied(step, nowMs: env.clock.now())
        quality = step.to
        autoNoticePending = !step.up
        Log.player.notice("Automatic: \(step.up ? "up" : "down", privacy: .public) to \(step.to.id, privacy: .public)")
        publishChoices()
        restart(at: livePosition(), alongside: true)
    }

    /// Automatic is chosen and watches.
    private var automatic: Bool { picked.automatic && auto.active }

    /// What `choice` plays: Automatic starts afresh for the file (at the
    /// original, or at most at `start`'s ceiling); every other quality
    /// plays as itself.
    private func playedFor(_ choice: QualityPreset, start: StartCeiling = .device) -> QualityPreset {
        guard choice.automatic, let file else {
            auto.stop()
            return choice.automatic ? Qualities.original : choice
        }
        let described = AutoFile(offered: Qualities.forFile(file), originalKbps: Qualities.fileKbps(file),
                                 lines: Qualities.linesClassOf(file), cap: Qualities.closest(file))
        return auto.begin(described, nowMs: env.clock.now(), start: start)
    }

    /// Tells Automatic what the original plays at in `info`: the file, or the server's conversion of it.
    private func syncAutoOriginal(_ info: PlaybackInfo) {
        guard automatic, quality.original else { return }
        let converted = info.method == .transcode
        auto.originalPlaysAt(converted ? serverQuality(info)?.maxBitrateKbps : Qualities.fileKbps(file),
                             converted: converted)
    }

    /// The quality the server converts the original at in `info`; `nil` when it doesn't, or plays a quality asked for.
    private func serverQuality(_ info: PlaybackInfo?) -> QualityPreset? {
        guard quality.original, let id = info?.autoQuality else { return nil }
        let preset = Qualities.byId(id)
        return preset.original || preset.automatic ? nil : preset
    }

    /// What plays in `info`: the quality, or the original's conversion at the server's quality.
    private func playingPreset(_ info: PlaybackInfo?) -> QualityPreset { serverQuality(info) ?? quality }

    /// `preset`'s name for the file playing: one taller than the file goes by the file's lines ("8 Mbps 720p").
    private func label(_ preset: QualityPreset) -> String {
        Qualities.label(preset, fileLines: Qualities.linesClassOf(file))
    }

    /// Why it plays lower when the player chose it: Automatic's lowering, else a fallback's reason.
    private func noteNow() -> String? {
        if automatic, let lowered = auto.loweredFor { return PlayerTexts.lowered(lowered) }
        return recovery.fallbackReason.map(PlayerTexts.fallback)
    }

    /// The Quality menu as chosen now, and what plays.
    private func publishChoices() {
        let converted = serverQuality(session?.info)
        let playing = label(converted ?? quality)
        let note = noteNow()
        let fileLine = file.map { f in
            [Words.resolution(width: f.width ?? f.video?.width, height: f.height ?? f.video?.height) ?? "",
             Words.bitrate(f.bitrate)].filter { !$0.isEmpty }.joined(separator: " · ")
        }
        let lowered = automatic && (converted != nil || !quality.original)
        state.qualities = Qualities.forFile(file).map { preset in
            let detail: String? = if preset.automatic {
                lowered ? [PlayerTexts.now(playing), note].compactMap { $0 }.joined(separator: " · ")
                    : PlayerTexts.automaticHint
            } else if preset.original {
                fileLine.flatMap { $0.isEmpty ? nil : $0 }
            } else {
                nil
            }
            let name = preset.automatic && picked.automatic ? PlayerTexts.automatic(playing) : label(preset)
            return MenuChoice(id: preset.id, label: name, detail: detail, selected: preset.id == picked.id)
        }
        state.quality = automatic ? PlayerTexts.automatic(playing) : label(quality)
        state.qualityNote = note
    }

    /// AVPlayer's largest stream size: the player's for Automatic, none for a quality picked.
    private func applyMaximumResolution() {
        let (width, height) = automatic ? viewPixels : (0, 0)
        engine?.setMaximumResolution(width: width, height: height)
    }

    /// A remux whose time runs on with no picture for 5 s plays a video
    /// AVPlayer can't show (HEVC copied into MPEG-TS): a transcode, once.
    private func checkPicture(_ sample: EngineSample, _ session: ServerPlaybackSession) {
        guard mediaShown, !pictureSeen, session.info.method == .remux, file?.video != nil, sample.playing else { return }
        let picture = sample.videoFrames.map { $0 > 0 } ?? (sample.presentationWidth > 0)
        if picture {
            pictureSeen = true
            noPictureFrom = nil
            return
        }
        guard let from = noPictureFrom else {
            noPictureFrom = sample.positionMs
            return
        }
        if sample.positionMs - from >= Self.noPictureMs, let action = recovery.onNoVideo(method: .remux) {
            Log.player.notice("no picture from the remux: asking for a conversion")
            act(action, starving: false)
        }
    }

    private func report(_ state: ProgressState) {
        session?.progress(positionMs: livePosition(), state: state)
    }

    private func livePosition() -> Int {
        guard mediaShown, session != nil, !ended, let engine else { return lastPositionMs }
        return engine.sample().positionMs
    }

    private func setPhase(_ phase: PlayerState.Phase) {
        if case .failed = state.phase { return }
        if state.phase != phase { state.phase = phase }
    }

    private func fail(_ message: String, canRetry: Bool) {
        state.phase = .failed(message: message, canRetry: canRetry)
    }

    private func failWith(_ error: any Error) {
        if case .sessionStopped(let message)? = error as? PleyError {
            stopByServer(message)
            return
        }
        let words = (error as? any LocalizedError)?.errorDescription ?? "something went wrong"
        let canRetry: Bool = switch error as? PleyError {
        case .api(let status, _, _): !(status == 403 || status == 404)
        case .signedOut, .insufficientScope: false
        default: true
        }
        fail(Self.sentence(words), canRetry: canRetry)
    }

    /// The app's lower-case texts as a sentence.
    static func sentence(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let first = trimmed.first else { return trimmed }
        let capital = first.uppercased() + trimmed.dropFirst()
        return ".…!?".contains(capital.last!) ? capital : capital + "."
    }
}

/// One playback, told apart from another by identity (the player screens are presented with it).
extension PlaybackController: Identifiable {}
