import Foundation
@testable import PleyCaps
import PleyModel
import Testing

@Suite("The caps report's rules")
struct CapsBuilderTests {
    @Test func hdrIsEmptyWhenTheDeviceIsntEligible() throws {
        let caps = report(try probe("iphone_a16_hdr") { $0.hdr.eligible = false })
        #expect(caps.hdrTypes == [])
        #expect(caps.dolbyVisionProfiles == [])
        #expect(!caps.hdr && !caps.dolbyVision)
        // A TV on an SDR input, whatever the old list of modes says.
        let tv = report(try probe("tv_4k_dv") { $0.hdr.eligible = false })
        #expect(tv.hdrTypes == [] && tv.dolbyVisionProfiles == [])
    }

    @Test func dolbyVisionOnlyOnADolbyVisionScreenAndNever7() throws {
        let hdr10 = report(try probe("tv_4k_hdr10"))
        #expect(hdr10.hdrTypes == [.hdr10, .hlg])
        #expect(hdr10.dolbyVisionProfiles == [])
        #expect(hdr10.hdr && !hdr10.dolbyVision)

        let dv = report(try probe("tv_4k_dv"))
        #expect(dv.hdrTypes == [.hdr10, .hlg, .dolbyVision])
        #expect(dv.dolbyVisionProfiles == [DolbyVisionCaps(profile: 5, maxLevel: 9), DolbyVisionCaps(profile: 8, maxLevel: 9)])
        #expect(dv.dolbyVision)
        for name in recordedDevices {
            let caps = report(try probe(name))
            #expect(!(caps.dolbyVisionProfiles ?? []).contains { $0.profile == 7 }, "\(name)")
            #expect(!(caps.hdrTypes ?? []).contains(.hdr10Plus), "\(name)")
        }
        // A 4K30 chip's level.
        let a10 = report(try probe("tv_4k_dv") { $0.model = "iPhone9,1"; $0.platform = .iOS })
        #expect(a10.dolbyVisionProfiles?.map(\.maxLevel) == [7, 7])
    }

    @Test func noHDRWithoutAMain10HardwareDecoder() throws {
        // HEVC decoded in software only.
        let software = report(try probe("iphone_a16_hdr") { $0.hardwareDecode["hevc"] = false })
        #expect(software.video?.map(\.codec) == ["h264"])
        #expect(software.hdrTypes == [] && software.dolbyVisionProfiles == [])
        // The A9's HEVC is 8-bit, even on an HDR screen.
        let a9 = report(try probe("ipad_5th_gen") { $0.hdr = HDRFacts(eligible: true, modes: ["hdr10"]) })
        #expect(a9.video?.first { $0.codec == "hevc" }?.profiles == ["Main"])
        #expect(a9.hdrTypes == [])
    }

    @Test func eligibleWithoutModesFallsBackToTheClass() throws {
        let tv = report(try probe("tv_4k_hdr10") { $0.hdr.modes = [] })
        #expect(tv.hdrTypes == [.hdr10, .hlg, .dolbyVision])
        #expect(tv.dolbyVisionProfiles?.map(\.profile) == [5, 8])
    }

    @Test func theMacIsADesktopWithDolbyVisionOnAppleSiliconOnly() throws {
        let m3 = report(try probe("mac_m3_xdr"))
        #expect(m3.device?.form == .desktop)
        #expect(m3.hdrTypes == [.hdr10, .hlg, .dolbyVision])
        #expect(m3.dolbyVisionProfiles?.map(\.profile) == [5, 8])
        // An Intel Mac on an HDR screen: HDR10 and HLG, never Dolby Vision.
        let intel = report(try probe("mac_intel_sdr") { $0.hdr = HDRFacts(eligible: true, edrHeadroom: 2) })
        #expect(intel.device?.form == .desktop)
        #expect(intel.hdrTypes == [.hdr10, .hlg])
        #expect(intel.dolbyVisionProfiles == [])
        // The window on an SDR screen: no HDR, though AVPlayer is eligible.
        let sdrScreen = report(try probe("mac_m3_xdr") { $0.hdr.edrHeadroom = 1 })
        #expect(sdrScreen.hdrTypes == [])
        // And without a screen known.
        #expect(report(try probe("mac_m3_xdr") { $0.hdr.edrHeadroom = nil }).hdrTypes == [])
    }

    @Test func formsFollowThePlatform() throws {
        #expect(report(try probe("iphone_a16_hdr")).device?.form == .phone)
        #expect(report(try probe("ipad_5th_gen")).device?.form == .tablet)
        #expect(report(try probe("tv_hd")).device?.form == .tv)
        #expect(report(try probe("tv_hd")).device?.model == "AppleTV5,3")
        #expect(report(try probe("tv_hd")).device?.fireTv == false)
    }

    @Test func spatialAudioReports6ChannelsAndBluetoothStereo2() throws {
        #expect(report(try probe("iphone_a16_hdr")).audioChannels == 6)
        #expect(report(try probe("iphone_a16_hdr") { $0.audio.spatial = false }).audioChannels == 2)
        #expect(report(try probe("iphone_a16_hdr") {
            $0.audio = AudioFacts(maxChannels: 6, route: AudioFacts.bluetooth)
        }).audioChannels == 2)
        #expect(report(try probe("tv_4k_hdr10")).audioChannels == 8)
        #expect(report(try probe("tv_4k_dv")).audioChannels == 6)
        #expect(report(try probe("tv_hd")).audioChannels == 2)
        // Bounded to 2…8.
        #expect(report(try probe("tv_hd") { $0.audio.maxChannels = 16 }).audioChannels == 8)
        #expect(report(try probe("tv_hd") { $0.audio.maxChannels = 1 }).audioChannels == 2)
        // Nothing is passed through: Apple decodes it all.
        #expect(report(try probe("tv_4k_hdr10")).audioPassthrough == [])
    }

    @Test func vp9OnlyWithHardwareDecoding() throws {
        #expect(!(report(try probe("iphone_a16_hdr")).video ?? []).contains { $0.codec == "vp9" })
        let m3 = report(try probe("mac_m3_xdr"))
        let vp9 = try #require(m3.video?.first { $0.codec == "vp9" })
        #expect(vp9.profiles == ["Profile 0", "Profile 2"])
        #expect((vp9.maxWidth, vp9.maxHeight, vp9.maxBitDepth) == (3840, 2160, 10))
        #expect(m3.videoCodecs == ["h264", "hevc", "av1", "vp9"])
        #expect(!(report(try probe("mac_m3_xdr") { $0.hardwareDecode["vp9"] = false }).video ?? [])
            .contains { $0.codec == "vp9" })
    }

    @Test func videoToolboxTakesCodecsAwayButNeverAddsThem() throws {
        // An M3 whose AV1 isn't decoded in hardware.
        #expect(report(try probe("mac_m3_xdr") { $0.hardwareDecode["av1"] = false }).videoCodecs
            == ["h264", "hevc", "vp9"])
        // An A16 that said AV1: its class has none.
        #expect(report(try probe("iphone_a16_hdr") { $0.hardwareDecode["av1"] = true }).videoCodecs
            == ["h264", "hevc"])
        // H.264 always.
        #expect(report(try probe("tv_hd")).videoCodecs == ["h264"])
    }

    @Test func modelsMatchTheirClass() throws {
        func id(_ model: String) -> String {
            DeviceClasses.matchModel(model).id
        }
        #expect(id("AppleTV5,3") == "apple_tv_hd")
        #expect(id("AppleTV6,2") == "a10x")
        #expect(id("AppleTV11,1") == "a11_a13")
        #expect(id("AppleTV14,1") == "a14_m2")
        #expect(id("iPad6,11") == "a9")
        #expect(id("iPad7,1") == "a10x")
        #expect(id("iPad7,11") == "a10")
        #expect(id("iPhone9,3") == "a10")
        #expect(id("iPhone15,4") == "a14_m2")
        #expect(id("iPad15,7") == "a14_m2")
        #expect(id("iPad15,3") == "a17_m3")
        #expect(id("iPhone16,1") == "a17_m3")
        #expect(DeviceClasses.matchChip("Apple M1 Max").id == "a14_m2")
        #expect(DeviceClasses.matchChip("Apple M3 Pro").id == "a17_m3")
        #expect(DeviceClasses.matchChip("Intel(R) Core(TM) i7-8559U CPU @ 2.70GHz").id == "intel_mac")
    }

    @Test func unknownModelsUseTheFallback() throws {
        // Newer than the table's newest of its kind: the newest class.
        #expect(DeviceClasses.matchModel("iPhone99,1").id == "a17_m3")
        #expect(DeviceClasses.matchModel("AppleTV15,1").id == "a17_m3")
        #expect(DeviceClasses.matchChip("Apple M9").id == "a17_m3")
        // Anything else: the A12's.
        #expect(DeviceClasses.matchModel("iPhone7,2").id == "a11_a13")
        #expect(DeviceClasses.matchModel("arm64").id == "a11_a13")
        #expect(DeviceClasses.matchModel("").id == "a11_a13")
        #expect(DeviceClasses.matchChip("").id == "a11_a13")
        let unknown = report(try probe("iphone_a16_hdr") { $0.model = "Watch7,1" })
        #expect(unknown.video?.first { $0.codec == "hevc" }?.maxLevel == 156)
    }

    @Test func tiersFollowTheClassAndMemory() throws {
        #expect(report(try probe("iphone_a16_hdr")).device?.tier == "high")
        #expect(report(try probe("ipad_5th_gen")).device?.tier == "low")
        #expect(report(try probe("mac_m3_xdr")).device?.tier == "high")
        #expect(report(try probe("mac_intel_sdr")).device?.tier == "high")
        #expect(report(try probe("tv_4k_hdr10")).device?.tier == "mid")
        #expect(report(try probe("tv_hd")).device?.tier == "low")
        // The A9 is low whatever its memory; an A11 never high.
        #expect(report(try probe("ipad_5th_gen") { $0.memoryBytes = 4 << 30 }).device?.tier == "low")
        #expect(report(try probe("ipad_5th_gen") { $0.model = "iPhone10,3"; $0.memoryBytes = 6 << 30 }).device?.tier
            == "mid")
        #expect(report(try probe("ipad_5th_gen") { $0.model = "iPhone10,3"; $0.memoryBytes = 2 << 30 }).device?.tier
            == "low")
    }

    @Test func screensAndTheirRates() throws {
        #expect(report(try probe("tv_4k_dv")).display
            == DisplayCaps(width: 3840, height: 2160, refreshRates: [23.976, 24, 25, 29.97, 30, 50, 59.94, 60]))
        #expect(report(try probe("tv_hd")).display == DisplayCaps(width: 1920, height: 1080, refreshRates: [60]))
        #expect(report(try probe("iphone_a16_hdr")).display == DisplayCaps(width: 2556, height: 1179,
                                                                            refreshRates: [120]))
        // Match Content means nothing off Apple TV.
        #expect(report(try probe("iphone_a16_hdr") { $0.display.matchContent = true }).display?.refreshRates == [120])
        // Long side first, whichever way round the screen was read.
        #expect(DisplayFacts(width: 1536, height: 2048, maxFps: 60) == DisplayFacts(width: 2048, height: 1536, maxFps: 60))
        #expect(report(try probe("tv_hd") { $0.display = DisplayFacts(width: 0, height: 0, maxFps: 60) }).display == nil)
    }

    @Test func whatEveryAppleDeviceSendsAlike() throws {
        for name in recordedDevices {
            let caps = report(try probe(name))
            #expect(caps.capsVersion == 2)
            #expect(caps.containers == ["mp4", "m4v", "mov", "m4a", "mp3", "flac", "wav"])
            // Opus from iOS and iPadOS 17: the iPad 5th generation's iPadOS 16 leaves it out.
            let opus = name == "ipad_5th_gen" ? [] : ["opus"]
            #expect(caps.audioDecode == ["aac", "mp3", "alac", "flac"] + opus + ["ac3", "eac3"])
            #expect(caps.audioCodecs == caps.audioDecode)
            #expect(caps.videoCodecs == caps.video?.map(\.codec))
            // Text and pictures alike, drawn by the app; pictures from the server's .mks, never burned in.
            #expect(caps.subtitleCodecs == ["subrip", "ass", "ssa", "webvtt", "mov_text", "hdmv_pgs_subtitle",
                                            "dvd_subtitle", "dvb_subtitle"])
            #expect(caps.pictureSubtitlesSideload == true)
            #expect(caps.audioTrackSwitching == true)
            #expect(caps.hls == HlsCaps(masterPlaylist: true, segments: ["fmp4", "ts"], variants: true))
            #expect(caps.video?.first?.codec == "h264")
            #expect(caps.video?.first?.maxBitDepth == 8)
        }
    }

    @Test func opusIsReportedFromIOS17() throws {
        #expect(report(try probe("ipad_5th_gen")).audioDecode?.contains("opus") == false)
        #expect(report(try probe("ipad_5th_gen") { $0.osVersion = "17.0" }).audioDecode?.contains("opus") == true)
        #expect(report(try probe("iphone_a16_hdr") { $0.osVersion = "16.7.10" }).audioDecode?.contains("opus") == false)
        #expect(report(try probe("tv_hd") { $0.osVersion = "18.0" }).audioDecode?.contains("opus") == true)
    }

    @Test func linesSayItInWords() throws {
        let lines = CapsBuilder.build(try probe("tv_4k_dv")).lines
        #expect(lines.contains("HEVC: Main, Main 10, up to 3840×2160 at 60 fps, 10-bit, level 156"))
        #expect(lines.contains("HDR played as it is: HDR10, HLG, Dolby Vision"))
        #expect(lines.contains("Dolby Vision profiles: 5 (level 9), 8 (level 9)"))
        #expect(lines.contains("Screen: 3840×2160, 23.976, 24, 25, 29.97, 30, 50, 59.94, 60 Hz, Match Content on"))
        #expect(lines.contains("Audio output: hdmi, 6 channels"))
    }
}
