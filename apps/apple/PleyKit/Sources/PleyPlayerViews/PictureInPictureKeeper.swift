#if os(iOS) || os(tvOS)
import AVKit
import UIKit

/// Keeps Picture in Picture going while the player's screen is away.
///
/// The player is a SwiftUI view: when Picture in Picture starts, the app
/// hides the full-screen player (`onStart`), so it shows behind, and
/// SwiftUI lets go of the `AVPlayerViewController`. The keeper holds it,
/// with its player and the subtitle layer inside, until Picture in
/// Picture ends:
///
/// - **Restore** (the window's button): the app shows the player again
///   (`onRestore`), which takes the kept controller back (`take(for:)`),
///   and AVKit is told the screen is back once it is in a window.
/// - **Closed** (the window's ✕): the app closes the playback (`onClosed`),
///   so its session ends.
///
/// It is every player controller's delegate, since the screen's own
/// coordinator goes with the view while Picture in Picture runs. AVKit
/// calls the delegate on the main thread.
@MainActor
public final class PictureInPictureKeeper: NSObject, AVPlayerViewControllerDelegate {
    public static let shared = PictureInPictureKeeper()

    /// Picture in Picture started: the app hides its player.
    public var onStart: @MainActor () -> Void = {}
    /// Its restore button: the app shows its player again.
    public var onRestore: @MainActor () -> Void = {}
    /// Its ✕: the app ends the playback.
    public var onClosed: @MainActor () -> Void = {}

    /// Picture in Picture runs.
    public private(set) var running = false
    /// The controller playing in Picture in Picture, held while its screen is away.
    private(set) var kept: AVPlayerViewController?
    /// AVKit waits for the screen to be back.
    private var restoring: ((Bool) -> Void)?
    private var restored = false
    private var waiting: Task<Void, Never>?

    /// Makes `controller` play in Picture in Picture through the keeper.
    func adopt(_ controller: AVPlayerViewController) {
        controller.delegate = self
        controller.allowsPictureInPicturePlayback = true
        #if os(iOS)
        // Leaving the app while it plays goes on in Picture in Picture.
        controller.canStartPictureInPictureAutomaticallyFromInline = true
        #endif
    }

    /// The controller kept for `player`, given back to the screen; any
    /// other kept controller is let go.
    func take(for player: AVPlayer?) -> AVPlayerViewController? {
        guard let kept else { return nil }
        guard player != nil, kept.player === player else {
            release()
            return nil
        }
        // Once Picture in Picture ended, SwiftUI holds it again.
        if !running { self.kept = nil }
        return kept
    }

    /// `controller` stays alive past its view: Picture in Picture shows it.
    func keeps(_ controller: AVPlayerViewController) -> Bool { running && kept === controller }

    /// The screen shows `controller` again: AVKit is told once it is in a window.
    func shown(_ controller: AVPlayerViewController) {
        guard restoring != nil, waiting == nil else { return }
        waiting = Task { [weak self, weak controller] in
            for _ in 0..<40 where controller?.viewIfLoaded?.window == nil {
                try? await Task.sleep(for: .milliseconds(25))
            }
            self?.finishRestore()
        }
    }

    /// Another playback, or none: Picture in Picture of the one before ends.
    public func playerChanged(_ player: AVPlayer?) {
        if let kept, kept.player !== player { release() }
    }

    /// Ends Picture in Picture, and lets go of the controller and its player.
    public func release() {
        waiting?.cancel()
        waiting = nil
        finishRestore()
        kept?.player = nil
        kept = nil
        running = false
    }

    private func finishRestore() {
        waiting = nil
        let done = restoring
        restoring = nil
        done?(true)
    }

    // MARK: AVPlayerViewControllerDelegate

    public nonisolated func playerViewControllerWillStartPictureInPicture(_ controller: AVPlayerViewController) {
        MainActor.assumeIsolated {
            running = true
            restored = false
            kept = controller
            onStart()
        }
    }

    public nonisolated func playerViewController(_ controller: AVPlayerViewController,
                                                 failedToStartPictureInPictureWithError error: any Error) {
        MainActor.assumeIsolated {
            running = false
            kept = nil
            onRestore()
        }
    }

    /// The app hides the player itself, so it shows behind.
    public nonisolated func playerViewControllerShouldAutomaticallyDismissAtPictureInPictureStart(
        _ controller: AVPlayerViewController) -> Bool {
        false
    }

    public nonisolated func playerViewController(
        _ controller: AVPlayerViewController,
        restoreUserInterfaceForPictureInPictureStopWithCompletionHandler completion: @escaping (Bool) -> Void) {
        // AVKit calls this on the main thread and wants the handler called there too.
        nonisolated(unsafe) let completion = completion
        MainActor.assumeIsolated {
            restored = true
            restoring = completion
            onRestore()
            // Never hidden (it stopped as it started): nothing to wait for.
            if controller.viewIfLoaded?.window != nil { finishRestore() } else { shown(controller) }
        }
    }

    public nonisolated func playerViewControllerDidStopPictureInPicture(_ controller: AVPlayerViewController) {
        MainActor.assumeIsolated {
            running = false
            guard kept === controller else { return }
            if restored {
                // Back on screen, SwiftUI holds it again; else the screen takes it when it shows.
                if controller.viewIfLoaded?.window != nil { kept = nil }
            } else {
                // Its ✕: the playback ends.
                release()
                onClosed()
            }
        }
    }
}
#endif
