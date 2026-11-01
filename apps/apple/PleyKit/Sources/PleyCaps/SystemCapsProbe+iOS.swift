#if os(iOS)
import AVFoundation
import UIKit

extension SystemCapsProbe {
    @MainActor
    func readPlatform(screen: ScreenFacts?) async -> ProbeInput {
        let device = UIDevice.current
        let screenFacts = screen ?? Self.sceneScreen()
        return ProbeInput(
            platform: device.userInterfaceIdiom == .pad ? .iPadOS : .iOS,
            osVersion: SystemFacts.osVersion,
            model: SystemFacts.model,
            memoryBytes: SystemFacts.memoryBytes,
            simulator: SystemFacts.simulator,
            hardwareDecode: SystemFacts.hardwareDecode(),
            hdr: HDRFacts(eligible: SystemFacts.hdrEligible, modes: DeprecatedHDRModes.read(),
                          edrHeadroom: screenFacts?.edrHeadroom),
            display: DisplayFacts(width: screenFacts?.width ?? 0, height: screenFacts?.height ?? 0,
                                  maxFps: screenFacts?.maxFps ?? 60),
            audio: AudioSessionFacts.read())
    }

    /// The screen of the app's first window scene, in pixels.
    @MainActor
    static func sceneScreen() -> ScreenFacts? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        guard let screen = (scenes.first { $0.activationState == .foregroundActive } ?? scenes.first)?.screen else {
            return nil
        }
        return ScreenFacts(screen)
    }
}

extension ScreenFacts {
    /// A UIKit screen's size in pixels, its fastest rate and how much
    /// brighter than SDR's white it goes.
    @MainActor
    public init(_ screen: UIScreen) {
        let size = screen.nativeBounds.size
        self.init(width: Int(size.width), height: Int(size.height), maxFps: screen.maximumFramesPerSecond,
                  edrHeadroom: Double(screen.potentialEDRHeadroom))
    }
}
#endif

#if os(iOS) || os(tvOS)
import AVFoundation

/// The audio route, from `AVAudioSession`. The category is set to playback
/// first, as the player sets it, since the channels depend on it; the
/// session isn't activated.
enum AudioSessionFacts {
    @MainActor
    static func read() -> AudioFacts {
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .moviePlayback)
        let outputs = session.currentRoute.outputs
        let spatial = outputs.contains { $0.isSpatialAudioEnabled }
        return AudioFacts(maxChannels: session.maximumOutputNumberOfChannels, spatial: spatial,
                          route: outputs.first.map { route($0.portType) } ?? AudioFacts.other)
    }

    static func route(_ port: AVAudioSession.Port) -> String {
        switch port {
        case .builtInSpeaker, .builtInReceiver: AudioFacts.speaker
        case .headphones, .lineOut: AudioFacts.headphones
        case .bluetoothA2DP, .bluetoothLE, .bluetoothHFP: AudioFacts.bluetooth
        case .HDMI: AudioFacts.hdmi
        case .airPlay: AudioFacts.airplay
        case .usbAudio: AudioFacts.usb
        default: AudioFacts.other
        }
    }
}
#endif
