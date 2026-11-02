sub test_caps_of_a_4k_hdr_roku_with_a_receiver()
    caps = capsBuild(fakeUltra())
    tkEqual(caps.caps_version, 2)
    tkEqual(caps.containers, ["mp4", "m4v", "mov", "mkv", "mp3", "m4a", "flac", "wav", "aac", "webm"])
    tkEqual(caps.video_codecs, ["h264", "hevc", "vp9", "av1"])
    tkEqual(caps.video[0], { codec: "h264", hardware: true, profiles: ["Constrained Baseline", "Baseline", "Main", "High"], max_width: 3840, max_height: 2160, max_bit_depth: 8, max_level: 51 })
    tkEqual(caps.video[1], { codec: "hevc", hardware: true, profiles: ["Main", "Main 10"], max_width: 3840, max_height: 2160, max_bit_depth: 10, max_level: 153 })
    tkEqual(caps.video[2], { codec: "vp9", hardware: true, profiles: ["Profile 0", "Profile 2"], max_width: 3840, max_height: 2160, max_bit_depth: 10 })
    tkEqual(caps.video[3], { codec: "av1", hardware: true, profiles: ["Main"], max_width: 3840, max_height: 2160, max_bit_depth: 10 })
    tkEqual(caps.video.Count(), 4)
    tkEqual(caps.hdr_types, ["hdr10", "hdr10plus", "hlg", "dolby_vision"])
    tkEqual(caps.dolby_vision_profiles, [{ profile: 5 }, { profile: 8 }])
    tkTrue(caps.hdr)
    tkTrue(caps.dolby_vision)
    tkEqual(caps.audio_decode, ["aac", "mp3", "ac3", "eac3", "flac", "alac", "opus", "vorbis", "pcm_s16le", "pcm_s24le", "pcm_s16be", "pcm_s24be"])
    tkEqual(caps.audio_passthrough, ["ac3", "eac3", "dts"])
    tkEqual(caps.audio_codecs, ["aac", "mp3", "ac3", "eac3", "flac", "alac", "opus", "vorbis", "pcm_s16le", "pcm_s24le", "pcm_s16be", "pcm_s24be", "dts"])
    tkEqual(caps.audio_channels, 6)
    tkEqual(caps.hls, { master_playlist: true, segments: ["ts"], variants: true })
    tkEqual(caps.subtitle_codecs, ["hdmv_pgs_subtitle"], "PGS drawn by the app; VobSub and DVB burned in")
    tkTrue(caps.picture_subtitles_sideload, "next to a stream too")
    tkEqual(caps.display, { width: 3840, height: 2160, refresh_rates: [60] })
    tkEqual(caps.device, { form: "tv", model: "Roku Ultra" })
end sub

sub test_caps_lines_say_what_diagnostics_shows()
    report = {
        display: { width: 3840, height: 2160, refresh_rates: [60] },
        hdr_types: ["hdr10", "hlg", "dolby_vision"],
        dolby_vision_profiles: [{ profile: 5 }, { profile: 8 }],
        video: [{ codec: "h264", max_width: 3840, max_height: 2160, max_bit_depth: 8 }, { codec: "hevc", max_width: 3840, max_height: 2160, max_bit_depth: 10 }],
        audio_decode: ["aac", "mp3", "ac3"],
        audio_passthrough: ["eac3", "dts"],
        audio_channels: 6,
        subtitle_codecs: ["hdmv_pgs_subtitle"],
        picture_subtitles_sideload: true,
        containers: ["mp4", "mkv"]
    }
    tkEqual(capsLines(report), [
        "Display: 3840×2160 at 60 Hz",
        "HDR: HDR10, HLG, Dolby Vision",
        "Dolby Vision: profiles 5, 8",
        "Video: H.264 3840×2160, HEVC 3840×2160 10-bit",
        "Audio decoded: aac, mp3, ac3",
        "Sent to the receiver: eac3, dts",
        "Sound: Surround (5.1)",
        "Picture subtitles: PGS drawn here, VobSub and DVB burned in",
        "Containers: mp4, mkv",
        "Streams: HLS, in several sizes"
    ])
    plain = capsLines({ hdr_types: [], video: [{ codec: "h264", max_width: 1920, max_height: 1080, max_bit_depth: 8 }], audio_decode: ["aac"], audio_passthrough: [], audio_channels: 2, containers: ["mp4"] })
    tkEqual(plain[0], "HDR: none", "no display mode known: no line for it")
    tkEqual(plain[3], "Sent to the receiver: nothing")
    tkEqual(plain[4], "Sound: Stereo")
    tkEqual(plain[5], "Containers: mp4", "no picture subtitles drawn: no line for them")
end sub

sub test_caps_of_a_1080p_stick_on_tv_speakers()
    caps = capsBuild(fakeExpress())
    tkEqual(caps.containers, ["mp4", "m4v", "mov", "mkv", "mp3", "m4a", "flac", "wav", "aac"])
    tkEqual(caps.video_codecs, ["h264"])
    tkEqual(caps.video, [{ codec: "h264", hardware: true, profiles: ["Constrained Baseline", "Baseline", "Main", "High"], max_width: 1920, max_height: 1080, max_bit_depth: 8, max_level: 42 }])
    tkEqual(caps.hdr_types, [])
    tkFalse(caps.hdr)
    tkFalse(caps.dolby_vision)
    tkFalse(caps.DoesExist("dolby_vision_profiles"))
    tkEqual(caps.audio_decode, ["aac", "mp3"])
    tkEqual(caps.audio_passthrough, [], "the stereo setting mixes down")
    tkEqual(caps.audio_channels, 2)
    tkEqual(caps.display, { width: 1920, height: 1080, refresh_rates: [60] })
end sub

sub test_caps_hdr_needs_a_10_bit_decoder()
    device = fakeUltra()
    device.spec.video.hevc = { "main": 5.1 }
    caps = capsBuild(device)
    tkEqual(caps.hdr_types, [], "an HDR screen with an 8-bit HEVC decoder")
    tkEqual(caps.video[1], { codec: "hevc", hardware: true, profiles: ["Main"], max_width: 3840, max_height: 2160, max_bit_depth: 8, max_level: 153 })
    tkFalse(caps.hdr)
end sub

sub test_caps_main_10_below_main_gets_two_entries()
    device = fakeUltra()
    device.spec.video.hevc = { "main": 5.1, "main 10": 4.1 }
    caps = capsBuild(device)
    tkEqual(caps.video[1].profiles, ["Main", "Main 10"])
    tkEqual(caps.video[1].max_level, 123)
    tkEqual(caps.video[1].max_height, 1080)
    tkEqual(caps.video[2].profiles, ["Main"])
    tkEqual(caps.video[2].max_level, 153)
end sub

sub test_caps_falls_back_to_hd_h264_when_firmware_says_nothing()
    device = fakeExpress()
    device.spec.video = {}
    caps = capsBuild(device)
    tkEqual(caps.video, [{ codec: "h264", hardware: true, profiles: ["Constrained Baseline", "Baseline", "Main", "High"], max_width: 1920, max_height: 1080, max_bit_depth: 8, max_level: 41 }])
end sub

sub test_caps_asks_roku_by_its_own_names()
    device = fakeUltra()
    capsBuild(device)
    asked = {}
    for each format in device.asked
        asked[FormatJson(format)] = true
    end for
    tkTrue(asked.DoesExist(FormatJson({ codec: "mpeg4 avc", level: "5.1", profile: "high" })), "H.264")
    tkTrue(asked.DoesExist(FormatJson({ codec: "hevc", level: "5.1", profile: "main 10" })), "HEVC Main 10")
    tkTrue(asked.DoesExist(FormatJson({ codec: "vp9", container: "mkv" })), "VP9 in Matroska")
    tkTrue(asked.DoesExist(FormatJson({ codec: "eac3", passthru: 1 })), "E-AC-3 to the receiver")
end sub

sub test_caps_vp9_only_from_matroska()
    device = fakeUltra()
    device.spec.video.vp9.containers = ["webm"]
    device.spec.video.Delete("av1")
    caps = capsBuild(device)
    tkEqual(caps.video_codecs, ["h264", "hevc"])
    tkFalse(capsListed(caps.containers, "webm"), "WebM needs VP9 or AV1")
end sub

sub test_caps_av1_is_asked_about_alone()
    device = fakeExpress()
    device.spec.video.av1 = { "main": 4.1, containers: [] }
    caps = capsBuild(device)
    tkEqual(caps.video[1], { codec: "av1", hardware: true, profiles: ["Main"], max_width: 1920, max_height: 1080, max_bit_depth: 10 }, "HD decoders, HD AV1")
    tkTrue(capsListed(caps.containers, "webm"))
    asked = false
    for each format in device.asked
        if format.codec = "av1" and format.container = invalid and format.profile = invalid then asked = true
    end for
    tkTrue(asked)
end sub

sub test_caps_audio_the_roku_decodes_itself()
    device = fakeExpress()
    device.spec.audio = ["aac", "mp3", "mp2", "dts"]
    tkEqual(capsBuild(device).audio_decode, ["aac", "mp3", "mp2", "dts"])
end sub

sub test_caps_reads_video_modes()
    tkEqual(capsVideoMode("1080p"), { width: 1920, height: 1080, rate: 60 })
    tkEqual(capsVideoMode("720p50"), { width: 1280, height: 720, rate: 50 })
    tkEqual(capsVideoMode("2160p24"), { width: 3840, height: 2160, rate: 24 })
    tkEqual(capsVideoMode("4320p60"), { width: 7680, height: 4320, rate: 60 })
    tkEqual(capsVideoMode("480i"), { width: 720, height: 480, rate: 60 })
    tkInvalid(capsVideoMode("1234p"))
    tkInvalid(capsVideoMode(""))
    tkInvalid(capsVideoMode(invalid))
end sub

sub test_caps_report_is_json()
    text = FormatJson(capsBuild(fakeUltra()))
    tkTrue(Len(text) > 100)
    tkEqual(ParseJson(text).hls.segments, ["ts"])
end sub
