import Foundation

/// Why the engine couldn't go on.
public enum EngineError: Error, Sendable, Equatable {
    /// The media couldn't be opened or decoded here: its format, its video
    /// (`audio` and `container` false) or only its audio or container.
    case unsupported(detail: String, audio: Bool, container: Bool)
    /// A playlist, segment or file answered with an HTTP error.
    case http(status: Int, detail: String)
    /// A playlist that couldn't be read.
    case playlist(detail: String)
    /// The server couldn't be reached, after the engine's own retries.
    case network(detail: String)
    /// Anything else, which a new session doesn't help with.
    case other(detail: String)
}

/// What the engine says as it plays one media.
public enum EngineEvent: Sendable, Equatable {
    /// The media opened and can play.
    case ready
    /// It played to its end.
    case ended
    case failed(EngineError)
    /// Someone played or paused with the system's own controls (or the
    /// system paused it: a call, headphones unplugged).
    case playRequested
    case pauseRequested
    /// The time moved by a jump: a seek, the system's scrubber or skip buttons.
    case jumped
    /// Someone chose another audio track in the system's own menu (a file
    /// played as it is, with several tracks).
    case audioChanged(EngineAudioOption)
}

/// The engine's state at one moment, read every 500 ms.
public struct EngineSample: Sendable, Equatable {
    public var positionMs: Int = 0
    /// 0 while unknown.
    public var durationMs: Int = 0
    /// Where the data loaded around the position ends.
    public var bufferedMs: Int = 0
    /// The media opened and can play.
    public var ready = false
    /// Frames move.
    public var playing = false
    /// It wants to play and waits for data (AVPlayer waits to minimize
    /// stalls, or the item stalled and hasn't played since).
    public var waiting = false
    /// Video frames decoded so far for this media; `nil` when the engine can't say.
    public var videoFrames: Int?
    /// The size of the picture shown; 0×0 while none shows (or no video).
    public var presentationWidth = 0
    public var presentationHeight = 0
    /// What the connection carried lately, kbps (the access log's last
    /// `observedBitrate`); `nil` while unknown.
    public var bandwidthKbps: Int?
    /// Video frames dropped so far, and seconds watched, for the struggle
    /// meter (the access log's `numberOfDroppedVideoFrames` and
    /// `durationWatched`, over this media's events).
    public var droppedFrames = 0
    public var watchedSeconds = 0.0

    public init() {}
}

/// The player underneath the controller: AVPlayer on device
/// (`AVPlayerEngine`), a fake in tests. One media at a time; its events
/// come on `events`, those of a media replaced since never.
@MainActor
public protocol PlayerEngine: AnyObject {
    var events: AsyncStream<EngineEvent> { get }
    /// Loads `url`, at `startMs` exactly; plays as soon as it can when `play`.
    func load(url: URL, startMs: Int, play: Bool)
    func play()
    func pause()
    func seek(toMs positionMs: Int)
    func sample() -> EngineSample
    /// Where the playhead is, in ms: what `sample()` says, read cheaply (the subtitles ask often).
    func positionMs() -> Int
    /// The largest picture the stream should play at, in pixels (AVPlayer's
    /// `preferredMaximumResolution`, for a stream in several sizes); 0×0 for no limit.
    func setMaximumResolution(width: Int, height: Int)
    /// The media's own audio tracks AVPlayer can switch between (a file
    /// played as it is), known once it is ready; empty for a stream.
    func audioOptions() -> [EngineAudioOption]
    /// Plays `option`'s audio; false when the media has no such option.
    @discardableResult
    func selectAudio(_ option: EngineAudioOption) -> Bool
    /// Lets go of the media and the player.
    func close()
}

extension PlayerEngine {
    public func positionMs() -> Int { sample().positionMs }
}
