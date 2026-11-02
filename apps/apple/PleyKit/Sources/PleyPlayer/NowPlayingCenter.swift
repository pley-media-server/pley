import CoreGraphics
import Foundation

/// A command the system offers for what plays: the lock screen, Control
/// Center, a headset's buttons, the Mac's media keys and its menu bar.
public enum RemoteCommand: Hashable, Sendable, CaseIterable {
    case play, pause, toggle, nextTrack, previousTrack, skipForward, skipBackward, changePosition
}

/// One of those commands, as the person used it.
public enum RemoteEvent: Equatable, Sendable {
    case play, pause, toggle, next, previous
    /// Forward (positive) or back, in seconds.
    case skip(seconds: Int)
    /// To a place, in ms.
    case seek(ms: Int)
}

/// What the system's Now Playing shows.
public struct NowPlayingInfo: Equatable, Sendable {
    public enum Kind: Sendable { case audio, video }

    public var title: String
    /// Music's artist; a video's show, or nothing.
    public var artist: String?
    /// Music's album; an episode's "S1 E2 · Title".
    public var album: String?
    public var durationMs: Int?
    public var elapsedMs: Int
    /// 1 while it plays, 0 while it doesn't.
    public var rate: Double
    public var kind: Kind
    public var isLive: Bool
    /// The album cover or the still, made beforehand.
    public var artwork: CGImage?
    /// Tells pictures apart without comparing them.
    public var artworkPath: String?

    public init(title: String, artist: String? = nil, album: String? = nil, durationMs: Int? = nil, elapsedMs: Int = 0,
                rate: Double = 0, kind: Kind, isLive: Bool = false, artwork: CGImage? = nil, artworkPath: String? = nil) {
        self.title = title
        self.artist = artist
        self.album = album
        self.durationMs = durationMs
        self.elapsedMs = elapsedMs
        self.rate = rate
        self.kind = kind
        self.isLive = isLive
        self.artwork = artwork
        self.artworkPath = artworkPath
    }

    public static func == (a: NowPlayingInfo, b: NowPlayingInfo) -> Bool {
        a.title == b.title && a.artist == b.artist && a.album == b.album && a.durationMs == b.durationMs
            && a.elapsedMs == b.elapsedMs && a.rate == b.rate && a.kind == b.kind && a.isLive == b.isLive
            && a.artworkPath == b.artworkPath && (a.artwork == nil) == (b.artwork == nil)
    }
}

/// The system's Now Playing and remote commands: MediaPlayer on device
/// (`SystemNowPlaying`), a fake in tests.
@MainActor
public protocol NowPlayingSystem: AnyObject {
    /// Shows `info`; `nil` takes Now Playing away.
    func show(_ info: NowPlayingInfo?)
    /// Turns on `commands` alone, each heard by `handler`; the ones before go.
    func handle(_ commands: Set<RemoteCommand>, _ handler: @escaping @MainActor @Sendable (RemoteEvent) -> Void)
}

/// The one owner of the system's Now Playing for music, video and Live TV:
/// whichever played last. AVKit's own updates are off
/// (`updatesNowPlayingInfoCenter = false`), so every command goes through
/// Pley's controllers: reported to the server, and refused for live.
///
/// - Music: play, pause, toggle, next and previous track, and a place.
/// - Video: play, pause, toggle, 10 s forward and back, and a place.
/// - Live TV: none (watched as it airs, as on Android).
@MainActor
public final class NowPlayingCenter {
    public enum Owner: Hashable, Sendable { case music, video, live }

    /// The commands each owner offers.
    public static func commands(for owner: Owner) -> Set<RemoteCommand> {
        switch owner {
        case .music: [.play, .pause, .toggle, .nextTrack, .previousTrack, .changePosition]
        case .video: [.play, .pause, .toggle, .skipForward, .skipBackward, .changePosition]
        case .live: []
        }
    }

    /// Who shows now; `nil`: no one.
    public private(set) var owner: Owner?
    /// What shows now.
    public private(set) var info: NowPlayingInfo?

    private let system: any NowPlayingSystem
    private var handler: (@MainActor (RemoteEvent) -> Void)?

    public init(system: any NowPlayingSystem) {
        self.system = system
    }

    /// `owner` plays now: its commands replace the ones before, each going
    /// to `handler`, and its `info` shows.
    public func take(_ owner: Owner, info: NowPlayingInfo, handler: @escaping @MainActor (RemoteEvent) -> Void) {
        let changed = self.owner != owner
        self.owner = owner
        self.handler = handler
        if changed {
            system.handle(Self.commands(for: owner)) { [weak self] event in self?.heard(event, from: owner) }
        }
        show(info)
    }

    /// What `owner` shows now; nothing when another owns Now Playing.
    public func update(_ owner: Owner, info: NowPlayingInfo) {
        guard self.owner == owner else { return }
        show(info)
    }

    /// `owner` stopped: Now Playing goes, unless another owns it.
    public func release(_ owner: Owner) {
        guard self.owner == owner else { return }
        self.owner = nil
        handler = nil
        info = nil
        system.handle([], { _ in })
        system.show(nil)
    }

    private func show(_ info: NowPlayingInfo) {
        guard info != self.info else { return }
        self.info = info
        system.show(info)
    }

    private func heard(_ event: RemoteEvent, from owner: Owner) {
        // A command of the owner before, on its way while the owner changed, is dropped.
        guard self.owner == owner else { return }
        handler?(event)
    }
}
