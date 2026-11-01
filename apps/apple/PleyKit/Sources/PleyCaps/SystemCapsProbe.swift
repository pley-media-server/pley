import AVFoundation
import CoreMedia
import Foundation
import VideoToolbox

/// Reads what the device plays from the system.
public protocol CapsProbe: Sendable {
    /// `screen` is the screen the player's window is on, when the app knows
    /// it; without it the probe takes the device's own screen.
    func read(screen: ScreenFacts?) async -> ProbeInput
}

/// The real probe: VideoToolbox, AVFoundation, the audio route and the
/// screen, a file per platform (`SystemCapsProbe+iOS.swift`, `+tvOS`,
/// `+macOS`). It reads UIKit and AppKit, so it runs on the main actor.
public struct SystemCapsProbe: CapsProbe {
    public init() {}

    public func read(screen: ScreenFacts?) async -> ProbeInput {
        await readPlatform(screen: screen)
    }
}

/// What every platform reads alike.
enum SystemFacts {
    /// What VideoToolbox decodes in hardware. VP9's decoder is supplemental:
    /// it is registered first where the system can (macOS 11, iOS and tvOS
    /// 26.2), and reported as not decoded below that.
    static func hardwareDecode() -> [String: Bool] {
        var vp9 = false
        #if os(macOS)
        VTRegisterSupplementalVideoDecoderIfAvailable(kCMVideoCodecType_VP9)
        vp9 = VTIsHardwareDecodeSupported(kCMVideoCodecType_VP9)
        #else
        if #available(iOS 26.2, tvOS 26.2, *) {
            VTRegisterSupplementalVideoDecoderIfAvailable(kCMVideoCodecType_VP9)
            vp9 = VTIsHardwareDecodeSupported(kCMVideoCodecType_VP9)
        }
        #endif
        return [
            "hevc": VTIsHardwareDecodeSupported(kCMVideoCodecType_HEVC),
            "av1": VTIsHardwareDecodeSupported(kCMVideoCodecType_AV1),
            "vp9": vp9,
        ]
    }

    /// `AVPlayer.eligibleForHDRPlayback`.
    static var hdrEligible: Bool { AVPlayer.eligibleForHDRPlayback }

    /// "26.1", or "16.7.10" when the patch isn't 0.
    static var osVersion: String {
        let v = ProcessInfo.processInfo.operatingSystemVersion
        return v.patchVersion == 0 ? "\(v.majorVersion).\(v.minorVersion)"
            : "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)"
    }

    static var memoryBytes: UInt64 { ProcessInfo.processInfo.physicalMemory }

    /// The simulated device's model identifier, in a simulator.
    static var simulatorModel: String? {
        ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"]
    }

    /// The model identifier: "iPhone15,2", "AppleTV14,1"; the simulated
    /// device's in a simulator.
    static var model: String {
        if let simulated = simulatorModel, !simulated.isEmpty { return simulated }
        #if os(macOS)
        return sysctl("hw.model") ?? "Mac"
        #else
        return sysctl("hw.machine") ?? "unknown"
        #endif
    }

    static var simulator: Bool {
        #if targetEnvironment(simulator)
        true
        #else
        false
        #endif
    }

    /// A string `sysctl` value, such as `hw.model`.
    static func sysctl(_ name: String) -> String? {
        var size = 0
        guard sysctlbyname(name, nil, &size, nil, 0) == 0, size > 0, size < 1024 else { return nil }
        var bytes = [CChar](repeating: 0, count: size)
        guard sysctlbyname(name, &bytes, &size, nil, 0) == 0 else { return nil }
        let text = String(decoding: bytes.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
        return text.isEmpty ? nil : text
    }
}

#if os(iOS) || os(tvOS)
/// `AVPlayer.availableHDRModes`: deprecated in iOS and tvOS 26, but the only
/// API that tells Dolby Vision apart below 26, and still answering in 26.
/// It is read in this one place, through a protocol so that the deprecation
/// is said once rather than at every use.
protocol HDRModesReading {
    var modes: [String] { get }
}

struct DeprecatedHDRModes: HDRModesReading {
    @available(iOS, deprecated: 26.0, message: "availableHDRModes is the only way to tell Dolby Vision apart")
    @available(tvOS, deprecated: 26.0, message: "availableHDRModes is the only way to tell Dolby Vision apart")
    var modes: [String] {
        let modes = AVPlayer.availableHDRModes
        var names: [String] = []
        if modes.contains(.hdr10) { names.append("hdr10") }
        if modes.contains(.hlg) { names.append("hlg") }
        if modes.contains(.dolbyVision) { names.append("dolby_vision") }
        return names
    }

    static func read() -> [String] {
        (DeprecatedHDRModes() as any HDRModesReading).modes
    }
}
#endif
