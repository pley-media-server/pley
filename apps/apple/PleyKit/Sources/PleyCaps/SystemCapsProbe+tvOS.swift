#if os(tvOS)
import AVFoundation
import AVKit
import UIKit

extension SystemCapsProbe {
    @MainActor
    func readPlatform(screen: ScreenFacts?) async -> ProbeInput {
        let window = Self.window()
        let uiScreen = window?.windowScene?.screen
        let size = uiScreen?.nativeBounds.size ?? .zero
        return ProbeInput(
            platform: .tvOS,
            osVersion: SystemFacts.osVersion,
            model: SystemFacts.model,
            memoryBytes: SystemFacts.memoryBytes,
            simulator: SystemFacts.simulator,
            hardwareDecode: SystemFacts.hardwareDecode(),
            // Through HDMI, these are the TV's.
            hdr: HDRFacts(eligible: SystemFacts.hdrEligible, modes: DeprecatedHDRModes.read()),
            display: DisplayFacts(width: screen?.width ?? Int(size.width), height: screen?.height ?? Int(size.height),
                                  maxFps: screen?.maxFps ?? uiScreen?.maximumFramesPerSecond ?? 60,
                                  // Match Content's frame rate, which AVKit follows.
                                  matchContent: window?.avDisplayManager.isDisplayCriteriaMatchingEnabled ?? false),
            audio: AudioSessionFacts.read())
    }

    /// The app's key window, or its first.
    @MainActor
    static func window() -> UIWindow? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let windows = scenes.flatMap(\.windows)
        return windows.first { $0.isKeyWindow } ?? windows.first
    }
}
#endif
