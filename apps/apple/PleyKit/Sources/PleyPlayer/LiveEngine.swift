import AVFoundation
import Foundation

/// What a live engine says as it plays a channel.
public enum LiveEngineEvent: Sendable, Equatable {
    /// Frames (or sound) move.
    case playing
    /// It wants to play and waits for data.
    case waiting
    /// Something paused it that wasn't the app (a headset, a stray command):
    /// live plays on.
    case paused
    /// The picture's size became known, or went: whether the channel shows a picture.
    case video(Bool)
    case failed(EngineError)
}

/// The player under a live playback: AVPlayer on device (`AVLiveEngine`),
/// a fake in tests. One channel at a time, from its live edge; no seeking.
@MainActor
public protocol LiveEngine: AnyObject {
    var events: AsyncStream<LiveEngineEvent> { get }
    /// Opens `url` (a keyed live HLS playlist) and plays it.
    func load(url: URL)
    func play()
    /// Whether what plays has a picture: a visual track, or a size.
    func hasVideo() -> Bool
    /// Lets go of the channel and the player; the engine isn't used again.
    func close()
}

/// AVPlayer as a live playback uses it: one `AVPlayer` for the engine's
/// life (`LiveVideoSurface` shows it), an `AVPlayerItem` per channel.
///
/// - The address is the keyed playlist the server gave
///   (`PleyAPI.liveMediaURL`): no token, no header.
/// - AVPlayer's own updates of the system's Now Playing stay off: live
///   has no transport for the system to offer.
/// - Every KVO watch is invalidated in `close()` (and per item when the
///   next channel loads), before the player or item is let go.
@MainActor
public final class AVLiveEngine: LiveEngine {
    public let player: AVPlayer
    public let events: AsyncStream<LiveEngineEvent>

    private let continuation: AsyncStream<LiveEngineEvent>.Continuation
    private var item: AVPlayerItem?
    private var itemWatches: [NSKeyValueObservation] = []
    private var playerWatch: NSKeyValueObservation?
    private var observers: [any NSObjectProtocol] = []
    private var lastHTTPStatus: Int?
    private var closed = false

    public init() {
        player = AVPlayer()
        player.automaticallyWaitsToMinimizeStalling = true
        #if os(iOS) || os(tvOS)
        player.allowsExternalPlayback = true
        #endif
        (events, continuation) = AsyncStream.makeStream(of: LiveEngineEvent.self, bufferingPolicy: .bufferingNewest(32))
        playerWatch = player.observe(\.timeControlStatus, options: [.new]) { [weak self] _, _ in
            Task { @MainActor in self?.statusChanged() }
        }
    }

    public func load(url: URL) {
        guard !closed else { return }
        detach()
        lastHTTPStatus = nil
        let item = AVPlayerItem(asset: AVURLAsset(url: url))
        // Close to the live edge, as a channel is watched as it airs.
        item.automaticallyPreservesTimeOffsetFromLive = true
        self.item = item
        itemWatches = [
            item.observe(\.status, options: [.new]) { [weak self] _, _ in
                Task { @MainActor in self?.itemStatusChanged() }
            },
            item.observe(\.presentationSize, options: [.new]) { [weak self] _, _ in
                Task { @MainActor in self?.sizeChanged() }
            },
        ]
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: AVPlayerItem.failedToPlayToEndTimeNotification, object: item, queue: .main) {
                [weak self] note in
                let error = note.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? NSError
                let detail = error?.localizedDescription ?? "the stream stopped"
                Task { @MainActor in self?.failed(detail) }
            },
            center.addObserver(forName: AVPlayerItem.newErrorLogEntryNotification, object: item, queue: .main) {
                [weak self] _ in
                Task { @MainActor in self?.readErrorLog() }
            },
        ]
        player.replaceCurrentItem(with: item)
        player.play()
    }

    public func play() {
        guard !closed, item != nil else { return }
        player.play()
    }

    public func hasVideo() -> Bool {
        guard let item else { return false }
        if item.presentationSize.width > 0 && item.presentationSize.height > 0 { return true }
        return item.tracks.contains { $0.isEnabled && $0.assetTrack?.mediaType == .video }
    }

    public func close() {
        guard !closed else { return }
        closed = true
        detach()
        playerWatch?.invalidate()
        playerWatch = nil
        player.pause()
        continuation.finish()
    }

    // MARK: Inside

    private func detach() {
        for watch in itemWatches { watch.invalidate() }
        itemWatches = []
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
        player.replaceCurrentItem(with: nil)
        item = nil
    }

    private func statusChanged() {
        guard !closed, item != nil else { return }
        switch player.timeControlStatus {
        case .playing:
            continuation.yield(.playing)
        case .waitingToPlayAtSpecifiedRate:
            continuation.yield(.waiting)
        case .paused:
            // Not the app's: it never pauses live. A failed item says so by its status.
            if item?.status != .failed { continuation.yield(.paused) }
        @unknown default:
            break
        }
    }

    private func itemStatusChanged() {
        guard let item, item.status == .failed else { return }
        failed(item.error?.localizedDescription ?? "the channel couldn't be opened")
    }

    private func sizeChanged() {
        guard let item else { return }
        continuation.yield(.video(item.presentationSize.width > 0 && item.presentationSize.height > 0))
    }

    private func readErrorLog() {
        guard let event = item?.errorLog()?.events.last else { return }
        if event.errorDomain == "CoreMediaErrorDomain", (400..<600).contains(event.errorStatusCode) {
            lastHTTPStatus = event.errorStatusCode
        }
    }

    private func failed(_ detail: String) {
        guard !closed else { return }
        if let status = lastHTTPStatus {
            continuation.yield(.failed(.http(status: status, detail: detail)))
        } else {
            continuation.yield(.failed(.network(detail: detail)))
        }
    }
}
