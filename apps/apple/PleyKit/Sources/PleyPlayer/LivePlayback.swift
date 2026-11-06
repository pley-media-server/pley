import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// Why a channel doesn't play, by what the server answered (docs/API.md,
/// "Watching"), with Android's sentence for it. A `409` lists the
/// channels holding every connection of the source (`connections`: who
/// watches each, and which may be taken over).
public struct LiveTrouble: Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        /// `409`: every connection of the channel's source is in use.
        case busy
        /// `410`: an administrator stopped the channel, the provider ended
        /// it, or another device took its connection.
        case stopped
        /// `404`: the channel was hidden or its source removed.
        case missing
        /// `403`: Live TV isn't open to this profile.
        case notAllowed
        /// `502`, `503` or `504`: the provider sends nothing, or not in time.
        case unavailable
        /// The server is out of reach.
        case offline
        case other
    }

    public var kind: Kind
    /// What the server said, made a sentence; the screens' titles are their own.
    public var message: String
    public var connections: [BusyConnection]

    public init(kind: Kind, message: String, connections: [BusyConnection] = []) {
        self.kind = kind
        self.message = message
        self.connections = connections
    }

    /// Whether trying again may help: not for a channel that is gone.
    public var canRetry: Bool { kind != .missing }

    /// The trouble `error` is: a refusal of the server's, a network failure, or else.
    public static func of(_ error: any Error) -> LiveTrouble {
        switch error as? PleyError {
        case .busy(let message, let connections)?:
            return LiveTrouble(kind: .busy, message: sentence(message ?? ""), connections: connections)
        case .sessionStopped(let message)?:
            return stopped(message)
        case .api(let status, _, let message)?:
            return LiveTrouble(kind: kind(of: status), message: sentence(message ?? localized("the server answered \(String(status))")))
        case let offline? where [.offline, .timedOut, .localNetworkBlocked].contains(offline) || offline.isUnreachable:
            return LiveTrouble(kind: .offline, message: sentence(offline.userMessage))
        case let other?:
            return LiveTrouble(kind: .other, message: sentence(other.userMessage))
        case nil:
            return LiveTrouble(kind: .other, message: sentence(error.localizedDescription))
        }
    }

    /// A stop at the server: who stopped it, when known, then why on a line
    /// of its own (Android's `session_stopped_message`); with neither,
    /// "Watching was stopped."
    public static func stopped(_ message: String?, who: String? = nil) -> LiveTrouble {
        let why = sentence(message ?? "")
        let text = [who, why].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: "\n")
        return LiveTrouble(kind: .stopped, message: text.isEmpty ? localized("Watching was stopped.") : text)
    }

    /// The kind of an HTTP status the server answered with.
    public static func kind(of status: Int) -> Kind {
        switch status {
        case 409: .busy
        case 410: .stopped
        case 404: .missing
        case 403: .notAllowed
        case 502, 503, 504: .unavailable
        default: .other
        }
    }

    /// The title the player shows for it, Android's: "All connections are in use"…
    public func title(channel: String) -> String {
        switch kind {
        case .busy: localized("All connections are in use")
        case .stopped: localized("Watching stopped")
        case .missing: localized("This channel isn't here any more")
        case .notAllowed: localized("Live TV isn't open to this profile")
        case .unavailable: localized("\(channel) isn't playing")
        case .offline: localized("Can't reach your server")
        case .other: localized("\(channel) didn't start")
        }
    }

    private static func sentence(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let first = trimmed.first else { return "" }
        let capital = first.uppercased() + trimmed.dropFirst()
        return ".…!?".contains(capital.last!) ? capital : capital + "."
    }
}

extension PleyError {
    fileprivate var isUnreachable: Bool {
        if case .unreachable = self { return true }
        return false
    }
}

/// Where watching a channel is.
public enum LivePhase: Equatable, Sendable {
    /// Nothing is tuned: before the first channel, or let go in the background.
    case idle
    /// Asking the server for the channel, then for its first segments.
    case tuning
    /// The channel's picture (or sound) is out.
    case playing
    case failed(LiveTrouble)
}

/// What a live player shows. Live TV is watched as it airs: no pausing, no going back.
public struct LiveState: Equatable, Sendable {
    public var channelId: Int?
    public var phase: LivePhase = .idle
    /// Waiting for data while playing, for longer than `LivePlayback.stallShownMs`.
    public var buffering = false
    /// The channel has sound only (radio): no picture comes.
    public var audioOnly = false

    public init(channelId: Int? = nil, phase: LivePhase = .idle) {
        self.channelId = channelId
        self.phase = phase
    }
}

/// Watching Live TV on this device (Android's `LivePlayback`), shared by
/// every device's live player:
///
/// - `tune` asks the server for the channel with this install's client
///   id, which ends what it watched before (zapping never holds two of the
///   provider's connections), lets the previous session go, and plays the
///   answer's keyed HLS. AVPlayer can't play a progressive MPEG-TS, so
///   there is no `stream.ts` shortcut.
/// - It is watched as it airs: a pause from anywhere plays on, and nothing seeks.
/// - When AVPlayer fails, the session's plan is asked once: a `410` there
///   is a stop, with the server's why; anything else tunes again, twice at
///   most before it says the channel didn't start.
/// - The session stopped from elsewhere (the server's `session_stopped`:
///   an administrator, or the profile on another device) or its
///   connection taken over (`live_taken_over`) stops it at once and says
///   why, without waiting for the stream to run out.
/// - `stop` lets the session and the engine go (the app went to the
///   background), `resume` tunes the channel again unless it failed (the
///   server stopped it, every connection in use), `close` ends it all.
@MainActor
public final class LivePlayback: ObservableObject {
    /// A stall shows the spinner once it has lasted this long (the web's 0.7 s).
    public static let stallShownMs: Int64 = 700
    /// After this long playing with no picture, the channel is sound only.
    public static let audioOnlyMs: Int64 = 1_500
    /// How often a failed stream tunes again by itself before it says so.
    public static let retunes = 2

    @Published public private(set) var state = LiveState()
    /// The engine playing now, for the screen's video surface; `nil` before the first tune and after `stop`.
    @Published public private(set) var engine: (any LiveEngine)?

    private let live: LiveTvRepository
    private let engines: @MainActor () -> any LiveEngine
    private let clientId: String
    /// The remote's commands the player obeys, and what remotes call this device, sent with each tune.
    private let controls: [Control]?
    private let deviceName: String?
    private let clock: any PlayerClock
    private var session: String?
    /// Stops heard while a tune waits for its session's id: the answer
    /// may name one of them, which then never plays.
    private var stoppedWhileTuning: [String: LiveTrouble] = [:]
    private var tuning: Task<Void, Never>?
    private var listening: Task<Void, Never>?
    private var stall: Task<Void, Never>?
    private var sound: Task<Void, Never>?
    private var closed = false
    private var retuned = 0
    /// Goes up with each tune and stop: an answer for an older one is let go.
    private var generation = 0

    public init(live: LiveTvRepository, clientId: String, clock: any PlayerClock = SystemPlayerClock(),
                engines: @escaping @MainActor () -> any LiveEngine = { AVLiveEngine() }, controls: [Control]? = nil,
                deviceName: String? = nil) {
        self.live = live
        self.clientId = clientId
        self.controls = controls
        self.deviceName = deviceName
        self.clock = clock
        self.engines = engines
    }

    /// The session watching now, if any: the one the server's events name.
    public var sessionId: String? { session }

    /// Starts watching `channelId` in place of what played.
    public func tune(_ channelId: Int) {
        retuned = 0
        tune(channelId, takeOver: nil)
    }

    /// Tunes the channel again: after a failure, or back from the
    /// background. `takeOver`: every connection being in use, the channel
    /// whose connection to take (one `LiveTrouble.connections` said may be).
    public func retry(takeOver: Int? = nil) {
        guard let id = state.channelId else { return }
        retuned = 0
        tune(id, takeOver: takeOver)
    }

    /// The server stopped session `sessionId` (`session_stopped`): if it is
    /// the one watching or tuning, it stops now and says who stopped it, in
    /// Android's words.
    public func stoppedElsewhere(_ sessionId: String, message: String?, byAdmin: Bool) {
        let who = byAdmin ? localized("Live TV was stopped by the server's administrator.")
            : localized("Live TV was stopped from another device.")
        endedAtServer(sessionId, .stopped(message, who: who))
    }

    /// Someone took session `sessionId`'s connection (`live_taken_over`):
    /// if it is the one watching or tuning, it stops now and says `message`.
    public func takenOver(_ sessionId: String, message: String) {
        endedAtServer(sessionId, .stopped(message))
    }

    /// Back in front: the channel left in `stop` plays again, from the live
    /// edge; one that failed (the server stopped it, it's gone, every
    /// connection is in use) keeps saying so until Watch again.
    public func resume() {
        if state.phase == .idle { retry() }
    }

    /// Lets the session and the engine go, keeping which channel it was:
    /// the app is in the background. Nothing plays and no connection is
    /// held until `resume`.
    public func stop() {
        cancelWork()
        letSessionGo()
        releaseEngine()
        if case .failed = state.phase {
            state.buffering = false
        } else {
            state = LiveState(channelId: state.channelId)
        }
    }

    /// Ends watching for good.
    public func close() {
        stop()
        closed = true
    }

    // MARK: Inside

    private func tune(_ channelId: Int, takeOver: Int?) {
        guard !closed else { return }
        cancelWork()
        letSessionGo()
        stoppedWhileTuning = [:]
        state = LiveState(channelId: channelId, phase: .tuning)
        let generation = generation
        // Not cancelled when another tune comes: the server may have
        // opened the session already, and only its answer says which to let go.
        tuning = Task { [weak self] in
            guard let self else { return }
            do {
                let (watch, url) = try await self.live.watch(channelId, clientId: self.clientId, takeOver: takeOver,
                                                             controls: self.controls, deviceName: self.deviceName)
                guard self.generation == generation, !self.closed else {
                    // Tuned elsewhere, stopped or closed meanwhile: this one isn't wanted.
                    self.live.leave(watch.sessionId)
                    return
                }
                if let trouble = self.stoppedWhileTuning.removeValue(forKey: watch.sessionId) {
                    // Stopped at the server before it played: nothing to let go.
                    self.fail(trouble)
                    return
                }
                self.session = watch.sessionId
                let engine = self.engine ?? self.makeEngine()
                engine.load(url: url)
            } catch {
                guard self.generation == generation, !(error is CancellationError) else { return }
                self.fail(LiveTrouble.of(error))
            }
        }
    }

    private func makeEngine() -> any LiveEngine {
        let engine = engines()
        self.engine = engine
        let events = engine.events
        listening = Task { [weak self, weak engine] in
            for await event in events {
                guard let self, let engine, self.engine === engine else { return }
                self.heard(event, from: engine)
            }
        }
        return engine
    }

    private func heard(_ event: LiveEngineEvent, from engine: any LiveEngine) {
        switch event {
        case .playing:
            stall?.cancel()
            stall = nil
            if state.phase == .tuning {
                state.phase = .playing
                retuned = 0
                watchForPicture(engine)
            }
            if state.buffering { state.buffering = false }
        case .waiting:
            // Said once it lasts: a channel that stutters for a moment doesn't flicker the spinner.
            guard state.phase == .playing, stall == nil else { return }
            let clock = clock
            stall = Task { [weak self] in
                try? await clock.sleep(ms: Self.stallShownMs)
                guard !Task.isCancelled, let self, self.state.phase == .playing else { return }
                self.state.buffering = true
            }
        case .paused:
            // Live has no pause (headphones pulled out, a stray command): it plays on.
            if state.phase == .playing || state.phase == .tuning { engine.play() }
        case .video(let shows):
            if shows && state.audioOnly { state.audioOnly = false }
        case .failed:
            guard let channelId = state.channelId, state.phase == .playing || state.phase == .tuning else { return }
            brokeOff(channelId)
        }
    }

    /// Once it has played a while with no picture, the channel is sound only.
    private func watchForPicture(_ engine: any LiveEngine) {
        sound?.cancel()
        let clock = clock
        sound = Task { [weak self, weak engine] in
            try? await clock.sleep(ms: Self.audioOnlyMs)
            guard !Task.isCancelled, let self, let engine, self.state.phase == .playing else { return }
            self.state.audioOnly = !engine.hasVideo()
        }
    }

    /// The stream stopped. The server ended watching (its connection taken
    /// over, the channel stopped): the plan's `410` says why. Otherwise it
    /// broke off: tuned again, twice at most.
    private func brokeOff(_ channelId: Int) {
        let asked = session
        cancelWork()
        let generation = generation
        tuning = Task { [weak self] in
            guard let self else { return }
            var ended: (any Error)?
            if let asked {
                do {
                    _ = try await self.live.plan(asked)
                } catch {
                    if case .sessionStopped? = error as? PleyError { ended = error }
                }
            }
            // Tuned elsewhere, stopped or closed meanwhile: nothing more to do.
            guard self.generation == generation, !self.closed else { return }
            if let ended {
                // Already gone at the server: nothing to let go.
                if self.session == asked { self.session = nil }
                self.fail(LiveTrouble.of(ended))
            } else if self.retuned < Self.retunes {
                self.retuned += 1
                self.tune(channelId, takeOver: nil)
            } else {
                self.fail(LiveTrouble(kind: .other, message: ""))
            }
        }
    }

    /// Watching `sessionId` ended at the server: nothing to let go, and the
    /// stream's end that follows is no break to tune again after.
    private func endedAtServer(_ sessionId: String, _ trouble: LiveTrouble) {
        guard !closed else { return }
        if session == nil, state.phase == .tuning {
            stoppedWhileTuning[sessionId] = trouble
            return
        }
        guard session == sessionId else { return }
        cancelWork()
        session = nil
        fail(trouble)
    }

    private func fail(_ trouble: LiveTrouble) {
        stall?.cancel()
        sound?.cancel()
        letSessionGo()
        releaseEngine()
        state.phase = .failed(trouble)
        state.buffering = false
    }

    private func cancelWork() {
        generation += 1
        tuning = nil
        stall?.cancel()
        stall = nil
        sound?.cancel()
        sound = nil
    }

    private func letSessionGo() {
        if let session { live.leave(session) }
        session = nil
    }

    private func releaseEngine() {
        listening?.cancel()
        listening = nil
        engine?.close()
        engine = nil
    }
}
