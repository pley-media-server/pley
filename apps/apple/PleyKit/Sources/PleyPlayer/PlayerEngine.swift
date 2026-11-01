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
    /// It wants to play and waits for data.
    public var waiting = false
    /// Video frames decoded so far for this media; `nil` when the engine can't say.
    public var videoFrames: Int?
    /// The size of the picture shown; 0×0 while none shows (or no video).
    public var presentationWidth = 0
    public var presentationHeight = 0
    /// What the connection carried lately, kbps; `nil` while unknown.
    public var bandwidthKbps: Int?
    /// Video frames dropped so far, and seconds watched, for the struggle meter.
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
    /// Lets go of the media and the player.
    func close()
}
