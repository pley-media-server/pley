import AVFoundation
import Combine
import Foundation
import PleyCaps
import PleyCommon
import PleyModel
import PleyNetwork

/// What a playback needs from the app: the server's API, what the device
/// plays, this install's client id, the report queue and the late
/// reports (both the app's, outliving any player), a clock and the engine.
@MainActor
public struct PlaybackEnvironment {
    public var api: PleyAPI
    public var caps: DeviceCaps
    public var clientId: String
    public var reporter: SessionReporter
    public var late: LateReports?
    public var clock: any PlayerClock
    public var engines: @MainActor () -> any PlayerEngine

    public init(api: PleyAPI, caps: DeviceCaps, clientId: String, reporter: SessionReporter, late: LateReports?,
                clock: any PlayerClock = SystemPlayerClock(),
                engines: @escaping @MainActor () -> any PlayerEngine = { AVPlayerEngine() }) {
        self.api = api
        self.caps = caps
        self.clientId = clientId
        self.reporter = reporter
        self.late = late
        self.clock = clock
        self.engines = engines
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
/// - **Recovery.** `Recovery`'s ladder for engine errors, 12 s stalls and a
///   remux without picture.
/// - **Background.** iPhone, iPad and Apple TV pause and report when the
///   app goes to the background; back after more than 60 s, the session is
///   checked and started again, paused, if the server forgot it.
///
/// Original quality only (`mode: auto`); one session at a time.
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
    }

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
        noPictureFrom = nil
        lastAdvanceAt = env.clock.now()
    }

    public func seek(byMs delta: Int) {
        seek(toMs: livePosition() + delta)
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

    /// A new session at `atMs`, ending the one before; the previous start is cancelled.
    private func restart(at atMs: Int) {
        guard !stopped, !closed else { return }
        sessionTask?.cancel()
        restarts += 1
        let run = restarts
        sessionTask = Task { [weak self] in
            guard let self else { return }
            do {
                try await self.begin(at: atMs)
            } catch is CancellationError {
            } catch {
                self.failWith(error)
            }
            if self.restarts == run { self.sessionTask = nil }
        }
    }

    private func begin(at atMs: Int) async throws {
        guard let item, let file else { return }
        // The session before ends first ("stopped", then DELETE), unless the server is slow to say so.
        if endSession(at: atMs) { await env.reporter.flush(timeoutMs: Self.flushMs, clock: env.clock) }
        try Task.checkCancellation()
        reset(at: atMs)
        let mode = recovery.fallbackMode ?? .auto
        let decision = caps.map { CapsPolicy.decide(file, $0) }
        var choices = PlaybackPlan.Choices(file: file, startMs: atMs, mode: mode,
                                           streaming: PlaybackPlan.streams(mode: mode, decision: decision, file: file,
                                                                           audio: nil, caps: caps))
        var sent = PlaybackPlan.request(itemId: item.id, choices, clientId: env.clientId, caps: caps)
        var started = try await ServerPlaybackSession.start(api: env.api, request: sent, reporter: env.reporter,
                                                            late: env.late)
        if PlaybackPlan.needsIndexes(started.info, sent: sent, choices) {
            started.discard()
            choices.streaming = true
            sent = PlaybackPlan.request(itemId: item.id, choices, clientId: env.clientId, caps: caps)
            started = try await ServerPlaybackSession.start(api: env.api, request: sent, reporter: env.reporter,
                                                            late: env.late)
        }
        guard !closed, !Task.isCancelled else {
            started.discard()
            throw CancellationError()
        }
        let url: URL
        do {
            url = try await env.api.mediaURL(started.info.url)
        } catch {
            started.discard()
            throw error
        }
        load(started, url: url, at: atMs)
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
        state.reasons = info.reasons
        state.videoRange = info.videoRange
        if info.durationMs > 0 { state.durationMs = info.durationMs }
        engine?.load(url: url, startMs: atMs, play: wantPlay)
    }

    /// A new session's start at `atMs`: nothing shown yet, nothing ended.
    private func reset(at atMs: Int) {
        lastPositionMs = atMs
        mediaShown = false
        ended = false
        noPictureFrom = nil
        pictureSeen = false
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
        guard !closed, let session else { return }
        switch event {
        case .ready:
            mediaShown = true
            lastAdvanceAt = env.clock.now()
            if let state = schedule.firstFrame(at: env.clock.now()) { session.progress(positionMs: lastPositionMs, state: state) }
            setPhase(wantPlay ? .playing : .paused)
        case .ended:
            onEnded()
        case .failed(let error):
            onError(error, in: session)
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

    private func act(_ action: Recovery.Action, starving: Bool) {
        switch action {
        case .newSession:
            restart(at: livePosition())
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
            state.notice = PlayerTexts.slowServer
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
        guard sessionTask == nil else {
            // The next session is on its way: this one's stalls no longer count.
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
