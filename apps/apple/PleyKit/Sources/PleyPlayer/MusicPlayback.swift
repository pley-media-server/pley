import Combine
import Foundation
import PleyCaps
import PleyCommon
import PleyModel
import PleyNetwork

/// Why music stopped playing, as Now Playing says it (Android's words).
public enum MusicError: Equatable, Sendable {
    /// Nobody is signed in (any more).
    case signedOut
    /// The server couldn't be reached.
    case offline
    /// The server couldn't play the track, nor convert it.
    case unplayable
    /// The server stopped the playback (an administrator, or the profile elsewhere).
    case stopped

    public var text: String {
        switch self {
        case .signedOut: "Sign in again to play."
        case .offline: "Can't reach the server."
        case .unplayable: "This track can't be played."
        case .stopped: "Playback was stopped by the server."
        }
    }
}

/// What music plays now, for the mini player, Now Playing and the system's
/// Now Playing. It changes with the track, the queue and playing, never with
/// the position, which is `MusicPlayback.progress`'s.
public struct MusicState: Equatable, Sendable {
    /// The track playing; `nil` when nothing does (the queue is empty).
    public var track: ItemSummary?
    /// The track's artist, else the album's.
    public var artist: String? { track?.grandparentTitle.flatMap { $0.isEmpty ? nil : $0 } }
    public var album: String? { track?.parentTitle.flatMap { $0.isEmpty ? nil : $0 } }
    public var albumId: Int? { track?.parentId }
    public var artistId: Int? { track?.grandparentId }
    /// The album art's path on the server: the track's own picture, else its album's poster.
    public var artPath: String? { track.flatMap(MusicState.artPath) }
    /// The track's length: the media's once known, else the server's; 0 when neither says.
    public var durationMs = 0
    /// Meant to play: Play/Pause shows Pause.
    public var playing = false
    /// Waiting for the server or the network before it plays.
    public var loading = false
    /// The queue played to its end: Play starts its last track again.
    public var ended = false
    /// Why it doesn't play; `nil` while it does.
    public var error: MusicError?
    /// The queue in the order it plays.
    public var queue: [ItemSummary] = []
    /// The current track's place in `queue`.
    public var index = 0
    public var shuffle = false
    public var sleep = SleepState()

    public init() {}

    /// Whether there is a queue: the mini player and Apple TV's Now Playing tab show.
    public var active: Bool { track != nil }
    public var hasNext: Bool { index + 1 < queue.count }
    /// What plays after the current track, with each one's place in the queue.
    public var upNext: [(position: Int, track: ItemSummary)] {
        hasNext ? (index + 1..<queue.count).map { ($0, queue[$0]) } : []
    }

    /// The art's path for `track`: its own picture, else its album's poster.
    public static func artPath(_ track: ItemSummary) -> String? {
        if let poster = track.posterUrl, !poster.isEmpty { return poster }
        return track.parentId.map { "/api/items/\($0)/images/poster" }
    }

    /// Equal by everything shown: the tuple list `upNext` is worked out.
    public static func == (a: MusicState, b: MusicState) -> Bool {
        a.track == b.track && a.durationMs == b.durationMs && a.playing == b.playing && a.loading == b.loading
            && a.ended == b.ended && a.error == b.error && a.queue == b.queue && a.index == b.index
            && a.shuffle == b.shuffle && a.sleep == b.sleep
    }
}

/// Where the track is, apart from `MusicState` so that only what draws
/// the time (a scrubber, a progress line) follows it, twice a second.
@MainActor
public final class MusicProgress: ObservableObject {
    @Published public internal(set) var positionMs = 0
    @Published public internal(set) var durationMs = 0

    public init() {}

    /// How far along, 0 to 1.
    public var fraction: Double {
        durationMs > 0 ? min(max(Double(positionMs) / Double(durationMs), 0), 1) : 0
    }
}

/// What music plays with: the server signed in to now, what the device
/// plays, music's own client id, the report queue, the late reports, a
/// clock and the output.
@MainActor
public struct MusicEnvironment {
    /// The API of the server and profile signed in to now; throws when there is none.
    public var api: @MainActor () async throws -> PleyAPI
    /// What this device plays, for the server's choice of direct play or a conversion.
    public var caps: @MainActor () async -> ClientCaps
    /// Music's own player id: a video starting doesn't end music's session, nor the other way round.
    public var clientId: String
    public var reporter: SessionReporter
    /// The queue of reports that couldn't reach this server and profile.
    public var late: @MainActor () async -> LateReports?
    public var clock: any PlayerClock
    /// Makes the output, the first time music plays.
    public var outputs: @MainActor () -> any MusicOutput
    /// A random number below its argument, for shuffling.
    public var random: @Sendable (Int) -> Int
    /// What the profile's remotes call this device; `nil`: the server names it from the User-Agent.
    public var deviceName: String?

    public init(api: @escaping @MainActor () async throws -> PleyAPI, caps: @escaping @MainActor () async -> ClientCaps,
                clientId: String, reporter: SessionReporter, late: @escaping @MainActor () async -> LateReports? = { nil },
                clock: any PlayerClock = SystemPlayerClock(),
                outputs: @escaping @MainActor () -> any MusicOutput = { AVMusicOutput() },
                random: @escaping @Sendable (Int) -> Int = { Int.random(in: 0..<$0) }, deviceName: String? = nil) {
        self.api = api
        self.deviceName = deviceName
        self.caps = caps
        self.clientId = clientId
        self.reporter = reporter
        self.late = late
        self.clock = clock
        self.outputs = outputs
        self.random = random
    }
}

/// Music's queue and the server's side of it, a port of the Android app's
/// `MusicPlayback` (the web's AudioPlayer.tsx):
///
/// - **A session per track** (`POST /api/playback`, `mode: auto`, keyed
///   URLs), the one before ended first (`stopped`, then `DELETE`, waiting
///   2 s at most) so a conversion never waits on the last one.
/// - **Reports**: progress every 10 s while playing and at a pause; at a
///   track's end, `stopped` at its length and `DELETE`, then the next track.
///   Reports that can't go out wait in `LateReports`.
/// - **Failures**: a track the device can't decode is asked for again as a
///   conversion; an expired session (after a long pause) starts again where
///   it was, three times at most; `session_stopped` stops the music and
///   says so, and Play starts the track again.
/// - **Previous** restarts a track that has played more than 3 s.
/// - **The profile's remotes** (`obey`): play, pause, seek, skip, next and
///   previous, for the track playing now, reported at once.
/// - **The sleep timer** pauses after the minutes chosen, or at the end of
///   the track (the next one waits, paused).
///
/// It lives as long as the app (`AppModel.music`), so it plays on with no
/// screen open, in the background too.
@MainActor
public final class MusicPlayback: ObservableObject {
    /// Progress reports while playing, as on the web.
    public static let progressEveryMs: Int64 = 10_000
    /// The remote's commands music obeys.
    public static let controls: [Control] = [.play, .pause, .seek, .skip, .next, .previous]
    /// New sessions asked in a row for one track before it counts as failed.
    public static let maxRecreates = 3
    /// The longest a new track waits for the last one's reports.
    public static let flushMs: Int64 = 2_000

    @Published public private(set) var state = MusicState()
    /// Where the track is, for what draws the time.
    public let progress = MusicProgress()
    /// Hears every change of `state` once it is published (the app's Now Playing).
    public var onChange: (@MainActor (MusicState) -> Void)?

    private let env: MusicEnvironment
    private var queue: MusicQueue
    private var out: (any MusicOutput)?
    private var outEvents: Task<Void, Never>?
    /// The session of the track playing; `nil` between tracks.
    public private(set) var session: ServerPlaybackSession?
    private var loadTask: Task<Void, Never>?
    private var ticker: Task<Void, Never>?
    /// Counts the tracks asked for, so only the latest one's answer loads.
    private var generation = 0
    /// The mode asked for after direct play failed to decode the track.
    private var fallback: PlaybackMode?
    /// New sessions asked for the current track since it last played.
    private var recreates = 0
    private var moving = false
    private var loading = false
    private var ended = false
    private var error: MusicError?
    /// A report answered `404`: the next play starts a new session where it was.
    private var expired = false
    private lazy var timer = SleepTimer(clock: env.clock, itemLeftMs: { [weak self] in self?.trackLeftMs() },
                                        itemKey: { [weak self] in self?.currentKey },
                                        onFire: { [weak self] in self?.pause() })

    public init(environment: MusicEnvironment) {
        env = environment
        queue = MusicQueue(random: environment.random)
        timer.onChange = { [weak self] _ in self?.publish() }
    }

    /// Whether anything is queued.
    public var active: Bool { !queue.isEmpty }
    /// The session of the track playing, by id, for the server's events.
    public var sessionId: String? { session?.id }

    /// Where the track is now.
    public var positionMs: Int {
        guard let out else { return 0 }
        return ended ? state.durationMs : out.positionMs
    }

    // MARK: Commands

    /// Plays `tracks` from `start`; with `shuffle`, in a random order that starts with that track.
    public func play(tracks: [ItemSummary], start: Int = 0, shuffle: Bool = false) {
        guard !tracks.isEmpty else { return }
        queue.start(tracks, at: start, shuffle: shuffle)
        startTrack(at: 0)
    }

    /// Plays; after the queue's end or a failure, starts the current track again.
    public func play() {
        guard !queue.isEmpty else { return }
        if ended {
            startTrack(at: 0)
        } else if error != nil || expired || session == nil && loadTask == nil {
            startTrack(at: lastPosition, sameTrack: true, play: true)
        } else {
            timer.resumed()
            output().play()
        }
    }

    public func pause() {
        out?.pause()
    }

    public func toggle() {
        if let out, out.playWhenReady, !ended, error == nil { pause() } else { play() }
    }

    public func next() {
        if queue.next() { startTrack(at: 0) }
    }

    /// Back to the start of the track, or the track before when it has played 3 s or less.
    public func previous() {
        switch queue.previous(positionMs: out?.positionMs ?? 0) {
        case .moved:
            startTrack(at: 0)
        case .restart:
            if out == nil || session == nil || ended || error != nil {
                startTrack(at: 0, sameTrack: !ended)
            } else {
                out?.seek(toMs: 0)
                progress.positionMs = 0
            }
        }
    }

    public func seek(toMs positionMs: Int) {
        guard !ended, session != nil else { return }
        let target = max(positionMs, 0)
        out?.seek(toMs: target)
        progress.positionMs = target
    }

    /// Plays the queue's track at `position`.
    public func jump(to position: Int) {
        if queue.jump(to: position) { startTrack(at: 0) }
    }

    public func setShuffle(_ shuffle: Bool) {
        queue.setShuffle(shuffle)
        publish()
    }

    public func setSleep(_ choice: SleepChoice) {
        if queue.isEmpty, choice != .off { return }
        timer.set(choice)
        publish()
    }

    /// Keep listening, in the sleep timer's last 30 s: the same time again.
    public func keepListening() {
        timer.extend()
    }

    /// A video or Live TV starts: the music waits where it was.
    public func pauseForVideo() {
        out?.pause()
    }

    /// The system's audio was taken (a call, an alarm): music pauses.
    public func interrupted() {
        out?.pause()
    }

    /// A command from the profile's remote on another device, for session
    /// `sessionId` (`player_command`): acted on only when it is the track
    /// playing now, as this device's own buttons would, then reported at
    /// once, so the remote sees it. A seek stops at the start and the end,
    /// a skip counts from where the track is, and next at the queue's end
    /// does nothing. A new track tells the server as its session starts.
    public func obey(_ command: PlayerCommand, for sessionId: String) {
        guard let session, session.id == sessionId, !ended, error == nil else { return }
        switch command.command {
        case .play:
            guard !state.playing else { return }
            play()
            session.progress(positionMs: lastPosition, state: .playing)
        case .pause:
            // A pause reports by itself.
            if state.playing { pause() }
        case .seek, .skip:
            let target = command.command == .seek ? command.positionMs : command.offsetMs.map { positionMs + $0 }
            guard let target else { return }
            let end = state.durationMs > 0 ? state.durationMs : Int.max
            let at = min(max(target, 0), end)
            seek(toMs: at)
            session.progress(positionMs: at, state: state.playing ? .playing : .paused)
        case .next:
            next()
        case .previous:
            previous()
            // Back to the start of this track: the same session, moved.
            if self.session === session { session.progress(positionMs: 0, state: state.playing ? .playing : .paused) }
        default:
            break
        }
    }

    /// The server stopped this track's session (`session_stopped`, or a
    /// report's `410`): the music stops and says so. Play starts the track again.
    public func stoppedByServer() {
        guard let session else { return }
        self.session = nil
        session.stoppedByServer(positionMs: lastPosition)
        loadTask?.cancel()
        loadTask = nil
        timer.cancel()
        out?.pause()
        fail(.stopped)
    }

    /// Ends the session, lets go of the output and forgets the queue
    /// (signing out, another profile).
    public func stop() {
        generation += 1
        loadTask?.cancel()
        loadTask = nil
        stopTicker()
        timer.cancel()
        endSession()
        outEvents?.cancel()
        outEvents = nil
        out?.release()
        out = nil
        queue.clear()
        fallback = nil
        recreates = 0
        moving = false
        loading = false
        ended = false
        error = nil
        expired = false
        progress.positionMs = 0
        progress.durationMs = 0
        publish()
    }

    /// Sends a progress report now, as the 10 s ticker does (the tests' ticker).
    func reportProgress() {
        session?.progress(positionMs: lastPosition, state: .playing)
    }

    // MARK: Inside

    private var currentKey: AnyHashable? {
        guard let track = queue.current else { return nil }
        return AnyHashable("\(queue.index)-\(track.id)")
    }

    private var lastPosition: Int { max(out?.positionMs ?? 0, 0) }

    /// The output, made the first time music plays.
    private func output() -> any MusicOutput {
        if let out { return out }
        let made = env.outputs()
        out = made
        if let avOutput = made as? AVMusicOutput {
            avOutput.onTime = { [weak self] ms in
                guard let self, !self.ended else { return }
                self.progress.positionMs = ms
                let duration = avOutput.durationMs
                if duration > 0, duration != self.state.durationMs { self.publish() }
            }
        }
        let events = made.events
        outEvents = Task { [weak self] in
            for await event in events { self?.heard(event) }
        }
        return made
    }

    /// Plays the current track from `atMs` in a new session, ending the one
    /// before. `sameTrack` keeps the conversion fallback and the count of
    /// sessions asked for: it is the same track, asked again.
    private func startTrack(at atMs: Int, sameTrack: Bool = false, play: Bool? = nil) {
        guard let track = queue.current else { return }
        if !sameTrack {
            fallback = nil
            recreates = 0
        }
        let wantPlay = play ?? (!sameTrack || error != nil || (out?.playWhenReady ?? true))
        generation += 1
        let asked = generation
        loadTask?.cancel()
        stopTicker()
        endSession()
        let output = output()
        if !sameTrack { output.stop() }
        ended = false
        error = nil
        expired = false
        loading = true
        moving = false
        progress.positionMs = atMs
        progress.durationMs = track.durationMs ?? 0
        publish()
        let env = env
        let mode = fallback ?? .auto
        loadTask = Task { [weak self] in
            // The session before goes first (stopped, then DELETE): the server's conversions.
            await env.reporter.flush(timeoutMs: Self.flushMs, clock: env.clock)
            do {
                let api = try await env.api()
                let caps = await env.caps()
                let late = await env.late()
                let request = PlaybackRequest(itemId: track.id, startMs: atMs > 0 ? atMs : nil, mode: mode,
                                              clientId: env.clientId, keyedUrls: true, controls: Self.controls,
                                              deviceName: env.deviceName, caps: caps)
                let started = try await ServerPlaybackSession.start(api: api, request: request,
                                                                    reporter: env.reporter, late: late)
                guard let self, asked == self.generation, !Task.isCancelled else {
                    started.discard()
                    return
                }
                let url: URL
                do {
                    url = try await api.mediaURL(started.info.url)
                } catch {
                    started.discard()
                    throw error
                }
                guard asked == self.generation, !Task.isCancelled else {
                    started.discard()
                    return
                }
                self.loaded(started, url: url, at: atMs, play: wantPlay)
            } catch is CancellationError {
            } catch {
                guard let self, asked == self.generation else { return }
                self.loadTask = nil
                self.fail(Self.errorOf(error))
            }
        }
    }

    private func loaded(_ started: ServerPlaybackSession, url: URL, at atMs: Int, play: Bool) {
        loadTask = nil
        session = started
        started.onAnswer = { [weak self, weak started] answer in
            guard let self, let started, self.session === started else { return }
            switch answer {
            case .stopped: self.stoppedByServer()
            case .expired: self.expired = true
            }
        }
        if started.info.durationMs > 0 { progress.durationMs = started.info.durationMs }
        out?.load(url: url, startMs: atMs, play: play)
        publish()
    }

    /// How long the track has left, for the sleep timer; `nil` while it isn't playing.
    private func trackLeftMs() -> Int? {
        guard session != nil, !ended, error == nil else { return nil }
        let duration = state.durationMs
        return duration > 0 ? duration - lastPosition : nil
    }

    /// Ends the current track's session at the output's position.
    private func endSession() {
        guard let session else { return }
        self.session = nil
        session.end(positionMs: lastPosition)
    }

    private func fail(_ why: MusicError) {
        loading = false
        moving = false
        error = why
        stopTicker()
        publish()
    }

    private func startTicker() {
        guard ticker == nil else { return }
        let clock = env.clock
        ticker = Task { [weak self] in
            while !Task.isCancelled {
                do { try await clock.sleep(ms: Self.progressEveryMs) } catch { return }
                self?.reportProgress()
            }
        }
    }

    private func stopTicker() {
        ticker?.cancel()
        ticker = nil
    }

    private func heard(_ event: MusicOutputEvent) {
        switch event {
        case .playing(let playing):
            guard moving != playing else { return }
            moving = playing
            if playing {
                loading = false
                recreates = 0
                startTicker()
            } else {
                stopTicker()
            }
            publish()
        case .buffering(let buffering):
            guard loading != buffering, session != nil else { return }
            loading = buffering
            publish()
        case .playWhenReady(let play):
            if play { timer.resumed() }
            if !play, !ended { session?.progress(positionMs: lastPosition, state: .paused) }
            publish()
        case .ended:
            trackEnded()
        case .failed(let failure):
            failed(failure)
        }
    }

    private func trackEnded() {
        let finished = session
        session = nil
        stopTicker()
        let length = (finished?.info.durationMs).flatMap { $0 > 0 ? $0 : nil } ?? max(out?.durationMs ?? 0, lastPosition)
        finished?.end(positionMs: length)
        // The sleep timer ends with this track: the next one waits, paused.
        let sleeping = timer.itemEnded(currentKey)
        if queue.next() {
            startTrack(at: 0, play: !sleeping)
        } else {
            ended = true
            moving = false
            loading = false
            progress.positionMs = progress.durationMs
            publish()
        }
    }

    private func failed(_ failure: MusicOutputError) {
        let direct = session?.info.method == .direct
        switch failure {
        case .stopped:
            stoppedByServer()
        case .decode where direct && fallback == nil:
            fallback = .transcode
            startTrack(at: lastPosition, sameTrack: true)
        case .gone where recreates < Self.maxRecreates, .network where recreates < Self.maxRecreates:
            recreates += 1
            startTrack(at: lastPosition, sameTrack: true)
        default:
            endSession()
            fail(failure == .network ? .offline : .unplayable)
        }
    }

    private func publish() {
        var next = MusicState()
        next.track = queue.current
        next.queue = queue.tracks
        next.index = queue.index
        next.shuffle = queue.shuffle
        let media = out?.durationMs ?? 0
        next.durationMs = media > 0 ? media : progress.durationMs > 0 ? progress.durationMs : queue.current?.durationMs ?? 0
        if progress.durationMs != next.durationMs { progress.durationMs = next.durationMs }
        next.playing = (out?.playWhenReady ?? false) && !ended && error == nil
        next.loading = loading && error == nil
        next.ended = ended
        next.error = error
        next.sleep = queue.isEmpty ? SleepState() : timer.state
        guard next != state else { return }
        state = next
        onChange?(next)
    }

    /// Why starting a track failed, as music says it.
    static func errorOf(_ error: any Error) -> MusicError {
        if LateReports.unreachable(error) { return .offline }
        switch error as? PleyError {
        case .signedOut, .insufficientScope, .api(401, _, _): return .signedOut
        case .sessionStopped: return .stopped
        default: return .unplayable
        }
    }
}
