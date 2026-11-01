import AVFoundation
import Foundation
import PleyCommon

/// AVPlayer as Pley plays with it. One `AVPlayer` for the engine's life
/// (the player views attach it), an `AVPlayerItem` per media.
///
/// - The media is a keyed address the server gave (`PleyAPI.mediaURL`):
///   AVPlayer sends no token, and no header is added to its requests.
/// - The start position is reached exactly (no tolerance) before it plays.
/// - An `AVPlayerItemVideoOutput` is attached to every item, so video is
///   decoded with or without a view on screen, and the frames it gives are
///   counted: a stream whose time runs with no frame is one AVPlayer can't
///   show (`Recovery.onNoVideo`).
/// - The access log gives the bandwidth and the dropped frames, the error
///   log the HTTP status of a failed playlist or segment.
/// - The system's controls act on the player itself: a play or pause they
///   make, and a jump in time, are told to the controller as events.
/// - Subtitles are the app's to draw: the asset's own are never selected.
@MainActor
public final class AVPlayerEngine: PlayerEngine {
    public let player: AVPlayer
    public let events: AsyncStream<EngineEvent>

    private let continuation: AsyncStream<EngineEvent>.Continuation
    private var item: AVPlayerItem?
    private var output: AVPlayerItemVideoOutput?
    private var status: NSKeyValueObservation?
    private var observers: [any NSObjectProtocol] = []
    private var playerObservers: [any NSObjectProtocol] = []
    private var startMs = 0
    private var wantsPlay = false
    private var ready = false
    private var hls = false
    private var frames = 0
    /// The last HTTP error the error log named for this item.
    private var lastHTTPStatus: Int?
    private var closed = false

    public init() {
        player = AVPlayer()
        player.automaticallyWaitsToMinimizeStalling = true
        player.appliesMediaSelectionCriteriaAutomatically = false
        (events, continuation) = AsyncStream.makeStream(of: EngineEvent.self, bufferingPolicy: .bufferingNewest(32))
        playerObservers = [
            NotificationCenter.default.addObserver(forName: AVPlayer.rateDidChangeNotification, object: player,
                                                   queue: .main) { [weak self] _ in
                Task { @MainActor in self?.rateChanged() }
            },
        ]
    }

    public func load(url: URL, startMs: Int, play: Bool) {
        guard !closed else { return }
        detach()
        self.startMs = max(startMs, 0)
        wantsPlay = play
        ready = false
        frames = 0
        lastHTTPStatus = nil
        hls = url.path.hasSuffix(".m3u8")
        let item = AVPlayerItem(asset: AVURLAsset(url: url))
        let output = AVPlayerItemVideoOutput(pixelBufferAttributes: nil)
        item.add(output)
        self.item = item
        self.output = output
        status = item.observe(\.status, options: [.new]) { [weak self] _, _ in
            Task { @MainActor in self?.statusChanged() }
        }
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: AVPlayerItem.didPlayToEndTimeNotification, object: item, queue: .main) {
                [weak self] _ in
                Task { @MainActor in self?.send(.ended) }
            },
            center.addObserver(forName: AVPlayerItem.failedToPlayToEndTimeNotification, object: item, queue: .main) {
                [weak self] note in
                let error = note.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? NSError
                Task { @MainActor in self?.failed(error) }
            },
            center.addObserver(forName: AVPlayerItem.newErrorLogEntryNotification, object: item, queue: .main) {
                [weak self] _ in
                Task { @MainActor in self?.readErrorLog() }
            },
            center.addObserver(forName: AVPlayerItem.timeJumpedNotification, object: item, queue: .main) {
                [weak self] _ in
                Task { @MainActor in self?.jumped() }
            },
        ]
        player.replaceCurrentItem(with: item)
    }

    public func play() {
        wantsPlay = true
        if ready { player.play() }
    }

    public func pause() {
        wantsPlay = false
        player.pause()
    }

    public func seek(toMs positionMs: Int) {
        player.seek(to: CMTime(value: CMTimeValue(max(positionMs, 0)), timescale: 1000), toleranceBefore: .zero,
                    toleranceAfter: .zero)
    }

    public func sample() -> EngineSample {
        var sample = EngineSample()
        guard let item else { return sample }
        let now = item.currentTime()
        sample.positionMs = Self.ms(now) ?? 0
        sample.durationMs = Self.ms(item.duration) ?? 0
        sample.ready = ready
        sample.playing = player.timeControlStatus == .playing
        sample.waiting = player.timeControlStatus == .waitingToPlayAtSpecifiedRate
            && player.reasonForWaitingToPlay != .noItemToPlay
        for range in item.loadedTimeRanges.map(\.timeRangeValue) where range.containsTime(now) {
            sample.bufferedMs = Self.ms(range.end) ?? sample.positionMs
        }
        if let output, output.hasNewPixelBuffer(forItemTime: now),
           output.copyPixelBuffer(forItemTime: now, itemTimeForDisplay: nil) != nil {
            frames += 1
        }
        sample.videoFrames = output == nil ? nil : frames
        sample.presentationWidth = Int(item.presentationSize.width)
        sample.presentationHeight = Int(item.presentationSize.height)
        if let events = item.accessLog()?.events, !events.isEmpty {
            if let observed = events.last?.observedBitrate, observed > 0 { sample.bandwidthKbps = Int(observed / 1000) }
            sample.droppedFrames = events.reduce(0) { $0 + max($1.numberOfDroppedVideoFrames, 0) }
            sample.watchedSeconds = events.reduce(0) { $0 + max($1.durationWatched, 0) }
        }
        return sample
    }

    public func close() {
        guard !closed else { return }
        closed = true
        player.pause()
        playerObservers.forEach(NotificationCenter.default.removeObserver)
        playerObservers = []
        detach()
        player.replaceCurrentItem(with: nil)
        continuation.finish()
    }

    // MARK: Inside

    private func detach() {
        status?.invalidate()
        status = nil
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = []
        if let item, let output { item.remove(output) }
        item = nil
        output = nil
    }

    private func statusChanged() {
        guard let item, !closed else { return }
        switch item.status {
        case .readyToPlay where !ready:
            guard startMs > 0 else { return began(item) }
            let seeking = ObjectIdentifier(item)
            item.seek(to: CMTime(value: CMTimeValue(startMs), timescale: 1000), toleranceBefore: .zero,
                      toleranceAfter: .zero) { [weak self] _ in
                Task { @MainActor in self?.began(seeking) }
            }
        case .failed:
            failed(item.error as NSError?)
        default:
            break
        }
    }

    /// The item stands at its start: it plays when wanted, and says it is ready.
    private func began(_ item: AVPlayerItem) {
        guard self.item === item, !ready, !closed else { return }
        ready = true
        if wantsPlay { player.play() }
        send(.ready)
        Task { [weak self] in await self?.deselectSubtitles(of: item) }
    }

    /// The asset's own subtitles stay off: the app draws the server's.
    private func deselectSubtitles(of item: AVPlayerItem) async {
        guard let group = try? await item.asset.loadMediaSelectionGroup(for: .legible), self.item === item else {
            return
        }
        item.select(nil, in: group)
    }

    /// The rate changed. One the engine didn't ask for came from the
    /// system's controls (or an interruption): the controller hears of it.
    /// Stopping at the end and a failure don't count.
    private func rateChanged() {
        guard !closed, ready, let item, item.status == .readyToPlay, item.error == nil else { return }
        let playing = player.rate != 0
        if playing == wantsPlay { return }
        if !playing, let end = Self.ms(item.duration), let at = Self.ms(item.currentTime()), at >= end - 500 { return }
        wantsPlay = playing
        send(playing ? .playRequested : .pauseRequested)
    }

    private func jumped() {
        guard !closed, ready else { return }
        send(.jumped)
    }

    private func began(_ seeking: ObjectIdentifier) {
        if let item, ObjectIdentifier(item) == seeking { began(item) }
    }

    private func send(_ event: EngineEvent) {
        guard !closed else { return }
        continuation.yield(event)
    }

    private func failed(_ error: NSError?) {
        guard !closed else { return }
        readErrorLog()
        let classified = Self.classify(error, httpStatus: lastHTTPStatus, hls: hls)
        Log.player.error("engine failed: \(String(describing: classified), privacy: .public)")
        send(.failed(classified))
    }

    /// The HTTP status of the error log's newest entry, if it is one.
    private func readErrorLog() {
        guard let event = item?.errorLog()?.events.last else { return }
        if (400..<600).contains(event.errorStatusCode) {
            lastHTTPStatus = event.errorStatusCode
        } else if let comment = event.errorComment, let status = Self.httpStatus(in: comment) {
            lastHTTPStatus = status
        }
    }

    // MARK: Classifying

    /// `error` as the recovery ladder reads it. AVFoundation says little
    /// about HTTP: the error log's status, or "HTTP 404" in a description,
    /// is how a playlist or segment that expired shows.
    nonisolated static func classify(_ error: NSError?, httpStatus known: Int?, hls: Bool) -> EngineError {
        let chain = errorChain(error)
        let detail = chain.map { "\($0.domain) \($0.code)" }.joined(separator: " / ")
        let status = known ?? chain.lazy.compactMap { e in
            httpStatus(in: e.localizedDescription) ?? (e.userInfo[NSLocalizedFailureReasonErrorKey] as? String)
                .flatMap(httpStatus(in:))
        }.first ?? chain.lazy.compactMap { $0.domain == "CoreMediaErrorDomain" ? coreMediaHTTP[$0.code] : nil }.first
        if let status { return .http(status: status, detail: detail) }
        if chain.contains(where: { $0.domain == NSURLErrorDomain }) { return .network(detail: detail) }
        for e in chain where e.domain == AVFoundationErrorDomain {
            switch AVError.Code(rawValue: e.code) {
            case .fileFormatNotRecognized, .operationNotSupportedForAsset, .failedToParse:
                return hls ? .playlist(detail: detail) : .unsupported(detail: detail, audio: false, container: true)
            case .decodeFailed, .decoderNotFound, .decoderTemporarilyUnavailable, .formatUnsupported,
                 .incompatibleAsset, .noCompatibleAlternatesForExternalDisplay:
                return .unsupported(detail: detail, audio: false, container: false)
            case .serverIncorrectlyConfigured:
                return .network(detail: detail)
            default:
                continue
            }
        }
        // Anything else: a stream's playlist that can't be read, or a file this device can't open.
        return hls ? .playlist(detail: detail) : .unsupported(detail: detail, audio: false, container: false)
    }

    /// CoreMedia's codes for an HTTP refusal of a playlist or segment.
    private nonisolated static let coreMediaHTTP: [Int: Int] = [-12938: 404, -12660: 403, -12939: 401]

    private nonisolated static func errorChain(_ error: NSError?) -> [NSError] {
        var chain: [NSError] = []
        var next = error
        while let e = next, chain.count < 8 {
            chain.append(e)
            next = e.userInfo[NSUnderlyingErrorKey] as? NSError
        }
        return chain
    }

    nonisolated static func httpStatus(in text: String) -> Int? {
        guard let match = text.firstMatch(of: /HTTP (\d{3})/), let status = Int(match.1),
              (400..<600).contains(status)
        else { return nil }
        return status
    }

    private static func ms(_ time: CMTime) -> Int? {
        guard time.isNumeric, time.seconds.isFinite else { return nil }
        return Int((time.seconds * 1000).rounded())
    }
}
