#if os(iOS) || os(tvOS)
import AVFoundation
import PleyCommon

/// The app's audio session on iPhone, iPad and Apple TV (the Mac has none):
///
/// - music: `.playback`, mode `.default`;
/// - video and Live TV: `.playback`, mode `.moviePlayback`;
/// - nothing open: deactivated, with `.notifyOthersOnDeactivation`, so
///   another app's sound comes back.
///
/// `.playback` plays with the silent switch on and, with the `audio`
/// background mode, on in the background. An interruption (a call, an
/// alarm) pauses music and video; its end with `.shouldResume` resumes
/// music only. Headphones pulled out (`oldDeviceUnavailable`) pause.
///
/// The notifications are heard on the poster's queue (`queue: nil`) by
/// `@Sendable` closures that read what they need there and hop to the
/// main actor.
@MainActor
public final class AudioSessionController {
    public enum Use: Equatable, Sendable {
        case music, video
    }

    /// Told to pause what plays: an interruption began, or the route went.
    public var onPause: (@MainActor () -> Void)?
    /// Told to resume music: an interruption ended and the system says to.
    public var onResumeMusic: (@MainActor () -> Void)?

    public private(set) var use: Use?
    private var observers: [any NSObjectProtocol] = []

    public init() {
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: nil) {
                @Sendable [weak self] note in
                let info = note.userInfo
                let type = (info?[AVAudioSessionInterruptionTypeKey] as? UInt)
                    .flatMap(AVAudioSession.InterruptionType.init(rawValue:))
                let options = (info?[AVAudioSessionInterruptionOptionKey] as? UInt)
                    .map(AVAudioSession.InterruptionOptions.init(rawValue:)) ?? []
                Task { @MainActor in self?.interrupted(type, resume: options.contains(.shouldResume)) }
            },
            center.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: nil) {
                @Sendable [weak self] note in
                let reason = (note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt)
                    .flatMap(AVAudioSession.RouteChangeReason.init(rawValue:))
                guard reason == .oldDeviceUnavailable else { return }
                Task { @MainActor in self?.onPause?() }
            },
        ]
    }

    /// What plays now: the session is set for it, or let go when nothing does.
    public func use(_ use: Use?) {
        guard use != self.use else { return }
        self.use = use
        let session = AVAudioSession.sharedInstance()
        do {
            switch use {
            case .music?:
                try session.setCategory(.playback, mode: .default)
                try session.setActive(true)
            case .video?:
                try session.setCategory(.playback, mode: .moviePlayback)
                try session.setActive(true)
            case nil:
                try session.setActive(false, options: .notifyOthersOnDeactivation)
            }
        } catch {
            Log.player.error("audio session: \((error as NSError).code, privacy: .public)")
        }
    }

    private func interrupted(_ type: AVAudioSession.InterruptionType?, resume: Bool) {
        switch type {
        case .began?:
            onPause?()
        case .ended?:
            if resume, use == .music { onResumeMusic?() }
        default:
            break
        }
    }
}
#endif
