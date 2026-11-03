' The player's menu: the playback's details and the file's chapters.

function detailsTestFile() as object
    return ParseJson("{""id"":7,""bitrate"":24000000,""width"":3840,""height"":2160,""streams"":[{""index"":0,""kind"":""video"",""codec"":""hevc"",""width"":3840,""height"":2160,""dynamic_range"":""dolby_vision"",""dv_profile"":8,""dv_bl_compat"":1},{""index"":1,""kind"":""audio"",""codec"":""truehd"",""channels"":8,""default"":true},{""index"":2,""kind"":""audio"",""codec"":""ac3"",""channels"":6}],""chapters"":[{""start_ms"":0,""end_ms"":60000,""title"":""Opening""},{""start_ms"":60000,""end_ms"":120000,""title"":null}]}")
end function

sub test_details_name_streams_as_the_other_apps()
    file = detailsTestFile()
    tkEqual(detVideo(file.streams[0]), "HEVC 4K Dolby Vision P8.1")
    tkEqual(detAudio(file.streams[1]), "TrueHD 7.1")
    tkEqual(detAudio({ codec: "aac", channels: 5 }), "AAC 5ch")
    tkEqual(detRange({ dynamic_range: "dolby_vision", dv_profile: 7, dv_el: "fel" }), "Dolby Vision P7 FEL")
    tkEqual(detRange({ dynamic_range: "hdr10plus" }), "HDR10+")
    tkEqual(detRange({ dynamic_range: "sdr" }), "")
    tkEqual(detCodec("hdmv_pgs_subtitle"), "PGS")
    tkEqual(detCodec("xyz"), "XYZ")
end sub

sub test_details_of_a_direct_play_and_a_conversion()
    file = detailsTestFile()
    direct = detPlayback({ method: "direct", audio_index: 2 }, file, -1, invalid, 0)
    tkEqual(direct.method, "Direct play")
    tkEqual(direct.why, "", "direct play goes without saying")
    tkEqual(direct.rows, [{ label: "Video", value: "HEVC 4K Dolby Vision P8.1" }, { label: "Audio", value: "AC3 5.1" }, { label: "Bitrate", value: "24 Mbps" }])
    live = { video: "transcode", audio: "transcode", video_in: "HEVC 3840×2160 Dolby Vision P8.1", video_out: "H.264 1920×1080", audio_in: "TrueHD 7.1", audio_out: "AAC 2.0", tonemap: "libplacebo", speed: 1.5, transcoded_ms: 70000 }
    info = { method: "transcode", audio_index: 1, reasons: ["This player doesn't decode Dolby Vision."], burn_subtitles: true, video_range: "SDR (tone-mapped)" }
    converted = detPlayback(info, file, -1, live, 40000)
    tkEqual(converted.method, "Transcoding")
    tkEqual(converted.why, "This player doesn't decode Dolby Vision.")
    tkEqual(converted.rows[0], { label: "Video", value: "HEVC 4K Dolby Vision P8.1 → H.264 1080p SDR" })
    tkEqual(converted.rows[1], { label: "Audio", value: "TrueHD 7.1 → AAC 2.0" })
    tkEqual(converted.rows[3], { label: "Shown as", value: "SDR (tone-mapped with libplacebo)" })
    tkEqual(converted.rows[4], { label: "Subtitles", value: "Burned in" })
    tkEqual(converted.rows[5], { label: "Speed", value: "1.5× real time · 30 s ready" })
    remux = detPlayback({ method: "remux", audio_converted: true }, file, -1, invalid, 0)
    tkEqual(remux.method, "Direct stream · Audio transcoding")
    tkEqual(remux.why, "", "the method says it")
    tkEqual(remux.rows[0].value, "HEVC 4K Dolby Vision P8.1", "the video copied")
    tkEqual(remux.rows[1].value, "TrueHD 7.1 → …", "the audio converted, its output not known yet")
end sub

sub test_details_chapters()
    chapters = detChapters(detailsTestFile())
    tkEqual(chapters, [{ label: "1. Opening · 0:00", startMs: 0 }, { label: "2. Chapter 2 · 1:00", startMs: 60000 }])
    tkEqual(detChapterAt(chapters, 59999), 0)
    tkEqual(detChapterAt(chapters, 60000), 1)
    tkEqual(detChapters(invalid).Count(), 0)
end sub
