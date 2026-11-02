import CoreGraphics
import Foundation
import MediaPlayer
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// The system's Now Playing through MediaPlayer: `MPNowPlayingInfoCenter`
/// for what shows, `MPRemoteCommandCenter` for the commands. The Mac's
/// media keys and its menu bar's Now Playing come from the same code, with
/// `playbackState` set too, which the Mac needs.
///
/// MediaPlayer calls the commands' targets and the artwork's handler on
/// queues of its own, so those closures are `@Sendable` and never the main
/// actor's: each hops to the main actor before it touches the app.
@MainActor
public final class SystemNowPlaying: NowPlayingSystem {
    private var targets: [(MPRemoteCommand, Any)] = []

    public init() {}

    public func show(_ info: NowPlayingInfo?) {
        let center = MPNowPlayingInfoCenter.default()
        guard let info else {
            center.nowPlayingInfo = nil
            #if os(macOS)
            center.playbackState = .stopped
            #endif
            return
        }
        var values: [String: Any] = [
            MPMediaItemPropertyTitle: info.title,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: Double(info.elapsedMs) / 1000,
            MPNowPlayingInfoPropertyPlaybackRate: info.rate,
            MPNowPlayingInfoPropertyDefaultPlaybackRate: 1.0,
            MPNowPlayingInfoPropertyMediaType: info.kind == .audio
                ? MPNowPlayingInfoMediaType.audio.rawValue : MPNowPlayingInfoMediaType.video.rawValue,
            MPNowPlayingInfoPropertyIsLiveStream: info.isLive,
        ]
        if let artist = info.artist { values[MPMediaItemPropertyArtist] = artist }
        if let album = info.album { values[MPMediaItemPropertyAlbumTitle] = album }
        if let duration = info.durationMs, duration > 0, !info.isLive {
            values[MPMediaItemPropertyPlaybackDuration] = Double(duration) / 1000
        }
        if let image = info.artwork {
            values[MPMediaItemPropertyArtwork] = Self.artwork(image)
        }
        center.nowPlayingInfo = values
        #if os(macOS)
        center.playbackState = info.rate > 0 ? .playing : .paused
        #endif
    }

    public func handle(_ commands: Set<RemoteCommand>, _ handler: @escaping @MainActor @Sendable (RemoteEvent) -> Void) {
        for (command, target) in targets { command.removeTarget(target) }
        targets = []
        let center = MPRemoteCommandCenter.shared()
        for kind in RemoteCommand.allCases {
            let command = Self.command(kind, in: center)
            command.isEnabled = commands.contains(kind)
            guard commands.contains(kind) else { continue }
            if kind == .skipForward || kind == .skipBackward {
                (command as? MPSkipIntervalCommand)?.preferredIntervals = [10]
            }
            let target = command.addTarget { @Sendable event in
                guard let heard = Self.event(kind, event) else { return .commandFailed }
                Task { @MainActor in handler(heard) }
                return .success
            }
            targets.append((command, target))
        }
    }

    // MARK: Inside

    private static func command(_ kind: RemoteCommand, in center: MPRemoteCommandCenter) -> MPRemoteCommand {
        switch kind {
        case .play: center.playCommand
        case .pause: center.pauseCommand
        case .toggle: center.togglePlayPauseCommand
        case .nextTrack: center.nextTrackCommand
        case .previousTrack: center.previousTrackCommand
        case .skipForward: center.skipForwardCommand
        case .skipBackward: center.skipBackwardCommand
        case .changePosition: center.changePlaybackPositionCommand
        }
    }

    /// The event MediaPlayer gave, read where it gave it.
    private nonisolated static func event(_ kind: RemoteCommand, _ event: MPRemoteCommandEvent) -> RemoteEvent? {
        switch kind {
        case .play: return .play
        case .pause: return .pause
        case .toggle: return .toggle
        case .nextTrack: return .next
        case .previousTrack: return .previous
        case .skipForward:
            return .skip(seconds: Int((event as? MPSkipIntervalCommandEvent)?.interval ?? 10))
        case .skipBackward:
            return .skip(seconds: -Int((event as? MPSkipIntervalCommandEvent)?.interval ?? 10))
        case .changePosition:
            guard let time = (event as? MPChangePlaybackPositionCommandEvent)?.positionTime else { return nil }
            return .seek(ms: Int(time * 1000))
        }
    }

    /// The picture, made into the platform's image once; MediaPlayer asks
    /// for it from its own queue.
    private static func artwork(_ image: CGImage) -> MPMediaItemArtwork {
        let size = CGSize(width: image.width, height: image.height)
        return MPMediaItemArtwork(boundsSize: size) { @Sendable _ in
            #if canImport(UIKit)
            UIImage(cgImage: image)
            #else
            NSImage(cgImage: image, size: size)
            #endif
        }
    }
}
