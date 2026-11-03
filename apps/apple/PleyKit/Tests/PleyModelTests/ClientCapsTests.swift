import Foundation
import PleyModel
import Testing

@Suite("The caps report is written as the server reads it")
struct ClientCapsTests {
    private func object(_ caps: ClientCaps) throws -> [String: Any] {
        try JSONSerialization.jsonObject(with: PleyJSON.encoder.encode(caps)) as? [String: Any] ?? [:]
    }

    /// apps/server/src/playback.rs `Caps`, version 1: five fields.
    @Test func versionOneAloneLeavesTheRestOut() throws {
        let v1 = ClientCaps(containers: ["mp4"], videoCodecs: ["h264"], audioCodecs: ["aac"], hdr: true)
        #expect(String(decoding: try PleyJSON.encoder.encode(v1), as: UTF8.self)
            == #"{"audio_codecs":["aac"],"containers":["mp4"],"dolby_vision":false,"hdr":true,"video_codecs":["h264"]}"#)
    }

    @Test func versionTwoTravelsWithVersionOne() throws {
        let v2 = ClientCaps(
            containers: ["mp4"], videoCodecs: ["hevc"], audioCodecs: ["aac"], hdr: true, capsVersion: 2,
            video: [VideoDecoderCaps(codec: "hevc", hardware: true, profiles: ["Main", "Main 10"], maxWidth: 3840,
                                     maxHeight: 2160, maxFps: 60, maxBitDepth: 10, maxLevel: 156)],
            hdrTypes: [.hdr10, .dolbyVision], dolbyVisionProfiles: [DolbyVisionCaps(profile: 8, maxLevel: 9)],
            audioDecode: ["aac"], audioPassthrough: [], audioChannels: 6, subtitleCodecs: ["subrip"],
            audioTrackSwitching: true,
            hls: HlsCaps(masterPlaylist: true, segments: ["fmp4", "ts"], variants: true),
            display: DisplayCaps(width: 3840, height: 2160, refreshRates: [23.976, 60]),
            device: DeviceCapsInfo(form: .tv, tier: "mid", model: "AppleTV14,1"))
        let body = try object(v2)
        #expect(Set(body.keys) == [
            "containers", "video_codecs", "audio_codecs", "hdr", "dolby_vision", "caps_version", "video", "hdr_types",
            "dolby_vision_profiles", "audio_decode", "audio_passthrough", "audio_channels", "subtitle_codecs",
            "audio_track_switching", "hls", "display", "device",
        ])
        let video = try #require((body["video"] as? [[String: Any]])?.first)
        #expect(Set(video.keys) == ["codec", "hardware", "profiles", "max_width", "max_height", "max_fps",
                                    "max_bit_depth", "max_level"])
        #expect(body["hdr_types"] as? [String] == ["hdr10", "dolby_vision"])
        #expect(Set((body["hls"] as? [String: Any] ?? [:]).keys) == ["master_playlist", "segments", "variants"])
        #expect(Set((body["device"] as? [String: Any] ?? [:]).keys) == ["form", "tier", "fire_tv", "model"])
        #expect((body["display"] as? [String: Any])?["refresh_rates"] as? [Double] == [23.976, 60])
        #expect(try PleyJSON.decoder.decode(ClientCaps.self, from: PleyJSON.encoder.encode(v2)) == v2)
    }

    @Test func aDecoderWithoutLimitsLeavesThemOut() throws {
        let entry = VideoDecoderCaps(codec: "vp9", hardware: true)
        let body = try JSONSerialization.jsonObject(with: PleyJSON.encoder.encode(entry)) as? [String: Any]
        #expect(body.map { Set($0.keys) } == ["codec", "hardware", "profiles"])
    }

    @Test func aReportMissingFieldsReadsWithDefaults() throws {
        let caps = try PleyJSON.decoder.decode(ClientCaps.self, from: Data(#"{"caps_version":2,"device":{}}"#.utf8))
        #expect(caps.containers == [] && !caps.hdr)
        #expect(caps.device?.form == .unknown)
        #expect(caps.video == nil)
    }
}
