import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// Sends playbacks' reports one after the other, in the order they were
/// made (progress, then `stopped`, then `DELETE`), on one serial queue
/// that outlives the player: a player that closes still gets its last
/// position and its session's end to the server. Reports are best effort:
/// each job handles its own failure, and nothing waits for one except
/// `flush()`.
public final class SessionReporter: Sendable {
    public typealias Job = @Sendable () async -> Void

    private let continuation: AsyncStream<Job>.Continuation
    private let worker: Task<Void, Never>

    public init() {
        let (jobs, continuation) = AsyncStream.makeStream(of: Job.self)
        self.continuation = continuation
        worker = Task {
            for await job in jobs { await job() }
        }
    }

    deinit {
        continuation.finish()
    }

    /// Queues `job` behind everything queued so far.
    public func send(_ job: @escaping Job) {
        continuation.yield(job)
    }

    /// Waits until everything queued so far went out.
    public func flush() async {
        await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
            continuation.yield { done.resume() }
        }
    }

    /// `flush()`, waiting `timeoutMs` at most.
    public func flush(timeoutMs: Int64, clock: any PlayerClock) async {
        await withTaskGroup(of: Void.self) { group in
            group.addTask { await self.flush() }
            group.addTask { try? await clock.sleep(ms: timeoutMs) }
            await group.next()
            group.cancelAll()
        }
    }
}

/// What a report's answer says about its session.
public enum ReportAnswer: Sendable, Equatable {
    /// `410 session_stopped`: stopped from elsewhere, with what the person who stopped it wrote.
    case stopped(message: String?)
    /// `404`: the session expired (90 s without a report); the next play or seek starts a new one.
    case expired
}

/// One session on the server (`POST /api/playback`): its progress reports
/// and its end, a port of the Android app's `ServerPlaybackSession`.
/// Ending it reports `stopped` at the last position, then deletes it (and
/// any transcode); it ends once.
///
/// A download played here (`local`) has no session on the server: its
/// reports go without `session_id`, each one that can't reach the server
/// is kept to send later with its age, and nothing is deleted. Where it
/// got to is also kept with the download (`played`), for Resume offline.
@MainActor
public final class ServerPlaybackSession {
    public let itemId: Int
    public let info: PlaybackInfo
    public private(set) var ended = false
    /// Hears what a report's answer said about the session.
    public var onAnswer: (@MainActor (ReportAnswer) -> Void)?

    private let api: PleyAPI
    private let reporter: SessionReporter
    private let late: LateReports?

    /// A download playing from this device, with no session on the server.
    public let local: Bool
    private let played: (@MainActor (Int) -> Void)?

    public var id: String { info.sessionId }

    public init(api: PleyAPI, itemId: Int, info: PlaybackInfo, reporter: SessionReporter, late: LateReports?) {
        self.api = api
        self.itemId = itemId
        self.info = info
        self.reporter = reporter
        self.late = late
        local = false
        played = nil
    }

    private init(api: PleyAPI, itemId: Int, info: PlaybackInfo, reporter: SessionReporter, late: LateReports?,
                 played: @escaping @MainActor (Int) -> Void) {
        self.api = api
        self.itemId = itemId
        self.info = info
        self.reporter = reporter
        self.late = late
        local = true
        self.played = played
    }

    /// The playback of the download of `fileId` at `url` (a `file://`
    /// address): played as it is, reported without a session; `played`
    /// hears each position reported.
    public static func local(api: PleyAPI, itemId: Int, fileId: Int, url: URL, durationMs: Int,
                             reporter: SessionReporter, late: LateReports?,
                             played: @escaping @MainActor (Int) -> Void) -> ServerPlaybackSession {
        let info = PlaybackInfo(sessionId: "", method: .direct, url: url.absoluteString, hls: false, fileId: fileId,
                                durationMs: durationMs)
        return ServerPlaybackSession(api: api, itemId: itemId, info: info, reporter: reporter, late: late,
                                     played: played)
    }

    /// `POST /api/items/{id}/progress` with the session, so it and its transcode stay alive.
    public func progress(positionMs: Int, state: ProgressState) {
        guard !ended else { return }
        played?(max(positionMs, 0))
        send(report(positionMs, state), heard: !local)
    }

    /// Reports `stopped` at `positionMs`, then `DELETE /api/playback/{session}`.
    public func end(positionMs: Int) {
        guard !ended else { return }
        ended = true
        played?(max(positionMs, 0))
        send(report(positionMs, .stopped), heard: false)
        guard !local else { return }
        let (api, id) = (api, id)
        reporter.send { try? await api.stopPlayback(id) }
    }

    /// The server stopped the session itself: `stopped` at `positionMs`
    /// keeps the resume point, and nothing is deleted.
    public func stoppedByServer(positionMs: Int) {
        guard !ended else { return }
        ended = true
        played?(max(positionMs, 0))
        send(report(positionMs, .stopped), heard: false)
    }

    /// Deletes a session that never played, or one moved away from: no
    /// `stopped`, which would move the profile's resume point.
    public func discard() {
        guard !ended else { return }
        ended = true
        guard !local else { return }
        let (api, id) = (api, id)
        reporter.send { try? await api.stopPlayback(id) }
    }

    private func report(_ positionMs: Int, _ state: ProgressState) -> ProgressReport {
        ProgressReport(positionMs: max(positionMs, 0), durationMs: info.durationMs > 0 ? info.durationMs : nil,
                       state: state, sessionId: local ? nil : id)
    }

    /// Queues `report`. A `paused` or `stopped` one that can't reach the
    /// server is kept to send later (`LateReports`); `heard`: its answer
    /// about the session goes to `onAnswer`.
    /// A download's `playing` report is kept too, as `paused`: offline,
    /// it may be all that says where it got to.
    private func send(_ report: ProgressReport, heard: Bool) {
        let (api, itemId, late, local) = (api, itemId, late, local)
        reporter.send { [weak self] in
            do {
                try await api.reportProgress(itemId: itemId, report)
            } catch PleyError.sessionStopped(let message) {
                if heard { await self?.heard(.stopped(message: message)) }
            } catch PleyError.api(404, _, _) {
                if heard { await self?.heard(.expired) }
            } catch {
                if report.state != .playing, LateReports.unreachable(error) {
                    await late?.keep(itemId: itemId, report)
                } else if local, LateReports.unreachable(error) {
                    var kept = report
                    kept.state = .paused
                    await late?.keep(itemId: itemId, kept)
                }
            }
        }
    }

    private func heard(_ answer: ReportAnswer) {
        onAnswer?(answer)
    }

    /// Starts a session for `request`. The request isn't cancelled halfway:
    /// the server may have made the session (and its transcode) already, so
    /// a caller cancelled meanwhile gets it deleted, then the cancellation.
    public static func start(api: PleyAPI, request: PlaybackRequest, reporter: SessionReporter,
                             late: LateReports?) async throws -> ServerPlaybackSession {
        let info = try await Task { try await api.startPlayback(request) }.value
        if Task.isCancelled {
            reporter.send { try? await api.stopPlayback(info.sessionId) }
            throw CancellationError()
        }
        Log.player.info("session \(String(info.sessionId.prefix(8)), privacy: .public) \(info.method.rawValue, privacy: .public)")
        return ServerPlaybackSession(api: api, itemId: request.itemId, info: info, reporter: reporter, late: late)
    }
}
