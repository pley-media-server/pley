import AVFoundation
import Foundation

/// Why a music output stopped.
public enum MusicOutputError: Equatable, Sendable {
    /// The device couldn't decode the track.
    case decode
    /// The session's media answered `404`: it expired.
    case gone
    /// The session's media answered `410`: the server stopped it.
    case stopped
    /// The server couldn't be reached.
    case network
    /// Anything else.
    case other
}

/// What a music output says as it plays.
public enum MusicOutputEvent: Equatable, Sendable {
    /// Sound moves (`true`), or it stopped moving.
    case playing(Bool)
    /// It wants to play and waits for data.
    case buffering(Bool)
    /// It was told to play (`true`) or to pause, by the app or by the system.
    case playWhenReady(Bool)
    /// The track played to its end.
    case ended
    case failed(MusicOutputError)
}

/// The player under music: AVPlayer on device (`AVMusicOutput`), a fake
/// in tests. One track at a time.
@MainActor
public protocol MusicOutput: AnyObject {
    var events: AsyncStream<MusicOutputEvent> { get }
    /// Where the track is, in ms.
    var positionMs: Int { get }
    /// The track's length as the media says it; 0 while unknown.
    var durationMs: Int { get }
    /// Whether it is meant to play (as opposed to paused).
    var playWhenReady: Bool { get }
    /// Opens `url` (a keyed media address) at `startMs`, playing when `play`.
    func load(url: URL, startMs: Int, play: Bool)
    func play()
    func pause()
    func seek(toMs positionMs: Int)
    /// Lets go of the track, keeping the output.
    func stop()
    /// Lets go of the player; the output isn't used again.
    func release()
}

/// AVPlayer as music uses it: one `AVPlayer` for the output's life, an
/// `AVPlayerItem` per track, AirPlay allowed.
///
/// - The address is the keyed one the server gave: no token, no header.
/// - Its time is watched on the main queue (`addPeriodicTimeObserver`
///   with `.main`, which runs the block there), for the scrubber.
/// - Every KVO watch is invalidated in `release()` (and per item when the
///   next track loads), before the player or the item is let go.
@MainActor
public final class AVMusicOutput: MusicOutput {
    public let player: AVPlayer
    public let events: AsyncStream<MusicOutputEvent>
    /// Hears the time move, twice a second while it plays.
    public var onTime: (@MainActor (Int) -> Void)?

    private let continuation: AsyncStream<MusicOutputEvent>.Continuation
    private var item: AVPlayerItem?
    private var itemWatches: [NSKeyValueObservation] = []
    private var playerWatches: [NSKeyValueObservation] = []
    private var observers: [any NSObjectProtocol] = []
    private var timeObserver: Any?
    private var lastHTTPStatus: Int?
    private var wantsPlay = false
    private var released = false

    public init() {
        player = AVPlayer()
        player.automaticallyWaitsToMinimizeStalling = true
        player.allowsExternalPlayback = true
        #if os(iOS) || os(tvOS)
        // Sound only: nothing to show on a TV over AirPlay.
        player.usesExternalPlaybackWhileExternalScreenIsActive = false
        #endif
        (events, continuation) = AsyncStream.makeStream(of: MusicOutputEvent.self, bufferingPolicy: .bufferingNewest(64))
        playerWatches = [
            player.observe(\.timeControlStatus, options: [.new]) { @Sendable [weak self] _, _ in
                Task { @MainActor in self?.statusChanged() }
            },
            player.observe(\.rate, options: [.new]) { @Sendable [weak self] _, _ in
                Task { @MainActor in self?.rateChanged() }
            },
        ]
        // `.main`: AVFoundation runs the block on the main queue, so it is the main actor's.
        timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(value: 1, timescale: 2), queue: .main) {
            [weak self] time in
            MainActor.assumeIsolated {
                guard let self, !self.released, time.isNumeric else { return }
                self.onTime?(Int(time.seconds * 1000))
            }
        }
    }

    public var positionMs: Int {
        let time = player.currentTime()
        return time.isNumeric ? max(Int(time.seconds * 1000), 0) : 0
    }

    public var durationMs: Int {
        guard let duration = item?.duration, duration.isNumeric, duration.seconds.isFinite else { return 0 }
        return max(Int(duration.seconds * 1000), 0)
    }

    public var playWhenReady: Bool { wantsPlay }

    public func load(url: URL, startMs: Int, play: Bool) {
        guard !released else { return }
        detach()
        lastHTTPStatus = nil
        let item = AVPlayerItem(asset: AVURLAsset(url: url))
        self.item = item
        itemWatches = [
            item.observe(\.status, options: [.new]) { @Sendable [weak self] _, _ in
                Task { @MainActor in self?.itemStatusChanged() }
            },
        ]
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: AVPlayerItem.didPlayToEndTimeNotification, object: item, queue: .main) {
                [weak self] _ in
                Task { @MainActor in self?.finished(item) }
            },
            center.addObserver(forName: AVPlayerItem.failedToPlayToEndTimeNotification, object: item, queue: .main) {
                [weak self] note in
                let error = note.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? NSError
                let code = error?.code
                Task { @MainActor in self?.failed(code: code) }
            },
            center.addObserver(forName: AVPlayerItem.newErrorLogEntryNotification, object: item, queue: .main) {
                [weak self] _ in
                Task { @MainActor in self?.readErrorLog() }
            },
        ]
        player.replaceCurrentItem(with: item)
        if startMs > 0 {
            player.seek(to: CMTime(value: CMTimeValue(startMs), timescale: 1000), toleranceBefore: .zero,
                        toleranceAfter: .zero)
        }
        setWants(play)
        if play { player.play() } else { player.pause() }
    }

    public func play() {
        guard !released, item != nil else { return }
        setWants(true)
        player.play()
    }

    public func pause() {
        guard !released else { return }
        setWants(false)
        player.pause()
    }

    public func seek(toMs positionMs: Int) {
        guard !released, item != nil else { return }
        player.seek(to: CMTime(value: CMTimeValue(max(positionMs, 0)), timescale: 1000), toleranceBefore: .zero,
                    toleranceAfter: .zero)
    }

    public func stop() {
        guard !released else { return }
        setWants(false)
        player.pause()
        detach()
    }

    public func release() {
        guard !released else { return }
        released = true
        detach()
        for watch in playerWatches { watch.invalidate() }
        playerWatches = []
        if let timeObserver { player.removeTimeObserver(timeObserver) }
        timeObserver = nil
        player.pause()
        continuation.finish()
    }

    // MARK: Inside

    private func setWants(_ play: Bool) {
        guard play != wantsPlay else { return }
        wantsPlay = play
        continuation.yield(.playWhenReady(play))
    }

    private func detach() {
        for watch in itemWatches { watch.invalidate() }
        itemWatches = []
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
        player.replaceCurrentItem(with: nil)
        item = nil
    }

    private func statusChanged() {
        guard !released, item != nil else { return }
        switch player.timeControlStatus {
        case .playing:
            continuation.yield(.buffering(false))
            continuation.yield(.playing(true))
        case .waitingToPlayAtSpecifiedRate:
            continuation.yield(.playing(false))
            continuation.yield(.buffering(true))
        case .paused:
            continuation.yield(.buffering(false))
            continuation.yield(.playing(false))
        @unknown default:
            break
        }
    }

    /// The system paused or played it (an interruption, AirPlay, a route
    /// change): what it is meant to do follows.
    private func rateChanged() {
        guard !released, item != nil else { return }
        if player.rate == 0, wantsPlay, player.timeControlStatus == .paused, item?.status != .failed,
           !atEnd {
            setWants(false)
        } else if player.rate > 0, !wantsPlay {
            setWants(true)
        }
    }

    private var atEnd: Bool {
        let duration = durationMs
        return duration > 0 && positionMs >= duration - 250
    }

    private func itemStatusChanged() {
        guard let item, item.status == .failed else { return }
        failed(code: (item.error as? NSError)?.code)
    }

    private func finished(_ ended: AVPlayerItem) {
        guard !released, ended === item else { return }
        continuation.yield(.ended)
    }

    private func readErrorLog() {
        guard let event = item?.errorLog()?.events.last else { return }
        if event.errorDomain == "CoreMediaErrorDomain", (400..<600).contains(event.errorStatusCode) {
            lastHTTPStatus = event.errorStatusCode
        }
    }

    private func failed(code: Int?) {
        guard !released else { return }
        let error: MusicOutputError
        switch lastHTTPStatus {
        case 404?: error = .gone
        case 410?: error = .stopped
        case let status? where status >= 400: error = .other
        default:
            if let code, Self.networkCodes.contains(code) {
                error = .network
            } else if let code, Self.decodeCodes.contains(code) {
                error = .decode
            } else {
                error = .other
            }
        }
        continuation.yield(.failed(error))
    }

    /// NSURLError's codes for a server out of reach.
    static let networkCodes: Set<Int> = [NSURLErrorNotConnectedToInternet, NSURLErrorCannotConnectToHost,
                                         NSURLErrorTimedOut, NSURLErrorNetworkConnectionLost,
                                         NSURLErrorCannotFindHost, NSURLErrorDNSLookupFailed]
    /// AVFoundation's codes for media it can't decode or open.
    static let decodeCodes: Set<Int> = [AVError.Code.decoderNotFound.rawValue, AVError.Code.decodeFailed.rawValue,
                                        AVError.Code.fileFormatNotRecognized.rawValue,
                                        AVError.Code.failedToParse.rawValue,
                                        AVError.Code.contentIsUnavailable.rawValue,
                                        AVError.Code.operationNotSupportedForAsset.rawValue,
                                        AVError.Code.formatUnsupported.rawValue]
}
