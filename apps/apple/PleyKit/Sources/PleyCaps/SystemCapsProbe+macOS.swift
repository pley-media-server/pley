#if os(macOS)
import AppKit
import CoreAudio

extension SystemCapsProbe {
    @MainActor
    func readPlatform(screen: ScreenFacts?) async -> ProbeInput {
        let screenFacts = screen ?? NSScreen.main.map(ScreenFacts.init)
        return ProbeInput(
            platform: .macOS,
            osVersion: SystemFacts.osVersion,
            model: SystemFacts.model,
            chip: SystemFacts.sysctl("machdep.cpu.brand_string"),
            memoryBytes: SystemFacts.memoryBytes,
            simulator: false,
            hardwareDecode: SystemFacts.hardwareDecode(),
            // The Mac has no list of HDR modes: the window's screen decides.
            hdr: HDRFacts(eligible: SystemFacts.hdrEligible, edrHeadroom: screenFacts?.edrHeadroom),
            display: DisplayFacts(width: screenFacts?.width ?? 0, height: screenFacts?.height ?? 0,
                                  maxFps: screenFacts?.maxFps ?? 60),
            audio: CoreAudioFacts.read())
    }
}

extension ScreenFacts {
    /// An AppKit screen's size in pixels, its fastest rate, and how much
    /// brighter than SDR's white it can go (above 1 on XDR and HDR screens).
    @MainActor
    public init(_ screen: NSScreen) {
        let scale = screen.backingScaleFactor
        self.init(width: Int((screen.frame.width * scale).rounded()), height: Int((screen.frame.height * scale).rounded()),
                  maxFps: screen.maximumFramesPerSecond,
                  edrHeadroom: Double(screen.maximumPotentialExtendedDynamicRangeColorComponentValue))
    }
}

/// The default output device, from Core Audio: AVAudioSession doesn't
/// exist on the Mac.
enum CoreAudioFacts {
    static func read() -> AudioFacts {
        guard let device = defaultOutput() else { return AudioFacts(maxChannels: 2, route: AudioFacts.other) }
        return AudioFacts(maxChannels: channels(device), route: route(device))
    }

    static func defaultOutput() -> AudioObjectID? {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultOutputDevice,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var device = AudioObjectID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        let status = AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size,
                                                &device)
        return status == noErr && device != kAudioObjectUnknown ? device : nil
    }

    /// The output streams' channels, added up.
    static func channels(_ device: AudioObjectID) -> Int {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreamConfiguration,
                                                 mScope: kAudioObjectPropertyScopeOutput,
                                                 mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(device, &address, 0, nil, &size) == noErr, size > 0 else { return 2 }
        let raw = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { raw.deallocate() }
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, raw) == noErr else { return 2 }
        let list = UnsafeMutableAudioBufferListPointer(raw.assumingMemoryBound(to: AudioBufferList.self))
        return list.reduce(0) { $0 + Int($1.mNumberChannels) }
    }

    static func route(_ device: AudioObjectID) -> String {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyTransportType,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var transport: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &transport) == noErr else {
            return AudioFacts.other
        }
        switch transport {
        case kAudioDeviceTransportTypeBuiltIn: return AudioFacts.speaker
        case kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE: return AudioFacts.bluetooth
        case kAudioDeviceTransportTypeHDMI, kAudioDeviceTransportTypeDisplayPort: return AudioFacts.hdmi
        case kAudioDeviceTransportTypeAirPlay: return AudioFacts.airplay
        case kAudioDeviceTransportTypeUSB: return AudioFacts.usb
        default: return AudioFacts.other
        }
    }
}
#endif
