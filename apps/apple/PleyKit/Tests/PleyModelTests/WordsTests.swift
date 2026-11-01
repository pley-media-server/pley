import PleyModel
import Testing

/// The Android app's format tests, case by case, where the Apple apps say the same.
@Suite struct MediaWordsTests {
    private func stream(_ kind: StreamKind = .video, _ codec: String = "h264", _ language: String? = nil,
                        _ title: String? = nil, channels: Int? = nil, profile: String? = nil, forced: Bool = false,
                        external: Bool = false, width: Int? = nil, height: Int? = nil, range: DynamicRange? = nil,
                        dvProfile: Int? = nil, dvBlCompat: Int? = nil, bitDepth: Int? = nil) -> MediaStream {
        var s = MediaStream(index: 0, kind: kind, codec: codec, language: language)
        s.title = title
        s.channels = channels
        s.profile = profile
        s.forced = forced
        s.external = external
        s.width = width
        s.height = height
        s.dynamicRange = range
        s.dvProfile = dvProfile
        s.dvBlCompat = dvBlCompat
        s.bitDepth = bitDepth
        return s
    }

    @Test func resolutions() {
        #expect(Words.resolution(width: 3840, height: 2160) == "4K")
        #expect(Words.resolution(width: 3840, height: 1600) == "4K")
        #expect(Words.resolution(width: 2560, height: 1440) == "1440p")
        #expect(Words.resolution(width: 1920, height: 800) == "1080p")
        #expect(Words.resolution(width: 1280, height: 720) == "720p")
        #expect(Words.resolution(width: 720, height: 576) == "576p")
        #expect(Words.resolution(width: 720, height: 480) == "480p")
        #expect(Words.resolution(width: 640, height: 360) == "SD")
        #expect(Words.resolution(width: nil, height: nil) == nil)
        #expect(Words.resolution(width: 0, height: 0) == nil)
    }

    @Test func codecsAndChannels() {
        #expect(Words.codec("h264") == "H.264")
        #expect(Words.codec("HEVC") == "HEVC")
        #expect(Words.codec("hdmv_pgs_subtitle") == "PGS")
        #expect(Words.codec("cinepak") == "CINEPAK")
        #expect(Words.codec(nil) == "")
        #expect(Words.channels(1) == "Mono")
        #expect(Words.channels(2) == "Stereo")
        #expect(Words.channels(6) == "5.1")
        #expect(Words.channels(8) == "7.1")
        #expect(Words.channels(5) == "5ch")
        #expect(Words.channels(nil, layout: "quad") == "quad")
        #expect(Words.channels(0) == "")
    }

    @Test func streamLabels() {
        #expect(Words.stream(stream(.audio, "eac3", "eng", channels: 6)) == "English (EAC3 5.1)")
        #expect(Words.stream(stream(.audio, "truehd", "eng", channels: 8, profile: "Dolby TrueHD + Dolby Atmos"))
            == "English (TrueHD Atmos 7.1)")
        #expect(Words.stream(stream(.audio, "aac", "eng", "Commentary", channels: 2))
            == "English – Commentary (AAC Stereo)")
        #expect(Words.stream(stream(.audio, "aac", nil, "Director's track", channels: 2))
            == "Director's track (AAC Stereo)")
        #expect(Words.stream(stream(.audio, "ac3", "eng", "English 5.1", channels: 6)) == "English (AC3 5.1)")
        #expect(Words.stream(stream(.audio, "opus")) == "Unknown (Opus)")
        #expect(Words.stream(stream(.subtitle, "subrip", "fre", forced: true, external: true))
            == "French (SRT, Forced, External)")
        #expect(Words.stream(stream(.subtitle, "subrip", "fre", "SDH", forced: true)) == "French – SDH (SRT, Forced)")
        #expect(Words.stream(stream(.subtitle, "hdmv_pgs_subtitle", "pt-BR", "SDH")) == "Portuguese (BR) – SDH (PGS)")
        #expect(Words.stream(stream(.subtitle, "subrip", "eng", "English")) == "English (SRT)")
        #expect(Words.stream(stream(.subtitle, "subrip", "eng", "English (Forced)")) == "English (SRT, Forced)")
        #expect(Words.stream(stream(.subtitle, "subrip", "eng", "Signs & Songs"))
            == "English – Signs & Songs (SRT, Forced)")
        #expect(Words.stream(stream(.subtitle, "subrip", "eng", "Unforced Errors"))
            == "English – Unforced Errors (SRT)")
        let dv = stream(.video, "hevc", width: 3840, height: 2160, range: .dolbyVision, dvProfile: 8, dvBlCompat: 1,
                        bitDepth: 10)
        #expect(Words.stream(dv) == "4K HEVC Dolby Vision P8.1 10-bit")
        #expect(Words.stream(stream(width: 1920, height: 1080, range: .sdr, bitDepth: 8)) == "1080p H.264")
    }

    @Test func versionsAndTheVideoLine() {
        var file = MediaFile(id: 1, path: "a.mkv", container: "mkv",
                             streams: [stream(.video, "hevc", width: 3840, height: 2160, range: .hdr10, bitDepth: 10),
                                       stream(.audio, "eac3", "eng", channels: 6)])
        file.width = 3840
        file.height = 2160
        file.videoCodec = "hevc"
        file.size = 62 * 1024 * 1024 * 1024
        file.edition = "Final Cut"
        #expect(Words.version(file, position: 0) == "Final Cut · 4K · HEVC · MKV · 62.0 GB")
        #expect(Words.version(MediaFile(id: 2, path: "b"), position: 1) == "Version 2")
        #expect(Words.videoLine(file) == "4K · HEVC · HDR10 · EAC3 5.1")
    }

    @Test func languagesAndTimes() {
        #expect(Words.language("eng") == "English")
        #expect(Words.language("pt-BR") == "Portuguese (BR)")
        #expect(Words.language("xx") == "XX")
        #expect(Words.language(" ") == nil)
        #expect(Words.bytes(1536) == "1.5 KB")
        #expect(Words.bytes(150 * 1024 * 1024) == "150 MB")
        #expect(Words.clock(ms: 754_000) == "12:34")
        #expect(Words.clock(ms: 3_723_000) == "1:02:03")
        #expect(Words.clock(ms: 0) == "0:00")
        #expect(Words.runtime(ms: 6_720_000) == "1h 52m")
        #expect(Words.runtime(ms: 3_480_000) == "58 min")
        #expect(Words.runtime(ms: 29_000) == "29s")
        #expect(Words.runtime(ms: nil) == "")
        #expect(Words.episode(season: 1, episode: 2) == "S1 E2")
        #expect(Words.episode(season: nil, episode: nil) == "")
    }
}
