#if os(iOS) || os(tvOS)
import AVFoundation
import PleyCommon

/// The app's audio session while a video plays: `.playback` for movies,
/// so sound plays with the silent switch on and the system tunes its
/// output for film; given back when the player closes.
public enum PlaybackAudioSession {
    public static func activate() {
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playback, mode: .moviePlayback)
            try session.setActive(true)
        } catch {
            Log.player.error("audio session: \((error as NSError).code, privacy: .public)")
        }
    }

    public static func deactivate() {
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}
#endif
