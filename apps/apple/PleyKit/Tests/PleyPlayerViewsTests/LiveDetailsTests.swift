import PleyModel
@testable import PleyPlayerViews
import Testing

/// A live channel's Details, from its session's plan.
@Suite struct LiveDetailsTests {
    /// The lines as "Title: text".
    private func said(_ plan: LivePlan?) -> [String] {
        LiveDetails.lines(plan).map { "\($0.title): \($0.text)" }
    }

    @Test func aChannelTheServerCopiesIsADirectStream() {
        let plan = LivePlan(videoCodec: "h264", width: 1920, height: 1080, frameRate: 50, bitDepth: 8,
                            audioCodec: "aac", audioChannels: 2, video: "copy", audio: "copy", kbps: 8_200)
        #expect(said(plan) == ["Playing: Direct stream", "Video: H.264 · 1920×1080 · 50 fps", "Audio: AAC Stereo",
                               "Bitrate: 8.2 Mbps"])
    }

    @Test func otherSoundIsMadeAACWhileTheVideoIsCopied() {
        let plan = LivePlan(videoCodec: "h264", width: 1280, height: 720, frameRate: 29.97, audioCodec: "eac3",
                            audioChannels: 6, video: "copy", audio: "aac", kbps: 850)
        #expect(said(plan) == ["Playing: Direct stream", "Video: H.264 · 1280×720 · 29.97 fps",
                               "Audio: EAC3 5.1 → AAC Stereo", "Bitrate: 850 kbps"])
    }

    @Test func aTranscodeSaysWhatIsMadeAndItsEncoder() {
        let plan = LivePlan(videoCodec: "hevc", width: 3840, height: 2160, frameRate: 50, bitDepth: 10,
                            audioCodec: "eac3", audioChannels: 6, video: "h264_nvenc", audio: "aac", outHeight: 1080,
                            videoKbps: 8_000, kbps: 12_300)
        #expect(said(plan) == ["Playing: Transcoding", "Video: HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p",
                               "Audio: EAC3 5.1 → AAC Stereo", "Bitrate: 12 Mbps → 8.0 Mbps",
                               "Encoder: h264_nvenc"])
    }

    @Test func interlacedVideoSaysSoAndKeepsItsLines() {
        let plan = LivePlan(videoCodec: "mpeg2video", width: 720, height: 576, frameRate: 25, interlaced: true,
                            bitDepth: 8, audioCodec: "mp2", audioChannels: 2, video: "libx264", audio: "aac",
                            outHeight: 576, videoKbps: 2_500, kbps: 4_100)
        #expect(said(plan) == ["Playing: Transcoding",
                               "Video: MPEG-2 · 720×576 · 25 fps · interlaced → H.264 576p",
                               "Audio: MP2 Stereo → AAC Stereo", "Bitrate: 4.1 Mbps → 2.5 Mbps",
                               "Encoder: libx264"])
    }

    @Test func anOlderServersPlanSaysWhatItKnows() {
        // No bit depth, transcode size or bitrates.
        let plan = LivePlan(videoCodec: "hevc", width: 1920, height: 1080, frameRate: 25, audioCodec: "ac3",
                            audioChannels: 6, video: "libx264", audio: "aac")
        #expect(said(plan) == ["Playing: Transcoding", "Video: HEVC · 1920×1080 · 25 fps → H.264 1080p",
                               "Audio: AC3 5.1 → AAC Stereo", "Encoder: libx264"])
    }

    @Test func radioHasNoVideoToSpeakOf() {
        let plan = LivePlan(audioCodec: "mp2", audioChannels: 2, video: "none", audio: "aac")
        #expect(said(plan) == ["Playing: Direct stream", "Audio: MP2 Stereo → AAC Stereo"])
    }

    @Test func aPlanNotKnownYetSaysOnlyThatItStarts() {
        #expect(said(nil) == ["Playing: Starting…"])
    }

    @Test func anEncoderNamesItsCodec() {
        #expect(LiveDetails.encoded("h264_videotoolbox") == "H.264")
        #expect(LiveDetails.encoded("hevc_qsv") == "HEVC")
        #expect(LiveDetails.encoded("libx265") == "HEVC")
        #expect(LiveDetails.encoded("mystery") == nil)
    }
}
