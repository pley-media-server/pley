import Foundation
import PleyModel

/// Turns what the probe read into the report the server decides from.
/// Pure: the same input gives the same report, on a device or in a test.
///
/// The device class gives each codec's limits; VideoToolbox can only take a
/// codec away. HDR is reported only when AVPlayer says the device is
/// eligible for it, since an HLS stream whose only variant is PQ doesn't
/// open on an SDR screen.
public enum CapsBuilder {
    /// Containers AVPlayer opens: no Matroska, and no MPEG-TS file.
    public static let containers = ["mp4", "m4v", "mov", "m4a", "mp3", "flac", "wav"]
    /// Audio AVFoundation decodes, by ffprobe's names. No TrueHD, no DTS: the
    /// server converts them.
    public static let audioDecode = ["aac", "mp3", "alac", "flac", "opus", "ac3", "eac3"]
    /// Subtitles the app draws itself: text from the server's WebVTT, and
    /// pictures (PGS, VobSub, DVB) from its `.mks` of the track.
    public static let subtitles = ["subrip", "ass", "ssa", "webvtt", "mov_text", "hdmv_pgs_subtitle", "dvd_subtitle",
                                   "dvb_subtitle"]
    /// fMP4 first: Apple's players take HEVC, HDR and Dolby Vision in HLS
    /// only from fMP4 segments, and the server sends them only to players
    /// that put it first.
    public static let hls = HlsCaps(masterPlaylist: true, segments: ["fmp4", "ts"], variants: true)
    /// The rates an Apple TV switches the TV to when Match Content's frame
    /// rate is on.
    public static let matchedRates: [Double] = [23.976, 24, 25, 29.97, 30, 50, 59.94, 60]
    /// The Dolby Vision profiles AVPlayer decodes: 5 and 8 (8.1, 8.4).
    /// Never 7, whose two layers it doesn't decode: the server makes it 8.1.
    public static let dolbyVisionProfiles = [5, 8]

    public static func build(_ input: ProbeInput) -> CapsSnapshot {
        let deviceClass = DeviceClasses.match(input)
        let video = videoEntries(deviceClass, input.hardwareDecode)
        let hdrTypes = hdrTypes(input, deviceClass, video: video)
        let dolbyVision = hdrTypes.contains(.dolbyVision)
        let dvProfiles = dolbyVision
            ? dolbyVisionProfiles.map { DolbyVisionCaps(profile: $0, maxLevel: deviceClass.dolbyVisionMaxLevel) }
            : []
        let report = ClientCaps(
            containers: containers,
            videoCodecs: unique(video.map(\.codec)),
            audioCodecs: audioDecode,
            hdr: hdrTypes.contains(.hdr10) || hdrTypes.contains(.hlg),
            dolbyVision: dolbyVision && dvProfiles.contains { $0.profile == 5 },
            capsVersion: ClientCaps.version2,
            video: video,
            hdrTypes: hdrTypes,
            dolbyVisionProfiles: dvProfiles,
            audioDecode: audioDecode,
            // Apple decodes everything itself, and sends PCM (or E-AC-3
            // with Atmos, made by the system) to a receiver.
            audioPassthrough: [],
            audioChannels: channels(input),
            subtitleCodecs: subtitles,
            // Only MP4 and MOV play as they are, and AVPlayer switches their
            // audio tracks.
            audioTrackSwitching: true,
            pictureSubtitlesSideload: true,
            hls: hls,
            display: display(input),
            device: DeviceCapsInfo(
                form: form(input.platform),
                tier: DeviceClasses.tier(deviceClass.tier, memoryBytes: input.memoryBytes),
                fireTv: false,
                model: input.model))
        return CapsSnapshot(report: report, deviceClass: deviceClass.id, lines: CapsLines.lines(report, input))
    }

    /// The class's entries VideoToolbox agrees with: H.264 always, HEVC and
    /// AV1 when decoded in hardware. VP9 has no class entry: it is added
    /// when VideoToolbox decodes it in hardware (after its supplemental
    /// decoder is registered), with the HEVC entry's size.
    static func videoEntries(_ deviceClass: DeviceClass, _ hardware: [String: Bool]) -> [VideoDecoderCaps] {
        var entries = deviceClass.video.filter { $0.codec == "h264" || hardware[$0.codec] == true }
        if hardware["vp9"] == true {
            let size = entries.first { $0.codec == "hevc" } ?? entries.first { $0.codec == "h264" }
            entries.append(VideoDecoderCaps(codec: "vp9", hardware: true, profiles: ["Profile 0", "Profile 2"],
                                            maxWidth: size?.maxWidth, maxHeight: size?.maxHeight,
                                            maxFps: size?.maxFps, maxBitDepth: 10))
        }
        return entries
    }

    /// The dynamic ranges reported: none unless AVPlayer is eligible for
    /// HDR, and none without a 10-bit HEVC decoder to feed the screen.
    /// Never HDR10+, which AVPlayer shows as HDR10.
    static func hdrTypes(_ input: ProbeInput, _ deviceClass: DeviceClass,
                         video: [VideoDecoderCaps]) -> [HDRType] {
        guard input.hdr.eligible else { return [] }
        guard video.contains(where: { $0.codec == "hevc" && $0.profiles.contains("Main 10") }) else { return [] }
        let dolbyVision = deviceClass.dolbyVisionMaxLevel != nil
        let shown: [HDRType]
        if input.platform == .macOS {
            // The Mac has no list of modes: the window's screen must show
            // more than SDR's white, and Dolby Vision needs Apple silicon.
            guard (input.hdr.edrHeadroom ?? 1) > 1 else { return [] }
            shown = dolbyVision ? [.hdr10, .hlg, .dolbyVision] : [.hdr10, .hlg]
        } else if input.hdr.modes.isEmpty {
            // Eligible, but the deprecated list said nothing.
            shown = dolbyVision ? [.hdr10, .hlg, .dolbyVision] : [.hdr10, .hlg]
        } else {
            let modes = Set(input.hdr.modes.map(HDRType.init(rawValue:)))
            shown = [HDRType.hdr10, .hlg, .dolbyVision].filter {
                modes.contains($0) && ($0 != .dolbyVision || dolbyVision)
            }
        }
        return shown
    }

    /// The channels the output takes, 2 to 8. Spatial audio renders 5.1;
    /// Bluetooth, headphones and the built-in speakers take stereo.
    static func channels(_ input: ProbeInput) -> Int {
        let audio = input.audio
        let channels: Int
        if audio.spatial {
            channels = 6
        } else if [AudioFacts.bluetooth, AudioFacts.headphones, AudioFacts.speaker].contains(audio.route) {
            channels = min(2, audio.maxChannels)
        } else {
            channels = audio.maxChannels
        }
        return min(max(channels, 2), 8)
    }

    /// The screen, and the rates it is switched to: on Apple TV with Match
    /// Content the usual film and TV rates, elsewhere its own.
    static func display(_ input: ProbeInput) -> DisplayCaps? {
        let d = input.display
        guard d.width > 0, d.height > 0 else { return nil }
        let rates = input.platform == .tvOS && d.matchContent ? matchedRates : [Double(d.maxFps)]
        return DisplayCaps(width: d.width, height: d.height, refreshRates: rates)
    }

    static func form(_ platform: ProbeInput.Platform) -> DeviceForm {
        switch platform {
        case .iOS: .phone
        case .iPadOS: .tablet
        case .tvOS: .tv
        case .macOS: .desktop
        }
    }

    static func unique(_ list: [String]) -> [String] {
        var seen = Set<String>()
        return list.filter { seen.insert($0).inserted }
    }
}
