import PleyModel
import PleyPlayer
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
        #expect(said(plan) == ["Playing: Direct stream · Audio transcoding", "Video: H.264 · 1280×720 · 29.97 fps",
                               "Audio: EAC3 5.1 → AAC Stereo", "Bitrate: 850 kbps"])
    }

    @Test func howItPlaysIsInThePlayersWords() {
        // What the player's Details say of a file played each way.
        func player(_ method: PlaybackMethod, audioConverted: Bool = false) -> String? {
            var state = PlayerState()
            state.method = method
            state.audioConverted = audioConverted
            return PlayerDetails.lines(state).first?.text
        }
        func live(video: String, audio: String) -> String? {
            LiveDetails.lines(LivePlan(videoCodec: "h264", audioCodec: "ac3", video: video, audio: audio)).first?.text
        }
        #expect(live(video: "copy", audio: "copy") == player(.remux))
        #expect(live(video: "copy", audio: "aac") == player(.remux, audioConverted: true))
        #expect(live(video: "libx264", audio: "aac") == player(.transcode, audioConverted: true))
        #expect(live(video: "libx264", audio: "aac") == "Transcoding")
    }

    @Test func aTranscodeSaysWhatIsMadeAndItsEncoder() {
        let plan = LivePlan(videoCodec: "hevc", width: 3840, height: 2160, frameRate: 50, bitDepth: 10,
                            audioCodec: "eac3", audioChannels: 6, video: "h264_nvenc", audio: "aac", outHeight: 1080,
                            videoKbps: 8_000, kbps: 12_300)
        #expect(said(plan) == ["Playing: Transcoding", "Video: HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p",
                               "Audio: EAC3 5.1 → AAC Stereo", "Bitrate: 12 Mbps → 8.0 Mbps",
                               "Encoder: NVIDIA NVENC"])
    }

    @Test func interlacedVideoSaysSoAndKeepsItsLines() {
        let plan = LivePlan(videoCodec: "mpeg2video", width: 720, height: 576, frameRate: 25, interlaced: true,
                            bitDepth: 8, audioCodec: "mp2", audioChannels: 2, video: "libx264", audio: "aac",
                            outHeight: 576, videoKbps: 2_500, kbps: 4_100)
        #expect(said(plan) == ["Playing: Transcoding",
                               "Video: MPEG-2 · 720×576 · 25 fps · interlaced → H.264 576p",
                               "Audio: MP2 Stereo → AAC Stereo", "Bitrate: 4.1 Mbps → 2.5 Mbps",
                               "Encoder: x264"])
    }

    @Test func anOlderServersPlanSaysWhatItKnows() {
        // No bit depth, transcode size or bitrates.
        let plan = LivePlan(videoCodec: "hevc", width: 1920, height: 1080, frameRate: 25, audioCodec: "ac3",
                            audioChannels: 6, video: "libx264", audio: "aac")
        #expect(said(plan) == ["Playing: Transcoding", "Video: HEVC · 1920×1080 · 25 fps → H.264 1080p",
                               "Audio: AC3 5.1 → AAC Stereo", "Encoder: x264"])
    }

    @Test func radioHasNoVideoToSpeakOf() {
        let plan = LivePlan(audioCodec: "mp2", audioChannels: 2, video: "none", audio: "aac")
        #expect(said(plan) == ["Playing: Direct stream · Audio transcoding", "Audio: MP2 Stereo → AAC Stereo"])
    }

    @Test func aPlanNotKnownYetSaysOnlyThatItStarts() {
        #expect(said(nil) == ["Playing: Starting…"])
    }

    @Test func anEncoderGoesByTheNamePeopleKnow() {
        #expect(LiveDetails.encoderName("hevc_nvenc") == "NVIDIA NVENC")
        #expect(LiveDetails.encoderName("libx265") == "x265")
        #expect(LiveDetails.encoderName("h264_qsv") == "Intel Quick Sync")
        #expect(LiveDetails.encoderName("hevc_amf") == "AMD AMF")
        #expect(LiveDetails.encoderName("h264_vaapi") == "VA-API")
        #expect(LiveDetails.encoderName("h264_videotoolbox") == "VideoToolbox")
        // One not known stays as the server names it.
        #expect(LiveDetails.encoderName("h264_newthing") == "h264_newthing")
    }

    @Test func anEncoderNamesItsCodec() {
        #expect(LiveDetails.encoded("h264_videotoolbox") == "H.264")
        #expect(LiveDetails.encoded("hevc_qsv") == "HEVC")
        #expect(LiveDetails.encoded("libx265") == "HEVC")
        #expect(LiveDetails.encoded("mystery") == nil)
    }
}
