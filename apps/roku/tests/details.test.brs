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
    sheet = detSheet(remux)
    tkEqual(sheet[0], { kind: "heading", text: "Direct stream · Audio transcoding" })
    tkEqual(sheet[2], { kind: "fact", text: "Audio", value: "TrueHD 7.1 › …" }, "an arrow Roboto has")
    tkEqual(sheet[3], { kind: "fact", text: "Bitrate", value: "24 Mbps" })
end sub

sub test_details_in_french_and_spanish()
    file = detailsTestFile()
    live = { video: "transcode", audio: "transcode", video_in: "HEVC 3840×2160 Dolby Vision P8.1", video_out: "H.264 1920×1080", audio_in: "TrueHD 7.1", audio_out: "AAC 2.0", tonemap: "libplacebo", speed: 1.5, transcoded_ms: 70000 }
    info = { method: "transcode", audio_index: 1, reasons: [], burn_subtitles: true }
    i18nUse("fr")
    converted = detPlayback(info, file, -1, live, 40000)
    tkEqual(converted.method, "Transcodage")
    labels = []
    for each row in converted.rows
        labels.Push(row.label)
    end for
    tkEqual(labels, ["Vidéo", "Audio", "Débit", "Affiché en", "Sous-titres", "Vitesse"])
    tkEqual(converted.rows[2].value, "24 Mbit/s")
    tkEqual(converted.rows[3].value, "SDR (mappage tonal avec libplacebo)")
    tkEqual(converted.rows[4].value, "Incrustés")
    tkEqual(converted.rows[5].value, "1,5× temps réel · 30 s d’avance")
    tkEqual(detPlayback(info, file, -1, live, 70000).rows[5].value, "1,5× temps réel", "nothing ready ahead")
    tkEqual(detChapters(file)[1].label, "2. Chapitre 2 · 1:00")
    i18nUse("es")
    tkEqual(detPlayback({ method: "remux", audio_converted: true }, file, -1, invalid, 0).method, "Transmisión directa · Audio transcodificado")
    tkEqual(detPlayback({ method: "direct" }, file, -1, invalid, 0).method, "Reproducción directa")
    tkEqual(detChapters(file)[1].label, "2. Capítulo 2 · 1:00")
end sub

' A Live TV session's plan, as the server answers it.
function detailsTestPlan(fields as string) as object
    return ParseJson("{" + fields + "}")
end function

sub test_details_of_a_live_channel_copied()
    copied = detLive(detailsTestPlan("""video_codec"":""h264"",""width"":1920,""height"":1080,""frame_rate"":50,""interlaced"":false,""bit_depth"":8,""audio_codec"":""aac"",""audio_channels"":2,""video"":""copy"",""audio"":""copy"",""out_height"":null,""video_kbps"":null,""kbps"":8200"))
    tkEqual(copied.method, "Direct stream", "the server's HLS stream with its video copied")
    tkEqual(copied.why, "", "nothing says why: the Roku plays HLS alone")
    tkEqual(copied.rows, [{ label: "Video", value: "H.264 · 1920×1080 · 50 fps" }, { label: "Audio", value: "AAC 2.0" }, { label: "Bitrate", value: "8.2 Mbps" }])
    ' Other sound is made AAC stereo while the video is copied.
    sound = detLive(detailsTestPlan("""video_codec"":""h264"",""width"":1280,""height"":720,""frame_rate"":29.97,""interlaced"":false,""audio_codec"":""eac3"",""audio_channels"":6,""video"":""copy"",""audio"":""aac"",""kbps"":850"))
    tkEqual(sound.method, "Direct stream · Audio transcoding", "a file's words for it")
    tkEqual(sound.method, detMethod({ method: "remux", audio_converted: true }))
    tkEqual(sound.rows, [{ label: "Video", value: "H.264 · 1280×720 · 29.97 fps" }, { label: "Audio", value: "EAC3 5.1 → AAC 2.0" }, { label: "Bitrate", value: "850 kbps" }])
    tkEqual(detSheet(sound)[2], { kind: "fact", text: "Audio", value: "EAC3 5.1 › AAC 2.0" }, "an arrow Roboto has")
    ' Radio: no video, nothing measured yet.
    radio = detLive(detailsTestPlan("""video_codec"":null,""width"":null,""height"":null,""frame_rate"":null,""interlaced"":false,""audio_codec"":""mp2"",""audio_channels"":2,""video"":""none"",""audio"":""aac"",""kbps"":0"))
    tkEqual(radio.method, "Direct stream · Audio transcoding")
    tkEqual(radio.rows, [{ label: "Audio", value: "MP2 2.0 → AAC 2.0" }])
end sub

sub test_details_of_a_live_channel_transcoded()
    hevc = detLive(detailsTestPlan("""video_codec"":""hevc"",""width"":3840,""height"":2160,""frame_rate"":50,""interlaced"":false,""bit_depth"":10,""audio_codec"":""eac3"",""audio_channels"":6,""video"":""h264_nvenc"",""audio"":""aac"",""out_height"":1080,""video_kbps"":8000,""kbps"":12300"))
    tkEqual(hevc.method, "Transcoding")
    tkEqual(hevc.why, "")
    tkEqual(hevc.rows, [{ label: "Video", value: "HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p" }, { label: "Audio", value: "EAC3 5.1 → AAC 2.0" }, { label: "Bitrate", value: "12 Mbps → 8 Mbps" }, { label: "Encoder", value: "NVIDIA NVENC" }])
    sheet = detSheet(hevc)
    tkEqual(sheet[0], { kind: "heading", text: "Transcoding" })
    tkEqual(sheet[1], { kind: "fact", text: "Video", value: "HEVC 10-bit · 3840×2160 · 50 fps › H.264 1080p" })
    ' Interlaced video made progressive, at the channel's own lines.
    interlaced = detLive(detailsTestPlan("""video_codec"":""mpeg2video"",""width"":720,""height"":576,""frame_rate"":25,""interlaced"":true,""bit_depth"":8,""audio_codec"":""mp2"",""audio_channels"":2,""video"":""libx264"",""audio"":""aac"",""out_height"":576,""video_kbps"":2500,""kbps"":4100"))
    tkEqual(interlaced.rows[0], { label: "Video", value: "MPEG-2 · 720×576 · 25 fps, interlaced → H.264 576p" })
    tkEqual(interlaced.rows[2], { label: "Bitrate", value: "4.1 Mbps → 2.5 Mbps" })
    tkEqual(interlaced.rows[3], { label: "Encoder", value: "x264" })
    tkEqual(detEncoderName("hevc_qsv"), "Intel Quick Sync")
    tkEqual(detEncoderName("h264_amf"), "AMD AMF")
    tkEqual(detEncoderName("hevc_vaapi"), "VA-API")
    tkEqual(detEncoderName("h264_videotoolbox"), "VideoToolbox")
    tkEqual(detEncoderName("h264_newthing"), "h264_newthing", "an encoder not known, as the server names it")
    tkEqual(detEncoded("hevc_qsv"), "HEVC")
    tkEqual(detEncoded("mystery"), "", "an encoder whose name doesn't say")
end sub

sub test_details_of_a_live_channel_from_an_older_server()
    ' No bit depth, transcode size or bitrates: the rows say what is known.
    older = detLive(detailsTestPlan("""video_codec"":""hevc"",""width"":1920,""height"":1080,""frame_rate"":25,""interlaced"":false,""audio_codec"":""ac3"",""audio_channels"":6,""video"":""libx264"",""audio"":""aac"""))
    tkEqual(older.method, "Transcoding")
    tkEqual(older.rows, [{ label: "Video", value: "HEVC · 1920×1080 · 25 fps → H.264 1080p" }, { label: "Audio", value: "AC3 5.1 → AAC 2.0" }, { label: "Encoder", value: "x264" }])
    copied = detLive(detailsTestPlan("""video_codec"":""h264"",""width"":null,""height"":null,""frame_rate"":null,""interlaced"":false,""audio_codec"":null,""audio_channels"":null,""video"":""copy"",""audio"":""copy"""))
    tkEqual(copied.method, "Direct stream")
    tkEqual(copied.rows, [{ label: "Video", value: "H.264" }])
    tkEqual(detLive({}).rows.Count(), 0, "nothing known, nothing said")
end sub

sub test_details_of_a_live_channel_in_other_languages()
    plan = detailsTestPlan("""video_codec"":""hevc"",""width"":1920,""height"":1080,""frame_rate"":29.97,""interlaced"":true,""bit_depth"":10,""audio_codec"":""ac3"",""audio_channels"":6,""video"":""h264_nvenc"",""audio"":""aac"",""out_height"":720,""video_kbps"":4500,""kbps"":9400")
    i18nUse("fr")
    details = detLive(plan)
    tkEqual(details.method, "Transcodage")
    tkEqual(detLive(detailsTestPlan("""video_codec"":""h264"",""audio_codec"":""ac3"",""video"":""copy"",""audio"":""aac""")).method, "Flux direct · Audio transcodé")
    tkEqual(details.rows, [{ label: "Vidéo", value: "HEVC 10 bits · 1920×1080 · 29,97 i/s, entrelacé → H.264 720p" }, { label: "Audio", value: "AC3 5.1 → AAC 2.0" }, { label: "Débit", value: "9,4 Mbit/s → 4,5 Mbit/s" }, { label: "Encodeur", value: "NVIDIA NVENC" }])
    i18nUse("es")
    details = detLive(plan)
    tkEqual(details.method, "Transcodificación")
    tkEqual(details.rows[0].value, "HEVC 10 bits · 1920×1080 · 29,97 fps, entrelazado → H.264 720p")
    tkEqual(details.rows[3].label, "Codificador")
    i18nUse("zh-CN")
    details = detLive(plan)
    tkEqual(details.rows[0].value, "HEVC 10 位 · 1920×1080 · 29.97 帧/秒，隔行扫描 → H.264 720p")
    tkEqual(details.rows[3].label, "编码器")
    i18nUse("zh-TW")
    tkEqual(detLive(plan).rows[3].label, "編碼器")
end sub

sub test_details_chapters()
    chapters = detChapters(detailsTestFile())
    tkEqual(chapters, [{ label: "1. Opening · 0:00", name: "Opening", startMs: 0 }, { label: "2. Chapter 2 · 1:00", name: "", startMs: 60000 }])
    tkEqual(detChapterAt(chapters, 59999), 0)
    tkEqual(detChapterAt(chapters, 60000), 1)
    tkEqual(detChapters(invalid).Count(), 0)
end sub
